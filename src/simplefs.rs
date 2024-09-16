use core::convert::TryInto;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

pub fn unpack(fs: Vec<u8>) -> Vec<(String, Vec<u8>)> {
    let mut files = vec![];
    let mut cursor: usize = 0;
    loop {
        let mut filename = String::new();
        loop {
            if cursor >= fs.len() || fs[cursor] == 0 {
                cursor += 1;
                break;
            } else {
                filename.push(fs[cursor] as char);
                cursor += 1;
            }
        }
        if cursor + 4 > fs.len() {
            break;
        }
        let file_len = u32::from_le_bytes(fs[cursor..cursor + 4].try_into().unwrap()) as usize;
        cursor += 4;
        if cursor + file_len > fs.len() {
            break;
        }
        let file_contents = fs[cursor..cursor + file_len].to_vec();
        cursor += file_len;
        files.push((filename, file_contents));
        if cursor >= fs.len() || fs[cursor] == 0 {
            break;
        }
    }
    files
}

pub fn pack(files: Vec<(String, Vec<u8>)>) -> Vec<u8> {
    let mut fs = vec![];

    for (filename, contents) in files {
        fs.extend_from_slice(filename.as_bytes());
        fs.push(0);
        let len = contents.len() as u32;
        fs.extend_from_slice(&len.to_le_bytes());
        fs.extend_from_slice(&contents);
    }
    fs
}
