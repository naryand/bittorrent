use crate::bencode::Item;

use std::{fmt, usize};
use std::net::UdpSocket;
use std::net::{SocketAddr, ToSocketAddrs};

use rand::random;
use sha1::{Sha1, Digest};
use serde::{Serialize, Deserialize};

const MAGIC: u64 = 0x41727101980;
const PEERS: usize = 32;

#[derive(Serialize, Deserialize, Debug)]
struct ConnectReq {
    protocol_id: u64,
    action: u32,
    transaction_id: u32,
}

#[derive(Serialize, Deserialize, Debug)]
struct ConnectResp {
    action: u32,
    transaction_id: u32,
    connection_id: u64,
}

#[derive(Serialize, Deserialize, Debug)]
struct AnnounceReq {
    connection_id: u64,
    action: u32,
    transaction_id: u32,
    info_hash: [u8; 20],
    peer_id: [u8; 20],
    downloaded: u64,
    left: u64,
    uploaded: u64,
    event: u32,
    ip_address: u32,
    key: u32,
    num_want: u32,
    port: u16,
}

#[derive(Serialize, Deserialize, Copy, Clone)]
pub struct IpPort {
    ip: u32,
    port: u16,
}

#[derive(Serialize, Deserialize, Debug)]
struct AnnounceResp {
    action: u32,
    transaction_id: u32,
    interval: u32,
    leechers: u32,
    seeders: u32,
}
#[derive(Serialize, Deserialize, Debug)]
struct AnnounceRespExt {
    resp: AnnounceResp,
    ip_port: [IpPort; PEERS],
}

impl AnnounceRespExt {
    fn from_be(&mut self) -> &mut AnnounceRespExt {
        self.resp.action = u32::from_be(self.resp.action);
        self.resp.transaction_id = u32::from_be(self.resp.transaction_id);
        self.resp.interval = u32::from_be(self.resp.interval);
        self.resp.leechers = u32::from_be(self.resp.leechers);
        self.resp.seeders = u32::from_be(self.resp.seeders);
        for i in 0..PEERS {
            self.ip_port[i].ip = u32::from_be(self.ip_port[i].ip);
            self.ip_port[i].port = u16::from_be(self.ip_port[i].port);
        }
        return self
    }
}

impl fmt::Debug for IpPort {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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

// gets the first UDP tracker addr from bencoded tree
pub fn get_udp_addr(tree: Vec<Item>) -> SocketAddr {
    let dict = tree[0].get_dict();
    let list = dict.get("announce-list").unwrap().get_list();
    let mut tracker = list[0].get_list()[0].get_str();
    for t in list.iter() {
        tracker = t.get_list()[0].get_str();
        if tracker.chars().nth(0) == Some('u') { break }
    }
    tracker.drain(0.."udp://".len());
    tracker.truncate(tracker.len()-"/announce".len());
    return tracker.to_socket_addrs().unwrap().nth(0).unwrap();
}

pub fn udp_announce_tracker(addr: SocketAddr, info_hash: [u8; 20]) -> Vec<IpPort> {
    // set up udp socket
    let socket = UdpSocket::bind("0.0.0.0:25565").expect("bind error");
    socket.set_read_timeout(
        Some(std::time::Duration::new(5, 0))).expect("timeout set error");

    // init structs and serialize
    let req = ConnectReq { 
        protocol_id: u64::to_be(MAGIC), action: 0, transaction_id: random::<u32>() 
    };
    let mut resp = ConnectResp { action: 0, transaction_id: 0, connection_id: 0 }; 
    let mut req_u8: Vec<u8> = bincode::serialize(&req).unwrap();
    let mut resp_u8: Vec<u8> = bincode::serialize(&resp).unwrap();

    // send connection request and get response
    socket.send_to(&req_u8, addr).expect("send error");
    socket.recv_from(&mut resp_u8).expect("recv error");

    // deserialize struct and check tx id
    resp = bincode::deserialize(&resp_u8).unwrap();

    // init structs and serialize
    let announce_req = AnnounceReq { 
        connection_id: resp.connection_id, action: u32::to_be(1), 
        transaction_id: random::<u32>(), info_hash: info_hash, 
        peer_id: [1; 20], downloaded: 0, left: 0, uploaded: 0,
        event: 0, ip_address: u32::to_be(1179085955), key: 0, 
        num_want: u32::to_be(PEERS as u32), port: u16::to_be(25565),
    };                                    
    let mut announce_resp = AnnounceRespExt { 
        resp: AnnounceResp { 
            action: 0, transaction_id: 0, interval: 0, leechers: 0, seeders: 0, 
        }, 
        ip_port: [IpPort{ip: 0, port: 0}; PEERS] 
    };
    req_u8 = bincode::serialize(&announce_req).unwrap();
    resp_u8 = bincode::serialize(&announce_resp.resp).unwrap();
    for _i in 0..(std::mem::size_of::<IpPort>()*PEERS) {
        resp_u8.push(0);
    }

    // send announce request and get response
    socket.send_to(&req_u8, addr).expect("send error");
    socket.recv_from(&mut resp_u8).expect("recv error");

    // deserialize and return peers
    announce_resp = bincode::deserialize(&resp_u8).unwrap();
    return announce_resp.from_be().ip_port.to_vec();
}