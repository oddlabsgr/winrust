//! `viscolor.txt`: the 24 visualization colours.

/// Used for entries a skin leaves out (the classic green-to-red analyzer palette).
pub const DEFAULT_COLORS: [u32; 24] = [
    0xFF00_0000,
    0xFF18_2129,
    0xFFEF_3110,
    0xFFCE_2910,
    0xFFD6_5A00,
    0xFFD6_6600,
    0xFFD6_7300,
    0xFFC6_7B08,
    0xFFDE_A518,
    0xFFD6_B521,
    0xFFBD_DE29,
    0xFF94_DE21,
    0xFF29_CE10,
    0xFF32_BE10,
    0xFF39_B510,
    0xFF31_9C08,
    0xFF29_9400,
    0xFF18_8408,
    0xFFFF_FFFF,
    0xFFD6_D6DE,
    0xFFB5_BDBD,
    0xFFA0_AAAF,
    0xFF94_9CA5,
    0xFF96_9696,
];

/// Each line that starts with three numbers (comma and/or blank separated) gives the next
/// colour; other lines are skipped without using up an entry. Values above 255 clamp.
pub fn parse(text: &str) -> [u32; 24] {
    let mut colors = DEFAULT_COLORS;
    let parsed = text.split('\n').filter_map(line_rgb);
    for (slot, rgb) in colors.iter_mut().zip(parsed) {
        *slot = rgb;
    }
    colors
}

/// Matches Webamp's `/^\s*(\d+)\s*,?\s*(\d+)\s*,?\s*(\d+)/`, including how the regex
/// splits run-together digits ("12,34" reads as 12, 3, 4).
fn line_rgb(line: &str) -> Option<u32> {
    let b = line.as_bytes();
    let digits = |from: usize| {
        b[from.min(b.len())..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count()
    };
    let separator = |mut i: usize| {
        while b.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        if b.get(i) == Some(&b',') {
            i += 1;
        }
        while b.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        i
    };
    let start = b.iter().take_while(|c| c.is_ascii_whitespace()).count();
    let n1 = digits(start);
    for l1 in (1..=n1).rev() {
        let s2 = if l1 < n1 {
            start + l1
        } else {
            separator(start + l1)
        };
        let n2 = digits(s2);
        for l2 in (1..=n2).rev() {
            let s3 = if l2 < n2 { s2 + l2 } else { separator(s2 + l2) };
            let n3 = digits(s3);
            if n3 > 0 {
                let num = |from: usize, len: usize| {
                    line[from..from + len]
                        .bytes()
                        .fold(0u32, |acc, d| {
                            acc.saturating_mul(10).saturating_add(u32::from(d - b'0'))
                        })
                        .min(255)
                };
                let (r, g, bl) = (num(start, l1), num(s2, l2), num(s3, n3));
                return Some(0xFF00_0000 | r << 16 | g << 8 | bl);
            }
        }
    }
    None
}
