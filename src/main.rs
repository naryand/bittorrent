// main function
mod bencode;
mod tcp_bt;
mod tracker;
mod field;
mod file;
mod hash;
mod torrent;

use {tcp_bt::peer::download_torrent, torrent::Torrent, bencode::{Item, decode::parse}};

use std::sync::Arc;

fn main() {
    // get arguments
    let args = std::env::args().collect::<Vec<String>>();
    let arg = match args.get(1) {
        Some(s) => s,
        None => {
            eprintln!("no torrent file specified");
            return;
        }
    };

    // read and parse torrent file
    let mut bytes: Vec<u8> = match std::fs::read(arg) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{} {:?}", e, arg);
            return;
        }
    };
    
    // download torrent
    let torrent = Arc::new(Torrent::new(&bytes));
    let tree: Vec<Item> = parse(&mut bytes);
    download_torrent(&torrent, tree);
}