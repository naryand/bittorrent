// tcp peer wire piece downloading
#![allow(dead_code)]

use super::{
    msg::{bytes::*, structs::*, SUBPIECE_LEN},
    parse::Parser,
    Connector,
};
use crate::{
    field::{constant::*, ByteField},
    tcp_bt::{parse::ParseItem, seed::fulfill_req},
    torrent::Torrent,
};

use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    usize, vec,
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::Mutex as TokioMutex,
    task::{self, JoinHandle},
};

async fn request_piece(
    write: &Arc<TokioMutex<OwnedWriteHalf>>,
    torrent: &Arc<Torrent>,
    index: u32,
) -> Option<usize> {
    let mut request = Request {
        head: Header {
            len: 13_u32.to_be(),
            byte: REQUEST,
        },
        index: index.to_be(),
        plen: SUBPIECE_LEN.to_be(),
        ..Request::default()
    };

    let remainder;
    if index as usize == torrent.num_pieces - 1 {
        remainder = torrent.file_len % torrent.piece_len;
    } else {
        remainder = torrent.piece_len;
    }

    let mut num_subpieces = remainder / SUBPIECE_LEN as usize;
    for i in 0..num_subpieces {
        request.offset = u32::to_be((i as u32) * SUBPIECE_LEN);
        let req_u8 = bincode::serialize(&request).unwrap();

        let w;
        {
            let mut strm = write.lock().await;
            w = strm.write_all(&req_u8).await;
        }
        w.ok()?;
    }

    if remainder % SUBPIECE_LEN as usize > 0 {
        let last_plen = remainder % SUBPIECE_LEN as usize;
        request.plen = u32::to_be(last_plen as u32);
        request.offset = u32::to_be(num_subpieces as u32 * SUBPIECE_LEN);
        let req_u8 = bincode::serialize(&request).unwrap();
        let w;
        {
            let mut strm = write.lock().await;
            w = strm.write_all(&req_u8).await;
        }
        w.ok()?;
        num_subpieces += 1;
    }

    Some(num_subpieces)
}

async fn read_piece(
    read: &Arc<TokioMutex<OwnedReadHalf>>,
    write: &Arc<TokioMutex<OwnedWriteHalf>>,
    parser: &Arc<Parser>,
    torrent: &Arc<Torrent>,
    field: &Arc<Mutex<ByteField>>,
    connector: &Arc<Connector>,
    count: &Arc<AtomicU32>,
    num_subpieces: usize,
) -> Option<()> {
    let subfield = ByteField {
        arr: vec![EMPTY; num_subpieces],
    };

    let am_subfield = Arc::new(Mutex::new(subfield));
    let (byte_tx, byte_rx) = async_channel::unbounded();
    let (req_tx, req_rx) = async_channel::unbounded();

    let read = Arc::clone(read);
    let write = Arc::clone(write);

    let torrent = Arc::clone(torrent);
    let field = Arc::clone(field);
    let connector = Arc::clone(connector);
    let count = Arc::clone(count);
    let subf = Arc::clone(&am_subfield);

    let seeder: JoinHandle<Option<()>> = task::spawn(async move {
        loop {
            let req = match req_rx.recv().await {
                Ok(r) => r,
                Err(_) => return Some(()),
            };
            match fulfill_req(&write, &torrent, &field, &count, &req).await {
                Some(_) => continue,
                None => return None,
            }
        }
    });

    let reader = task::spawn(async move {
        let mut buf = vec![0; 65536];
        loop {
            if connector.brk.load(Ordering::Relaxed) {
                return None;
            }
            if task::block_in_place(|| {
                let subf = subf.lock().unwrap();
                if subf.is_full() {
                    return None;
                }
                return Some(());
            })
            .is_none()
            {
                break;
            }
            
            let r;
            {
                let mut strm = read.lock().await;
                r = strm.read(&mut buf).await;
            }
            let bytes = r.ok()?;

            if byte_tx.send(buf[..bytes].to_vec()).await.is_err() {
                break;
            }
        }
        drop(byte_tx);
        return Some(());
    });

    let item = ParseItem {
        rx: byte_rx,
        tx: req_tx,
        handle: reader,
        field: Some(Arc::clone(&am_subfield)),
    };

    parser.tx.send(item).await.unwrap();
    
    seeder.await.unwrap()?;

    return Some(());
}

// represents a single connection to a peer, continously fetches subpieces
pub async fn torrent_fetcher(
    read: &Arc<TokioMutex<OwnedReadHalf>>,
    write: &Arc<TokioMutex<OwnedWriteHalf>>,
    parser: &Arc<Parser>,
    torrent: &Arc<Torrent>,
    field: &Arc<Mutex<ByteField>>,
    connector: &Arc<Connector>,
    count: &Arc<AtomicU32>,
) -> Vec<usize> {
    let mut idxs = vec![];
    // get pieces
    loop {
        let mut nums = vec![];
        // peers reject if you request more than 1 piece
        for _ in 0..1_usize {
            // pick a piece
            let piece_idx = match task::block_in_place(move || {
                // critical section
                let mut pf = connector
                    .piece
                    .wait_while(field.lock().unwrap(), |f| {
                        if connector.brk.load(Ordering::Relaxed) {
                            return false;
                        }
                        if f.is_full() {
                            return false;
                        }
                        f.get_empty().is_none()
                    })
                    .unwrap();
                if let Some(p) = pf.get_empty() {
                    pf.arr[p] = IN_PROGRESS;
                    return Some(p);
                } else {
                    return None;
                }
            }) {
                Some(p) => p,
                None => return idxs,
            };
            idxs.push(piece_idx);

            // fetch piece
            let num = request_piece(write, torrent, piece_idx as u32).await;

            if let Some(n) = num {
                nums.push(n)
            } else {
                return idxs;
            }
        }
        for num_subpieces in nums {
            if read_piece(
                &read,
                &write,
                parser,
                torrent,
                field,
                connector,
                count,
                num_subpieces,
            )
            .await
            .is_none()
            {
                return idxs;
            }
        }
    }
}
