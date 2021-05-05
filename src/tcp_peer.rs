#![allow(dead_code)]

use crate::{bdecoder::{parse, Item}, http_tracker::{get_http_addr, http_announce_tracker}, tcp_msg::*, udp_tracker::get_info_hash};

use std::{fs::File, io::{Read, Write}, net::{Ipv4Addr, TcpStream}, path::Path, sync::{Arc, Mutex, Weak, atomic::AtomicBool}, thread::{self, JoinHandle}, usize, vec};

#[cfg(target_family="windows")]
use std::os::windows::prelude::*;
#[cfg(target_family="unix")]
use std::os::unix::fs::FileExt;

use sha1::{Digest, Sha1};

pub const EMPTY: u8 = 0;
pub const IN_PROGRESS: u8 = 1;
pub const COMPLETE: u8 = 2;

#[derive(Default)]
pub struct ByteField {
    pub arr: Vec<(u8, Option<std::sync::Weak<AtomicBool>>)>,
}

impl ByteField {
    pub fn is_full(&self) -> bool {
        let nonfull: usize = self.arr.iter().filter(|(x, _y)| *x < 2).count();
        if nonfull == 0 { return true }
        else { return false }
    }

    fn get_empty(&self) -> Option<usize> {
        if self.is_full() { return None }
        for i in 0..(self.arr.len()) {
            if self.arr[i].0 == EMPTY {
                return Some(i);
            }
        }
        return None;
    }
}

// add Option return value
pub fn send_handshake(stream: &mut TcpStream, info_hash: [u8; 20], peer_id: [u8; 20]) -> Option<()> {
    // make handshake
    let handshake = Handshake { info_hash: info_hash, peer_id: peer_id, ..Default::default() };
    let interest = Header { len: 1u32.to_be(), byte: INTEREST };
    let mut handshake_u8 = bincode::serialize(&handshake).unwrap();

    // send hanshake
    handshake_u8.append(&mut bincode::serialize(&interest).unwrap());
    stream.write_all(&handshake_u8).expect("handshake write error");
    let mut buf: Vec<u8> = vec![0; 8192];
    match stream.read(&mut buf) {
        Ok(_) => return Some(()),
        Err(_) => return None,
    }
}

fn fetch_subpiece(stream: &mut TcpStream, index: u32, offset: u32, 
               plen: u32, field: &mut ByteField) -> Option<Piece> {
    let mut req = Request { 
        head: Header { len: 13u32.to_be(), byte: REQUEST }, ..Default::default()
    };
    
    req.index = index.to_be();
    req.offset = offset.to_be();
    req.plen = plen.to_be();
    
    let req_u8 = bincode::serialize(&req).unwrap();
    
    match stream.write_all(&req_u8) {
        Ok(_) => {}
        Err(_) => return None,
    }

    loop {
        let mut msg: Vec<u8> = vec![];
        let mut extbuf: Vec<u8> = vec![];
        loop {
            let mut buf: Vec<u8> = vec![0; 32767];
            let bytes; 
            match stream.read(&mut buf) {
                Ok(b) => bytes = b,
                Err(_) => return None,
            }

            buf.truncate(bytes);
            if bytes == 0 { return None; }

            for i in &buf {
                extbuf.push(*i);
            }

            // std::thread::sleep(std::time::Duration::from_millis(1000));
            // println!("{} {} {}", stream.peer_addr().unwrap(), bytes, extbuf.len());

            if try_parse(&extbuf) {
                // println!("extbuflen {}", extbuf.len());
                for i in &extbuf {
                    msg.push(*i);
                }
                extbuf.clear();
                break;
            }
        }
        // println!("msglen {}", msg.len());
        
        let parsed = parse_msg(&mut msg);
        let mut piece: Piece = Default::default();
        piece.data = Vec::new();
        
        for m in parsed {
            piece = match m {
                Message::Piece(piece) => piece,
                _ => continue,
            };
            
            if piece.data.len() == 0 { continue }
            
            field.arr[(piece.offset.to_le()/plen) as usize].0 = 1;
            return Some(piece);
        }
    }
}

