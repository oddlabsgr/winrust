//! Drawing helpers on top of `winrust_skin::Image`.

use super::font;
use super::palette::*;
use winrust_skin::{Image, Rect};

pub fn rect(x: u32, y: u32, w: u32, h: u32) -> Rect {
    Rect::new(x, y, w, h)
}

/// Linear blend of two colours, `t` in 0..=1.
pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    let ch = |shift: u32| {
        let ca = ((a >> shift) & 0xFF) as f32;
        let cb = ((b >> shift) & 0xFF) as f32;
        ((ca + (cb - ca) * t).round() as u32) << shift
    };
    0xFF00_0000 | ch(16) | ch(8) | ch(0)
}

pub fn hline(img: &mut Image, x: u32, y: u32, w: u32, c: u32) {
    img.fill_rect(rect(x, y, w, 1), c);
}

pub fn vline(img: &mut Image, x: u32, y: u32, h: u32, c: u32) {
    img.fill_rect(rect(x, y, 1, h), c);
}

/// Vertical gradient from `top` to `bottom`.
pub fn vgradient(img: &mut Image, r: Rect, top: u32, bottom: u32) {
    for dy in 0..r.h {
        let t = if r.h > 1 {
            dy as f32 / (r.h - 1) as f32
        } else {
            0.0
        };
        hline(img, r.x, r.y + dy, r.w, mix(top, bottom, t));
    }
}

/// One-pixel frame: `tl` on the top and left edges, `br` on the bottom and right.
pub fn frame(img: &mut Image, r: Rect, tl: u32, br: u32) {
    if r.w == 0 || r.h == 0 {
        return;
    }
    hline(img, r.x, r.y, r.w, tl);
    vline(img, r.x, r.y, r.h, tl);
    hline(img, r.x, r.y + r.h - 1, r.w, br);
    vline(img, r.x + r.w - 1, r.y, r.h, br);
}

pub fn inset(r: Rect, by: u32) -> Rect {
    rect(
        r.x + by,
        r.y + by,
        r.w.saturating_sub(2 * by),
        r.h.saturating_sub(2 * by),
    )
}

/// A raised metal surface: gradient face with a light top-left and dark bottom-right edge.
pub fn raised(img: &mut Image, r: Rect) {
    vgradient(img, r, FACE_TOP, FACE_BOTTOM);
    frame(img, r, BEVEL_LIGHT, BEVEL_DARK);
}

/// The same surface pushed in.
pub fn pressed(img: &mut Image, r: Rect) {
    vgradient(img, r, FACE_PRESSED_TOP, FACE_PRESSED_BOTTOM);
    frame(img, r, BEVEL_DARK, BEVEL_LIGHT);
}

/// A recessed black display with a dark outer edge.
pub fn lcd(img: &mut Image, r: Rect) {
    img.fill_rect(r, LCD_BG);
    frame(img, r, SHADOW, BEVEL_LIGHT);
}

/// A sunken groove (slider track).
pub fn groove(img: &mut Image, r: Rect) {
    img.fill_rect(r, GROOVE);
    frame(img, r, SHADOW, BEVEL_LIGHT);
}

/// Text in the 4×5 font; returns the width drawn.
pub fn text(img: &mut Image, x: u32, y: u32, s: &str, c: u32) -> u32 {
    let mut cx = x;
    for ch in s.chars() {
        let g = font::glyph(ch);
        for (row, bits) in g.rows.iter().enumerate() {
            for (col, b) in bits.bytes().enumerate() {
                if b == b'#' {
                    img.set(cx + col as u32, y + row as u32, c);
                }
            }
        }
        cx += g.width + 1;
    }
    cx.saturating_sub(x + 1)
}

pub fn text_width(s: &str) -> u32 {
    let w: u32 = s.chars().map(|c| font::glyph(c).width + 1).sum();
    w.saturating_sub(1)
}

/// Text centred horizontally in `r`, vertically at `r.y + (r.h - 5) / 2`.
pub fn text_centered(img: &mut Image, r: Rect, s: &str, c: u32) {
    let w = text_width(s);
    let x = r.x + r.w.saturating_sub(w) / 2;
    let y = r.y + r.h.saturating_sub(5) / 2;
    text(img, x, y, s, c);
}

/// Text with a one-pixel dark shadow below-right, for labels on metal.
pub fn label(img: &mut Image, x: u32, y: u32, s: &str, c: u32) {
    text(img, x + 1, y + 1, s, SHADOW);
    text(img, x, y, s, c);
}

pub fn label_centered(img: &mut Image, r: Rect, s: &str, c: u32) {
    let w = text_width(s);
    let x = r.x + r.w.saturating_sub(w) / 2;
    let y = r.y + r.h.saturating_sub(5) / 2;
    label(img, x, y, s, c);
}

/// Draws a small bitmap given as rows of `#` (colour `c`) and `.` (left alone).
pub fn bitmap(img: &mut Image, x: u32, y: u32, rows: &[&str], c: u32) {
    for (dy, row) in rows.iter().enumerate() {
        for (dx, b) in row.bytes().enumerate() {
            if b == b'#' {
                img.set(x + dx as u32, y + dy as u32, c);
            }
        }
    }
}

/// `bitmap` centred in `r`.
pub fn bitmap_centered(img: &mut Image, r: Rect, rows: &[&str], c: u32) {
    let w = rows.iter().map(|s| s.len()).max().unwrap_or(0) as u32;
    let h = rows.len() as u32;
    bitmap(
        img,
        r.x + r.w.saturating_sub(w) / 2,
        r.y + r.h.saturating_sub(h) / 2,
        rows,
        c,
    );
}
