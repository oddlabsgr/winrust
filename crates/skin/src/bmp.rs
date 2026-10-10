//! Tolerant BMP decoder for skin bitmaps, plus a 24-bit encoder.
//!
//! Skins carry BMPs written by many old tools, often with wrong header fields. Rules:
//! - The file-size field is ignored. A pixel-data offset that is zero, points into the
//!   headers, or points past the end of the file is replaced by the offset right after the
//!   palette (or bitfield masks).
//! - Info headers of 12 bytes (OS/2 1.x), 16 and 64 (OS/2 2.x), 40, 52, 56, 108 and 124.
//! - 1/4/8-bit: a colour count of 0 (or more than 2^bpp) means 2^bpp entries. Entries
//!   missing from the file and indices past the palette are black.
//! - 16-bit is 5-5-5 unless BI_BITFIELDS gives masks; 32-bit ignores alpha unless masks
//!   are given. Channels scale to 8 bits with rounding (31 → 255).
//! - RLE8 and RLE4 with end-of-line, end-of-bitmap and delta codes.
//! - Negative height means top-down rows. Rows are padded to 4 bytes.
//! - Pixels the file does not provide (truncated data, skipped RLE pixels) are black.
//! - Width or height of 0 or above [`MAX_SIDE`] is an error.
//!
//! Every decoded pixel is opaque.

use crate::image::{BLACK, Image};