pub fn hash_write_piece(piece: Vec<Piece>, hash: Vec<u8>, 
                        file: &Arc<Mutex<File>>, piece_len: usize) -> JoinHandle<Option<()>> {
    let mut flat_piece: Vec<u8> = vec![];
    for s in piece.iter() {
        flat_piece.extend_from_slice(&s.data); // assumes ordered by offset
    }

    let f = Arc::clone(file);

    let handle = thread::spawn(move || {
        let mut hasher = Sha1::new();
        hasher.update(flat_piece);
        let piece_hash = hasher.finalize().to_vec();

        if piece_hash.iter().zip(&hash).filter(|&(a, b)| a == b).count() != 20 {
            return None;
        }

        for s in piece.iter() {
            let offset = (s.index.to_le() as usize*piece_len)+s.offset.to_le() as usize;
            // critical section
            { 
                let file = f.lock().unwrap();
                #[cfg(target_family="windows")]
                file.seek_write(&s.data, offset as u64).expect("file write error");
                #[cfg(target_family="unix")]
                file.write_all_at(&s.data, offset as u64).expect("file write error");
            }
        }
        return Some(());
    });
    return handle;
}

pub fn split_hashes(hashes: Vec<u8>) -> Vec<Vec<u8>> {
    let num_pieces: usize = hashes.len()/20;
    let mut split_hashes: Vec<Vec<u8>> = vec![vec![0; 0]; num_pieces];
    for i in 0..num_pieces {
        split_hashes[i].extend_from_slice(&hashes[(i*20)..((i+1)*20)])
    }
    return split_hashes;
}

pub fn file_getter(stream: &mut TcpStream, piece_len: usize, num_pieces: usize, 
                   file_len: usize, hashes: &Vec<Vec<u8>>, file: &Arc<Mutex<File>>, 
                   field: &Arc<Mutex<ByteField>>, alive: &Arc<AtomicBool>) {

    let mut threads: Vec<JoinHandle<Option<()>>> = vec![];
    // make request and piece bytefield
    let mut req = Request { 
        head: Header { len: 13u32.to_be(), byte: REQUEST }, index: 0, offset: 0, plen: SUBPIECE_LEN.to_be() 
    };
    let num_subpieces = piece_len/SUBPIECE_LEN as usize;

    let piece_field = field.clone();

    // indices this thread downloaded
    let mut indices: Vec<usize> = vec![];

    // get pieces
    // all except last piece
    loop {
        let mut piece: Vec<Piece> = vec![];
        let piece_idx;
        { // critical section
            let mut pf = piece_field.lock().unwrap();
            piece_idx = match pf.get_empty() {
                Some(p) => p,
                None => break
            };
            pf.arr[piece_idx].0 = IN_PROGRESS;
            pf.arr[piece_idx].1 = Some(Arc::downgrade(alive));
        }
        indices.push(piece_idx);

        if piece_idx != num_pieces-1 {
            req.index = piece_idx as u32;
            
            let mut subfield: ByteField = Default::default();
            subfield.arr = vec![(0, None); num_subpieces];

            // subpieces
            loop {
                let sub_idx = match subfield.get_empty() {
                    Some(sub) => sub,
                    None => break
                };

                req.offset = (sub_idx as u32)*SUBPIECE_LEN;
                let subp = fetch_subpiece(stream, req.index, req.offset, 
                    SUBPIECE_LEN, &mut subfield);
                if subp.is_none() { return; }
                piece.push(subp.unwrap());
            }
            piece.sort_by_key(|x| x.offset);
            threads.push(
                hash_write_piece(
                    piece.to_vec(), hashes[piece_idx].to_vec(), file, piece_len));
            { // critical section
                let mut pf = piece_field.lock().unwrap();
                pf.arr[piece_idx] = (COMPLETE, None);
            }
        } else {
            let mut piece: Vec<Piece> = vec![];
            // last piece
            let last_remainder: usize = file_len-(num_pieces-1)*piece_len;
            let num_last_subs: usize = last_remainder/SUBPIECE_LEN as usize;
            let mut last_subfield: ByteField = Default::default();
            last_subfield.arr = vec![(0, None); num_last_subs];

            // all except last subpiece
            req.index = num_pieces as u32 - 1;
            loop {
                let sub = last_subfield.get_empty();
                if sub == None { break }
                let sub_idx = sub.unwrap();
                
                req.offset = (sub_idx as u32)*SUBPIECE_LEN;
                
                let subp = fetch_subpiece(stream, req.index, req.offset, 
                    SUBPIECE_LEN, &mut last_subfield);
                if subp.is_none() { return; }
                piece.push(subp.unwrap());
            }


            // last subpiece
            let last_sub_len: usize = last_remainder-(num_last_subs*SUBPIECE_LEN as usize);
            let mut final_subfield: ByteField = Default::default();
            
            req.offset = (num_last_subs as u32)*SUBPIECE_LEN;
            req.plen = last_sub_len as u32;
            final_subfield.arr = vec![(0, None); (req.offset/req.plen) as usize + 1];

            let subp = fetch_subpiece(stream, req.index, req.offset, 
                req.plen, &mut final_subfield);
            if subp.is_none() { return; }
            piece.push(subp.unwrap());
            piece.sort_by_key(|x| x.offset);
            threads.push(
                hash_write_piece(
                    piece.to_vec(), hashes[num_pieces-1].to_vec(), file, piece_len));
            { // critical section
                let mut pf = piece_field.lock().unwrap();
                pf.arr[piece_idx] = (COMPLETE, None);
            }
        }
    }
    
    for t in threads {
        match t.join().unwrap() {
            Some(()) => continue,
            None => { // if any hashes didn't match
                { // critical section
                    let mut pf = piece_field.lock().unwrap();
                    // discard all indices
                    for i in &indices {
                        pf.arr[*i] = (EMPTY, None);
                    }
                    return;
                }
            }
        }
    }
}

