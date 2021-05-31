#![allow(dead_code)]

use std::{
    collections::VecDeque,
    net::TcpStream,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::JoinHandle,
    time::Duration,
};

use crate::{
    field::{constant::*, ByteField},
    hash::Hasher,
    tcp_bt::{
        fetch::torrent_fetcher,
        parse::{spawn_parsers, Parser},
        seed::torrent_seeder,
        send_handshake,
    },
    torrent::Torrent,
};

use super::seed::Peer;

pub struct Connector {
    // do something with the TcpStream
    pub queue: Mutex<VecDeque<Peer>>,
    pub loops: Condvar,
    pub piece: Condvar,
    pub brk: AtomicBool,
}

impl Connector {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            loops: Condvar::new(),
            piece: Condvar::new(),
            brk: AtomicBool::new(false),
        }
    }
}

pub fn spawn_connectors(
    connector: &Arc<Connector>,
    hasher: &Arc<Hasher>,
    torrent: &Arc<Torrent>,
    field: &Arc<Mutex<ByteField>>,
    count: &Arc<AtomicU32>,
    threads: usize,
) -> Vec<JoinHandle<()>> {
    let mut handles = vec![];
    let parser = Arc::new(Parser::new());
    spawn_parsers(&parser, hasher, torrent, field, count, 24);
    for i in 0..threads {
        let connector = Arc::clone(connector);
        let parser = Arc::clone(&parser);
        let torrent = Arc::clone(torrent);
        let field = Arc::clone(field);
        let builder = std::thread::Builder::new().name(format!("Connector{:?}", i));
        handles.push(
            builder
                .spawn(move || {
                    loop {
                        let peer;
                        {
                            let mut guard = connector
                                .loops
                                .wait_while(connector.queue.lock().unwrap(), |q| {
                                    if connector.brk.load(Ordering::Relaxed) {
                                        return false;
                                    }

                                    q.is_empty()
                                })
                                .unwrap();
                            if connector.brk.load(Ordering::Relaxed) {
                                break;
                            }

                            peer = match guard.pop_front() {
                                Some(s) => s,
                                None => break,
                            }
                        }

                        let mut stream = match peer {
                            Peer::Addr(addr) => {
                                let timeout = Duration::from_secs(5);
                                match TcpStream::connect_timeout(&addr, timeout) {
                                    Ok(s) => s,
                                    Err(_) => continue,
                                }
                            }
                            Peer::Stream(s) => s,
                        };

                        match send_handshake(&mut stream, torrent.info_hash, torrent.info_hash) {
                            Some(_) => {}
                            None => continue,
                        }
                        stream.set_nonblocking(true).unwrap();

                        let v = torrent_fetcher(&mut stream, &parser, &torrent, &field, &connector);
                        // resets in progress pieces
                        let mut complete = true;
                        {
                            let mut f = field.lock().unwrap();
                            for i in &v {
                                if f.arr[*i] == IN_PROGRESS {
                                    f.arr[*i] = EMPTY;
                                    connector.piece.notify_one();
                                }
                            }
                            for i in &f.arr {
                                if *i == EMPTY {
                                    complete = false;
                                }
                            }
                        }
                        if complete {
                            torrent_seeder(&mut stream, &parser, &connector);
                        }
                    }
                })
                .unwrap(),
        );
    }

    handles
}
