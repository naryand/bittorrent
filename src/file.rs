// file io functionality
#![allow(dead_code)]

use crate::{
    bencode::Item,
    hash::Hasher,
    tcp_bt::msg::{
        bytes::PIECE,
        structs::{Header, Piece},
        SUBPIECE_LEN,
    },
    torrent::Torrent,
};

use std::{collections::BTreeMap, io::SeekFrom, ops::Deref, path::Path, str::from_utf8, sync::Arc};

use tokio::{
    fs::{create_dir_all, File, OpenOptions},
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    sync::Mutex as TokioMutex,
    task,
};

// stores file object and length of each file
pub struct FileSize {
    file: Arc<TokioMutex<File>>,
    len: usize,
}

// writes and maps a subpiece to it's file(s)
pub async fn write_subpiece(piece: &Piece, piece_len: usize, files: &Arc<Vec<FileSize>>) {
    let mut start = (piece.index as usize * piece_len) + piece.offset as usize;
    let mut end = start + piece.data.len();
    let mut next_file = 0_u64;

    for filesize in files.deref() {
        if start > filesize.len && next_file == 0 {
            start -= filesize.len;
            end -= filesize.len;
            continue;
        }

        if next_file > 0 {
            // write the rest onto the next file
            {
                let mut f = filesize.file.lock().await;
                f.seek(SeekFrom::Start(0)).await.unwrap();
                f.write_all(&piece.data[(end - start)..]).await.unwrap();
            }
            return;
        }
        if end > filesize.len {
            next_file = (end - filesize.len) as u64;
            end = filesize.len;
        }
        {
            let mut f = filesize.file.lock().await;
            f.seek(SeekFrom::Start(start as u64)).await.unwrap();
            f.write_all(&piece.data[0..(end - start)]).await.unwrap();
        }

        if next_file == 0 {
            return;
        }
    }
}

// reads a subpiece mapped from it's file(s)
pub async fn read_subpiece(index: usize, offset: usize, torrent: &Arc<Torrent>) -> Option<Piece> {
    let mut start = (index * torrent.piece_len) + offset;
    let mut end = start + SUBPIECE_LEN as usize;
    let mut next_file = 0_u64;
    let mut piece_buf: Vec<u8> = vec![];

    for filesize in torrent.files.deref() {
        if start > filesize.len && next_file == 0 {
            start -= filesize.len;
            end -= filesize.len;
            continue;
        }

        if next_file > 0 {
            // read the rest from the next file
            let mut buf: Vec<u8> = vec![0; next_file as usize];
            {
                let mut f = filesize.file.lock().await;
                f.seek(SeekFrom::Start(0)).await.unwrap();
                f.read_exact(&mut buf).await.ok()?;
            }
            piece_buf.append(&mut buf);
            break;
        }
        if end > filesize.len {
            next_file = (end - filesize.len) as u64;
            end = filesize.len;
        }

        piece_buf = vec![0; end - start];
        {
            let mut f = filesize.file.lock().await;
            f.seek(SeekFrom::Start(start as u64)).await.unwrap();
            f.read_exact(&mut piece_buf).await.ok()?;
        }
        if next_file == 0 {
            break;
        }
    }

    let piece = Piece {
        head: Header {
            len: piece_buf.len() as u32 + 9,
            byte: PIECE,
        },
        index: index as u32,
        offset: offset as u32,
        data: piece_buf,
    };

    Some(piece)
}

// reads all subpieces and queues them for hashing threads to verify as complete or not
pub async fn resume_torrent(torrent: &Arc<Torrent>, hasher: &Arc<Hasher>) {
    for i in 0..torrent.num_pieces {
        let mut piece = vec![];
        for j in 0..(torrent.piece_len / SUBPIECE_LEN as usize) {
            let subp = match read_subpiece(i, j * SUBPIECE_LEN as usize, torrent).await {
                Some(subp) => subp,
                None => continue,
            };
            if subp.data.is_empty() {
                continue;
            }
            piece.push(subp);
        }
        if piece.is_empty() {
            continue;
        }
        task::block_in_place(|| {
            let mut q = hasher.queue.lock().unwrap();
            q.push_back(piece);
            hasher.loops.notify_one();
        });
    }

    task::block_in_place(|| {
        // wait thread until hashing finishes
        #[rustfmt::skip]
        let _guard = hasher
            .empty
            .wait_while(hasher.queue.lock().unwrap(),
                |q| !q.is_empty()
            ).unwrap();
    });
}

// parses out each file from the info dict
pub async fn parse_file(info: &BTreeMap<Vec<u8>, Item>) -> (Arc<Vec<FileSize>>, usize) {
    // single file
    if let Some(s) = info.get("length".as_bytes()) {
        // file length
        let file_len = s.get_int() as usize;
        // name of the file
        let filename = info.get("name".as_bytes()).unwrap().get_str();
        // create file and return
        let path = Path::new(std::str::from_utf8(&filename).unwrap());
        let dest = Arc::new(TokioMutex::new(
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(path)
                .await
                .unwrap(),
        ));
        let file_size = FileSize {
            file: dest,
            len: file_len,
        };

        (Arc::new(vec![file_size]), file_len)
    } else {
        // multifile
        // get parent folder name and file dicts
        let name = info.get("name".as_bytes()).unwrap().get_str();
        let files = info.get("files".as_bytes()).unwrap().get_list();
        let mut ret: Vec<FileSize> = vec![];
        // for each dict
        for f in files {
            let dict = f.get_dict();
            // get length
            let len = dict.get("length".as_bytes()).unwrap().get_int() as usize;
            // parse out path
            let mut path_list = dict.get("path".as_bytes()).unwrap().get_list();
            // end filename
            let end_file = path_list.pop().unwrap().get_str();
            let filename = from_utf8(&end_file).unwrap();
            // parent folders to the filename
            let mut base = "./".to_string() + from_utf8(&name).unwrap();
            for folder in path_list {
                let folder_name = "/".to_string() + from_utf8(&folder.get_str()).unwrap();
                base.push_str(&folder_name);
            }
            // create parents and file
            create_dir_all(base.clone()).await.unwrap();
            let full_path = base + "/" + filename;
            let file_path = Path::new(&full_path);
            let file = Arc::new(TokioMutex::new(
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open(file_path)
                    .await
                    .unwrap(),
            ));
            ret.push(FileSize { file, len });
        }

        // get total length from each file
        let mut total_len = 0_usize;
        for filesize in &ret {
            total_len += filesize.len;
        }

        (Arc::new(ret), total_len)
    }
}
