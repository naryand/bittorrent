#![allow(dead_code)]

use super::msg::structs::*;

use crate::{
    field::{constant::*, ByteField},
    hash::Hasher,
    tcp_bt::msg::{partial_parse, Message, SUBPIECE_LEN},
};

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    vec,
};

use tokio::{runtime::Handle, task};

use async_channel::{self, Receiver, Sender};

pub struct ParseItem {
    pub rx: Receiver<Vec<u8>>,
    pub tx: Sender<Request>,
    pub handle: task::JoinHandle<Option<()>>,
    pub field: Option<Arc<Mutex<ByteField>>>,
}
pub struct Parser {
    pub tx: Sender<ParseItem>,
    pub rx: Receiver<ParseItem>,
    pub brk: AtomicBool,
}

impl Parser {
    pub fn new() -> Self {
        let (tx, rx) = async_channel::unbounded::<ParseItem>();
        Self {
            tx,
            rx,
            brk: AtomicBool::new(false),
        }
    }
}

pub fn spawn_parsers(
    parser: &Arc<Parser>,
    hasher: &Arc<Hasher>,
    handle: Handle,
    threads: usize,
) -> Vec<JoinHandle<()>> {
    let mut handles = vec![];
    for i in 0..threads {
        let parser = Arc::clone(parser);
        let hasher = Arc::clone(hasher);
        let handle = handle.clone();
        let builder = std::thread::Builder::new().name(format!("Parser{}", i));
        handles.push(
            builder
                .spawn(move || loop {
                    if parser.brk.load(Ordering::Relaxed) {
                        break;
                    }

                    let item = match handle.block_on(parser.rx.recv()) {
                        Ok(i) => i,
                        Err(_) => break
                    };

                    let mut extbuf = vec![];
                    let mut pieces = vec![];
                    'buf: loop {
                        match handle.block_on(item.rx.recv()) {
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
                                            if f.is_full() {
                                                pieces.push(piece);
                                                break 'buf;
                                            }
                                        }
                                        pieces.push(piece);
                                    }
                                }
                                Message::Request(req) => match handle.block_on(item.tx.send(req)) {
                                    Ok(_) => {}
                                    Err(_) => break,
                                },
                                _ => continue,
                            }
                        }
                    }

                    item.handle.abort();
                    let _ = handle.block_on(item.handle);
                    item.rx.close();

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
