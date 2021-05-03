#![allow(dead_code)]

use std::{fmt, fs::File, io::{Read, Write}, net::TcpStream, 
          sync::{Arc, Mutex}, thread::{self, JoinHandle}, usize, vec};

#[cfg(target_family="windows")]
use std::os::windows::prelude::*;
#[cfg(target_family="unix")]
use std::os::unix::fs::FileExt;

use serde::{Serialize, Deserialize};
use sha1::{Digest, Sha1};

// #[derive(Debug)]
enum Message {
    Handshake(Handshake),
    Choke(Header),
    Unchoke(Header),
    Interest(Header),
    Uninterest(Header),
    Have(Have),
    Bitfield(Bitfield),
    Request(Request),
    Piece(Piece),
    Cancel(Cancel),
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Message::Handshake(_) => write!(f, "Handshake"),
            Message::Choke(_) => write!(f, "Choke"),
            Message::Unchoke(_) => write!(f, "Unchoke"),
            Message::Interest(_) => write!(f, "Interest"),
            Message::Uninterest(_) => write!(f, "Uninterest"),
            Message::Have(_) => write!(f, "Have"),
            Message::Bitfield(_) => write!(f, "Bitfield"),
            Message::Request(_) => write!(f, "Request"),
            Message::Piece(_) => write!(f, "Piece"),
            Message::Cancel(_) => write!(f, "Cancel"),
        }
    }
}

fn parse_handshake(msg: &mut Vec<u8>) -> Option<Handshake> {
    let mut handshake: Handshake = Default::default();
    if msg.len() < 68 { return None; }
    
    handshake.len = msg[0];
    msg.drain(0..1);

    for i in 0..19 {
        handshake.protocol[i] = msg[i];
    } msg.drain(0..19);

    for i in 0..8 {
        handshake.reserved[i] = msg[i];
    } msg.drain(0..8);

    for i in 0..20 {
        handshake.info_hash[i] = msg[i];
    } msg.drain(0..20);

    for i in 0..20 {
        handshake.peer_id[i] = msg[i];
    } msg.drain(0..20);

    if !handshake.test() { return None; }
 
    return Some(handshake);
}

fn parse_u32(msg: &mut Vec<u8>) -> u32 {
    let mut a: [u8; 4] = [0; 4];
    a.copy_from_slice(&msg[0..4]);
    let ret = u32::from_be_bytes(a);
    msg.drain(0..4);
    return ret;
}

fn parse_header(msg: &mut Vec<u8>) -> Option<Header> {
    let mut head: Header = Default::default();
    if msg.len() < 5 { return None; }
    head.len = parse_u32(msg);
    head.byte = msg[0];
    msg.drain(0..1);
    if !head.test() { return None; }
    return Some(head);
}

fn parse_have(msg: &mut Vec<u8>) -> Option<Have> {
    let mut have: Have = Default::default();
    if msg.len() < 9 { return None; }
    have.head = match parse_header(msg) {
        Some(h) => h,
        None => return None,
    };
    have.index = parse_u32(msg);
    if !have.test() { return None; }
    return Some(have);
}

fn parse_bitfield(msg: &mut Vec<u8>) -> Option<Bitfield> {
    let mut bitfield: Bitfield = Default::default();
    bitfield.head = match parse_header(msg) {
        Some(h) => h,
        None => return None,
    };
    if msg.len() < (bitfield.head.len-1) as usize { return None; }
    for i in 0..((bitfield.head.len-1) as usize) {
        bitfield.data.push(msg[i]);
    } msg.drain(0..((bitfield.head.len-1) as usize));
    if !bitfield.test() { return None; }
    return Some(bitfield);
}

fn parse_request(msg: &mut Vec<u8>) -> Option<Request> {
    let mut req: Request = Default::default();
    if msg.len() < 17 { return None; }
    req.head = match parse_header(msg) {
        Some(h) => h,
        None => return None,
    };
    req.index = parse_u32(msg);
    req.offset = parse_u32(msg);
    req.plen = parse_u32(msg);
    if !req.test() { return None; }
    return Some(req);
}

fn parse_piece(msg: &mut Vec<u8>) -> Option<Piece> {
    let mut piece: Piece = Default::default();
    piece.head = match parse_header(msg) {
        Some(h) => h,
        None => return None,
    };
    piece.index = parse_u32(msg);
    piece.offset = parse_u32(msg);
    if msg.len() < (piece.head.len-9) as usize { return None; }
    piece.data.append(&mut msg[0..((piece.head.len-9) as usize)].to_vec());
    let mut copy = msg[((piece.head.len-9) as usize)..msg.len()].to_vec();
    msg.clear();
    msg.append(&mut copy);
    if !piece.test() { return None; }
    return Some(piece);
}

