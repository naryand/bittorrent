// tracker subfolder and IpPort implementation
#![allow(dead_code)]

pub mod http;
pub mod udp;

use sha1::{Sha1, Digest};
use serde::{Serialize, Deserialize};
#[derive(Serialize, Deserialize, Copy, Clone)]
pub struct IpPort {
    pub ip: u32,
    pub port: u16,
}

impl IpPort {
    // takes in byte string of ip:port pairs and parses them
    fn from_bytes(bytes: Vec<u8>) -> Vec<Self> {
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
}

impl std::fmt::Debug for IpPort {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let one: u64 = (self.ip as u64 & (0xff<<24)) >> 24;
        let two = (self.ip & (0xff<<16)) >> 16; 
        let three = (self.ip & (0xff<<8)) >> 8; 
        let four = (self.ip) & 0xff; 
        write!(f, "[ip: {}.{}.{}.{}, port: {}]", 
               one, two, three, four, self.port)
    }
}

// computes info_hash from .torrent bytes
pub fn get_info_hash(mut bytes: Vec<u8>) -> [u8; 20] {
    let mut len: usize = 0;
    for c in bytes.windows(7) {
        len += 1;
        if c.eq("4:infod".as_bytes()) {
            break
        }
    }
    bytes.drain(0..len+5);
    bytes.pop();
    
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    return hasher.finalize().into();
}