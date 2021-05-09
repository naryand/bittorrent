#![allow(dead_code)]

use crate::{bdecoder::*, http_tracker::*, tcp_msg::*, udp_tracker::*};

use std::{collections::{BTreeMap, VecDeque}, fs::{File, OpenOptions, create_dir_all}, io::{Read, Write}, 
net::{Ipv4Addr, SocketAddr, TcpStream}, ops::Deref, path::Path, str::from_utf8, 
sync::{Arc, Condvar, Mutex, Weak, atomic::AtomicBool}, thread::{self, JoinHandle}, time::Duration, usize, vec};

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
    stream.read(&mut buf).ok()?;
    return Some(());
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
            
            field.arr[(piece.offset.to_le()/plen) as usize].0 = 1;
            return Some(piece);
        }
    }
}

fn write_piece(piece: &Piece, piece_len: usize, files: &Arc<Vec<FileSize>>) {
    let mut start = (piece.index.to_le() as usize*piece_len)+piece.offset.to_le() as usize;
    let mut end = start+piece.data.len();
    let mut next_file = 0u64;

    for filesize in files.deref() {
        if start > filesize.len && next_file == 0 { 
            start -= filesize.len;
            end -= filesize.len;
            continue;
        }

        if next_file > 0 {
            { // critical section
                let f = filesize.file.lock().unwrap();
                #[cfg(target_family="windows")]
                f.seek_write(&piece.data[(end-start)..], 0u64).unwrap();
                #[cfg(target_family="unix")]
                f.write_all_at(&piece.data[(end-start)..], 0u64).unwrap();
            }
            return;
        } else {
            if end > filesize.len { next_file = (end-filesize.len) as u64; end = filesize.len; }
            
            { // critical section
                let f = filesize.file.lock().unwrap();
                #[cfg(target_family="windows")]
                f.seek_write(&piece.data[0..(end-start)], start as u64).unwrap();
                #[cfg(target_family="unix")]
                f.write_all_at(&piece.data[0..(end-start)], start as u64).unwrap();
            }
            if next_file == 0 { return; }
        }
    }
}

fn spawn_hash_write(queue: &Arc<Mutex<VecDeque<(Vec<Piece>, Vec<u8>)>>>, field: &Arc<Mutex<ByteField>>, 
                    files: &Arc<Vec<FileSize>>, conn_cond: &Arc<Condvar>, empty_cond: &Arc<Condvar>,
                    hash_cond: &Arc<Condvar>, break_hash: &Arc<AtomicBool>, piece_len: usize, threads: usize) 
                    -> Vec<JoinHandle<()>> {

    let mut handles = vec![];

    for _ in 0..threads {
        let q = Arc::clone(&queue);
        let piece_field = Arc::clone(&field);
        let dests = Arc::clone(&files);
        let breakloop = Arc::clone(break_hash);
        let hcond = Arc::clone(&hash_cond);
        let ccond = Arc::clone(conn_cond);
        let econd = Arc::clone(&empty_cond);

        let handle = thread::spawn(move || {
            loop {
                let tuple;
                { // critical section
                    let mut guard = hcond.wait_while(q.lock().unwrap(),
                |q| {
                        return q.is_empty();
                    }).unwrap();
                    if breakloop.load(std::sync::atomic::Ordering::Relaxed) { break }
                    tuple = guard.pop_front().unwrap();
                }
                econd.notify_all();
                let (piece, hash) = tuple;
                let index = piece[0].index as usize;
                let mut flat_piece: Vec<u8> = vec![];
                for s in piece.iter() {
                    flat_piece.extend_from_slice(&s.data); // assumes ordered by offset
                }

                let mut hasher = Sha1::new();
                hasher.update(flat_piece);
                let piece_hash = hasher.finalize().to_vec();

                if piece_hash.iter().zip(&hash).filter(|&(a, b)| *a == *b).count() != 20 {
                    { // critical section
                        // unreserve piece
                        let mut pf = piece_field.lock().unwrap();
                        pf.arr[index] = (EMPTY, None);
                        // notify waiting connections
                        ccond.notify_one();
                    }
                    continue;
                }

                for s in &piece {
                    write_piece(s, piece_len, &dests);
                }
                { // critical section
                    let mut pf = piece_field.lock().unwrap();
                    pf.arr[index] = (COMPLETE, None);
                }
            }
        });
        handles.push(handle);
    }
    return handles;
}

