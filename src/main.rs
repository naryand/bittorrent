#![allow(dead_code)]
mod bencode;
mod udp;

use std::{io::{Read, Write}, net::TcpStream, os::unix::prelude::FileExt};

use bencode::Item;
use bencode::parse;
use udp::*;

use serde::{Serialize, Deserialize};
#[derive(Debug)]
pub enum Message {
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
    } msg.drain(0..(bitfield.head.len as usize)-1);
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
    for i in 0..((piece.head.len-9) as usize) {
        piece.data.push(msg[i]);
    } msg.drain(0..(piece.head.len as usize)-1);
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

pub fn parse_msg(msg: &mut Vec<u8>) -> Vec<Message> {
    let mut list = Vec::<Message>::new();
    loop {
        if is_zero(msg) { break }
        let _byte = match msg.get(0) {
            Some(byte) => *byte,
            None => break,
        };
        let byte = match msg.get(4) {
            Some(byte) => *byte,
            None => continue,
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
            _ => list.push(Message::Handshake(parse_handshake(msg))),
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

#[derive(Serialize, Deserialize, Debug)]
pub struct Handshake {
    len: u8,
    protocol: [u8; 19],
    reserved: [u8; 8],
    info_hash: [u8; 20],
    peer_id: [u8; 20],
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Header {
    len: u32,
    byte: u8,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Have {
    head: Header,
    index: u32,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Bitfield {
    head: Header,
    data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Request {
    head: Header,
    index: u32,
    offset: u32,
    plen: u32,
}
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Piece {
    head: Header,
    index: u32,
    offset: u32,
    data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Cancel {
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

fn main() {
    let bytes: Vec<u8> = std::fs::read("./b.torrent").expect("read error");
    let mut str: Vec<char> = bytes.iter().map(|b| *b as char).collect::<Vec<_>>();
    let tree: Vec<Item> = parse(&mut str);
    let info_hash = get_info_hash(bytes);
    // let _peers = udp_announce_tracker(get_udp_addr(tree), info_hash);

    let dict = tree[0].get_dict();
    let info = dict.get("info".as_bytes()).unwrap().get_dict();
    let piece_len = info.get("piece length".as_bytes()).unwrap().get_int() as u64;
    let num_pieces = (info.get("pieces".as_bytes()).unwrap().get_str().len()/20) as u64;

    let handshake = Handshake { 
        info_hash: info_hash, peer_id: info_hash, ..Default::default() 
    };
    let handshake_u8 = bincode::serialize(&handshake).unwrap();

    let interest = Header { len: 1u32.to_be(), byte: INTEREST };
    let interest_u8 = bincode::serialize(&interest).unwrap();

    let mut req = Request { 
        head: Header { len: 13u32.to_be(), byte: REQUEST }, index: 0, offset: 0, plen: 0x4000u32.to_be() 
    };
    
    
    let mut stream = TcpStream::connect("127.0.0.1:25663").expect("connect error");
    stream.write_all(&handshake_u8).expect("write error");
    stream.write_all(&interest_u8).expect("write error");
    
    let file = std::fs::File::create("/home/naryan/Desktop/b.mkv").unwrap();

    for i in 0..num_pieces {
        for j in 0..(piece_len>>14) {
            let mut buf: Vec<u8> = vec![0; 32768];
            let req_u8 = bincode::serialize(&req).unwrap();
            stream.write_all(&req_u8).expect("write error");
            stream.read(&mut buf).expect("read error");
            
            let msg = parse_msg(&mut buf);
            let mut piece: Piece = Default::default();
            for m in msg {
                piece = match m {
                    Message::Piece(piece) => piece,
                    _ => continue,
                };
            }
            file.write_all_at(
                &piece.data, i*piece_len+(0x4000*j) as u64).expect("file write error");
            std::thread::sleep(std::time::Duration::from_micros(1));

            req.offset = (0x4000*j as u32).to_be();
        }
        req.index = (i as u32).to_be();
        req.offset = 0;
    }
}
