#![allow(dead_code)]

use super::{connect::spawn_connector_task, msg::structs::Request, parse::Parser, Connector};

use crate::{
    field::{constant::*, ByteField},
    file::read_subpiece,
    tcp_bt::parse::ParseItem,
    torrent::Torrent,
};

use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpListener, TcpStream,
    },
    sync::Mutex as TokioMutex,
    task::{self, JoinHandle},
};

use async_channel;

pub enum Peer {
    Addr(SocketAddr),
    Stream(TcpStream),
}

pub async fn spawn_listener(
    listener: TcpListener,
    parser: &Arc<Parser>,
    torrent: &Arc<Torrent>,
    field: &Arc<Mutex<ByteField>>,
    connector: &Arc<Connector>,
    count: &Arc<AtomicU32>,
) -> JoinHandle<()> {
    let connector = Arc::clone(connector);
    let parser = Arc::clone(parser);
    let torrent = Arc::clone(torrent);
    let field = Arc::clone(field);
    let count = Arc::clone(count);
    return task::spawn(async move {
        let mut handles = vec![];
        loop {
            match listener.accept().await {
                Ok((s, _)) => {
                    handles.push(
                        spawn_connector_task(
                            Peer::Stream(s),
                            &parser,
                            &torrent,
                            &field,
                            &connector,
                            &count,
                        )
                        .await,
                    );
                    if connector.brk.load(Ordering::Relaxed) {
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("{}", e);
                    break;
                }
            }
        }

        for t in handles {
            t.await.unwrap();
        }
    });
}

pub async fn fulfill_req(
    write: &Arc<TokioMutex<OwnedWriteHalf>>,
    torrent: &Arc<Torrent>,
    field: &Arc<Mutex<ByteField>>,
    count: &Arc<AtomicU32>,
    req: &Request,
) -> Option<()> {
    task::block_in_place(|| {
        let f = field.lock().unwrap();
        if f.arr[req.index as usize] != COMPLETE {
            return None;
        } else {
            return Some(());
        }
    })?;

    let index = req.index as usize;
    let offset = req.offset as usize;

    let subp = match read_subpiece(index, offset, torrent).await {
        Some(s) => s,
        None => return None,
    };

    let subp_u8 = subp.as_bytes();
    let w;
    {
        let mut strm = write.lock().await;
        w = strm.write_all(&subp_u8).await;
    }
    w.ok()?;
    count.fetch_add(1, Ordering::Relaxed);

    Some(())
}

pub async fn torrent_seeder(
    read: &Arc<TokioMutex<OwnedReadHalf>>,
    write: &Arc<TokioMutex<OwnedWriteHalf>>,
    parser: &Arc<Parser>,
    torrent: &Arc<Torrent>,
    field: &Arc<Mutex<ByteField>>,
    connector: &Arc<Connector>,
    count: &Arc<AtomicU32>,
) {
    let (byte_tx, byte_rx) = async_channel::unbounded();
    let (req_tx, req_rx) = async_channel::unbounded();

    let read = Arc::clone(read);
    let write = Arc::clone(write);

    let torrent = Arc::clone(torrent);
    let field = Arc::clone(field);
    let connector = Arc::clone(connector);
    let count = Arc::clone(count);

    let reader = task::spawn(async move {
        let mut buf = vec![0; 65536];
        loop {
            let r;
            {
                let mut strm = read.lock().await;
                r = strm.read(&mut buf).await;
            }
            let bytes = match r {
                Ok(b) => b,
                Err(_) => return None,
            };
            if connector.brk.load(Ordering::Relaxed) || bytes == 0 {
                break;
            }

            byte_tx.send(buf[..bytes].to_vec()).await.unwrap();
        }
        drop(byte_tx);
        return Some(());
    });

    let item = ParseItem {
        rx: byte_rx,
        tx: req_tx,
        handle: reader,
        field: None,
    };
    if parser.tx.send(item).await.is_err() {
        return;
    }

    let seeder = task::spawn(async move {
        loop {
            let req = match req_rx.recv().await {
                Ok(r) => r,
                Err(_) => return,
            };

            match fulfill_req(&write, &torrent, &field, &count, &req).await {
                Some(_) => {}
                None => return,
            }
        }
    });

    // reader.await.unwrap();
    seeder.await.unwrap();

    return;
}
