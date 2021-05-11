// piece hashing threads + queue
#![allow(dead_code)]

use crate::{field::{ByteField, constant::*}, file::{write_subpiece}, 
            tcp_bt::{msg::structs::Piece}, torrent::Torrent};

use std::{collections::VecDeque, sync::{Arc, Condvar, atomic::AtomicBool, Mutex}, thread::JoinHandle};

use sha1::{Digest, Sha1};

// struct for holding relevant variables for hashing threads
pub struct Hasher {
    pub queue: Mutex<VecDeque<(Vec<Piece>, Vec<u8>)>>,
    pub conns: Condvar,
    pub empty: Condvar,
    pub loops: Condvar,
    pub brk: AtomicBool,
}

impl Hasher {
    pub fn new() -> Self {
        Hasher {
            queue: Mutex::new(VecDeque::new()),
            conns: Condvar::new(),
            empty: Condvar::new(),
            loops: Condvar::new(),
            brk: AtomicBool::new(false),
            
        }
    }
}

// spawns the hashing threads
pub fn spawn_hash_write(hasher: &Arc<Hasher>, field: &Arc<Mutex<ByteField>>, torrent: &Arc<Torrent>, 
                        threads: usize) -> Vec<JoinHandle<()>> {

    let mut handles = vec![];

    for _ in 0..threads {
        let hq = Arc::clone(hasher);
        let piece_field = Arc::clone(&field);
        let torrent = Arc::clone(torrent);
        let files = Arc::clone(&torrent.files);

        let handle = std::thread::spawn(move || {
            loop {
                let tuple;
                { // critical section
                    let mut guard = 
                    hq.loops.wait_while(hq.queue.lock().unwrap(),
                    |q| {
                        return q.is_empty();
                    }).unwrap();

                    if hq.brk.load(std::sync::atomic::Ordering::Relaxed) { break }

                    tuple = guard.pop_front().unwrap();
                }
                hq.empty.notify_all();
                let (piece, hash) = tuple;
                let index = piece[0].index as usize;
                let mut flat_piece: Vec<u8> = vec![];
                for s in piece.iter() {
                    flat_piece.extend_from_slice(&s.data); // assumes ordered by offset
                }

                let mut hasher = Sha1::new();
                hasher.update(flat_piece);
                let piece_hash = hasher.finalize().to_vec();

                if piece_hash.iter().zip(&hash).filter(|&(a, b)| *a == *b).count() != 20 {
                    { // critical section
                        // unreserve piece
                        let mut pf = piece_field.lock().unwrap();
                        pf.field[index] = (EMPTY, None);
                        // notify waiting connections
                        hq.conns.notify_one();
                    }
                    continue;
                }

                for s in &piece {
                    write_subpiece(s, torrent.piece_len, &files);
                }
                { // critical section
                    let mut pf = piece_field.lock().unwrap();
                    pf.field[index] = (COMPLETE, None);
                }
            }
        });
        handles.push(handle);
    }
    return handles;
}

// splits hashes from 1d rasterized to 2d
pub fn split_hashes(hashes: Vec<u8>) -> Vec<Vec<u8>> {
    let num_pieces: usize = hashes.len()/20;
    let mut split_hashes: Vec<Vec<u8>> = vec![vec![0; 0]; num_pieces];
    for i in 0..num_pieces {
        split_hashes[i].extend_from_slice(&hashes[(i*20)..((i+1)*20)])
    }
    return split_hashes;
}