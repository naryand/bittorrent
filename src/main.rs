mod bencode;
mod udp_tracker;
mod tcp_peer;

use bencode::*;
use udp_tracker::*;
use tcp_peer::*;

use std::{net::TcpStream};

fn main() {
    // read and parse torrent file
    let bytes: Vec<u8> = std::fs::read("./a.torrent").expect("read error");
    let mut str: Vec<u8> = bytes.clone();
    let tree: Vec<Item> = parse(&mut str);
    let info_hash = get_info_hash(bytes);
    // let peers = udp_announce_tracker(get_udp_addr(tree), info_hash);

    // get info dict values
    let dict = tree[0].get_dict();
    let info = dict.get("info".as_bytes()).unwrap().get_dict();
    let piece_len = info.get("piece length".as_bytes()).unwrap().get_int() as u64;
    let num_pieces = (info.get("pieces".as_bytes()).unwrap().get_str().len()/20) as u64;
    let file_len = info.get("length".as_bytes()).unwrap().get_int() as u64;
    let hashes = info.get("pieces".as_bytes()).unwrap().get_str();


    // connect and send handshake
    let mut stream = TcpStream::connect("172.20.144.1:25663").expect("connect error");
    send_handshake(&mut stream, info_hash, info_hash);
    
    // make dest file
    let file = std::fs::File::create("/home/naryan/d.mkv").unwrap();
    
    // get subpieces
    let subpieces: Vec<Piece> = get_subpieces(&mut stream, piece_len, num_pieces, file_len);
    
    // check piece hashes
    if !subpieces_check_hash(&subpieces, hashes) { panic!("hashes don't match") }

    // write subpieces
    write_subpieces(subpieces, &file, piece_len);
}