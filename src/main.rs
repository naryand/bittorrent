mod udp;
mod bencode;

use udp::*;
use bencode::parse;

fn main() {
    let bytes= std::fs::read("./a.torrent").expect("read error");
    let mut str= bytes.iter().map(|b| *b as char).collect::<Vec<_>>();
    let tree = parse(&mut str);

    let peers = udp_announce_tracker(get_udp_addr(tree), get_info_hash(bytes));
    println!("{:?}", peers);
}