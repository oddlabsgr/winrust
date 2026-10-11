//! Small icons drawn with simple shapes.

use super::draw::rect;
use winrust_skin::{Image, Rect};

/// A right-pointing triangle `h` pixels tall (odd), `(h + 1) / 2` wide, at (x, y).
pub fn tri_right(img: &mut Image, x: u32, y: u32, h: u32, c: u32) {
    for i in 0..h {
        let w = i.min(h - 1 - i) + 1;
        img.fill_rect(rect(x, y + i, w, 1), c);
    }
}

pub fn tri_left(img: &mut Image, x: u32, y: u32, h: u32, c: u32) {
    let full = h.div_ceil(2);
    for i in 0..h {
        let w = i.min(h - 1 - i) + 1;
        img.fill_rect(rect(x + full - w, y + i, w, 1), c);
    }
}

/// An upward triangle `w` pixels wide (odd), `(w + 1) / 2` tall.
pub fn tri_up(img: &mut Image, x: u32, y: u32, w: u32, c: u32) {
    let h = w.div_ceil(2);
    for i in 0..h {
        img.fill_rect(rect(x + h - 1 - i, y + i, 2 * i + 1, 1), c);
    }
}

pub fn tri_down(img: &mut Image, x: u32, y: u32, w: u32, c: u32) {
    let h = w.div_ceil(2);
    for i in 0..h {
        img.fill_rect(rect(x + i, y + i, w - 2 * i, 1), c);
    }
}

/// Transport icons, drawn centred in an area by [`transport`]; `size` is 7 for buttons and
/// 5 for mini buttons.
#[derive(Clone, Copy)]
pub enum Transport {
    Previous,
    Play,
    Pause,
    Stop,
    Next,
    Eject,
}

pub fn transport(img: &mut Image, kind: Transport, area: Rect, size: u32, c: u32) {
    let Rect { x, y, w, h } = area;
    let tri_w = size.div_ceil(2);
    let (iw, ih) = match kind {
        Transport::Previous | Transport::Next => (1 + 2 * tri_w, size),
        Transport::Play => (tri_w, size),
        Transport::Pause => (if size >= 7 { 6 } else { 4 }, size),
        Transport::Stop => (size, size),
        Transport::Eject => (size, tri_w + 2),
    };
    let ox = x + w.saturating_sub(iw) / 2;
    let oy = y + h.saturating_sub(ih) / 2;
    match kind {
        Transport::Play => tri_right(img, ox, oy, size, c),
        Transport::Previous => {
            img.fill_rect(rect(ox, oy, 1, size), c);
            tri_left(img, ox + 1, oy, size, c);
            tri_left(img, ox + 1 + tri_w, oy, size, c);
        }
        Transport::Next => {
            tri_right(img, ox, oy, size, c);
            tri_right(img, ox + tri_w, oy, size, c);
            img.fill_rect(rect(ox + 2 * tri_w, oy, 1, size), c);
        }
        Transport::Pause => {
            let bar = if size >= 7 { 2 } else { 1 };
            img.fill_rect(rect(ox, oy, bar, size), c);
            img.fill_rect(rect(ox + iw - bar, oy, bar, size), c);
        }
        Transport::Stop => img.fill_rect(rect(ox, oy, size, size), c),
        Transport::Eject => {
            tri_up(img, ox, oy, size, c);
            img.fill_rect(rect(ox, oy + tri_w + 1, size, 1), c);
        }
    }
}

pub const MENU: [&str; 5] = ["#####", ".....", "#####", ".....", "#####"];
pub const MINIMIZE: [&str; 5] = [".....", ".....", ".....", ".....", "#####"];
pub const CLOSE: [&str; 5] = ["#...#", ".#.#.", "..#..", ".#.#.", "#...#"];
pub const SHADE: [&str; 5] = [".....", "#####", "#...#", "#####", "....."];
pub const UNSHADE: [&str; 5] = ["#####", "#...#", "#...#", "#...#", "#####"];
pub const REPEAT: [&str; 7] = [
    "..#.....", ".#######", "..#....#", "#......#", "#....#..", "#######.", ".....#..",
];
