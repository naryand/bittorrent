// tcp peer wire piece downloading
#![allow(dead_code)]

use super::{msg::{bytes::*, structs::*, Message, try_parse, parse_msg, SUBPIECE_LEN}, send_handshake};
use crate::{bencode::{Item}, field::{ByteField, constant::*}, file::{resume_torrent}, 
            hash::{Hasher, spawn_hash_write}, torrent::Torrent, tracker::{http::{get_addr, announce}}};

use std::{io::{Read, Write}, net::{Ipv4Addr, SocketAddr, TcpStream}, 
          sync::{Arc, Mutex, Weak, atomic::AtomicBool}, time::Duration, usize, vec};

// fetches a single subpiece
fn fetch_subpiece(stream: &mut TcpStream, field: &mut ByteField, 
                  index: u32, offset: u32, plen: u32) -> Option<Piece> {
    let mut req = Request { 
        head: Header { len: 13u32.to_be(), byte: REQUEST }, ..Default::default()
    };
    
    req.index = index.to_be();
    req.offset = offset.to_be();
    req.plen = plen.to_be();
    
    let req_u8 = bincode::serialize(&req).unwrap();
    
    stream.write_all(&req_u8).ok()?;

    loop {
        let mut msg: Vec<u8> = vec![];
        let mut extbuf: Vec<u8> = vec![];
        loop {
            let mut buf: Vec<u8> = vec![0; 32767];
            let bytes = stream.read(&mut buf).ok()?;

            buf.truncate(bytes);
            if bytes == 0 { return None; }

            for i in &buf {
                extbuf.push(*i);
            }

            if try_parse(&extbuf) {
                for i in &extbuf {
                    msg.push(*i);
                }
                extbuf.clear();
                break;
            }
        }
        
        let parsed = parse_msg(&mut msg);
        let mut piece: Piece = Default::default();
        piece.data = Vec::new();
        
        for m in parsed {
            piece = match m {
                Message::Piece(piece) => piece,
                _ => continue,
            };
            
            if piece.data.len() == 0 { continue }
            
            field.field[(piece.offset.to_le()/plen) as usize].0 = 1;
            return Some(piece);
        }
    }
}

// represents a single connection to a peer, continously fetches subpieces
pub fn torrent_fetcher(stream: &mut TcpStream, torrent: &Arc<Torrent>, field: &Arc<Mutex<ByteField>>, 
                       alive: &Arc<AtomicBool>, hasher: &Arc<Hasher>, brk_conns: &Arc<AtomicBool>) {

    // make request and piece bytefield
    let mut req = Request { 
        head: Header { len: 13u32.to_be(), byte: REQUEST }, index: 0, offset: 0, plen: SUBPIECE_LEN.to_be() 
    };
    let tor = Arc::clone(torrent);
    let num_subpieces = tor.piece_len/SUBPIECE_LEN as usize;
    let piece_field = Arc::clone(field);
    let hq = Arc::clone(hasher);
    let bcon = Arc::clone(brk_conns);

    // get pieces
    loop {
        // pick a piece
        let mut piece: Vec<Piece> = vec![];
        let piece_idx;
        { // critical section
            let mut pf = hq.conns.wait_while(piece_field.lock().unwrap(), 
            |pf| {
                if bcon.load(std::sync::atomic::Ordering::Relaxed) { return false; }
                match pf.get_empty() {
                    Some(_) => return false,
                    None => return true,
                }
            }).unwrap();
            piece_idx = match pf.get_empty() {
                Some(p) => p,
                None => return,
            };
            pf.field[piece_idx] = (IN_PROGRESS, Some(Arc::downgrade(alive)));
        }

        // all except last piece
        if piece_idx != tor.num_pieces-1 {
            req.index = piece_idx as u32;
            let mut subfield = ByteField { field: vec![(0, None); num_subpieces] };

            // subpieces
            loop {
                let sub_idx = match subfield.get_empty() {
                    Some(sub) => sub,
                    None => break
                };

                req.offset = (sub_idx as u32)*SUBPIECE_LEN;
                let subp 
                = fetch_subpiece(stream, &mut subfield, req.index, req.offset, SUBPIECE_LEN);

                if subp.is_none() { return; }
                piece.push(subp.unwrap());
            }
            piece.sort_by_key(|x| x.offset);
            { // critical section
                let mut q = hq.queue.lock().unwrap();
                q.push_back((piece, tor.hashes[piece_idx].to_vec()));
                hq.loops.notify_one();
            }
        } else {
            let mut piece: Vec<Piece> = vec![];
            // last piece
            let last_remainder: usize = tor.file_len-(tor.num_pieces-1)*tor.piece_len;
            let num_last_subs: usize = last_remainder/SUBPIECE_LEN as usize;
            let mut last_subfield = ByteField { field: vec![(0, None); num_last_subs] };

            // all except last subpiece
            req.index = tor.num_pieces as u32 - 1;
            loop {
                let sub = last_subfield.get_empty();
                if sub == None { break }
                let sub_idx = sub.unwrap();
                
                req.offset = (sub_idx as u32)*SUBPIECE_LEN;
                
                let subp 
                = fetch_subpiece(stream, &mut last_subfield, req.index, req.offset, SUBPIECE_LEN);

                if subp.is_none() { return; }
                piece.push(subp.unwrap());
            }

            // last subpiece
            let last_sub_len: usize = last_remainder-(num_last_subs*SUBPIECE_LEN as usize);
            req.offset = (num_last_subs as u32)*SUBPIECE_LEN;
            req.plen = last_sub_len as u32;
            let mut final_subfield = ByteField { 
                field: vec![(0, None); (req.offset/req.plen) as usize + 1] 
            };

            let subp 
            = fetch_subpiece(stream, &mut final_subfield, req.index, req.offset, req.plen);

            if subp.is_none() { return; }
            piece.push(subp.unwrap());

            piece.sort_by_key(|x| x.offset);
            { // critical section
                let mut q = hq.queue.lock().unwrap();
                q.push_back((piece, tor.hashes[tor.num_pieces-1].to_vec()));
                hq.loops.notify_one();
            }
        }
    }
}

