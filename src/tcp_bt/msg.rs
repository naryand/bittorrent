// parsing for tcp peer wire messages
#![allow(dead_code)]

// constants for byte in each message
pub mod bytes {
    pub const CHOKE: u8 = 0;
    pub const UNCHOKE: u8 = 1;
    pub const INTEREST: u8 = 2;
    pub const UNINTEREST: u8 = 3;
    pub const HAVE: u8 = 4;
    pub const BITFIELD: u8 = 5;
    pub const REQUEST: u8 = 6;
    pub const PIECE: u8 = 7;
    pub const CANCEL: u8 = 8;
    pub const HANDSHAKE: u8 = 0x54;
}

// takes off top 4 bytes to make u32
fn parse_u32(msg: &[u8]) -> u32 {
    let mut bytes: [u8; 4] = [0; 4];
    bytes.copy_from_slice(&msg[0..4]);
    u32::from_be_bytes(bytes)
}

// structs for each type of message
pub mod structs {
    use super::{bytes::*, parse_u32};

    use serde::Serialize;
    #[derive(Serialize, Debug)]
    pub struct Handshake {
        pub len: u8,
        pub protocol: [u8; 19],
        pub reserved: [u8; 8],
        pub info_hash: [u8; 20],
        pub peer_id: [u8; 20],
    }

    impl Default for Handshake {
        fn default() -> Handshake {
            let name = b"BitTorrent protocol";
            let mut p: [u8; 19] = [0; 19];
            p.copy_from_slice(&name[0..19]);
            Handshake {
                len: 19,
                protocol: p,
                reserved: [0; 8],
                info_hash: [0; 20],
                peer_id: [0; 20],
            }
        }
    }

    impl Handshake {
        fn test(&self) -> bool {
            if self.len != 19 {
                return false;
            }
            self.protocol
                .iter()
                .zip(b"BitTorrent protocol")
                .all(|(a, b)| *a == *b)
        }

        pub fn parse(msg: &mut Vec<u8>) -> Option<Self> {
            if msg.len() < 68 {
                return None;
            }
            let mut handshake = Handshake {
                len: msg[0],
                ..Handshake::default()
            };

            handshake.protocol.clone_from_slice(&msg[1..20]);
            handshake.reserved.clone_from_slice(&msg[20..28]);
            handshake.info_hash.clone_from_slice(&msg[28..48]);
            handshake.peer_id.clone_from_slice(&msg[48..68]);

            if handshake.test() {
                msg.drain(0..68);
                Some(handshake)
            } else {
                None
            }
        }
    }

    #[derive(Serialize, Debug, Default, Clone)]
    pub struct Header {
        pub len: u32,
        pub byte: u8,
    }

    impl Header {
        fn test(&self) -> bool {
            self.byte <= CANCEL || self.byte == HANDSHAKE
        }

        pub fn parse(msg: &[u8]) -> Option<Self> {
            if msg.len() < 5 {
                return None;
            }
            let head = Header {
                len: parse_u32(&msg[0..4]),
                byte: msg[4],
            };

            if head.test() {
                Some(head)
            } else {
                None
            }
        }

        pub fn as_bytes(&self) -> Vec<u8> {
            let mut bytes = vec![];
            bytes.append(&mut u32::to_be_bytes(self.len).to_vec());
            bytes.push(self.byte);
            bytes
        }
    }

    #[derive(Serialize, Debug, Default)]
    pub struct Have {
        pub head: Header,
        pub index: u32,
    }

    impl Have {
        fn test(&self) -> bool {
            !(self.head.len != 5 || self.head.byte != HAVE)
        }

        pub fn parse(msg: &mut Vec<u8>) -> Option<Self> {
            if msg.len() < 9 {
                return None;
            }
            let have = Have {
                head: Header::parse(msg)?,
                index: parse_u32(&msg[5..9]),
            };

            if have.test() {
                msg.drain(0..9);
                Some(have)
            } else {
                None
            }
        }
    }

    #[derive(Serialize, Debug, Default)]
    pub struct Bitfield {
        pub head: Header,
        pub data: Vec<u8>,
    }

    impl Bitfield {
        fn test(&self) -> bool {
            if self.head.byte != BITFIELD {
                return false;
            }
            self.head.len as usize == (self.data.len() + 1)
        }

        pub fn parse(msg: &mut Vec<u8>) -> Option<Self> {
            let mut bitfield = Bitfield {
                head: Header::parse(msg)?,
                ..Bitfield::default()
            };
            if msg.len() < (bitfield.head.len + 4) as usize {
                return None;
            }

            bitfield.data = vec![];
            bitfield
                .data
                .extend_from_slice(&msg[5..((bitfield.head.len + 4) as usize)]);

            if bitfield.test() {
                let mut copy = msg[((bitfield.head.len + 4) as usize)..].to_vec();
                msg.clear();
                msg.append(&mut copy);
                Some(bitfield)
            } else {
                None
            }
        }
    }

