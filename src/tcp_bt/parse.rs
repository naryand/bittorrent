#![allow(dead_code)]

use std::{
    collections::VecDeque,
    net::TcpStream,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::Receiver,
        Arc, Condvar, Mutex,
    },
    thread::JoinHandle,
    vec,
};

use crate::{
    field::{constant::*, ByteField},
    hash::Hasher,
    tcp_bt::{
        msg::{partial_parse, Message, SUBPIECE_LEN},
        seed::fulfill_req,
    },
    torrent::Torrent,
};

pub struct ParseItem {
    pub rx: Receiver<Vec<u8>>,
    pub stream: Arc<Mutex<TcpStream>>,
    pub field: Option<Arc<Mutex<ByteField>>>,
}
pub struct Parser {
    pub queue: Mutex<VecDeque<ParseItem>>,
    pub loops: Condvar,
    pub brk: AtomicBool,
}

impl Parser {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            loops: Condvar::new(),
            brk: AtomicBool::new(false),
        }
    }
}

pub fn spawn_parsers(
    parser: &Arc<Parser>,
    hasher: &Arc<Hasher>,
    torrent: &Arc<Torrent>,
    piece_field: &Arc<Mutex<ByteField>>,
    count: &Arc<AtomicU32>,
    threads: usize,
) -> Vec<JoinHandle<()>> {
    let mut handles = vec![];
    for i in 0..threads {
        let parser = Arc::clone(parser);
        let hasher = Arc::clone(hasher);
        let torrent = Arc::clone(torrent);
        let piece_field = Arc::clone(piece_field);
        let count = Arc::clone(count);
        let builder = std::thread::Builder::new().name(format!("Parser{}", i));
        handles.push(
            builder
                .spawn(move || 'q: loop {
                    let item;
                    {
                        let mut guard = parser
                            .loops
                            .wait_while(parser.queue.lock().unwrap(), |q| {
                                if parser.brk.load(Ordering::Relaxed) {
                                    return false;
                                }

                                q.is_empty()
                            })
                            .unwrap();
                        if parser.brk.load(Ordering::Relaxed) {
                            break;
                        }

                        item = match guard.pop_front() {
                            Some(s) => s,
                            None => break,
                        };
                    }

                    let mut extbuf = vec![];
                    let mut pieces = vec![];
                    loop {
                        match item.rx.recv() {
                            Ok(b) => unsafe {
                                let x = extbuf.len();
                                let y = b.len();
                                extbuf.reserve(y);
                                extbuf.set_len(x + y);
                                let src = b.as_ptr();
                                let d: *mut u8 = extbuf.as_mut_ptr();
                                let dst = d.add(x);
                                std::ptr::copy_nonoverlapping(src, dst, y);
                            },
                            Err(_) => break,
                        }

                        let (_, parsed) = partial_parse(&mut extbuf);
                        for m in parsed {
                            match m {
                                Message::Piece(piece) => {
                                    if item.field.is_some() {
                                        let field = item.field.as_ref().unwrap();
                                        {
                                            let mut f = field.lock().unwrap();
                                            f.arr[(piece.offset / SUBPIECE_LEN) as usize] =
                                                COMPLETE;
                                        }
                                        pieces.push(piece);
                                    }
                                }
                                Message::Request(req) => {
                                    let mut s = item.stream.lock().unwrap();
                                    match fulfill_req(&mut s, &torrent, &piece_field, &count, &req)
                                    {
                                        Some(_) => continue,
                                        None => continue 'q,
                                    }
                                }
                                _ => continue,
                            }
                        }
                    }
                    if pieces.is_empty() {
                        continue;
                    }
                    {
                        // critical section
                        let mut q = hasher.queue.lock().unwrap();
                        q.push_back(pieces);
                        hasher.loops.notify_one();
                    }
                })
                .unwrap(),
        );
    }

    handles
}
