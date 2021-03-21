// mod bencode;


// use bencode::Item;
// use bencode::parse;
use std::fmt;
use std::net::UdpSocket;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use rand::random;
use serde::{Serialize, Deserialize};

const MAGIC: i64 = 0x41727101980;
const PEERS: usize = 5;

#[derive(Serialize, Deserialize, Debug)]
struct ConnectReq {
    protocol_id: i64,
    action: i32,
    transaction_id: i32,
}

#[derive(Serialize, Deserialize, Debug)]
struct ConnectResp {
    action: i32,
    transaction_id: i32,
    connection_id: i64,
}

#[derive(Serialize, Deserialize, Debug)]
struct AnnounceReq {
    connection_id: i64,
    action: i32,
    transaction_id: i32,
    info_hash: [u8; 20],
    peer_id: [i8; 20],
    downloaded: i64,
    left: i64,
    uploaded: i64,
    event: i32,
    ip_address: u32,
    key: u32,
    num_want: i32,
    port: u16,
}

#[derive(Serialize, Deserialize, Copy, Clone)]
struct IpPort {
    ip_address: i32,
    tcp_port: u16,
}

#[derive(Serialize, Deserialize, Debug)]
struct AnnounceResp {
    action: i32,
    transaction_id: i32,
    interval: i32,
    leechers: i32,
    seeders: i32,
    ip_port: [IpPort; PEERS],
    ip_port1: [IpPort; PEERS],
    ip_port2: [IpPort; PEERS],
    ip_port3: [IpPort; PEERS],
    ip_port4: [IpPort; PEERS],
}

impl AnnounceResp {
    fn from_be(&mut self) -> &mut AnnounceResp {
        self.action = i32::from_be(self.action);
        self.transaction_id = i32::from_be(self.transaction_id);
        self.interval = i32::from_be(self.interval);
        self.leechers = i32::from_be(self.leechers);
        self.seeders = i32::from_be(self.seeders);
        for i in 0..PEERS {
            self.ip_port[i].ip_address = i32::from_be(self.ip_port[i].ip_address);
            self.ip_port[i].tcp_port = u16::from_be(self.ip_port[i].tcp_port);
        }
        return self;
    }
}

impl fmt::Debug for IpPort {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let one: u64 = (self.ip_address as u64 & 0xff000000u64) >> 24; 
        let two = (self.ip_address & 0xff0000) >> 16; 
        let three = (self.ip_address & 0xff00) >> 8; 
        let four = (self.ip_address) & 0xff; 
        write!(f, "IpPort {{ ip_address: {}.{}.{}.{}, tcp_port: {} }}", 
               one, two, three, four, self.tcp_port)
    }
}

fn main() {
    // let bytes: Vec<u8> = std::fs::read("./a.torrent").expect("read error");
    // let mut str: Vec<char> = bytes.iter().map(|b| *b as char).collect::<Vec<_>>();
    // let tree: Vec<Item> = parse(&mut str);
    // println!("{:?}", tree);

    let socket = UdpSocket::bind("0.0.0.0:25565").expect("bind error");
    socket.set_read_timeout(Some(std::time::Duration::new(5, 0))).expect("timeout set error");

    let req = ConnectReq { protocol_id: i64::to_be(MAGIC), action: 0, transaction_id: random::<i32>() };
    let mut resp = ConnectResp { action: 0, transaction_id: 0, connection_id: 0 };
    

    let mut b: Vec<u8> = bincode::serialize(&req).unwrap();
    let mut c: Vec<u8> = bincode::serialize(&resp).unwrap();

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(185,181,60,67)), 80);
    socket.send_to(&b, addr).expect("send error");
    socket.recv_from(&mut c).expect("recv error");

    resp = bincode::deserialize(&c).unwrap();



    let hash: [u8; 20] = [0x7e,0x1f,0x50,0xa4,0x78,0x01,0x8b,0xe8,0xff,0x4d,
                          0xf5,0xc1,0x68,0x1f,0xa1,0x85,0x32,0xc8,0x50,0xf3];
    let announce_req = AnnounceReq { connection_id: resp.connection_id, action: i32::to_be(1), 
                                     transaction_id: random::<i32>(), info_hash: hash, 
                                     peer_id: [0; 20], downloaded: 0, left: 0, uploaded: 0,
                                     event: 0, ip_address: 0, key: 0, num_want: PEERS as i32, port: 0};
                                     
    let mut announce_resp = AnnounceResp {action: 0, transaction_id: 0, interval: 0, leechers: 0,
                                          seeders: 0, ip_port: [IpPort{ip_address: 0, tcp_port: 0}; PEERS],
                                          ip_port1: [IpPort{ip_address: 0, tcp_port: 0}; PEERS],
                                          ip_port2: [IpPort{ip_address: 0, tcp_port: 0}; PEERS],
                                          ip_port3: [IpPort{ip_address: 0, tcp_port: 0}; PEERS],
                                          ip_port4: [IpPort{ip_address: 0, tcp_port: 0}; PEERS],};

    b = bincode::serialize(&announce_req).unwrap();
    c = bincode::serialize(&announce_resp).unwrap();

    socket.send_to(&b, addr).expect("send error");
    socket.recv_from(&mut c).expect("recv error");

    announce_resp = bincode::deserialize(&c).unwrap();

    println!("{:?}", announce_resp.from_be());
}