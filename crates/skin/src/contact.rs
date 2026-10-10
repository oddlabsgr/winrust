//! Contact sheet: every sprite of a skin in table order, then the `text.bmp` sheet, for
//! golden tests and for checking a real skin by eye.

use crate::image::{Image, Rect};
use crate::skin::Skin;
use crate::sprites::{SPRITES, Sheet};

const WIDTH: u32 = 640;
const GAP: u32 = 2;
/// Fills the space between sprites.
pub const BACKDROP: u32 = 0xFFFF_00FF;

pub fn contact_sheet(skin: &Skin) -> Image {
    let mut tiles: Vec<(&Image, Rect)> = SPRITES
        .iter()
        .map(|def| {
            let s = skin.sprite(def.id);
            (s.image, s.rect)
        })
        .collect();
    let (tw, th) = Sheet::Text.size();
    tiles.push((skin.sheet(Sheet::Text), Rect::new(0, 0, tw, th)));

    let mut places = Vec::with_capacity(tiles.len());
    let (mut x, mut y, mut row_height) = (GAP, GAP, 0);
    for (_, r) in &tiles {
        if x > GAP && x + r.w + GAP > WIDTH {
            x = GAP;
            y += row_height + GAP;
            row_height = 0;
        }
        places.push((x, y));
        x += r.w + GAP;
        row_height = row_height.max(r.h);
    }
    let mut out = Image::new(WIDTH, y + row_height + GAP, BACKDROP);
    for ((image, rect), (x, y)) in tiles.iter().zip(places) {
        out.blit(image, *rect, x, y);
    }
    out
}
