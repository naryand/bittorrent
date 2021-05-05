#![allow(dead_code)]

use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub enum Item {
    Int(i64),
    String(Vec<u8>),
    List(Vec<Item>),
    Dict(BTreeMap<Vec<u8>, Item>),
}


impl Item {
    #[allow(dead_code)]
    pub fn get_int(&self) -> i64 {
        let int = match &self {
            Item::Int(int) => int,
            _ => unreachable!(),
        };
        return int.clone();
    }
    #[allow(dead_code)]
    pub fn get_str(&self) -> Vec<u8> {
        let str = match &self {
            Item::String(str) => str,
            _ => unreachable!(),
        };
        return str.clone();
    }
    #[allow(dead_code)]
    pub fn get_list(&self) -> Vec<Item> {
        let list = match &self {
            Item::List(list) => list,
            _ => unreachable!(),
        };
        return list.clone();
    }
    #[allow(dead_code)]
    pub fn get_dict(&self) -> BTreeMap<Vec<u8>, Item> {
        let dict = match &self {
            Item::Dict(dict) => dict,
            _ => unreachable!(),
        };
        return dict.clone();
    }
}

fn parse_int(str: &mut Vec<u8>) -> i64 {
    let mut len: usize = 0;
    let mut int_string: String = String::new();
    for c in str.iter() {
        len += 1;
        if *c == 'i' as u8 { continue }
        else if *c == 'e' as u8 { break }
        int_string.push(*c as char);
    }
    str.drain(0..len);
    return int_string.parse::<i64>().unwrap();
}

fn parse_str(str: &mut Vec<u8>) -> Vec<u8> {
    let mut int_len: usize = 0;
    let mut int_string: String = String::new();
    for c in str.iter() {
        int_len += 1;
        if *c == ':' as u8 { break }
        int_string.push(*c as char);
    }
    let len: usize = int_string.parse::<usize>().unwrap();
    str.drain(0..int_len);

    let mut s: Vec<u8> = Vec::new();
    for i in 0..len {
        s.push(str[i] as u8);
    }
    str.drain(0..len);
    return s;
}

fn parse_list(str: &mut Vec<u8>) -> Vec<Item> {
    str.drain(0..1);
    let mut list: Vec<Item> = Vec::<Item>::new();
    loop {
        match *str.iter().nth(0).unwrap() as char {
            'i' => list.push(Item::Int(parse_int(str))),
            'l' => list.push(Item::List(parse_list(str))),
            'd' => list.push(Item::Dict(parse_dict(str))),
            '0'..='9' => list.push(Item::String(parse_str(str))),
            'e' => break,
            _ => unreachable!(),
        }
    }
    str.drain(0..1);
    return list;
}

fn parse_dict(str: &mut Vec<u8>) -> BTreeMap<Vec<u8>, Item> {
    str.drain(0..1);
    let mut dict: BTreeMap<Vec<u8>, Item> = BTreeMap::new();
    loop {
        if *str.iter().nth(0).unwrap() == 'e' as u8 { break }
        let s = parse_str(str);
        match *str.iter().nth(0).unwrap() as char {
            'i' => dict.insert(s, Item::Int(parse_int(str))),
            'l' => dict.insert(s, Item::List(parse_list(str))),
            'd' => dict.insert(s, Item::Dict(parse_dict(str))),
            '0'..='9' => dict.insert(s, Item::String(parse_str(str))),
            _ => unreachable!(),
        };
    }
    str.drain(0..1);
    return dict;
}

pub fn parse(str: &mut Vec<u8>) -> Vec<Item> {
    let mut tree: Vec<Item> = Vec::<Item>::new();
    loop {
        let c: u8 = match str.iter().nth(0) {
            Some(c) => *c,
            None => break,
        };
        match c as char {
            'i' => tree.push(Item::Int(parse_int(str))),
            'l' => tree.push(Item::List(parse_list(str))),
            'd' => tree.push(Item::Dict(parse_dict(str))),
            '0'..='9' => tree.push(Item::String(parse_str(str))),
            _ => break,
        }
    }
    return tree;
}