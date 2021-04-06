use std::collections::BTreeMap;

#[derive(Debug, Clone)]
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

fn parse_int(str: &mut Vec<char>) -> i64 {
    let mut len: usize = 0;
    let mut int_string: String = String::new();
    for c in str.iter() {
        len += 1;
        if *c == 'i' { continue }
        else if *c == 'e' { break }
        int_string.push(*c);
    }
    str.drain(0..len);
    return int_string.parse::<i64>().unwrap();
}

fn parse_str(str: &mut Vec<char>) -> Vec<u8> {
    let mut int_len: usize = 0;
    let mut int_string: String = String::new();
    for c in str.iter() {
        int_len += 1;
        if *c == ':' { break }
        int_string.push(*c);
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

fn parse_list(str: &mut Vec<char>) -> Vec<Item> {
    str.drain(0..1);
    let mut list: Vec<Item> = Vec::<Item>::new();
    loop {
        match str.iter().nth(0).unwrap() {
            'i' => list.push(Item::Int(parse_int(str))),
            'l' => list.push(Item::List(parse_list(str))),
            'd' => list.push(Item::Dict(parse_dict(str))),
            'e' => break,
            _ => list.push(Item::String(parse_str(str))),
        }
    }
    str.drain(0..1);
    return list;
}

fn parse_dict(str: &mut Vec<char>) -> BTreeMap<Vec<u8>, Item> {
    str.drain(0..1);
    let mut dict: BTreeMap<Vec<u8>, Item> = BTreeMap::new();
    loop {
        if *str.iter().nth(0).unwrap() == 'e' { break }
        let s = parse_str(str);
        match str.iter().nth(0).unwrap() {
            'i' => dict.insert(s, Item::Int(parse_int(str))),
            'l' => dict.insert(s, Item::List(parse_list(str))),
            'd' => dict.insert(s, Item::Dict(parse_dict(str))),
            _ => dict.insert(s, Item::String(parse_str(str))),
        };
    }
    str.drain(0..1);
    return dict;
}

pub fn parse(str: &mut Vec<char>) -> Vec<Item> {
    let mut tree: Vec<Item> = Vec::<Item>::new();
    loop {
        let c: char = match str.iter().nth(0) {
            Some(c) => *c,
            None => break,
        };
        match c {
            'i' => tree.push(Item::Int(parse_int(str))),
            'l' => tree.push(Item::List(parse_list(str))),
            'd' => tree.push(Item::Dict(parse_dict(str))),
            '\0' => break,
            _ => tree.push(Item::String(parse_str(str))),
        }
    }
    return tree;
}