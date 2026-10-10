//! Glyph cells of `text.bmp`: 5×6 pixels, 31 per row, three rows (Webamp's `FONT_LOOKUP`).

use crate::image::Rect;

pub const GLYPH_W: u32 = 5;
pub const GLYPH_H: u32 = 6;

/// The `text.bmp` cell for `c`. Letters ignore case; Å, Ö and Ä have their own glyphs;
/// other accented Latin letters use the plain letter; anything else is a space.
pub fn glyph(c: char) -> Rect {
    let (row, col) = cell(c).unwrap_or((0, 30));
    Rect::new(col * GLYPH_W, row * GLYPH_H, GLYPH_W, GLYPH_H)
}

fn cell(c: char) -> Option<(u32, u32)> {
    let c = deaccent(c).to_ascii_lowercase();
    Some(match c {
        'a'..='z' => (0, c as u32 - 'a' as u32),
        '"' => (0, 26),
        '@' => (0, 27),
        ' ' => (0, 30),
        '0'..='9' => (1, c as u32 - '0' as u32),
        '…' => (1, 10),
        '.' => (1, 11),
        ':' => (1, 12),
        '(' => (1, 13),
        ')' => (1, 14),
        '-' => (1, 15),
        '\'' => (1, 16),
        '!' => (1, 17),
        '_' => (1, 18),
        '+' => (1, 19),
        '\\' => (1, 20),
        '/' => (1, 21),
        '[' | '<' | '{' => (1, 22),
        ']' | '>' | '}' => (1, 23),
        '^' => (1, 24),
        '&' => (1, 25),
        '%' => (1, 26),
        ',' => (1, 27),
        '=' => (1, 28),
        '$' => (1, 29),
        '#' => (1, 30),
        'å' | 'Å' => (2, 0),
        'ö' | 'Ö' => (2, 1),
        'ä' | 'Ä' => (2, 2),
        '?' => (2, 3),
        '*' => (2, 4),
        _ => return None,
    })
}

/// Latin-1 letters without their accent, except Å, Ö and Ä (which have glyphs).
fn deaccent(c: char) -> char {
    match c {
        'À'..='Ã' | 'à'..='ã' => 'a',
        'Ç' | 'ç' => 'c',
        'È'..='Ë' | 'è'..='ë' => 'e',
        'Ì'..='Ï' | 'ì'..='ï' => 'i',
        'Ñ' | 'ñ' => 'n',
        'Ò'..='Õ' | 'ò'..='õ' | 'Ø' | 'ø' => 'o',
        'Ù'..='Ü' | 'ù'..='ü' => 'u',
        'Ý' | 'ý' | 'ÿ' => 'y',
        'ß' => 's',
        other => other,
    }
}
