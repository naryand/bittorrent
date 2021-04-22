mod bdecoder;
mod bencoder;
mod udp_tracker;
mod tcp_peer;

use {bencoder::encode, bdecoder::Item};

use std::{str::{from_utf8}};

fn main() {
    let tree = vec![Item::List( vec![Item::Int(42),  Item::List(vec![Item::Int(43)])  ] )];
    println!("{}", from_utf8(&encode(tree)).unwrap());
}