/// Largest accepted width or height. Classic skin bitmaps are far smaller.
pub const MAX_SIDE: u32 = 2048;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BmpError {
    #[error("not a BMP file")]
    NotBmp,
    #[error("truncated BMP header")]
    Truncated,
    #[error("unsupported BMP: {0}")]
    Unsupported(String),
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

struct Header {
    width: u32,
    height: u32,
    top_down: bool,
    bpp: u16,
    compression: u32,
    palette: Vec<u32>,
    masks: Option<[u32; 3]>,
    data_start: usize,
}

const BI_RGB: u32 = 0;
const BI_RLE8: u32 = 1;
const BI_RLE4: u32 = 2;
const BI_BITFIELDS: u32 = 3;

fn parse_header(data: &[u8]) -> Result<Header, BmpError> {
    if data.get(..2) != Some(b"BM") {
        return Err(BmpError::NotBmp);
    }
    let offset = u32_at(data, 10).ok_or(BmpError::Truncated)? as usize;
    let size = u32_at(data, 14).ok_or(BmpError::Truncated)? as usize;
    let (width, height, bpp, compression, colors_used, entry) = match size {
        12 => {
            let w = u16_at(data, 18).ok_or(BmpError::Truncated)?;
            let h = u16_at(data, 20).ok_or(BmpError::Truncated)? as i16;
            let bpp = u16_at(data, 24).ok_or(BmpError::Truncated)?;
            (i64::from(w), i64::from(h), bpp, BI_RGB, 0, 3)
        }
        16 | 40 | 52 | 56 | 64 | 108 | 124 => {
            let w = u32_at(data, 18).ok_or(BmpError::Truncated)? as i32;
            let h = u32_at(data, 22).ok_or(BmpError::Truncated)? as i32;
            let bpp = u16_at(data, 28).ok_or(BmpError::Truncated)?;
            let (compression, colors) = if size >= 40 {
                (
                    u32_at(data, 30).ok_or(BmpError::Truncated)?,
                    u32_at(data, 46).ok_or(BmpError::Truncated)?,
                )
            } else {
                (BI_RGB, 0)
            };
            (i64::from(w), i64::from(h), bpp, compression, colors, 4)
        }
        other => return Err(BmpError::Unsupported(format!("header size {other}"))),
    };
    if width <= 0
        || width > i64::from(MAX_SIDE)
        || height == 0
        || height.abs() > i64::from(MAX_SIDE)
    {
        return Err(BmpError::Unsupported(format!("size {width}x{height}")));
    }
    let supported = match bpp {
        1 => compression == BI_RGB,
        4 => matches!(compression, BI_RGB | BI_RLE4),
        8 => matches!(compression, BI_RGB | BI_RLE8),
        16 | 32 => matches!(compression, BI_RGB | BI_BITFIELDS),
        24 => compression == BI_RGB,
        _ => false,
    };
    if !supported {
        return Err(BmpError::Unsupported(format!(
            "{bpp} bits per pixel with compression {compression}"
        )));
    }

    let mut cursor = 14 + size;
    let masks = if compression == BI_BITFIELDS {
        let at = 14 + 40;
        let m = [
            u32_at(data, at).ok_or(BmpError::Truncated)?,
            u32_at(data, at + 4).ok_or(BmpError::Truncated)?,
            u32_at(data, at + 8).ok_or(BmpError::Truncated)?,
        ];
        if size == 40 {
            cursor += 12;
        }
        Some(m)
    } else {
        None
    };
    let mut palette = Vec::new();
    if bpp <= 8 {
        let max = 1usize << bpp;
        let count = match colors_used as usize {
            0 => max,
            n if n > max => max,
            n => n,
        };
        for i in 0..count {
            let at = cursor + i * entry;
            palette.push(match data.get(at..at + 3) {
                Some(&[b, g, r]) => rgb(r, g, b),
                _ => BLACK,
            });
        }
        cursor += count * entry;
    }
    let data_start = if offset < 14 + size || offset >= data.len() {
        cursor
    } else {
        offset
    };
    Ok(Header {
        width: width as u32,
        height: height.unsigned_abs() as u32,
        top_down: height < 0,
        bpp,
        compression,
        palette,
        masks,
        data_start,
    })
}

fn rgb(r: u8, g: u8, b: u8) -> u32 {
    0xFF00_0000 | u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
}

/// The bits of `value` under `mask`, scaled to 0..=255.
fn channel(value: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let max = u64::from(mask >> shift);
    let v = u64::from((value & mask) >> shift);
    ((v * 255 + max / 2) / max) as u8
}

fn masked(value: u32, [r, g, b]: [u32; 3]) -> u32 {
    rgb(channel(value, r), channel(value, g), channel(value, b))
}

pub fn decode(data: &[u8]) -> Result<Image, BmpError> {
    let h = parse_header(data)?;
    let mut img = Image::new(h.width, h.height, BLACK);
    let row_of = |file_row: u32| {
        if h.top_down {
            file_row
        } else {
            h.height - 1 - file_row
        }
    };
    let pal = |i: usize| h.palette.get(i).copied().unwrap_or(BLACK);
    let pixels = data.get(h.data_start..).unwrap_or(&[]);
    match h.compression {
        BI_RLE8 | BI_RLE4 => decode_rle(&h, pixels, &mut img, &row_of, &pal),
        _ => {
            let stride = (h.width as usize * h.bpp as usize).div_ceil(32) * 4;
            for file_row in 0..h.height {
                let Some(row) = pixels.get(file_row as usize * stride..) else {
                    break;
                };
                let y = row_of(file_row);
                for x in 0..h.width {
                    let xs = x as usize;
                    let color = match h.bpp {
                        1 => row
                            .get(xs / 8)
                            .map(|&b| pal(usize::from((b >> (7 - xs % 8)) & 1))),
                        4 => row.get(xs / 2).map(|&b| {
                            pal(usize::from(if xs.is_multiple_of(2) {
                                b >> 4
                            } else {
                                b & 15
                            }))
                        }),
                        8 => row.get(xs).map(|&b| pal(usize::from(b))),
                        16 => u16_at(row, xs * 2).map(|v| {
                            masked(u32::from(v), h.masks.unwrap_or([0x7C00, 0x03E0, 0x001F]))
                        }),
                        24 => row.get(xs * 3..xs * 3 + 3).map(|p| rgb(p[2], p[1], p[0])),
                        _ => u32_at(row, xs * 4)
                            .map(|v| masked(v, h.masks.unwrap_or([0xFF_0000, 0xFF00, 0xFF]))),
                    };
                    match color {
                        Some(c) => img.set(x, y, c),
                        None => break,
                    }
                }
            }
        }
    }
    Ok(img)
}

fn decode_rle(
    h: &Header,
    data: &[u8],
    img: &mut Image,
    row_of: &dyn Fn(u32) -> u32,
    pal: &dyn Fn(usize) -> u32,
) {
    let four = h.compression == BI_RLE4;
    let (mut x, mut row) = (0u32, 0u32);
    let mut put = |x: &mut u32, row: u32, index: u8| {
        if *x < h.width && row < h.height {
            img.set(*x, row_of(row), pal(usize::from(index)));
        }
        *x += 1;
    };
    let mut i = 0;
    while let (Some(&n), Some(&v)) = (data.get(i), data.get(i + 1)) {
        i += 2;
        if n > 0 {
            for k in 0..n {
                let index = if !four {
                    v
                } else if k.is_multiple_of(2) {
                    v >> 4
                } else {
                    v & 15
                };
                put(&mut x, row, index);
            }
            continue;
        }
        match v {
            0 => {
                x = 0;
                row += 1;
            }
            1 => return,
            2 => {
                let (Some(&dx), Some(&dy)) = (data.get(i), data.get(i + 1)) else {
                    return;
                };
                i += 2;
                x += u32::from(dx);
                row += u32::from(dy);
            }
            count => {
                let bytes = if four {
                    usize::from(count).div_ceil(2)
                } else {
                    usize::from(count)
                };
                for k in 0..usize::from(count) {
                    let Some(&b) = data.get(i + if four { k / 2 } else { k }) else {
                        return;
                    };
                    let index = if !four {
                        b
                    } else if k.is_multiple_of(2) {
                        b >> 4
                    } else {
                        b & 15
                    };
                    put(&mut x, row, index);
                }
                i += bytes + bytes % 2;
            }
        }
        if row >= h.height {
            return;
        }
    }
}

/// Encodes an image as a bottom-up 24-bit BMP (alpha dropped).
pub fn encode24(img: &Image) -> Vec<u8> {
    let stride = (img.width as usize * 3).div_ceil(4) * 4;
    let data_len = stride * img.height as usize;
    let mut out = Vec::with_capacity(54 + data_len);
    out.extend(b"BM");
    out.extend(((54 + data_len) as u32).to_le_bytes());
    out.extend([0; 4]);
    out.extend(54u32.to_le_bytes());
    out.extend(40u32.to_le_bytes());
    out.extend(img.width.to_le_bytes());
    out.extend(img.height.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(24u16.to_le_bytes());
    out.extend([0; 4]);
    out.extend((data_len as u32).to_le_bytes());
    out.extend([0; 16]);
    for y in (0..img.height).rev() {
        let start = out.len();
        for x in 0..img.width {
            let [_, r, g, b] = img.get(x, y).to_be_bytes();
            out.extend([b, g, r]);
        }
        out.resize(start + stride, 0);
    }
    out
}

/// Decodes a skin bitmap: BMP with the tolerant decoder above, falling back to the `image`
/// crate (which also reads PNG, as WACUP-era skins may use).
pub fn decode_any(data: &[u8]) -> Result<Image, String> {
    let own = match decode(data) {
        Ok(img) => return Ok(img),
        Err(BmpError::NotBmp) => None,
        Err(e) => Some(e),
    };
    let mut reader = image::ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(64 << 20);
    reader.limits(limits);
    match reader.decode() {
        Ok(decoded) => {
            let rgba = decoded.to_rgba8();
            let pixels = rgba.pixels().map(|p| rgb(p[0], p[1], p[2])).collect();
            Ok(Image {
                width: rgba.width(),
                height: rgba.height(),
                pixels,
            })
        }
        Err(e) => Err(match own {
            Some(own) => own.to_string(),
            None => e.to_string(),
        }),
    }
}