pub fn tcp_download_pieces(p: &Path) {
    // read and parse torrent file
    let bytes: Vec<u8> = std::fs::read(p).expect("read error");
    let mut str: Vec<u8> = bytes.clone();
    let tree: Vec<Item> = parse(&mut str);
    let info_hash = get_info_hash(bytes);
    let addr = get_http_addr(tree.clone());
    
    // get info dict values
    let dict = tree[0].get_dict();
    let info = dict.get("info".as_bytes()).unwrap().get_dict();
    let piece_len = info.get("piece length".as_bytes()).unwrap().get_int() as usize;
    let num_pieces = (info.get("pieces".as_bytes()).unwrap().get_str().len()/20) as usize;
    let file_len = info.get("length".as_bytes()).unwrap().get_int() as usize;
    let hashes = info.get("pieces".as_bytes()).unwrap().get_str();
    let filename = info.get("name".as_bytes()).unwrap().get_str();
    let split_hashes = Arc::new(split_hashes(hashes));

    let path = Path::new(std::str::from_utf8(&filename).unwrap());
    let dest = Arc::new(Mutex::new(File::create(path).unwrap()));
    let field: Arc<Mutex<ByteField>> = Arc::new(
        Mutex::new(ByteField { arr: vec![(EMPTY, None); num_pieces]}));

    let mut threads = vec![];
    let mut count: usize = 0;
    
    const LOOP_SLEEP: u64 = 1;
    const ANNOUNCE_INTERVAL: usize = 60/LOOP_SLEEP as usize;

    loop {
        let mut indices_avail = false;
        let mut progress = 0;
        let field = field.clone();
        { // critical section
            // break loop when all pieces complete
            let mut pf = field.lock().unwrap();
            if pf.is_full() { break }

            // if thread exited prematurely discard it's indice
            for i in 0..pf.arr.len() {
                if pf.arr[i].0 == IN_PROGRESS {
                    // if alive was dropped
                    if Weak::upgrade(pf.arr[i].1.as_ref().unwrap()).is_none() {
                        pf.arr[i].0 = EMPTY;
                        indices_avail = true;
                    }
                } else if pf.arr[i].0 == EMPTY {
                    indices_avail = true;
                } else if pf.arr[i].0 == COMPLETE {
                    progress += 1;
                }
            }
        }
        println!("progress {}/{}", progress, num_pieces);

        if count % ANNOUNCE_INTERVAL == 0 && indices_avail {
            let peers;
            match http_announce_tracker(addr, info_hash) {
                Ok(p) => peers = p,
                Err(e) => {
                    eprintln!("{}", e);
                    count = 1;
                    continue;
                }
            }
            for peer in peers {
                let addr = (Ipv4Addr::from(peer.ip), peer.port);
                let shashes = split_hashes.clone();
                let file = dest.clone();
                let pf = field.clone();

                let builder = std::thread::Builder::new().name(format!("{:?}", addr.0));
                let handle = builder.spawn(move || {
                    // dropped when thread exits
                    let alive = Arc::new(AtomicBool::new(true));

                    if addr.1 == 25565 { return } // localhost
                    let stream = TcpStream::connect(addr);

                    match stream {
                        Ok(mut stream) => {
                            stream.set_nonblocking(false).unwrap();
                            stream.set_read_timeout(Some(std::time::Duration::from_secs(30))).unwrap();

                            match send_handshake(&mut stream, info_hash, info_hash) {
                                Some(_) => {},
                                None => return,
                            }

                            file_getter(&mut stream, piece_len, num_pieces, file_len, 
                                        &shashes, &file, &pf, &alive);
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

    for t in threads {
        match t.join() {
            _ => {}
        }
    }
}