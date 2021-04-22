#![allow(dead_code)]

use std::{fmt, fs::File, io::{Read, Write}, net::TcpStream, 
          sync::{Arc, Mutex}, thread::{self, JoinHandle}, usize, vec};

#[cfg(target_family="windows")]
use std::os::windows::prelude::*;
#[cfg(target_family="unix")]
use std::os::unix::fs::FileExt;

use serde::{Serialize, Deserialize};
use sha1::{Sha1, Digest};

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

fn parse_handshake(msg: &mut Vec<u8>) -> Handshake {
    let mut handshake: Handshake = Default::default();

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
 
    return handshake;
}

fn parse_u32(msg: &mut Vec<u8>) -> u32 {
    let mut a: [u8; 4] = [0; 4];
    a.copy_from_slice(&msg[0..4]);
    let ret = u32::from_be_bytes(a);
    msg.drain(0..4);
    return ret;
}

fn parse_header(msg: &mut Vec<u8>) -> Header {
    let mut head: Header = Default::default();
    head.len = parse_u32(msg);
    head.byte = msg[0];
    msg.drain(0..1);
    return head;
}

fn parse_have(msg: &mut Vec<u8>) -> Have {
    let mut have: Have = Default::default();
    have.head = parse_header(msg);
    have.index = parse_u32(msg);
    return have;
}

fn parse_bitfield(msg: &mut Vec<u8>) -> Bitfield {
    let mut bitfield: Bitfield = Default::default();
    bitfield.head = parse_header(msg);
    for i in 0..((bitfield.head.len-1) as usize) {
        bitfield.data.push(msg[i]);
    } msg.drain(0..((bitfield.head.len-1) as usize));
    return bitfield;
}

fn parse_request(msg: &mut Vec<u8>) -> Request {
    let mut req: Request = Default::default();
    req.head = parse_header(msg);
    req.index = parse_u32(msg);
    req.offset = parse_u32(msg);
    req.plen = parse_u32(msg);
    return req;
}

fn parse_piece(msg: &mut Vec<u8>) -> Piece {
    let mut piece: Piece = Default::default();
    piece.head = parse_header(msg);
    piece.index = parse_u32(msg);
    piece.offset = parse_u32(msg);
    piece.data.append(&mut msg[0..((piece.head.len-9) as usize)].to_vec());
    let mut copy = msg[((piece.head.len-9) as usize)..msg.len()].to_vec();
    msg.clear();
    msg.append(&mut copy);
    return piece;
}

fn parse_cancel(msg: &mut Vec<u8>) -> Cancel {
    let mut cancel: Cancel = Default::default();
    cancel.head = parse_header(msg);
    cancel.index = parse_u32(msg);
    cancel.offset = parse_u32(msg);
    cancel.plen = parse_u32(msg);
    return cancel;
}

fn is_zero(msg: &Vec<u8>) -> bool {
    for i in msg.iter() {
        if *i != 0 {
            return false;
        }
    } return true;
}

fn parse_msg(msg: &mut Vec<u8>) -> Vec<Message> {
    let mut list = Vec::<Message>::new();
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
            CHOKE => list.push(Message::Choke(parse_header(msg))),
            UNCHOKE => list.push(Message::Unchoke(parse_header(msg))),
            INTEREST => list.push(Message::Interest(parse_header(msg))),
            UNINTEREST => list.push(Message::Uninterest(parse_header(msg))),
            HAVE => list.push(Message::Have(parse_have(msg))),
            BITFIELD => list.push(Message::Bitfield(parse_bitfield(msg))),
            REQUEST => list.push(Message::Request(parse_request(msg))),
            PIECE => list.push(Message::Piece(parse_piece(msg))),
            CANCEL => list.push(Message::Cancel(parse_cancel(msg))),
            HANDSHAKE => list.push(Message::Handshake(parse_handshake(msg))),
            _ => unreachable!(),
        }
    }
    return list;
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

