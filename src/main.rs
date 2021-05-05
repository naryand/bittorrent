mod bdecoder;
mod bencoder;
mod udp_tracker;
mod tcp_msg;
mod tcp_peer;
mod http_tracker;

use tcp_peer::*;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    tcp_download_pieces(std::path::Path::new(&args[1]));
}