fn parse_cancel(msg: &mut Vec<u8>) -> Option<Cancel> {
    let mut cancel: Cancel = Default::default();
    cancel.head = match parse_header(msg) {
        Some(h) => h,
        None => return None,
    };
    if msg.len() < 12 { return None; }
    cancel.index = parse_u32(msg);
    cancel.offset = parse_u32(msg);
    cancel.plen = parse_u32(msg);
    if !cancel.test() { return None; }
    return Some(cancel);
}

fn is_zero(msg: &Vec<u8>) -> bool {
    for i in msg.iter() {
        if *i != 0 {
            return false;
        }
    } return true;
}

fn parse_msg(msg: &mut Vec<u8>) -> Vec<Message> {
    let mut list: Vec<Message> = vec![];
    loop {
        if is_zero(msg) { break }
        let _byte = match msg.get(0) {
            Some(byte) => *byte,
            None => break,
        };
        let byte = match msg.get(4) {
            Some(byte) => *byte,
            None => break,
        };
        match byte {
            CHOKE => list.push(Message::Choke(parse_header(msg).unwrap())),
            UNCHOKE => list.push(Message::Unchoke(parse_header(msg).unwrap())),
            INTEREST => list.push(Message::Interest(parse_header(msg).unwrap())),
            UNINTEREST => list.push(Message::Uninterest(parse_header(msg).unwrap())),
            HAVE => list.push(Message::Have(parse_have(msg).unwrap())),
            BITFIELD => list.push(Message::Bitfield(parse_bitfield(msg).unwrap())),
            REQUEST => list.push(Message::Request(parse_request(msg).unwrap())),
            PIECE => list.push(Message::Piece(parse_piece(msg).unwrap())),
            CANCEL => list.push(Message::Cancel(parse_cancel(msg).unwrap())),
            HANDSHAKE => list.push(Message::Handshake(parse_handshake(msg).unwrap())),
            _ => {
                // println!("{:?}", msg);
                unreachable!("parse_msg");
            },
        }
    }
    return list;
}

// TODO: change parse fns to return Option<Message>
// return None if parsing invariants fail
// test(), bounds check, etc
// unwrap() in main parser 
// return false on None on try_parse

// returns false too much
// and somehow gets a 5 byte vec causing unreachable
fn try_parse(original: &Vec<u8>) -> bool {
    if original.len() == 0 { return false; }
    let mut msg: Vec<u8> = vec![];
    for i in original {
        msg.push(*i);
    }
    loop {
        if is_zero(&msg) { break }
        let _byte = match msg.get(0) {
            Some(byte) => *byte,
            None => break,
        };
        let byte = match msg.get(4) {
            Some(byte) => *byte,
            None => break,
        };
        match byte {
            CHOKE => if parse_header(&mut msg).is_none() { return false; }
            UNCHOKE => if parse_header(&mut msg).is_none() { return false; }
            INTEREST => if parse_header(&mut msg).is_none() { return false; }
            UNINTEREST => if parse_header(&mut msg).is_none() { return false; }
            HAVE => if parse_have(&mut msg).is_none() { return false; },
            BITFIELD => if parse_bitfield(&mut msg).is_none() { return false; }
            REQUEST => if parse_request(&mut msg).is_none() { return false; }
            PIECE => if parse_piece(&mut msg).is_none() { return false; }
            CANCEL => if parse_cancel(&mut msg).is_none() { return false; }
            HANDSHAKE => if parse_handshake(&mut msg).is_none() { return false; }
            _ => return false,
        }
    }
    return true;
}

const CHOKE: u8 = 0;
const UNCHOKE: u8 = 1;
const INTEREST: u8 = 2;
const UNINTEREST: u8 = 3;
const HAVE: u8 = 4;
const BITFIELD: u8 = 5;
const REQUEST: u8 = 6;
const PIECE: u8 = 7;
const CANCEL: u8 = 8;
const HANDSHAKE: u8 = 0x54;

pub const SUBPIECE_LEN: u32 = 0x4000;

#[derive(Serialize, Deserialize, Debug)]
struct Handshake {
    len: u8,
    protocol: [u8; 19],
    reserved: [u8; 8],
    info_hash: [u8; 20],
    peer_id: [u8; 20],
}

