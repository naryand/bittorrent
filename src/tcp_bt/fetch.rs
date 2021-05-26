// tcp peer wire piece downloading
#![allow(dead_code)]

use super::{
    msg::{bytes::*, parse_msg, structs::*, try_parse, Message, SUBPIECE_LEN},
    Connector,
};
use crate::{
    field::{constant::*, ByteField},
    hash::Hasher,
    tcp_bt::seed::fulfill_req,
    torrent::Torrent,
};

use std::{
    io::{ErrorKind, Read, Write},
    net::TcpStream,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
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
    torrent: &Arc<Torrent>,
    piece_field: &Arc<Mutex<ByteField>>,
    connector: &Arc<Connector>,
    count: &Arc<AtomicU32>,
    num_subpieces: usize,
) -> Option<Vec<Piece>> {
    let mut pieces = vec![];
    let mut subfield = ByteField {
        arr: vec![EMPTY; num_subpieces],
    };

    while !subfield.is_full() {
        let mut msg: Vec<u8> = vec![];
        let mut extbuf: Vec<u8> = vec![];
        loop {
            let mut buf: Vec<u8> = vec![0; 32767];
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
                            std::thread::sleep(std::time::Duration::from_micros(1));
                        } else {
                            return None;
                        }
                    }
                }
            }
            if bytes == 0 {
                return None;
            }

            buf.truncate(bytes);
            extbuf.extend_from_slice(&buf);

            if try_parse(&extbuf) {
                msg.extend_from_slice(&extbuf);
                break;
            }
        }
        let parsed = parse_msg(&mut msg);
        for m in parsed {
            let piece = match m {
                Message::Piece(piece) => piece,
                Message::Request(req) => {
                    match fulfill_req(stream, torrent, piece_field, count, &req) {
                        Some(_) => continue,
                        None => return None,
                    }
                }
                _ => continue,
            };
            subfield.arr[(piece.offset / SUBPIECE_LEN) as usize] = COMPLETE;
            pieces.push(piece);
        }
    }

    Some(pieces)
}

// represents a single connection to a peer, continously fetches subpieces
pub fn torrent_fetcher(
    stream: &mut TcpStream,
    hasher: &Arc<Hasher>,
    torrent: &Arc<Torrent>,
    field: &Arc<Mutex<ByteField>>,
    connector: &Arc<Connector>,
    count: &Arc<AtomicU32>,
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
            let p = read_piece(stream, torrent, field, connector, count, num_subpieces);
            let mut piece;
            if let Some(p) = p {
                piece = p
            } else {
                return idxs;
            }

            // push piece to hash+writer queue
            piece.sort_by_key(|x| x.offset);
            {
                // critical section
                let mut q = hasher.queue.lock().unwrap();
                q.push_back(piece);
                hasher.loops.notify_one();
            }
        }
    }
}
