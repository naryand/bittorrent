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

use std::{
    collections::BTreeMap,
    io::SeekFrom,
    ops::Deref,
    path::{Component, Path, PathBuf},
    str::from_utf8,
    sync::Arc,
};

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

fn safe_path_component(bytes: &[u8]) -> &str {
    let component = from_utf8(bytes).unwrap();

    if component.is_empty() || component.contains('/') || component.contains('\\') {
        panic!("unsafe torrent path component");
    }

    let mut components = Path::new(component).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => component,
        _ => panic!("unsafe torrent path component"),
    }
}

fn assert_no_symlink(path: &Path) {
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() {
            panic!("refusing to write through symlink in torrent path");
        }
    }
}

fn assert_no_symlinks_in_path(path: &Path) {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        assert_no_symlink(&current);
    }
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
        let path = PathBuf::from(safe_path_component(&filename));
        assert_no_symlinks_in_path(&path);
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
        let base_name = safe_path_component(&name);
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
            let filename = safe_path_component(&end_file);
            // parent folders to the filename
            let mut base = PathBuf::from(base_name);
            for folder in path_list {
                let folder_bytes = folder.get_str();
                base.push(safe_path_component(&folder_bytes));
            }
            // create parents and file
            assert_no_symlinks_in_path(&base);
            create_dir_all(&base).await.unwrap();
            assert_no_symlinks_in_path(&base);
            let file_path = base.join(filename);
            assert_no_symlinks_in_path(&file_path);
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

#[cfg(test)]
mod tests {
    use super::safe_path_component;

    #[test]
    fn safe_path_component_accepts_plain_name() {
        assert_eq!(safe_path_component(b"file.txt"), "file.txt");
    }

    #[test]
    #[should_panic(expected = "unsafe torrent path component")]
    fn safe_path_component_rejects_parent_directory() {
        safe_path_component(b"..");
    }

    #[test]
    #[should_panic(expected = "unsafe torrent path component")]
    fn safe_path_component_rejects_embedded_separator() {
        safe_path_component(b"nested/file.txt");
    }

    #[test]
    #[should_panic(expected = "unsafe torrent path component")]
    fn safe_path_component_rejects_windows_separator() {
        safe_path_component(b"nested\\file.txt");
    }
}