    #[derive(Serialize, Debug, Default)]
    pub struct Request {
        pub head: Header,
        pub index: u32,
        pub offset: u32,
        pub plen: u32,
    }

    impl Request {
        fn test(&self) -> bool {
            if self.head.byte != REQUEST {
                return false;
            }
            self.head.len == 13
        }

        pub fn parse(msg: &mut Vec<u8>) -> Option<Self> {
            if msg.len() < 17 {
                return None;
            }
            let req = Request {
                head: Header::parse(&msg[0..5])?,
                index: parse_u32(&msg[5..9]),
                offset: parse_u32(&msg[9..13]),
                plen: parse_u32(&msg[13..17]),
            };

            if req.test() {
                msg.drain(0..17);
                Some(req)
            } else {
                None
            }
        }
    }
    #[derive(Debug, Default, Clone)]
    pub struct Piece {
        pub head: Header,
        pub index: u32,
        pub offset: u32,
        pub data: Vec<u8>,
    }

    impl Piece {
        fn test(&self) -> bool {
            if self.head.byte != PIECE {
                return false;
            }
            self.head.len as usize == (self.data.len() + 9)
        }

        pub fn parse(msg: &mut Vec<u8>) -> Option<Self> {
            let mut piece = Piece {
                head: Header::parse(&msg[0..5])?,
                ..Piece::default()
            };
            if msg.len() < (piece.head.len + 4) as usize {
                return None;
            }
            piece.index = parse_u32(&msg[5..9]);
            piece.offset = parse_u32(&msg[9..13]);
            unsafe {
                piece.data = Vec::new();
                piece.data.reserve((piece.head.len - 9) as usize);
                piece.data.set_len((piece.head.len - 9) as usize);
                let src = msg.as_ptr().add(13);
                let dst = piece.data.as_mut_ptr();
                std::ptr::copy_nonoverlapping(src, dst, (piece.head.len - 9) as usize);
                piece.data.set_len((piece.head.len - 9) as usize);
            }

            if piece.test() {
                let x = msg.len() - (piece.head.len + 4) as usize;
                unsafe {
                    std::ptr::drop_in_place(std::ptr::slice_from_raw_parts_mut(
                        msg.as_mut_ptr(),
                        (piece.head.len + 4) as usize,
                    ));
                    let src = msg.as_ptr().add((piece.head.len + 4) as usize);
                    let dst = msg.as_mut_ptr();
                    std::ptr::copy(src, dst, x);
                    msg.set_len(x);
                }
                Some(piece)
            } else {
                None
            }
        }

        pub fn as_bytes(&self) -> Vec<u8> {
            let mut bytes = vec![];
            bytes.append(&mut self.head.as_bytes());
            bytes.append(&mut u32::to_be_bytes(self.index).to_vec());
            bytes.append(&mut u32::to_be_bytes(self.offset).to_vec());
            bytes.extend_from_slice(&self.data);

            bytes
        }
    }

    #[derive(Serialize, Debug, Default)]
    pub struct Cancel {
        pub head: Header,
        pub index: u32,
        pub offset: u32,
        pub plen: u32,
    }

    impl Cancel {
        fn test(&self) -> bool {
            if self.head.byte != CANCEL {
                return false;
            }
            self.head.len == 13
        }

        pub fn parse(msg: &mut Vec<u8>) -> Option<Self> {
            if msg.len() < 17 {
                return None;
            }
            let cancel = Cancel {
                head: Header::parse(&msg[0..5])?,
                index: parse_u32(&msg[5..9]),
                offset: parse_u32(&msg[9..13]),
                plen: parse_u32(&msg[13..17]),
            };

            if cancel.test() {
                msg.drain(0..17);
                Some(cancel)
            } else {
                None
            }
        }
    }
}

use self::{bytes::*, structs::*};

pub const SUBPIECE_LEN: u32 = 0x4000;
// enum for each type of message
pub enum Message {
    Handshake(Handshake),
    Choke(Header),
    Unchoke(Header),
    Interest(Header),
    Uninterest(Header),
    Have(Have),
    Bitfield(Bitfield),
    Request(Request),
    Piece(Piece),
    Cancel(Cancel),
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Message::Handshake(_) => write!(f, "Handshake"),
            Message::Choke(_) => write!(f, "Choke"),
            Message::Unchoke(_) => write!(f, "Unchoke"),
            Message::Interest(_) => write!(f, "Interest"),
            Message::Uninterest(_) => write!(f, "Uninterest"),
            Message::Have(_) => write!(f, "Have"),
            Message::Bitfield(_) => write!(f, "Bitfield"),
            Message::Request(_) => write!(f, "Request"),
            Message::Piece(_) => write!(f, "Piece"),
            Message::Cancel(_) => write!(f, "Cancel"),
        }
    }
}

fn is_zero(msg: &[u8]) -> bool {
    if msg.is_empty() {
        return false;
    }
    for i in msg.iter() {
        if *i != 0 {
            return false;
        }
    }
    true
}

