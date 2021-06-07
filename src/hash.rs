// piece hashing threads + queue
#![allow(dead_code)]

use crate::{
    field::{constant::*, ByteField},
    file::write_subpiece,
    tcp_bt::{connect::Connector, msg::structs::Piece},
    torrent::Torrent,
};

use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::JoinHandle,
};

use sha1::{Digest, Sha1};
use tokio::runtime::Handle;

// struct for holding relevant variables for hashing threads
pub struct Hasher {
    pub queue: Mutex<VecDeque<Vec<Piece>>>,
    pub empty: Condvar,
    pub loops: Condvar,
    pub brk: AtomicBool,
}

impl Hasher {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            empty: Condvar::new(),
            loops: Condvar::new(),
            brk: AtomicBool::new(false),
        }
    }
}

// spawns the hashing threads
pub fn spawn_hash_write(
    hasher: &Arc<Hasher>,
    field: &Arc<Mutex<ByteField>>,
    torrent: &Arc<Torrent>,
    connector: &Arc<Connector>,
    handle: Handle,
    threads: usize,
) -> Vec<JoinHandle<()>> {
    let mut handles = vec![];

    for i in 0..threads {
        let hasher = Arc::clone(hasher);
        let piece_field = Arc::clone(&field);
        let torrent = Arc::clone(torrent);
        let connector = Arc::clone(connector);
        let files = Arc::clone(&torrent.files);
        let handle = handle.clone();

        let builder = std::thread::Builder::new().name(format!("Hash{}", i));
        let handle = builder
            .spawn(move || {
                loop {
                    let mut piece;
                    {
                        // critical section
                        let mut guard = hasher
                            .loops
                            .wait_while(hasher.queue.lock().unwrap(), |q| {
                                if hasher.brk.load(Ordering::Relaxed) {
                                    return false;
                                }
                                q.is_empty()
                            })
                            .unwrap();
                        if hasher.brk.load(Ordering::Relaxed) {
                            break;
                        }

                        piece = match guard.pop_front() {
                            Some(t) => t,
                            None => break,
                        }
                    }
                    hasher.empty.notify_all();
                    let index = piece[0].index as usize;
                    let mut flat_piece = Vec::with_capacity(torrent.piece_len);
                    piece.sort_by_key(|x| x.offset);
                    for s in &piece {
                        flat_piece.extend_from_slice(&s.data); // assumes ordered by offset
                    }

                    let mut hasher = Sha1::new();
                    hasher.update(flat_piece);
                    let piece_hash = hasher.finalize().to_vec();

                    if piece_hash
                        .iter()
                        .zip(&torrent.hashes[index])
                        .filter(|&(a, b)| *a == *b)
                        .count()
                        != 20
                    {
                        {
                            // critical section
                            // unreserve piece
                            let mut pf = piece_field.lock().unwrap();
                            pf.arr[index] = EMPTY;
                            // notify waiting connections
                            connector.piece.notify_one();
                        }
                        continue;
                    }
                    for s in &piece {
                        handle.block_on(write_subpiece(s, torrent.piece_len, &files));
                    }
                    {
                        // critical section
                        let mut pf = piece_field.lock().unwrap();
                        pf.arr[index] = COMPLETE;
                    }
                }
            })
            .unwrap();
        handles.push(handle);
    }

    handles
}

// splits hashes from 1d rasterized to 2d
pub fn split_hashes(hashes: &[u8]) -> Vec<Vec<u8>> {
    let num_pieces: usize = hashes.len() / 20;
    let mut split_hashes: Vec<Vec<u8>> = vec![vec![0; 0]; num_pieces];
    for i in 0..num_pieces {
        split_hashes[i].extend_from_slice(&hashes[(i * 20)..((i + 1) * 20)])
    }

    split_hashes
}
