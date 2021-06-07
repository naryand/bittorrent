// tcp_bt subfolder and tcp peer wire handshaking
#![allow(dead_code)]

pub mod connect;
pub mod fetch;
pub mod msg;
pub mod parse;
pub mod seed;

use crate::{
    field::{constant::*, ByteField},
    file::resume_torrent,
    hash::{spawn_hash_write, Hasher},
    tcp_bt::{
        connect::{spawn_connector_task, Connector},
        msg::{bytes::*, structs::*, SUBPIECE_LEN},
        parse::{spawn_parsers, Parser},
        seed::{spawn_listener, Peer},
    },
    torrent::Torrent,
    tracker::{announce, get_addr},
};

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
};

use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    runtime::Handle,
    task::{self, JoinHandle},
    time,
};

use num_cpus;

pub async fn send_handshake(
    stream: &mut TcpStream,
    info_hash: [u8; 20],
    peer_id: [u8; 20],
) -> Option<()> {
    // make handshake
    let handshake = Handshake {
        info_hash,
        peer_id,
        ..Handshake::default()
    };
    let interest = Header {
        len: 1_u32.to_be(),
        byte: INTEREST,
    };
    let mut handshake_u8 = bincode::serialize(&handshake).unwrap();

    // send handshake
    handshake_u8.append(&mut bincode::serialize(&interest).unwrap());
    stream.write_all(&handshake_u8).await.ok()?;

    // receive handshake
    let mut buf: Vec<u8> = vec![0; 8192];
    stream.peek(&mut buf).await.ok()?;
    Some(())
}

// makes connections to peers and downloads the torrent files
impl Torrent {
    pub async fn start(self) {
        let torrent = Arc::new(self);
        // parse torrent file
        let addr = get_addr(&torrent.tree).unwrap();

        // local tracker testing
        // use std::net::ToSocketAddrs;
        // let addr =
            // crate::tracker::Addr::Http("127.0.0.1:8000".to_socket_addrs().unwrap().nth(0).unwrap());

        // piece field
        let field: Arc<Mutex<ByteField>> = Arc::new(Mutex::new(ByteField {
            arr: vec![EMPTY; torrent.num_pieces],
        }));
        let connector = Arc::new(Connector::new());

        // spawn hashing thread pool
        let hasher = Arc::new(Hasher::new());
        let handle = Handle::current();
        let threads = num_cpus::get();
        let hasher_handles = spawn_hash_write(
            &hasher,
            &field,
            &torrent,
            &connector,
            handle.clone(),
            threads,
        );

        // resume any partial pieces
        resume_torrent(&torrent, &hasher).await;

        // start parser thread pool
        let parser = Arc::new(Parser::new());
        let parser_handles = spawn_parsers(&parser, &hasher, handle.clone(), 50);

        let scount = Arc::new(AtomicU32::new(0));
        let mut conn_handles: Vec<JoinHandle<()>> = vec![];

        let listener = TcpListener::bind(("0.0.0.0", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let l_handle =
            spawn_listener(listener, &parser, &torrent, &field, &connector, &scount).await;

        let tor = Arc::clone(&torrent);
        let num_subpieces = tor.piece_len / SUBPIECE_LEN as usize;

        // main loop control
        let mut seeded = 0_usize;
        let mut counter = 0_usize;
        const ANNOUNCE_INTERVAL: usize = 60 / LOOP_SLEEP as usize;
        const LOOP_SLEEP: usize = 1;

        // shutdown when share ratio >= 1
        while seeded < tor.num_pieces {
            let mut progress = 0_usize;
            task::block_in_place(|| {
                let pf = field.lock().unwrap();
                for i in &pf.arr {
                    if *i == COMPLETE {
                        progress += 1;
                    }
                }
            });
            print!("progress {}/{}, ", progress, tor.num_pieces);
            println!("seeded {}/{}", seeded, tor.num_pieces);

            if counter % ANNOUNCE_INTERVAL == 0 {
                let peers = match announce(addr, tor.info_hash, port).await {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("{}", e);
                        counter = 1;
                        continue;
                    }
                };
                for peer in peers {
                    if peer.port == port {
                        continue;
                    }
                    let addr = SocketAddr::new(IpAddr::from(Ipv4Addr::from(peer.ip)), peer.port);
                    let connector = Arc::clone(&connector);
                    conn_handles.push(
                        spawn_connector_task(
                            Peer::Addr(addr),
                            &parser,
                            &torrent,
                            &field,
                            &connector,
                            &scount,
                        )
                        .await,
                    );
                }
            }

            counter += 1;
            time::sleep(std::time::Duration::from_secs(LOOP_SLEEP as u64)).await;

            seeded = scount.load(Ordering::Relaxed) as usize / num_subpieces;
            if scount.load(Ordering::Relaxed) as usize % num_subpieces > 0 {
                seeded += 1;
            }
        }

        // shutdown
        println!("shutting down");
        // break hasher loops
        hasher.brk.store(true, Ordering::Relaxed);
        hasher.loops.notify_all();
        // break connection loops
        connector.brk.store(true, Ordering::Relaxed);
        // break parser loops
        parser.brk.store(true, Ordering::Relaxed);
        parser.rx.close();
        // join threads
        task::block_in_place(|| {
            for t in hasher_handles {
                t.join().unwrap();
            }
            for t in parser_handles {
                t.join().unwrap();
            }
        });
        l_handle.abort();
        let _ = l_handle.await;
    } // need to abort hanging reads
}
