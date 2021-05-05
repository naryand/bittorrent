// note: no error handling
#![allow(dead_code)]

// now comment it
//
use crate::udp_tracker::IpPort;
use crate::bdecoder::{parse, Item};

use std::{io::{Read, Write}, net::{TcpStream, ToSocketAddrs, SocketAddr}};

// takes in byte string of ip:port pairs and parses them
fn parse_ip_port(bytes: Vec<u8>) -> Vec<IpPort> {
    let mut peers: Vec<IpPort> = vec![];
    if bytes.len() % 6 != 0 { return peers; }
    for chunk in bytes.chunks(6) { // IpPort is u32 ip, u16 port, 6 bytes
        let peer: IpPort = IpPort { 
            // big endian
            ip: u32::from_ne_bytes([chunk[3], chunk[2], chunk[1], chunk[0]]),
            port: u16::from_ne_bytes([chunk[5], chunk[4]]),
        };
        peers.push(peer);
    }
    return peers;
}

// takes in info_hash and tracker addr, announces and gets peer IpPorts
pub fn http_announce_tracker(addr: SocketAddr, info_hash: [u8; 20]) -> Vec<IpPort> {
    let mut get: Vec<u8> = vec![];
    // prefix
    let mut base: String = "GET /announce?info_hash=".to_string();
    // appending each hex as a string
    for byte in info_hash.iter() {
        base.push_str(&format!("%{:02x}", byte));
    }
    // append suffix of get request
    base.push_str("&peer_id=-qB4250-rj6kZQu4P_Mh&port=25663&uploaded=0&downloaded=0&left=1456927919&corrupt=0&key=8B26698B&event=started&numwant=200&compact=1&no_peer_id=1&supportcrypto=1&redundant=0 HTTP/1.1\r\n\r\n");
    // convert base to Vec<u8> and append to get vector
    get.extend_from_slice(&base.as_bytes());
    // connect to the tracker
    let mut stream = TcpStream::connect(addr).unwrap();
    // send the get request to the tracker
    stream.write_all(&get).unwrap();
    // read it's reply
    let mut buf: Vec<u8> = vec![0; 10000];
    stream.read(&mut buf).unwrap(); 
    // remove http header
    buf.drain(0.."HTTP/1.1 200 OK\r\n\r\n".len());
    // parse out ip port and return
    let tree: Vec<Item> = parse(&mut buf);
    let peers = tree[0].get_dict().get("peers".as_bytes()).unwrap().get_str();
    return parse_ip_port(peers)
}

// gets the first UDP tracker addr from bencoded tree
pub fn get_http_addr(tree: Vec<Item>) -> SocketAddr {
    let dict = tree[0].get_dict();
    let list = dict.get("announce-list".as_bytes()).unwrap().get_list();
    let mut tracker = list[0].get_list()[0].get_str();
    for t in list.iter() {
        tracker = t.get_list()[0].get_str();
        if *tracker.iter().nth(0).unwrap() == ('h' as u8) { break }
    }
    tracker.drain(0.."http://".len());
    tracker.truncate(tracker.len()-"/announce".len());
    return std::str::from_utf8(&tracker).unwrap().to_socket_addrs().unwrap().nth(0).unwrap();
}