pub fn split_hashes(hashes: Vec<u8>) -> Vec<Vec<u8>> {
    let num_pieces: usize = hashes.len()/20;
    let mut split_hashes: Vec<Vec<u8>> = vec![vec![0; 0]; num_pieces];
    for i in 0..num_pieces {
        split_hashes[i].extend_from_slice(&hashes[(i*20)..((i+1)*20)])
    }
    return split_hashes;
}

pub fn file_getter(stream: &mut TcpStream, piece_len: usize, num_pieces: usize, file_len: usize, 
                   hashes: &Vec<Vec<u8>>, field: &Arc<Mutex<ByteField>>, alive: &Arc<AtomicBool>, 
                   queue: &Arc<Mutex<VecDeque<(Vec<Piece>, Vec<u8>)>>>, hash_cond: &Arc<Condvar>,
                   conn_cond: &Arc<Condvar>, break_conns: &Arc<AtomicBool>) {

    // make request and piece bytefield
    let mut req = Request { 
        head: Header { len: 13u32.to_be(), byte: REQUEST }, index: 0, offset: 0, plen: SUBPIECE_LEN.to_be() 
    };
    let num_subpieces = piece_len/SUBPIECE_LEN as usize;
    let piece_field = Arc::clone(field);
    let q = Arc::clone(queue);
    let hcond = Arc::clone(hash_cond);
    let ccond = Arc::clone(conn_cond);
    let bcon = Arc::clone(break_conns);

    // get pieces
    loop {
        // pick a piece
        let mut piece: Vec<Piece> = vec![];
        let piece_idx;
        { // critical section
            let mut pf = ccond.wait_while(piece_field.lock().unwrap(), 
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
            pf.arr[piece_idx] = (IN_PROGRESS, Some(Arc::downgrade(alive)));
        }

        // all except last piece
        if piece_idx != num_pieces-1 {
            req.index = piece_idx as u32;
            let mut subfield = ByteField { arr: vec![(0, None); num_subpieces] };

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
            { // critical section
                let mut queue = q.lock().unwrap();
                queue.push_back((piece, hashes[piece_idx].to_vec()));
                hcond.notify_one();
            }
        } else {
            let mut piece: Vec<Piece> = vec![];
            // last piece
            let last_remainder: usize = file_len-(num_pieces-1)*piece_len;
            let num_last_subs: usize = last_remainder/SUBPIECE_LEN as usize;
            let mut last_subfield = ByteField { arr: vec![(0, None); num_last_subs] };

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
            req.offset = (num_last_subs as u32)*SUBPIECE_LEN;
            req.plen = last_sub_len as u32;
            let mut final_subfield = ByteField { 
                arr: vec![(0, None); (req.offset/req.plen) as usize + 1] 
            };

            let subp = fetch_subpiece(stream, req.index, req.offset, 
                                                  req.plen, &mut final_subfield);

            if subp.is_none() { return; }
            piece.push(subp.unwrap());

            piece.sort_by_key(|x| x.offset);
            { // critical section
                let mut queue = q.lock().unwrap();
                queue.push_back((piece, hashes[num_pieces-1].to_vec()));
                hcond.notify_one();
            }
        }
    }
}

pub struct FileSize {
    file: Arc<Mutex<File>>,
    len: usize,
}

fn read_piece(index: usize, offset: usize, piece_len: usize, files: &Arc<Vec<FileSize>>) -> Option<Piece> {
    let mut start = (index*piece_len)+offset;
    let mut end = start+SUBPIECE_LEN as usize;
    let mut next_file = 0u64;
    let mut piece_buf: Vec<u8> = vec![];

    for filesize in files.deref() {
        if start > filesize.len && next_file == 0 { 
            start -= filesize.len;
            end -= filesize.len;
            continue;
        }

        if next_file > 0 {
            let mut buf: Vec<u8> = vec![0; next_file as usize];
            { // critical section
                let f = filesize.file.lock().unwrap();
                #[cfg(target_family="windows")] {
                    f.seek_read(&mut buf, 0).ok()?;
                }
                #[cfg(target_family="unix")] {
                    f.read_exact_at(&mut buf, 0).ok()?;
                }
            }
            piece_buf.append(&mut buf);
            break;
        } else {
            if end > filesize.len { next_file = (end-filesize.len) as u64; end = filesize.len; }
            
            piece_buf = vec![0; end-start];
            { // critical section
                let f = filesize.file.lock().unwrap();
                #[cfg(target_family="windows")] {
                    f.seek_read(&mut piece_buf, start as u64).ok()?;
                }
                #[cfg(target_family="unix")] {
                    f.read_exact_at(&mut piece_buf, start as u64).ok()?;
                }
            }
            if next_file == 0 { break }
        }
    }

    let piece = Piece {
        head: Header {
            len: piece_buf.len() as u32+9, byte: PIECE,
        }, index: index as u32, offset: offset as u32,
        data: piece_buf,
    };
    return Some(piece);
}

