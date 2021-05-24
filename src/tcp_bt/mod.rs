// tcp_bt subfolder and tcp peer wire handshaking
#![allow(dead_code)]

pub mod msg;
pub mod seed;
pub mod fetch;

use crate::{LISTENING_PORT, bencode::Item, field::{ByteField, constant::*}, file::resume_torrent, 
            hash::{Hasher, spawn_hash_write}, tcp_bt::{fetch::torrent_fetcher, 
            msg::{SUBPIECE_LEN, bytes::*, structs::*}, seed::{Connector, Peer, spawn_listener, 
            torrent_seeder}}, torrent::Torrent, tracker::{get_addr, announce}};

use std::{io::Write, net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream}, 
          sync::{Arc, Mutex, atomic::{AtomicU32, Ordering}}, thread::JoinHandle, time::Duration};

pub fn send_handshake(stream: &mut TcpStream, info_hash: [u8; 20], peer_id: [u8; 20]) -> Option<()> {
    // make handshake
    let handshake = Handshake { 
        info_hash: info_hash, 
        peer_id: peer_id, ..Default::default() 
    };
    let interest = Header { len: 1u32.to_be(), byte: INTEREST };
    let mut handshake_u8 = bincode::serialize(&handshake).unwrap();

    // send handshake
    handshake_u8.append(&mut bincode::serialize(&interest).unwrap());
    stream.write_all(&handshake_u8).ok()?;

    // receive handshake
    let mut buf: Vec<u8> = vec![0; 8192];
    stream.set_nonblocking(false).unwrap();
    stream.peek(&mut buf).ok()?;
    stream.set_nonblocking(true).unwrap();
    return Some(());
}

// makes connections to peers and downloads the torrent files
pub fn add_torrent(torrent: &Arc<Torrent>, tree: Vec<Item>) {
    // parse torrent file
    let addr = get_addr(tree).unwrap();

    // local tracker testing
    // use std::net::ToSocketAddrs;
    // let addr = crate::tracker::Addr::Http("127.0.0.1:8000".to_socket_addrs().unwrap().nth(0).unwrap());
    
    // piece field
    let field: Arc<Mutex<ByteField>> = Arc::new(Mutex::new(ByteField { 
            arr: vec![EMPTY; torrent.num_pieces]
    }));
    let connector = Arc::new(Connector::new());
    
    // spawn hashing thread pool
    let hasher = Arc::new(Hasher::new());
    let a = spawn_hash_write(&hasher, &field, &torrent, &connector, 24);
        
    // resume any partial pieces
    resume_torrent(&torrent, &hasher);

    // start connection thread pool
    let scount = Arc::new(AtomicU32::new(0));
    let b = spawn_listener(&connector);
    let c = spawn_connectors(&connector, &hasher, &torrent, &field, &scount, 50);

    // main loop control
    let mut count: usize = 0;
    const ANNOUNCE_INTERVAL: usize = 60/LOOP_SLEEP as usize;
    const LOOP_SLEEP: u64 = 1;

    let tor = Arc::clone(&torrent);
    let num_subpieces = tor.piece_len as u32/SUBPIECE_LEN;

    loop {
        let mut progress = 0;
        let seeded = scount.load(Ordering::Relaxed)/num_subpieces;
        // shutdown when share ratio == 1
        if seeded >= tor.num_pieces as u32 { break }
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
        
        if count % ANNOUNCE_INTERVAL == 0 {
            let peers;
            match announce(addr, tor.info_hash) {
                Ok(p) => peers = p,
                Err(e) => {
                    eprintln!("{}", e);
                    count = 1;
                    continue;
                }
            }
            for peer in peers {
                if peer.port == LISTENING_PORT { continue }
                let addr = SocketAddr::new(IpAddr::from(Ipv4Addr::from(peer.ip)), peer.port);
                let connector = Arc::clone(&connector);
                {
                    let mut q = connector.queue.lock().unwrap();
                    q.push_back(Peer::Addr(addr));
                    connector.loops.notify_one();
                }
            }
        }

        count += 1;
        std::thread::sleep(std::time::Duration::from_secs(LOOP_SLEEP));
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
    for t in c {
        t.join().unwrap();
    }
    for t in a {
        t.join().unwrap();
    }

    b.join().unwrap();
}

fn spawn_connectors(connector: &Arc<Connector>, hasher: &Arc<Hasher>, torrent: &Arc<Torrent>, 
                    field: &Arc<Mutex<ByteField>>, count: &Arc<AtomicU32>, 
                    threads: usize) -> Vec<JoinHandle<()>> {

    let mut handles = vec![];
    for i in 0..threads {
        let connector = Arc::clone(connector);
        let hasher = Arc::clone(hasher);
        let torrent = Arc::clone(torrent);
        let field = Arc::clone(field);
        let count = Arc::clone(count);
        let builder = std::thread::Builder::new().name(format!("{:?}", i));
        handles.push(builder.spawn(move || {
            loop {
                let peer;
                {
                    let mut guard = 
                    connector.loops.wait_while(connector.queue.lock().unwrap(), 
                    |q| {
                        if connector.brk.load(Ordering::Relaxed) { return false; }
                        return q.is_empty();
                    }).unwrap();
                    if connector.brk.load(Ordering::Relaxed) { break }

                    peer = match guard.pop_front() {
                        Some(s) => s,
                        None => break,
                    }
                }
                
                let mut stream = match peer {
                    Peer::Addr(addr) => {
                        let timeout = Duration::from_secs(5);
                        match TcpStream::connect_timeout(&addr, timeout) {
                            Ok(s) => s,
                            Err(_) => continue,
                        }
                    }
                    Peer::Stream(s) => s,
                };

                stream.set_nonblocking(false).unwrap();
                // stream.set_read_timeout(Some(Duration::from_secs(15))).unwrap();

                match send_handshake(&mut stream, torrent.info_hash, torrent.info_hash) {
                    Some(_) => {},
                    None => continue,
                }
                let v = torrent_fetcher(&mut stream, &hasher, &torrent, &field, &connector, &count);
                // resets in progress pieces
                let mut complete = true;
                {
                    let mut f = field.lock().unwrap();
                    for i in &v {
                        if f.arr[*i] == IN_PROGRESS { f.arr[*i] = EMPTY; }
                    }
                    for i in &f.arr {
                        if *i == EMPTY { complete = false; }
                    }
                }
                if complete { torrent_seeder(&mut stream, &torrent, &field, &connector, &count); }     
            }
        }).unwrap());
    }
    return handles;
}