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
fn parse_u32(msg: &mut Vec<u8>) -> u32 {
    let mut bytes: [u8; 4] = [0; 4];
    bytes.copy_from_slice(&msg[0..4]);
    msg.drain(0..4);
    u32::from_be_bytes(bytes)
}

// structs for each type of message
pub mod structs {
    use super::{bytes::*, parse_u32};

    use serde::{Deserialize, Serialize};
    #[derive(Serialize, Deserialize, Debug)]
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
            msg.drain(0..1);

            handshake.protocol.clone_from_slice(&msg[..19]);
            msg.drain(0..19);

            handshake.reserved.clone_from_slice(&msg[..8]);
            msg.drain(0..8);

            handshake.info_hash.clone_from_slice(&msg[..20]);
            msg.drain(0..20);

            handshake.peer_id.clone_from_slice(&msg[..20]);
            msg.drain(0..20);

            if handshake.test() {
                Some(handshake)
            } else {
                None
            }
        }
    }

    #[derive(Serialize, Deserialize, Debug, Default, Clone)]
    pub struct Header {
        pub len: u32,
        pub byte: u8,
    }

    impl Header {
        fn test(&self) -> bool {
            self.byte <= CANCEL || self.byte == HANDSHAKE
        }

        pub fn parse(msg: &mut Vec<u8>) -> Option<Self> {
            if msg.len() < 5 {
                return None;
            }
            let head = Header {
                len: parse_u32(msg),
                byte: msg[0],
            };
            msg.drain(0..1);

            if head.test() {
                Some(head)
            } else {
                None
            }
        }

        pub fn as_bytes(&self) -> Vec<u8> {
            let mut bytes = vec![];
            bytes.append(&mut u32::to_ne_bytes(self.len).to_vec());
            bytes.push(self.byte);
            bytes
        }
    }

    #[derive(Serialize, Deserialize, Debug, Default)]
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
                index: parse_u32(msg),
            };

            if have.test() {
                Some(have)
            } else {
                None
            }
        }
    }

    #[derive(Serialize, Deserialize, Debug, Default)]
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
            if msg.len() < (bitfield.head.len - 1) as usize {
                return None;
            }

            bitfield
                .data
                .append(&mut msg[..((bitfield.head.len - 1) as usize)].to_vec());
            let mut copy = msg[((bitfield.head.len - 1) as usize)..].to_vec();
            msg.clear();
            msg.append(&mut copy);

            if bitfield.test() {
                Some(bitfield)
            } else {
                None
            }
        }
    }

    #[derive(Serialize, Deserialize, Debug, Default)]
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
                head: Header::parse(msg)?,
                index: parse_u32(msg),
                offset: parse_u32(msg),
                plen: parse_u32(msg),
            };

            if req.test() {
                Some(req)
            } else {
                None
            }
        }
    }
    #[derive(Serialize, Deserialize, Debug, Default, Clone)]
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
                head: Header::parse(msg)?,
                ..Piece::default()
            };
            if msg.len() < (piece.head.len - 1) as usize {
                return None;
            }
            piece.index = parse_u32(msg);
            piece.offset = parse_u32(msg);

            piece
                .data
                .append(&mut msg[0..((piece.head.len - 9) as usize)].to_vec());
            let mut copy = msg[((piece.head.len - 9) as usize)..].to_vec();
            msg.clear();
            msg.append(&mut copy);

            if piece.test() {
                Some(piece)
            } else {
                None
            }
        }

        pub fn as_bytes(&self) -> Vec<u8> {
            let mut bytes = vec![];
            bytes.append(&mut self.head.as_bytes());
            bytes.append(&mut u32::to_ne_bytes(self.index).to_vec());
            bytes.append(&mut u32::to_ne_bytes(self.offset).to_vec());
            bytes.extend_from_slice(&self.data);
            bytes
        }
    }

    #[derive(Serialize, Deserialize, Debug, Default)]
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
                head: Header::parse(msg)?,
                index: parse_u32(msg),
                offset: parse_u32(msg),
                plen: parse_u32(msg),
            };

            if cancel.test() {
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
pub fn parse_msg(msg: &mut Vec<u8>) -> Vec<Message> {
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
                if Header::parse(&mut msg).is_none() {
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
