mod bdecoder;
mod bencoder;
mod udp_tracker;
mod tcp_peer;
mod http_tracker;

use http_tracker::http_announce_tracker;

use std::net::ToSocketAddrs;

fn main() {
    let addr = "anidex.moe:6969".to_socket_addrs().unwrap().nth(0).unwrap();
    let info_hash = [0x12,0x87,0x5e,0x05,08,0xe8,0x7f,0x96,0x80,0xb8,0xaa,0x2b,0xa2,0xc2,0xbf,0x09,0x7c,0xe3,0xba,0x05];
    println!("{:?}", http_announce_tracker(addr, info_hash))
}