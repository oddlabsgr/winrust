#![allow(dead_code)]

use std::io::{Cursor, Write};
use winrust_skin::{Image, bmp};

pub fn solid_bmp(w: u32, h: u32, color: u32) -> Vec<u8> {
    bmp::encode24(&Image::new(w, h, color))
}

/// A deflated zip of (name, contents) entries, in order.
pub fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
    out.into_inner()
}
