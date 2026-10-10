//! The debug skin: every sprite a flat colour of its own, so a wrong sprite or offset is
//! obvious in golden images. Sprites are painted in table order, so where sprites overlap
//! the later one shows. Each `text.bmp` cell gets a colour too.

use crate::bmp;
use crate::image::{Image, Rect};
use crate::skin::Skin;
use crate::sprites::{SPRITES, Sheet};
use crate::text::{GLYPH_H, GLYPH_W};
use std::io::{Cursor, Write};

/// Sheet pixels no sprite covers.
pub const BACKGROUND: u32 = 0xFF20_2020;

/// Colour of sprite `index` in table order; `text.bmp` cell (row, col) is
/// `SPRITES.len() + row * 31 + col`. Unique for the first 288 indices, and never dark.
pub fn color(index: usize) -> u32 {
    let i = (index % 288) as u32;
    let r = 64 + (i % 6) * 38;
    let g = 64 + (i / 6 % 6) * 38;
    let b = 64 + (i / 36) * 27;
    0xFF00_0000 | r << 16 | g << 8 | b
}

/// The debug skin's sheets in `Sheet::ALL` order.
pub fn sheets() -> Vec<Image> {
    let mut sheets: Vec<Image> = Sheet::ALL
        .iter()
        .map(|s| {
            let (w, h) = s.size();
            Image::new(w, h, BACKGROUND)
        })
        .collect();
    for (i, def) in SPRITES.iter().enumerate() {
        sheets[def.sheet as usize].fill_rect(def.rect, color(i));
    }
    let text = &mut sheets[Sheet::Text as usize];
    for row in 0..3 {
        for col in 0..31 {
            let cell = Rect::new(col * GLYPH_W, row * GLYPH_H, GLYPH_W, GLYPH_H);
            text.fill_rect(cell, color(SPRITES.len() + (row * 31 + col) as usize));
        }
    }
    sheets
}

impl Skin {
    pub fn debug() -> Skin {
        Skin::complete(sheets())
    }
}

/// The debug skin as a `.wsz`, with upper-case BMP names inside a folder (as many real
/// skins are packed).
pub fn wsz() -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (sheet, image) in Sheet::ALL.iter().zip(sheets()) {
        let name = format!("Debug Skin/{}.BMP", sheet.stem().to_ascii_uppercase());
        zip.start_file(name, options).expect("writing to memory");
        zip.write_all(&bmp::encode24(&image))
            .expect("writing to memory");
    }
    zip.finish().expect("writing to memory");
    out.into_inner()
}
