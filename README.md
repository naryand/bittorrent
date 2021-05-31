## BitTorrent
[![Rust](https://github.com/naryand/bittorrent/actions/workflows/rust.yml/badge.svg)](https://github.com/naryand/bittorrent/actions/workflows/rust.yml)
---

Client for the peer to peer BitTorrent protocol from scratch in Rust.

## Features

- Bencode encoding + decoding
- Parsing `.torrent` files for their metadata
- Discovering peers with HTTP and UDP tracker protocols
- TCP peer wire message parsing
- Concurrently downloading pieces from many peers
- Multithreaded SHA1 hash checking for verifying pieces
- Downloading single and multi-file torrents
- Resuming partially complete torrents
- Seeding requested pieces
- Pipelining piece requests for higher throughput

### To do
- Asynchronous IO on a multithreaded runtime
- DHT, PEX, NAT traversal for more peers
- Rarest first/Choking/Super seeding algorithms
- Graphical/Web interface
- uTorrent transport protocol
- Piece paging/caching

## Usage

To run, clone the repo and run
```
cargo run --release [torrent]
```
where `[torrent]` is the path to the .torrent file. The client will proceed to download the torrent into the working directory. 

Progress is given in completed pieces out of the total.