// parses peer wire messages
pub fn parse_msg(msg: &'static mut Vec<u8>) -> Vec<Message> {
    let mut list: Vec<Message> = vec![];
    loop {
        if is_zero(msg) {
            break;
        }
        let _byte = match msg.get(0) {
            Some(byte) => *byte,
            None => break,
        };
        let byte = match msg.get(4) {
            Some(byte) => *byte,
            None => break,
        };
        match byte {
            CHOKE => list.push(Message::Choke(Header::parse(msg).unwrap())),
            UNCHOKE => list.push(Message::Unchoke(Header::parse(msg).unwrap())),
            INTEREST => list.push(Message::Interest(Header::parse(msg).unwrap())),
            UNINTEREST => list.push(Message::Uninterest(Header::parse(msg).unwrap())),
            HAVE => list.push(Message::Have(Have::parse(msg).unwrap())),
            BITFIELD => list.push(Message::Bitfield(Bitfield::parse(msg).unwrap())),
            REQUEST => list.push(Message::Request(Request::parse(msg).unwrap())),
            PIECE => list.push(Message::Piece(Piece::parse(msg).unwrap())),
            CANCEL => list.push(Message::Cancel(Cancel::parse(msg).unwrap())),
            HANDSHAKE => list.push(Message::Handshake(Handshake::parse(msg).unwrap())),
            _ => {
                // println!("{:?}", msg);
                unreachable!("parse_msg");
            }
        }
    }

    list
}

// returns whether the current message buffer is parseable or not
pub fn try_parse(original: &[u8]) -> bool {
    if original.is_empty() {
        return false;
    }
    let mut msg = original.to_vec();
    loop {
        if is_zero(&msg) {
            return false;
        }
        let _byte = match msg.get(0) {
            Some(byte) => *byte,
            None => return true,
        };
        let byte = match msg.get(4) {
            Some(byte) => *byte,
            None => return false,
        };
        match byte {
            CHOKE | UNCHOKE | INTEREST | UNINTEREST => {
                if Header::parse(&msg).is_none() {
                    return false;
                }
            }
            HAVE => {
                if Have::parse(&mut msg).is_none() {
                    return false;
                }
            }
            BITFIELD => {
                if Bitfield::parse(&mut msg).is_none() {
                    return false;
                }
            }
            REQUEST => {
                if Request::parse(&mut msg).is_none() {
                    return false;
                }
            }
            PIECE => {
                if Piece::parse(&mut msg).is_none() {
                    return false;
                }
            }
            CANCEL => {
                if Cancel::parse(&mut msg).is_none() {
                    return false;
                }
            }
            HANDSHAKE => {
                if Handshake::parse(&mut msg).is_none() {
                    return false;
                }
            }
            _ => return false,
        }
    }
}

pub fn partial_parse(msg: &mut Vec<u8>) -> (bool, Vec<Message>) {
    let mut list = vec![];
    if msg.is_empty() {
        return (false, list);
    }
    loop {
        if is_zero(&msg) {
            return (false, list);
        }
        let _byte = match msg.get(0) {
            Some(byte) => *byte,
            None => return (true, list),
        };
        let byte = match msg.get(4) {
            Some(byte) => *byte,
            None => return (false, list),
        };
        match byte {
            CHOKE => match Header::parse(msg) {
                Some(x) => {
                    msg.drain(0..5);
                    list.push(Message::Choke(x));
                }
                None => return (false, list),
            },
            UNCHOKE => match Header::parse(msg) {
                Some(x) => {
                    msg.drain(0..5);
                    list.push(Message::Unchoke(x));
                }
                None => return (false, list),
            },
            INTEREST => match Header::parse(msg) {
                Some(x) => {
                    msg.drain(0..5);
                    list.push(Message::Interest(x));
                }
                None => return (false, list),
            },
            UNINTEREST => match Header::parse(msg) {
                Some(x) => {
                    msg.drain(0..5);
                    list.push(Message::Uninterest(x));
                }
                None => return (false, list),
            },
            HAVE => match Have::parse(msg) {
                Some(x) => list.push(Message::Have(x)),
                None => return (false, list),
            },
            BITFIELD => match Bitfield::parse(msg) {
                Some(x) => list.push(Message::Bitfield(x)),
                None => return (false, list),
            },
            REQUEST => match Request::parse(msg) {
                Some(x) => list.push(Message::Request(x)),
                None => return (false, list),
            },
            PIECE => match Piece::parse(msg) {
                Some(x) => list.push(Message::Piece(x)),
                None => return (false, list),
            },
            CANCEL => match Cancel::parse(msg) {
                Some(x) => list.push(Message::Cancel(x)),
                None => return (false, list),
            },
            HANDSHAKE => match Handshake::parse(msg) {
                Some(x) => list.push(Message::Handshake(x)),
                None => return (false, list),
            },
            _ => return (false, list),
        }
    }
}
