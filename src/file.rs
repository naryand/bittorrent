// file io functionality
#![allow(dead_code)]

use crate::{tcp_bt::{msg::{SUBPIECE_LEN, bytes::PIECE, structs::{Header, Piece}}}, 
            bencode::Item, hash::Hasher, torrent::Torrent};

use std::{collections::{BTreeMap}, fs::{File, OpenOptions, create_dir_all}, ops::Deref, 
          path::Path, str::from_utf8, sync::{Arc, Mutex}};

#[cfg(target_family="windows")]
use std::os::windows::prelude::*;
#[cfg(target_family="unix")]
use std::os::unix::fs::FileExt;

// stores file object and length of each file
pub struct FileSize {
    file: Arc<Mutex<File>>,
    len: usize,
}

// writes and maps a subpiece to it's file(s)
pub fn write_subpiece(piece: &Piece, piece_len: usize, files: &Arc<Vec<FileSize>>) {
    let mut start = (piece.index as usize*piece_len)+piece.offset as usize;
    let mut end = start+piece.data.len();
    let mut next_file = 0u64;

    for filesize in files.deref() {
        if start > filesize.len && next_file == 0 { 
            start -= filesize.len;
            end -= filesize.len;
            continue;
        }

        if next_file > 0 { // write the rest onto the next file
            { // critical section
                let f = filesize.file.lock().unwrap();
                #[cfg(target_family="windows")]
                f.seek_write(&piece.data[(end-start)..], 0u64).unwrap();
                #[cfg(target_family="unix")]
                f.write_all_at(&piece.data[(end-start)..], 0u64).unwrap();
            }
            return;
        } else {
            if end > filesize.len { next_file = (end-filesize.len) as u64; end = filesize.len; }
            
            { // critical section
                let f = filesize.file.lock().unwrap();
                #[cfg(target_family="windows")]
                f.seek_write(&piece.data[0..(end-start)], start as u64).unwrap();
                #[cfg(target_family="unix")]
                f.write_all_at(&piece.data[0..(end-start)], start as u64).unwrap();
            }
            if next_file == 0 { return; }
        }
    }
}

// reads a subpiece mapped from it's file(s)
pub fn read_subpiece(index: usize, offset: usize, torrent: &Arc<Torrent>) -> Option<Piece> {
    let mut start = (index*torrent.piece_len)+offset;
    let mut end = start+SUBPIECE_LEN as usize;
    let mut next_file = 0u64;
    let mut piece_buf: Vec<u8> = vec![];

    for filesize in torrent.files.deref() {
        if start > filesize.len && next_file == 0 { 
            start -= filesize.len;
            end -= filesize.len;
            continue;
        }

        if next_file > 0 { // read the rest from the next file
            let mut buf: Vec<u8> = vec![0; next_file as usize];
            { // critical section
                let f = filesize.file.lock().unwrap();
                #[cfg(target_family="windows")] {
                    f.seek_read(&mut buf, 0).ok()?;
                }
                #[cfg(target_family="unix")] {
                    f.read_exact_at(&mut buf, 0).ok()?;
                }
            }
            piece_buf.append(&mut buf);
            break;
        } else {
            if end > filesize.len { next_file = (end-filesize.len) as u64; end = filesize.len; }
            
            piece_buf = vec![0; end-start];
            { // critical section
                let f = filesize.file.lock().unwrap();
                #[cfg(target_family="windows")] {
                    f.seek_read(&mut piece_buf, start as u64).ok()?;
                }
                #[cfg(target_family="unix")] {
                    f.read_exact_at(&mut piece_buf, start as u64).ok()?;
                }
            }
            if next_file == 0 { break }
        }
    }

    let piece = Piece {
        head: Header {
            len: piece_buf.len() as u32+9, byte: PIECE,
        }, index: index as u32, offset: offset as u32,
        data: piece_buf,
    };
    return Some(piece);
}

// reads all subpieces and queues them for hashing threads to verify as complete or not
pub fn resume_torrent(torrent: &Arc<Torrent>, hasher: &Arc<Hasher>) {
    for i in 0..torrent.num_pieces {
        let mut piece = vec![];
        for j in 0..(torrent.piece_len/SUBPIECE_LEN as usize) {
            let subp = match read_subpiece(i, j*SUBPIECE_LEN as usize, torrent) {
                Some(subp) => subp,
                None => continue,
            };
            if subp.data.len() == 0 { continue; }
            piece.push(subp);
        }
        if piece.len() == 0 { continue; }
        {
            let mut q = hasher.queue.lock().unwrap();
            q.push_back((piece, torrent.hashes[i].to_vec()));
            hasher.loops.notify_one();
        }
    }
    
    { // wait thread until hashing finishes
        let _guard = 
        hasher.empty.wait_while(hasher.queue.lock().unwrap(), 
        |q| {
            return !q.is_empty();
        }).unwrap();
    }
}

// parses out each file from the info dict
pub fn parse_file(info: BTreeMap<Vec<u8>, Item>) -> (Arc<Vec<FileSize>>, usize) {
    match info.get("length".as_bytes()) {
        // single file
        Some(s) => {
            // file length
            let file_len = s.get_int() as usize;
            // name of the file
            let filename = info.get("name".as_bytes()).unwrap().get_str();
            // create file and return
            let path = Path::new(std::str::from_utf8(&filename).unwrap());
            let dest = Arc::new(Mutex::new(
                OpenOptions::new().read(true).write(true).create(true).open(path).unwrap()));
            let file_size = FileSize { file: dest, len: file_len };
            return (Arc::new(vec![file_size]), file_len);
        }
        // multifile
        None => {
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
                create_dir_all(base.clone()).unwrap();
                let full_path = base+"/"+filename;
                let file_path = Path::new(&full_path);
                let file = Arc::new(Mutex::new(
                        OpenOptions::new().read(true).write(true).create(true).open(file_path).unwrap()));
                ret.push(FileSize { file, len });
            }

            // get total length from each file
            let mut total_len = 0usize;
            for filesize in &ret {
                total_len += filesize.len;
            }

            return (Arc::new(ret), total_len);
        }
    }
}