const SUBPIECE_LEN: u32 = 0x4000;

#[derive(Serialize, Deserialize, Debug)]
struct Handshake {
    len: u8,
    protocol: [u8; 19],
    reserved: [u8; 8],
    info_hash: [u8; 20],
    peer_id: [u8; 20],
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
struct Header {
    len: u32,
    byte: u8,
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct Have {
    head: Header,
    index: u32,
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct Bitfield {
    head: Header,
    data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct Request {
    head: Header,
    index: u32,
    offset: u32,
    plen: u32,
}
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct Piece {
    head: Header,
    index: u32,
    offset: u32,
    data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct Cancel {
    head: Header,
    index: u32,
    offset: u32,
    plen: u32,
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
struct ByteField {
    arr: Vec<u8>,
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
    let mut buf: Vec<u8> = vec![0; (plen+13) as usize];
    
    stream.write_all(&req_u8).expect("write error");
    
    stream.read_exact(&mut buf).expect("read error");
    
    let msg = parse_msg(&mut buf);
    let mut piece: Piece = Default::default();
    piece.data = Vec::new();
    
    for m in msg {
        piece = match m {
            Message::Piece(piece) => piece,
            _ => continue,
        };
        
        if piece.data.len() == 0 { continue }
        
        field.arr[(piece.offset.to_le()/plen) as usize] = 1;
        
        return Some(piece);
    }
    return None;
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

pub fn get_file(stream: &mut TcpStream, piece_len: usize, num_pieces: usize, file_len: usize,
                hashes: Vec<Vec<u8>>, file: &Arc<Mutex<File>>) {

    let mut threads: Vec<JoinHandle<()>> = vec![];
    // make request and piece bytefield
    let mut req = Request { 
        head: Header { len: 13u32.to_be(), byte: REQUEST }, index: 0, offset: 0, plen: SUBPIECE_LEN.to_be() 
    };
    let num_subpieces = piece_len/SUBPIECE_LEN as usize;
    let mut piece_field: ByteField = Default::default();
    piece_field.arr = vec![0; num_pieces-1];

    // get pieces
    // all except last piece
    loop {
        let mut piece: Vec<Piece> = vec![];
        let p = piece_field.get_empty();
        if p == None { break }
        let piece_idx = p.unwrap();
        
        req.index = piece_idx as u32;
        
        let mut subfield: ByteField = Default::default();
        subfield.arr = vec![0; num_subpieces];

        // subpieces
        loop {
            let sub = subfield.get_empty();
            if sub == None { break }
            let sub_idx = sub.unwrap();

            req.offset = (sub_idx as u32)*SUBPIECE_LEN;
            piece.push(fetch_subpiece(stream, req.index, req.offset, 
                      SUBPIECE_LEN, &mut subfield).unwrap());
        }
        piece.sort_by_key(|x| x.offset);
        threads.push(
            hash_write_piece(piece.to_vec(), hashes[piece_idx].to_vec(), file, piece_len));
        piece_field.arr[piece_idx] = 1;
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
        
        piece.push(fetch_subpiece(stream, req.index, req.offset, 
                  SUBPIECE_LEN, &mut last_subfield).unwrap());
    }


    // last subpiece
    let last_sub_len: usize = last_remainder-(num_last_subs*SUBPIECE_LEN as usize);
    let mut final_subfield: ByteField = Default::default();
    
    req.offset = (num_last_subs as u32)*SUBPIECE_LEN;
    req.plen = last_sub_len as u32;
    final_subfield.arr = vec![0; (req.offset/req.plen) as usize + 1];

    piece.push(fetch_subpiece(stream, req.index, req.offset, 
                   req.plen, &mut final_subfield).unwrap());
    piece.sort_by_key(|x| x.offset);
    threads.push(
        hash_write_piece(piece.to_vec(), hashes[num_pieces-1].to_vec(), file, piece_len));
    
    for t in threads {
        t.join().unwrap();
    }
}