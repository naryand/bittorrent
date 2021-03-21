mod bencode;

use bencode::Item;
use bencode::parse;

fn main() {
    let bytes: Vec<u8> = std::fs::read("./src/a.torrent").expect("read error");
    let mut str: Vec<char> = bytes.iter().map(|b| *b as char).collect::<Vec<_>>();
    let tree: Vec<Item> = parse(&mut str);
    println!("{:?}", tree);
}
