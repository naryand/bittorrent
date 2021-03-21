use std::collections::BTreeMap;

#[derive(Debug)]
pub enum Item {
    Int(i32),
    String(String),
    List(Vec<Item>),
    Dict(BTreeMap<String, Item>),
}

fn parse_int(str: &mut Vec<char>) -> i32 {
    let mut len: usize = 0;
    let mut int_string: String = String::new();
    for c in str.iter() {
        len += 1;
        if *c == 'i' { continue }
        else if *c == 'e' { break }
        int_string.push(*c);
    }
    str.drain(0..len);
    return int_string.parse::<i32>().unwrap();
}

fn parse_str(str: &mut Vec<char>) -> String {
    let mut int_len: usize = 0;
    let mut int_string: String = String::new();
    for c in str.iter() {
        int_len += 1;
        if *c == ':' { break }
        int_string.push(*c);
    }
    let len: usize = int_string.parse::<usize>().unwrap();
    str.drain(0..int_len);

    let mut s: String = String::new();
    for i in 0..len {
        s.push(str[i]);
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

fn parse_dict(str: &mut Vec<char>) -> BTreeMap<String, Item> {
    str.drain(0..1);
    let mut dict: BTreeMap<String, Item> = BTreeMap::<String, Item>::new();
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