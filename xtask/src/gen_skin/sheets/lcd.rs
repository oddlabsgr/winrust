//! LCD sheets: digits, the text font, mono/stereo and play-state indicators.

use super::*;
use crate::gen_skin::draw::*;
use crate::gen_skin::palette::*;
use crate::gen_skin::{
    font,
    icons::{self, Transport},
};
use winrust_skin::SpriteId as S;
use winrust_skin::text as glyphs;

/// Seven-segment digit in a 9×13 cell: 2-pixel bars and strokes, 1-pixel middle bar on
/// row 6 (Winamp reads the minus sign from that row of the `2`).
fn digit(img: &mut Image, x: u32, y: u32, d: Option<u8>, c: u32) {
    // Segments: a top, b upper right, c lower right, d bottom, e lower left, f upper left, g middle.
    let on: [bool; 7] = match d {
        Some(0) => [true, true, true, true, true, true, false],
        Some(1) => [false, true, true, false, false, false, false],
        Some(2) => [true, true, false, true, true, false, true],
        Some(3) => [true, true, true, true, false, false, true],
        Some(4) => [false, true, true, false, false, true, true],
        Some(5) => [true, false, true, true, false, true, true],
        Some(6) => [true, false, true, true, true, true, true],
        Some(7) => [true, true, true, false, false, false, false],
        Some(8) => [true; 7],
        Some(9) => [true, true, true, true, false, true, true],
        Some(_) => [false; 7],
        None => [false, false, false, false, false, false, true],
    };
    let seg = [
        rect(2, 0, 5, 2),
        rect(7, 2, 2, 4),
        rect(7, 7, 2, 4),
        rect(2, 11, 5, 2),
        rect(0, 7, 2, 4),
        rect(0, 2, 2, 4),
        rect(2, 6, 5, 1),
    ];
    for (lit, s) in on.iter().zip(seg) {
        if *lit {
            img.fill_rect(rect(x + s.x, y + s.y, s.w, s.h), c);
        }
    }
}

pub fn numbers() -> Image {
    let mut img = canvas(Sheet::Numbers);
    img.fill_rect(rect(0, 0, img.width, img.height), LCD_BG);
    for d in 0..10u8 {
        digit(&mut img, u32::from(d) * 9, 0, Some(d), LCD_ON);
    }
    img
}

pub fn nums_ex() -> Image {
    let mut img = canvas(Sheet::NumsEx);
    img.fill_rect(rect(0, 0, img.width, img.height), LCD_BG);
    for d in 0..10u8 {
        digit(&mut img, u32::from(d) * 9, 0, Some(d), LCD_ON);
    }
    let minus = r(S::MinusSignEx);
    digit(&mut img, minus.x, minus.y, None, LCD_ON);
    img
}

/// One character per `text.bmp` cell (the space cell stays empty).
pub const TEXT_CHARSET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ\"@0123456789….:()-'!_+\\/[]^&%,=$#ÅÖÄ?*";

/// Every character the `text.bmp` map knows, drawn into its cell.
pub fn text() -> Image {
    let mut img = canvas(Sheet::Text);
    img.fill_rect(rect(0, 0, img.width, img.height), LCD_BG);
    for c in TEXT_CHARSET.chars() {
        let cell = glyphs::glyph(c);
        let g = font::glyph(c);
        // Narrow glyphs sit in the middle of the 4-pixel letter width.
        let offset = if g.width <= 2 { (4 - g.width) / 2 } else { 0 };
        bitmap(&mut img, cell.x + offset, cell.y, &g.rows, LCD_ON);
    }
    img
}

pub fn monoster() -> Image {
    let mut img = canvas(Sheet::Monoster);
    let cells = [
        (S::MainStereoSelected, "STEREO", LCD_ON),
        (S::MainStereo, "STEREO", LCD_DIM),
        (S::MainMonoSelected, "MONO", LCD_ON),
        (S::MainMono, "MONO", LCD_DIM),
    ];
    for (id, word, c) in cells {
        let b = r(id);
        lcd(&mut img, b);
        // In the window MONO sits left of STEREO: one panel, bevelled outside only.
        let mono = matches!(id, S::MainMono | S::MainMonoSelected);
        let inner = if mono { b.x + b.w - 1 } else { b.x };
        vline(&mut img, inner, b.y + 1, b.h - 2, LCD_BG);
        text_centered(&mut img, b, word, c);
    }
    img
}

pub fn playpaus() -> Image {
    let mut img = canvas(Sheet::Playpaus);
    img.fill_rect(rect(0, 0, img.width, img.height), LCD_BG);
    let play = r(S::MainPlayingIndicator);
    icons::tri_right(&mut img, play.x + 2, play.y + 1, 7, LCD_ON);
    let pause = r(S::MainPausedIndicator);
    icons::transport(&mut img, Transport::Pause, pause, 7, COPPER);
    let stop = r(S::MainStoppedIndicator);
    img.fill_rect(rect(stop.x + 2, stop.y + 2, 5, 5), INK_DIM);
    // Only the first 3 columns of the work indicators show: "not working" beside the play
    // triangle during playback, "working" while a stream buffers.
    let idle = r(S::MainNotWorkingIndicator);
    img.fill_rect(rect(idle.x, idle.y + 1, 2, 7), LCD_ON);
    let busy = r(S::MainWorkingIndicator);
    img.fill_rect(rect(busy.x, busy.y + 1, 2, 7), RUST);
    img
}
