// main function
mod bencode;
mod field;
mod file;
mod hash;
mod tcp_bt;
mod torrent;
mod tracker;

use {
    bencode::{decode::parse, Item},
    tcp_bt::add_torrent,
    torrent::Torrent,
};

use std::sync::Arc;

const LISTENING_PORT: u16 = 37834;

fn main() {
    // get arguments
    let args = std::env::args().collect::<Vec<String>>();
    let arg = if let Some(s) = args.get(1) {
        s
    } else {
        eprintln!("no torrent file specified");
        return;
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
    add_torrent(&torrent, &tree);
}
