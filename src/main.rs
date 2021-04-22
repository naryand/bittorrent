mod bdecoder;
mod bencoder;
mod udp_tracker;
mod tcp_peer;

use bdecoder::*;
use udp_tracker::*;
use tcp_peer::*;

use std::{net::{TcpStream, Ipv4Addr}, vec, sync::{Arc, Mutex}, fs::File};

fn main() {
    // read and parse torrent file
    let bytes: Vec<u8> = std::fs::read("./a.torrent").expect("read error");
    let mut str: Vec<u8> = bytes.clone();
    let tree: Vec<Item> = parse(&mut str);
    let info_hash = get_info_hash(bytes);
    let peers = udp_announce_tracker(get_udp_addr(tree.clone()), info_hash);

    // get info dict values
    let dict = tree[0].get_dict();
    let info = dict.get("info".as_bytes()).unwrap().get_dict();
    let piece_len = info.get("piece length".as_bytes()).unwrap().get_int() as usize;
    let num_pieces = (info.get("pieces".as_bytes()).unwrap().get_str().len()/20) as usize;
    let file_len = info.get("length".as_bytes()).unwrap().get_int() as usize;
    let hashes = info.get("pieces".as_bytes()).unwrap().get_str();
    let split_hashes = Arc::new(split_hashes(hashes));

    let dest = Arc::new(Mutex::new(File::create("/home/naryan/b.mkv").unwrap()));
    let field: Arc<Mutex<ByteField>> = Arc::new(Mutex::new(ByteField { arr: vec![0; num_pieces]}));

    let mut threads = vec![];

    for peer in peers {
        let addr = (Ipv4Addr::from(peer.ip), peer.port);
        let shashes = split_hashes.clone();
        let file = dest.clone();
        let pf = field.clone();

        let builder = std::thread::Builder::new().name(format!("{:?}", addr.0));
        let handle = builder.spawn(move || {
            let stream = TcpStream::connect(addr);

            match stream {
                Ok(mut stream) => {
                    println!("connected! {:?}", stream);
                    send_handshake(&mut stream, info_hash, info_hash);

                    file_getter(&mut stream, piece_len, num_pieces, file_len, &shashes, &file, &pf);
                    return;
                }
                Err(_) => return,
            }
        }).unwrap();
        threads.push(handle);
    }

    for t in threads {
        t.join().unwrap();
    }
}