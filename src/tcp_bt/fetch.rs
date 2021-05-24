// tcp peer wire piece downloading
#![allow(dead_code)]

use super::{
    msg::{bytes::*, parse_msg, structs::*, try_parse, Message, SUBPIECE_LEN},
    seed::Connector,
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

// fetches a single subpiece
fn fetch_subpiece(
    stream: &mut TcpStream,
    torrent: &Arc<Torrent>,
    piece_field: &Arc<Mutex<ByteField>>,
    connector: &Arc<Connector>,
    count: &Arc<AtomicU32>,
    field: &mut ByteField,
    req: &Request,
) -> Option<Piece> {
    let mut request = Request {
        head: Header {
            len: 13_u32.to_be(),
            byte: REQUEST,
        },
        ..Request::default()
    };

    request.index = req.index.to_be();
    request.offset = req.offset.to_be();
    request.plen = req.plen.to_be();

    let req_u8 = bincode::serialize(&request).unwrap();

    stream.write_all(&req_u8).ok()?;
    loop {
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

            buf.truncate(bytes);
            if bytes == 0 {
                return None;
            }

            extbuf.extend_from_slice(&buf);

            if try_parse(&extbuf) {
                msg.extend_from_slice(&extbuf);
                break;
            }
        }

        let parsed = parse_msg(&mut msg);
        let mut piece;

        for m in parsed {
            piece = match m {
                Message::Piece(piece) => piece,
                Message::Request(req) => {
                    match fulfill_req(stream, torrent, piece_field, count, &req) {
                        Some(_) => continue,
                        None => return None,
                    }
                }
                _ => continue,
            };

            if piece.data.is_empty() {
                continue;
            }

            field.arr[(piece.offset / req.plen) as usize] = COMPLETE;
            return Some(piece);
        }
    }
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
    // make request and piece bytefield
    let mut req = Request {
        head: Header {
            len: 13_u32.to_be(),
            byte: REQUEST,
        },
        index: 0,
        offset: 0,
        plen: 0,
    };
    let num_subpieces = torrent.piece_len / SUBPIECE_LEN as usize;
    let mut idxs = vec![];

    // get pieces
    loop {
        // pick a piece
        let mut piece: Vec<Piece> = vec![];
        let piece_idx;
        {
            // critical section
            let mut pf = field.lock().unwrap();
            piece_idx = match pf.get_empty() {
                Some(p) => p,
                None => return idxs,
            };
            pf.arr[piece_idx] = IN_PROGRESS;
        }
        idxs.push(piece_idx);

        // all except last piece
        if piece_idx < torrent.num_pieces - 1 {
            req.index = piece_idx as u32;
            let mut subfield = ByteField {
                arr: vec![EMPTY; num_subpieces],
            };

            // subpieces
            while let Some(sub_idx) = subfield.get_empty() {
                req.offset = (sub_idx as u32) * SUBPIECE_LEN;
                req.plen = SUBPIECE_LEN;
                let subp = fetch_subpiece(
                    stream,
                    torrent,
                    field,
                    connector,
                    count,
                    &mut subfield,
                    &req,
                );

                if subp.is_none() {
                    return idxs;
                }
                piece.push(subp.unwrap());
            }

            piece.sort_by_key(|x| x.offset);
            {
                // critical section
                let mut q = hasher.queue.lock().unwrap();
                q.push_back((piece, torrent.hashes[piece_idx].clone()));
                hasher.loops.notify_one();
            }
        } else {
            // last piece
            let last_remainder: usize =
                torrent.file_len - (torrent.num_pieces - 1) * torrent.piece_len;
            let num_last_subs: usize = last_remainder / SUBPIECE_LEN as usize;
            let mut last_subfield = ByteField {
                arr: vec![EMPTY; num_last_subs],
            };

            // all except last subpiece
            req.index = torrent.num_pieces as u32 - 1;
            loop {
                let sub = last_subfield.get_empty();
                if sub == None {
                    break;
                }
                let sub_idx = sub.unwrap();

                req.offset = (sub_idx as u32) * SUBPIECE_LEN;
                req.plen = SUBPIECE_LEN;

                let subp = fetch_subpiece(
                    stream,
                    torrent,
                    field,
                    connector,
                    count,
                    &mut last_subfield,
                    &req,
                );
                if subp.is_none() {
                    return idxs;
                }
                piece.push(subp.unwrap());
            }

            // last subpiece
            let last_sub_len: usize = last_remainder - (num_last_subs * SUBPIECE_LEN as usize);
            if last_sub_len != 0 {
                req.offset = (num_last_subs as u32) * SUBPIECE_LEN;
                req.plen = last_sub_len as u32;
                let mut final_subfield = ByteField {
                    arr: vec![EMPTY; (req.offset / req.plen) as usize + 1],
                };

                let subp = fetch_subpiece(
                    stream,
                    torrent,
                    field,
                    connector,
                    count,
                    &mut final_subfield,
                    &req,
                );

                if subp.is_none() {
                    return idxs;
                }
                piece.push(subp.unwrap());
            }

            piece.sort_by_key(|x| x.offset);
            {
                // critical section
                let mut q = hasher.queue.lock().unwrap();
                q.push_back((piece, torrent.hashes[torrent.num_pieces - 1].clone()));
                hasher.loops.notify_one();
            }
        }
    }
}
