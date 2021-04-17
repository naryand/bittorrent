mod bencode;
mod udp_tracker;
mod tcp_peer;

use bencode::*;
use udp_tracker::*;
use tcp_peer::*;

use std::{net::TcpStream, sync::{Arc, Mutex}, fs::File};

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
    let piece_len = info.get("piece length".as_bytes()).unwrap().get_int() as usize;
    let num_pieces = (info.get("pieces".as_bytes()).unwrap().get_str().len()/20) as usize;
    let file_len = info.get("length".as_bytes()).unwrap().get_int() as usize;
    let hashes = info.get("pieces".as_bytes()).unwrap().get_str();
    let split_hashes = split_hashes(hashes);


    // connect and send handshake
    let mut stream = TcpStream::connect("172.21.0.1:25663").expect("connect error");
    send_handshake(&mut stream, info_hash, info_hash);
    
    // make dest file
    let file = Arc::new(Mutex::new(File::create("/home/naryan/b.mkv").unwrap()));

    // get file
    get_file(&mut stream, piece_len, num_pieces, file_len, split_hashes, &file);
}