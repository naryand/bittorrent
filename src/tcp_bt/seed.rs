#![allow(dead_code)]

use super::{msg::structs::Request, parse::Parser, Connector};

use crate::{
    field::{constant::*, ByteField},
    file::read_subpiece,
    tcp_bt::parse::ParseItem,
    torrent::Torrent,
};

use std::{
    io::{ErrorKind, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::JoinHandle,
};

pub enum Peer {
    Addr(SocketAddr),
    Stream(TcpStream),
}

pub fn spawn_listener(connector: &Arc<Connector>, listener: &Arc<TcpListener>) -> JoinHandle<()> {
    let connector = Arc::clone(connector);
    let listener = Arc::clone(listener);
    let builder = std::thread::Builder::new().name("listener".to_owned());
    builder
        .spawn(move || {
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
        .unwrap()
}

pub fn fulfill_req(
    stream: &mut TcpStream,
    torrent: &Arc<Torrent>,
    field: &Arc<Mutex<ByteField>>,
    count: &Arc<AtomicU32>,
    req: &Request,
) -> Option<()> {
    {
        let f = field.lock().unwrap();
        if f.arr[req.index as usize] != COMPLETE {
            return Some(());
        }
    }

    let index = req.index as usize;
    let offset = req.offset as usize;

    let subp = match read_subpiece(index, offset, torrent) {
        Some(s) => s,
        None => return None,
    };

    let subp_u8 = subp.as_bytes();
    stream.write_all(&subp_u8).ok()?;
    count.fetch_add(1, Ordering::Relaxed);

    Some(())
}

pub fn torrent_seeder(stream: &mut TcpStream, parser: &Arc<Parser>, connector: &Arc<Connector>) {
    let (tx, rx) = mpsc::channel();
    let item = ParseItem {
        rx,
        stream: Arc::new(Mutex::new(stream.try_clone().unwrap())),
        field: None,
    };
    {
        let mut q = parser.queue.lock().unwrap();
        q.push_back(item);
        parser.loops.notify_one();
    }
    let mut buf = vec![0; 65536];
    while !connector.brk.load(Ordering::Relaxed) {
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
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_micros(1));
                    } else {
                        return;
                    }
                }
            }
        }
        if bytes == 0 {
            return;
        }

        tx.send(buf[..bytes].to_vec()).unwrap();
    }
    drop(tx);
}
