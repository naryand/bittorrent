// byte field used for torrent control flow
#![allow(dead_code)]

pub mod constant {
    pub const EMPTY: u8 = 0;
    pub const IN_PROGRESS: u8 = 1;
    pub const COMPLETE: u8 = 2;
}

use self::constant::*;

pub struct ByteField {
    pub arr: Vec<u8>,
}

impl ByteField {
    // returns true if every index is marked complete
    pub fn is_full(&self) -> bool {
        let nonfull: usize = self.arr.iter().filter(|x| **x < 2).count();
        if nonfull == 0 { return true }
        else { return false }
    }

    // returns an index which is marked empty
    pub fn get_empty(&self) -> Option<usize> {
        for i in 0..self.arr.len() {
            if self.arr[i] == EMPTY {
                return Some(i);
            }
        }
        return None;
    }
}