// makes connections to peers and downloads the torrent files
pub fn download_torrent(torrent: &Arc<Torrent>, tree: Vec<Item>) {
    // parse torrent file
    let addr = get_addr(tree).unwrap();
    
    // piece field
    let field: Arc<Mutex<ByteField>> = Arc::new(Mutex::new(ByteField { 
            field: vec![(EMPTY, None); torrent.num_pieces]
    }));

    // connection loop breaker
    let break_conns = Arc::new(AtomicBool::new(false));
    
    // spawn hashing threads
    let hq = Arc::new(Hasher::new());
    let mut handles = spawn_hash_write(&hq, &field, &torrent, 24);
    let mut threads = vec![];
    threads.append(&mut handles);
        
    // resume any partial pieces
    resume_torrent(&torrent, &hq);

    // main loop control
    let mut count: usize = 0;
    const ANNOUNCE_INTERVAL: usize = 60/LOOP_SLEEP as usize;
    const LOOP_SLEEP: u64 = 1;

    let loop_torrent = Arc::clone(&torrent);

    loop {
        let mut indices_avail = false;
        let mut progress = 0;
        let piece_field = Arc::clone(&field);
        { // critical section
            // break loop when all pieces complete
            let mut pf = piece_field.lock().unwrap();
            if pf.is_full() { break }
            
            // if thread exited prematurely discard it's indice
            for i in 0..pf.field.len() {
                if pf.field[i].0 == IN_PROGRESS {
                    // if alive was dropped
                    if Weak::upgrade(pf.field[i].1.as_ref().unwrap()).is_none() {
                        // unreserve piece and notify waiting connections
                        pf.field[i] = (EMPTY, None);
                        hq.conns.notify_one();
                        indices_avail = true;
                    }
                } else if pf.field[i].0 == EMPTY {
                    indices_avail = true;
                } else if pf.field[i].0 == COMPLETE {
                    progress += 1;
                }
            }
        }
        println!("progress {}/{}", progress, loop_torrent.num_pieces);
        
        if count % ANNOUNCE_INTERVAL == 0 && indices_avail {
            let peers;
            match announce(addr, loop_torrent.info_hash) {
                Ok(p) => peers = p,
                Err(e) => {
                    eprintln!("{}", e);
                    count = 1;
                    continue;
                }
            }
            for peer in peers {
                let addr = (Ipv4Addr::from(peer.ip), peer.port);
                let field = Arc::clone(&piece_field);
                let brk_conns = Arc::clone(&break_conns);
                let hasher = Arc::clone(&hq);
                let torrent = Arc::clone(&loop_torrent);

                let builder = std::thread::Builder::new().name(format!("{:?}", addr.0));
                let handle = builder.spawn(move || {
                    // dropped when thread exits
                    let alive = Arc::new(AtomicBool::new(true));

                    if addr.1 == 25565 { return } // localhost
                    let address = &SocketAddr::new(std::net::IpAddr::V4(addr.0), addr.1);
                    let timeout = Duration::from_secs(5);
                    let stream = TcpStream::connect_timeout(address, timeout);

                    match stream {
                        Ok(mut stream) => {
                            stream.set_nonblocking(false).unwrap();
                            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();

                            match send_handshake(&mut stream, torrent.info_hash, torrent.info_hash) {
                                 Some(_) => {}
                                 None => return,
                            }

                            torrent_fetcher(&mut stream, &torrent, &field, &alive, &hasher, &brk_conns);
                            return;
                        }
                        Err(_) => return,
                    }
                }).unwrap();
                threads.push(handle);
            }
        }

        count += 1;
        std::thread::sleep(std::time::Duration::from_secs(LOOP_SLEEP));
    }
    
    // shutdown
    // break hasher loops
    hq.brk.store(true, std::sync::atomic::Ordering::Relaxed);
    {
        let mut q = hq.queue.lock().unwrap();
        q.push_back((vec![], vec![]));
    }
    hq.loops.notify_all();
    // break connection loops
    break_conns.store(true, std::sync::atomic::Ordering::Relaxed);
    hq.conns.notify_all();
    // join threads
    for t in threads {
        match t.join() {
            _ => {}
        }
    }
}