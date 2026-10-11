//! One drawing function per sheet. Sprites are drawn at their rectangles from the sprite
//! table, so the art always lines up with what the loader reads.

pub mod lcd;

use super::palette::*;
use winrust_skin::{Image, Rect, Sheet, SpriteId};

pub fn r(id: SpriteId) -> Rect {
    id.def().rect
}

pub fn canvas(sheet: Sheet) -> Image {
    let (w, h) = sheet.size();
    Image::new(w, h, SHADOW)
}
