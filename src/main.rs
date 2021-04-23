mod bdecoder;
mod bencoder;
mod udp_tracker;
mod tcp_peer;

use bdecoder::{parse, Item};
use udp_tracker::IpPort;
use std::{io::{Read, Write}, net::{TcpStream, ToSocketAddrs}};

fn parse_ip_port(bytes: Vec<u8>) -> Vec<IpPort> {
    let mut peers: Vec<IpPort> = vec![];
    if bytes.len() % 6 != 0 { return peers; }
    for chunk in bytes.chunks(6) {
        let peer: IpPort = IpPort { 
            // big endian
            ip: u32::from_ne_bytes([chunk[3], chunk[2], chunk[1], chunk[0]]),
            port: u16::from_ne_bytes([chunk[5], chunk[4]]),
        };
        peers.push(peer);
    }
    return peers;
}
fn main() {
    let addr = "anidex.moe:6969".to_socket_addrs().unwrap().nth(0).unwrap();
    let get: Vec<u8> = "GET /announce?info_hash=%12%87%5e%05%08%e8%7f%96%80%b8%aa%2b%a2%c2%bf%09%7c%e3%ba%05&peer_id=-qB4250-rj6kZQu4P_Mh&port=25663&uploaded=0&downloaded=0&left=1456927919&corrupt=0&key=8B26698B&event=started&numwant=200&compact=1&no_peer_id=1&supportcrypto=1&redundant=0 HTTP/1.1\r\n\r\n".as_bytes().to_vec();
    let mut stream = TcpStream::connect(addr).unwrap();

    stream.write_all(&get).unwrap();
    
    let mut buf: Vec<u8> = vec![0; 10000];
    stream.read(&mut buf).unwrap(); 

    buf.drain(0.."HTTP/1.1 200 OK\r\n\r\n".len());

    let tree: Vec<Item> = parse(&mut buf);
    let peers = tree[0].get_dict().get("peers".as_bytes()).unwrap().get_str();
    println!("{:?}", parse_ip_port(peers));
} 