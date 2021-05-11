// tcp_bt subfolder and tcp peer wire handshaking
#![allow(dead_code)]

pub mod msg;
pub mod peer;

use crate::{tcp_bt::msg::{structs::*, bytes::*}};

use std::{io::{Read, Write}, net::TcpStream};

pub fn send_handshake(stream: &mut TcpStream, info_hash: [u8; 20], peer_id: [u8; 20]) -> Option<()> {
    // make handshake
    let handshake = Handshake { info_hash: info_hash, peer_id: peer_id, ..Default::default() };
    let interest = Header { len: 1u32.to_be(), byte: INTEREST };
    let mut handshake_u8 = bincode::serialize(&handshake).unwrap();

    // send handshake
    handshake_u8.append(&mut bincode::serialize(&interest).unwrap());
    stream.write_all(&handshake_u8).expect("handshake write error");

    // receive handhake
    let mut buf: Vec<u8> = vec![0; 8192];
    stream.read(&mut buf).ok()?;
    return Some(());
}