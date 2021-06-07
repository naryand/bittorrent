#![allow(dead_code)]

use super::seed::Peer;

use crate::{
    field::{constant::*, ByteField},
    tcp_bt::{fetch::torrent_fetcher, parse::Parser, seed::torrent_seeder, send_handshake},
    torrent::Torrent,
};

use std::sync::{
    atomic::{AtomicBool, AtomicU32},
    Arc, Condvar, Mutex,
};

use tokio::{
    net::TcpStream,
    sync::Mutex as TokioMutex,
    task::{self, JoinHandle},
};

pub struct Connector {
    pub piece: Condvar,
    pub brk: AtomicBool,
}

impl Connector {
    pub fn new() -> Self {
        Self {
            piece: Condvar::new(),
            brk: AtomicBool::new(false),
        }
    }
}

pub async fn spawn_connector_task(
    peer: Peer,
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
        let mut stream = match peer {
            Peer::Addr(addr) => match TcpStream::connect(&addr).await {
                Ok(s) => s,
                Err(_) => return,
            },
            Peer::Stream(s) => s,
        };

        match send_handshake(&mut stream, torrent.info_hash, torrent.info_hash).await {
            Some(_) => {}
            None => return,
        }

        let (reader, writer) = stream.into_split();
        let am_reader = Arc::new(TokioMutex::new(reader));
        let am_writer = Arc::new(TokioMutex::new(writer));

        let mut complete = false;
        task::block_in_place(|| {
            let f = field.lock().unwrap();
            if f.is_full() {
                complete = true;
            }
        });
        if complete {
            torrent_seeder(
                &am_reader, &am_writer, &parser, &torrent, &field, &connector, &count,
            )
            .await;
            return;
        }

        let v = torrent_fetcher(
            &am_reader, &am_writer, &parser, &torrent, &field, &connector, &count,
        )
        .await;
        // resets in progress pieces
        task::block_in_place(|| {
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
        });
        torrent_seeder(
            &am_reader, &am_writer, &parser, &torrent, &field, &connector, &count,
        )
        .await;
    });
}
