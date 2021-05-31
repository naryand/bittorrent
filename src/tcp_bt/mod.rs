// tcp_bt subfolder and tcp peer wire handshaking
#![allow(dead_code)]

pub mod connect;
pub mod fetch;
pub mod msg;
pub mod parse;
pub mod seed;

use crate::{
    bencode::Item,
    field::{constant::*, ByteField},
    file::resume_torrent,
    hash::{spawn_hash_write, Hasher},
    tcp_bt::{
        connect::{spawn_connectors, Connector},
        msg::{bytes::*, structs::*, SUBPIECE_LEN},
        seed::{spawn_listener, Peer},
    },
    torrent::Torrent,
    tracker::{announce, get_addr},
};

use std::{
    io::Write,
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
};

pub fn send_handshake(
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
    stream.write_all(&handshake_u8).ok()?;

    // receive handshake
    let mut buf: Vec<u8> = vec![0; 8192];
    stream.peek(&mut buf).ok()?;
    Some(())
}

// makes connections to peers and downloads the torrent files
pub fn add_torrent(torrent: &Arc<Torrent>, tree: &[Item]) {
    // parse torrent file
    let addr = get_addr(&tree).unwrap();

    // local tracker testing
    // use std::net::ToSocketAddrs;
    // let addr =
    //     crate::tracker::Addr::Http("127.0.0.1:8000".to_socket_addrs().unwrap().nth(0).unwrap());

    // piece field
    let field: Arc<Mutex<ByteField>> = Arc::new(Mutex::new(ByteField {
        arr: vec![EMPTY; torrent.num_pieces],
    }));
    let connector = Arc::new(Connector::new());

    // spawn hashing thread pool
    let hasher = Arc::new(Hasher::new());
    let hasher_handles = spawn_hash_write(&hasher, &field, &torrent, &connector, 24);

    // resume any partial pieces
    resume_torrent(&torrent, &hasher);

    // start connection thread pool
    let listener = Arc::new(TcpListener::bind(("0.0.0.0", 0)).unwrap());
    let port = listener.local_addr().unwrap().port();
    let listener_handle = spawn_listener(&connector, &listener);

    let scount = Arc::new(AtomicU32::new(0));
    let connector_handles = spawn_connectors(&connector, &hasher, &torrent, &field, &scount, 50);

    let tor = Arc::clone(&torrent);
    let num_subpieces = tor.piece_len / SUBPIECE_LEN as usize;

    // main loop control
    let mut seeded = 0_usize;
    let mut counter = 0_usize;
    const ANNOUNCE_INTERVAL: usize = 60 / LOOP_SLEEP as usize;
    const LOOP_SLEEP: usize = 1;

    // shutdown when share ratio >= 1
    while seeded < tor.num_pieces {
        let mut progress = 0;
        {
            let pf = field.lock().unwrap();
            for i in &pf.arr {
                if *i == COMPLETE {
                    progress += 1;
                }
            }
        }
        print!("progress {}/{}, ", progress, tor.num_pieces);
        println!("seeded {}/{}", seeded, tor.num_pieces);

        if counter % ANNOUNCE_INTERVAL == 0 {
            let peers = match announce(addr, tor.info_hash, port) {
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
                {
                    let mut q = connector.queue.lock().unwrap();
                    q.push_back(Peer::Addr(addr));
                    connector.loops.notify_one();
                }
            }
        }

        counter += 1;
        std::thread::sleep(std::time::Duration::from_secs(LOOP_SLEEP as u64));

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
    connector.loops.notify_all();
    // join threads
    for t in connector_handles {
        t.join().unwrap();
    }
    for t in hasher_handles {
        t.join().unwrap();
    }

    listener_handle.join().unwrap();
}
