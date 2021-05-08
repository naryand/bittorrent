mod bdecoder;
mod bencoder;
mod udp_tracker;
mod tcp_msg;
mod tcp_peer;
mod http_tracker;

use tcp_peer::*;

fn main() {
    let args = std::env::args().collect::<Vec<String>>();
    let arg = match args.get(1) {
        Some(s) => s,
        None => {
            eprintln!("no torrent file specified");
            return;
        }
    };
    tcp_download_pieces(std::path::Path::new(arg));
}