impl Handshake {
    fn test(&self) -> bool {
        if self.len != 19 { return false; }
        self.protocol.iter().zip("BitTorrent protocol".as_bytes()).all(|(a,b)| a == b)
    }
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
struct Header {
    len: u32,
    byte: u8,
}

impl Header {
    fn test(&self) -> bool {
        return (self.byte >= CHOKE && self.byte <= CANCEL) || self.byte == HANDSHAKE;
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct Have {
    head: Header,
    index: u32,
}

impl Have {
    fn test(&self) -> bool {
        if self.head.len != 5 { return false; }
        if self.head.byte != HAVE { return false; }
        return true;
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct Bitfield {
    head: Header,
    data: Vec<u8>,
}

impl Bitfield {
    fn test(&self) -> bool {
        if self.head.byte != BITFIELD { return false; }
        return self.head.len == (self.data.len()+1) as u32;
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct Request {
    head: Header,
    index: u32,
    offset: u32,
    plen: u32,
}

impl Request {
    fn test(&self) -> bool {
        if self.head.byte != REQUEST { return false; }
        return self.head.len == 13;
    }
}
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct Piece {
    head: Header,
    index: u32,
    offset: u32,
    data: Vec<u8>,
}

impl Piece {
    fn test(&self) -> bool {
        if self.head.byte != PIECE { return false; }
        return self.head.len == (self.data.len()+9) as u32;
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct Cancel {
    head: Header,
    index: u32,
    offset: u32,
    plen: u32,
}

impl Cancel {
    fn test(&self) -> bool {
        if self.head.byte != CANCEL { return false; }
        return self.head.len == 13;
    }
}

impl Default for Handshake {
    fn default() -> Handshake {
        let name = "BitTorrent protocol".as_bytes();
        let mut p: [u8; 19] = [0; 19];
        p.copy_from_slice(&name[0..19]);
        Handshake {
            len: 19,
            protocol: p,
            reserved: [0; 8],
            info_hash: [0; 20],
            peer_id: [0; 20],
        }
    }
}

#[derive(Default)]
pub struct ByteField {
    pub arr: Vec<u8>,
}

impl ByteField {
    fn is_full(&self) -> bool {
        let empty_bytes: usize = self.arr.iter().filter(|x| **x == 0).count();
        if empty_bytes == 0 { return true }
        else { return false }
    }

    fn get_empty(&self) -> Option<usize> {
        if self.is_full() { return None }
        for i in 0..(self.arr.len()) {
            if self.arr[i] == 0 {
                return Some(i);
            }
        }
        return None;
    }
}

// add Option return value
pub fn send_handshake(stream: &mut TcpStream, info_hash: [u8; 20], peer_id: [u8; 20]) {
    // make handshake
    let handshake = Handshake { info_hash: info_hash, peer_id: peer_id, ..Default::default() };
    let interest = Header { len: 1u32.to_be(), byte: INTEREST };
    let mut handshake_u8 = bincode::serialize(&handshake).unwrap();

    // send hanshake
    handshake_u8.append(&mut bincode::serialize(&interest).unwrap());
    stream.write_all(&handshake_u8).expect("handshake write error");
    let mut buf: Vec<u8> = vec![0; 8192];
    stream.read(&mut buf).expect("handshake read error");
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
    
    stream.write_all(&req_u8).expect("write error");

    loop {
        let mut msg: Vec<u8> = vec![];
        let mut extbuf: Vec<u8> = vec![];
        loop {
            let mut buf: Vec<u8> = vec![0; 32767];
            let bytes = stream.read(&mut buf).expect("read error");

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
            
            field.arr[(piece.offset.to_le()/plen) as usize] = 1;
            return Some(piece);
        }
    }
}

pub fn hash_write_piece(piece: Vec<Piece>, hash: Vec<u8>, 
                        file: &Arc<Mutex<File>>, piece_len: usize) -> JoinHandle<()> {
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
            panic!("hashes don't match");
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

pub fn file_getter(stream: &mut TcpStream, piece_len: usize, num_pieces: usize, file_len: usize,
                hashes: &Vec<Vec<u8>>, file: &Arc<Mutex<File>>, field: &Arc<Mutex<ByteField>>) {

    let mut threads: Vec<JoinHandle<()>> = vec![];
    // make request and piece bytefield
    let mut req = Request { 
        head: Header { len: 13u32.to_be(), byte: REQUEST }, index: 0, offset: 0, plen: SUBPIECE_LEN.to_be() 
    };
    let num_subpieces = piece_len/SUBPIECE_LEN as usize;

    let piece_field = field.clone();

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
            pf.arr[piece_idx] = 1;
        }
        
        req.index = piece_idx as u32;
        
        let mut subfield: ByteField = Default::default();
        subfield.arr = vec![0; num_subpieces];

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
            hash_write_piece(piece.to_vec(), hashes[piece_idx].to_vec(), file, piece_len));
    }

    let mut piece: Vec<Piece> = vec![];
    // last piece
    let last_remainder: usize = file_len-(num_pieces-1)*piece_len;
    let num_last_subs: usize = last_remainder/SUBPIECE_LEN as usize;
    let mut last_subfield: ByteField = Default::default();
    last_subfield.arr = vec![0; num_last_subs];

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
    final_subfield.arr = vec![0; (req.offset/req.plen) as usize + 1];

    let subp = fetch_subpiece(stream, req.index, req.offset, 
        req.plen, &mut final_subfield);
    if subp.is_none() { return; }
    piece.push(subp.unwrap());
    piece.sort_by_key(|x| x.offset);
    threads.push(
        hash_write_piece(piece.to_vec(), hashes[num_pieces-1].to_vec(), file, piece_len));
    
    for t in threads {
        t.join().unwrap();
    }
}