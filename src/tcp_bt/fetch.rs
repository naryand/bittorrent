// tcp peer wire piece downloading
#![allow(dead_code)]

use super::{
    msg::{bytes::*, structs::*, SUBPIECE_LEN},
    parse::Parser,
    Connector,
};
use crate::{
    field::{constant::*, ByteField},
    tcp_bt::parse::ParseItem,
    torrent::Torrent,
};

use std::{
    io::{ErrorKind, Read, Write},
    net::TcpStream,
    sync::{atomic::Ordering, mpsc, Arc, Mutex},
    usize, vec,
};

fn request_piece(stream: &mut TcpStream, torrent: &Arc<Torrent>, index: u32) -> Option<usize> {
    let mut request = Request {
        head: Header {
            len: 13_u32.to_be(),
            byte: REQUEST,
        },
        index: index.to_be(),
        plen: SUBPIECE_LEN.to_be(),
        ..Request::default()
    };

    let remainder;
    if index as usize == torrent.num_pieces - 1 {
        remainder = torrent.file_len % torrent.piece_len;
    } else {
        remainder = torrent.piece_len;
    }

    let mut num_subpieces = remainder / SUBPIECE_LEN as usize;
    for i in 0..num_subpieces {
        request.offset = u32::to_be((i as u32) * SUBPIECE_LEN);
        let req_u8 = bincode::serialize(&request).unwrap();
        stream.write_all(&req_u8).ok()?;
    }

    if remainder % SUBPIECE_LEN as usize > 0 {
        let last_plen = remainder % SUBPIECE_LEN as usize;
        request.plen = u32::to_be(last_plen as u32);
        request.offset = u32::to_be(num_subpieces as u32 * SUBPIECE_LEN);
        let req_u8 = bincode::serialize(&request).unwrap();
        stream.write_all(&req_u8).ok()?;
        num_subpieces += 1;
    }
    Some(num_subpieces)
}

fn read_piece(
    stream: &mut TcpStream,
    parser: &Arc<Parser>,
    connector: &Arc<Connector>,
    num_subpieces: usize,
) -> Option<()> {
    let subfield = ByteField {
        arr: vec![EMPTY; num_subpieces],
    };

    let am_subfield = Arc::new(Mutex::new(subfield));
    let (tx, rx) = mpsc::channel();
    let item = ParseItem {
        rx,
        stream: Arc::new(Mutex::new(stream.try_clone().unwrap())),
        field: Some(Arc::clone(&am_subfield)),
    };
    {
        let mut q = parser.queue.lock().unwrap();
        q.push_back(item);
        parser.loops.notify_one();
    }
    let mut buf = vec![0; 65536];
    loop {
        let bytes;
        loop {
            match stream.read(&mut buf) {
                Ok(b) => {
                    bytes = b;
                    break;
                }
                Err(e) => {
                    if e.kind() == ErrorKind::WouldBlock {
                        if connector.brk.load(Ordering::Relaxed) {
                            return None;
                        }
                        {
                            let subf = am_subfield.lock().unwrap();
                            if subf.is_full() {
                                drop(tx);
                                return Some(());
                            }
                        }
                        std::thread::sleep(std::time::Duration::from_micros(500));
                    } else {
                        return None;
                    }
                }
            }
        }
        if bytes == 0 {
            return None;
        }

        tx.send(buf[..bytes].to_vec()).unwrap();
    }
}

// represents a single connection to a peer, continously fetches subpieces
pub fn torrent_fetcher(
    stream: &mut TcpStream,
    parser: &Arc<Parser>,
    torrent: &Arc<Torrent>,
    field: &Arc<Mutex<ByteField>>,
    connector: &Arc<Connector>,
) -> Vec<usize> {
    let mut idxs = vec![];
    // get pieces
    loop {
        let mut nums = vec![];
        // peers reject if you request more than 1 piece
        for _ in 0..1 {
            // pick a piece
            let piece_idx;
            {
                // critical section
                let mut pf = connector
                    .piece
                    .wait_while(field.lock().unwrap(), |f| {
                        if connector.brk.load(Ordering::Relaxed) {
                            return false;
                        }
                        if f.is_full() {
                            return false;
                        }
                        f.get_empty().is_none()
                    })
                    .unwrap();
                if let Some(p) = pf.get_empty() {
                    piece_idx = p
                } else {
                    return idxs;
                }
                pf.arr[piece_idx] = IN_PROGRESS;
            }
            idxs.push(piece_idx);

            // fetch piece
            let num = request_piece(stream, torrent, piece_idx as u32);

            if let Some(n) = num {
                nums.push(n)
            } else {
                return idxs;
            }
        }
        for num_subpieces in nums {
            if read_piece(stream, parser, connector, num_subpieces).is_none() {
                return idxs;
            }
        }
    }
}
