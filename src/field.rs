// byte field used for torrent control flow
#![allow(dead_code)]

pub mod constant {
    pub const EMPTY: u8 = 0;
    pub const IN_PROGRESS: u8 = 1;
    pub const COMPLETE: u8 = 2;
}

use self::constant::*;

use std::sync::{Weak, atomic::AtomicBool};

pub struct ByteField {
    pub field: Vec<(u8, Option<Weak<AtomicBool>>)>,
}

impl ByteField {
    // returns true if every index is marked complete
    pub fn is_full(&self) -> bool {
        let nonfull: usize = self.field.iter().filter(|(x, _y)| *x < 2).count();
        if nonfull == 0 { return true }
        else { return false }
    }

    // returns an index which is marked empty
    pub fn get_empty(&self) -> Option<usize> {
        if self.is_full() { return None }
        for i in 0..(self.field.len()) {
            if self.field[i].0 == EMPTY {
                return Some(i);
            }
        }
        return None;
    }
}