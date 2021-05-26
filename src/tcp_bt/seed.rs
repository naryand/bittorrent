#![allow(dead_code)]

use super::{msg::structs::Request, Connector};

use crate::{
    field::{constant::*, ByteField},
    file::read_subpiece,
    tcp_bt::msg::{parse_msg, try_parse, Message},
    torrent::Torrent,
    LISTENING_PORT,
};

use std::{
    io::{ErrorKind, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
};

pub enum Peer {
    Addr(SocketAddr),
    Stream(TcpStream),
}

pub fn spawn_listener(connector: &Arc<Connector>) -> JoinHandle<()> {
    let connector = Arc::clone(connector);

    std::thread::spawn(move || {
        let listener = TcpListener::bind(("0.0.0.0", LISTENING_PORT)).unwrap();
        listener.set_nonblocking(true).unwrap();

        for stream in listener.incoming() {
            match stream {
                Ok(s) => {
                    let mut q = connector.queue.lock().unwrap();
                    q.push_back(Peer::Stream(s));
                    connector.loops.notify_one();
                }
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::WouldBlock {
                        if connector.brk.load(Ordering::Relaxed) {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    } else {
                        eprintln!("{}", e);
                        break;
                    }
                }
            }
        }
    })
}

fn read_request(stream: &mut TcpStream, connector: &Arc<Connector>) -> Option<Request> {
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
        for m in parsed {
            let req = match m {
                Message::Request(r) => r,
                _ => continue,
            };

            return Some(req);
        }
    }
}

pub fn fulfill_req(
    stream: &mut TcpStream,
    torrent: &Arc<Torrent>,
    field: &Arc<Mutex<ByteField>>,
    count: &Arc<AtomicU32>,
    req: &Request,
) -> Option<()> {
    let f = field.lock().unwrap();
    if f.arr[req.index as usize] != COMPLETE {
        return Some(());
    }

    let index = req.index as usize;
    let offset = req.offset as usize;

    let mut subp = match read_subpiece(index, offset, torrent) {
        Some(s) => s,
        None => return Some(()),
    };

    subp.head.len = subp.head.len.to_be();
    subp.index = subp.index.to_be();
    subp.offset = subp.offset.to_be();

    let subp_u8 = subp.as_bytes();

    match stream.write_all(&subp_u8) {
        Ok(_) => {}
        Err(_) => return None,
    }

    count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    Some(())
}

pub fn torrent_seeder(
    stream: &mut TcpStream,
    torrent: &Arc<Torrent>,
    field: &Arc<Mutex<ByteField>>,
    connector: &Arc<Connector>,
    count: &Arc<AtomicU32>,
) {
    while !connector.brk.load(std::sync::atomic::Ordering::Relaxed) {
        match read_request(stream, connector) {
            Some(req) => {
                let f = field.lock().unwrap();
                if f.arr[req.index as usize] != COMPLETE {
                    continue;
                }

                let index = req.index as usize;
                let offset = req.offset as usize;

                let mut subp = match read_subpiece(index, offset, torrent) {
                    Some(s) => s,
                    None => continue,
                };

                subp.head.len = subp.head.len.to_be();
                subp.index = subp.index.to_be();
                subp.offset = subp.offset.to_be();

                let subp_u8 = subp.as_bytes();

                match stream.write_all(&subp_u8) {
                    Ok(_) => {}
                    Err(_) => return,
                }

                count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            None => return,
        }
    }
}