fn resume_torrent(files: &Arc<Vec<FileSize>>, queue: &Arc<Mutex<VecDeque<(Vec<Piece>, Vec<u8>)>>>,
                  split_hashes: &Arc<Vec<Vec<u8>>>, empty_cond: &Arc<Condvar>, hash_cond: &Arc<Condvar>,
                  num_pieces: usize, piece_len: usize) {

    for i in 0..num_pieces {
        let mut piece = vec![];
        for j in 0..(piece_len/SUBPIECE_LEN as usize) {
            let subp = match read_piece(i, j*SUBPIECE_LEN as usize, piece_len, files) {
                Some(subp) => subp,
                None => continue,
            };
            if subp.data.len() == 0 { continue; }
            piece.push(subp);
        }
        if piece.len() == 0 { continue; }
        {
            let mut q = queue.lock().unwrap();
            q.push_back((piece, split_hashes[i].to_vec()));
            hash_cond.notify_one();
        }
    }
    
    { // wait thread until hashing finishes
        let _guard = empty_cond.wait_while(queue.lock().unwrap(), 
        |q| {
            return !q.is_empty();
        }).unwrap();
    }
}

fn parse_file(info: BTreeMap<Vec<u8>, Item>) -> (Arc<Vec<FileSize>>, usize) {
                  
    match info.get("length".as_bytes()) {
        // single file
        Some(s) => {
            // file length
            let file_len = s.get_int() as usize;
            // name of the file
            let filename = info.get("name".as_bytes()).unwrap().get_str();
            // create file and return
            let path = Path::new(std::str::from_utf8(&filename).unwrap());
            let dest = Arc::new(Mutex::new(
                OpenOptions::new().read(true).write(true).create(true).open(path).unwrap()));
            let file_size = FileSize { file: dest, len: file_len };
            return (Arc::new(vec![file_size]), file_len);
        }
        // multifile
        None => {
            // get parent folder name and file dicts
            let name = info.get("name".as_bytes()).unwrap().get_str();
            let files = info.get("files".as_bytes()).unwrap().get_list();
            let mut ret: Vec<FileSize> = vec![];
            // for each dict
            for f in files {
                let dict = f.get_dict();
                // get length
                let len = dict.get("length".as_bytes()).unwrap().get_int() as usize;
                // parse out path
                let mut path_list = dict.get("path".as_bytes()).unwrap().get_list();
                // end filename
                let end_file = path_list.pop().unwrap().get_str();
                let filename = from_utf8(&end_file).unwrap();
                // parent folders to the filename
                let mut base = "./".to_string() + from_utf8(&name).unwrap();
                for folder in path_list {
                    let folder_name = "/".to_string() + from_utf8(&folder.get_str()).unwrap();
                    base.push_str(&folder_name);
                }
                // create parents and file
                create_dir_all(base.clone()).unwrap();
                let full_path = base+"/"+filename;
                let file_path = Path::new(&full_path);
                let file = Arc::new(Mutex::new(
                        OpenOptions::new().read(true).write(true).create(true).open(file_path).unwrap()));
                ret.push(FileSize { file, len });
            }

            // get total length from each file
            let mut total_len = 0usize;
            for filesize in &ret {
                total_len += filesize.len;
            }

            return (Arc::new(ret), total_len);
        }
    }
}

