// main function
mod bencode;
mod field;
mod file;
mod hash;
mod tcp_bt;
mod torrent;
mod tracker;

use torrent::Torrent;

#[tokio::main]
async fn main() {
    // get arguments
    let args = std::env::args().collect::<Vec<String>>();
    let arg = if let Some(s) = args.get(1) {
        s
    } else {
        eprintln!("no torrent file specified");
        return;
    };

    // read and parse torrent file
    let bytes: Vec<u8> = match tokio::fs::read(arg).await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{} {:?}", e, arg);
            return;
        }
    };

    // download torrent
    let torrent = Torrent::new(&bytes).await;
    torrent.start().await;
}