pub fn tcp_download_pieces(p: &Path) {
    // read and parse torrent file
    let bytes: Vec<u8> = match std::fs::read(p) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{} {:?}", e, p);
            return;
        }
    };
    let mut str: Vec<u8> = bytes.clone();
    let tree: Vec<Item> = parse(&mut str);
    let info_hash = get_info_hash(bytes);
    let addr = get_http_addr(tree.clone()).unwrap();
    
    // get info dict values
    let dict = tree[0].get_dict();
    let info = dict.get("info".as_bytes()).unwrap().get_dict();
    let piece_len = info.get("piece length".as_bytes()).unwrap().get_int() as usize;
    let num_pieces = (info.get("pieces".as_bytes()).unwrap().get_str().len()/20) as usize;
    let hashes = info.get("pieces".as_bytes()).unwrap().get_str();
    
    // make hashing queue
    let split_hashes = Arc::new(split_hashes(hashes));
    let queue: Arc<Mutex<VecDeque<(Vec<Piece>, Vec<u8>)>>> = Arc::new(Mutex::new(VecDeque::new()));
    
    // parse files
    let (files, file_len) = parse_file(info);
    
    // piece field
    let field: Arc<Mutex<ByteField>> = Arc::new(
    Mutex::new(ByteField { arr: vec![(EMPTY, None); num_pieces]}));
    
    // conds and loop breakers
    let hash_cond = Arc::new(Condvar::new()); // signal when queuing new piece
    let conn_cond = Arc::new(Condvar::new()); // signal when piece unreserved
    let empty_cond = Arc::new(Condvar::new()); // signal when hashing queue empty
    let break_conns = Arc::new(AtomicBool::new(false)); // breaks tcp loops
    let break_hash = Arc::new(AtomicBool::new(false)); // breaks hash loops

    
    // spawn hashing threads
    let mut handles = spawn_hash_write(&queue, &field, &files, &conn_cond, &empty_cond, 
                                                         &hash_cond, &break_hash ,piece_len, 24);
        
    // resume any partial pieces
    resume_torrent(&files, &queue, &split_hashes, &empty_cond, &hash_cond, num_pieces, piece_len);

    let mut threads = vec![];
    threads.append(&mut handles);
    
    let mut count: usize = 0;
    const ANNOUNCE_INTERVAL: usize = 60/LOOP_SLEEP as usize;
    const LOOP_SLEEP: u64 = 1;

    loop {
        let mut indices_avail = false;
        let mut progress = 0;
        let field = Arc::clone(&field);
        { // critical section
            // break loop when all pieces complete
            let mut pf = field.lock().unwrap();
            if pf.is_full() { break }
            
            // if thread exited prematurely discard it's indice
            for i in 0..pf.arr.len() {
                if pf.arr[i].0 == IN_PROGRESS {
                    // if alive was dropped
                    if Weak::upgrade(pf.arr[i].1.as_ref().unwrap()).is_none() {
                        // unreserve piece and notify waiting connections
                        pf.arr[i] = (EMPTY, None);
                        conn_cond.notify_one();
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
                let shashes = Arc::clone(&split_hashes);
                let pf = Arc::clone(&field);
                let que = Arc::clone(&queue);
                let hcond = Arc::clone(&hash_cond);
                let ccond = Arc::clone(&conn_cond);
                let bcon = Arc::clone(&break_conns);

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

                            match send_handshake(&mut stream, info_hash, info_hash) {
                                 Some(_) => {}
                                 None => return,
                            }

                            file_getter(&mut stream, piece_len, num_pieces, file_len, 
                                        &shashes, &pf, &alive, &que, &hcond, 
                                        &ccond, &bcon);
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
    
    // break hashing and connection loops
    break_hash.store(true, std::sync::atomic::Ordering::Relaxed);
    {
        let mut q = queue.lock().unwrap();
        q.push_back((vec![], vec![]));
    }
    hash_cond.notify_all();
    break_conns.store(true, std::sync::atomic::Ordering::Relaxed);
    conn_cond.notify_all();
    
    for t in threads {
        match t.join() {
            _ => {}
        }
    }
}