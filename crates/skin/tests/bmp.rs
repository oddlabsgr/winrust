use winrust_skin::Image;
use winrust_skin::bmp::{BmpError, decode, decode_any, encode24};
use winrust_skin::image::BLACK;

const RED: u32 = 0xFFFF_0000;
const GREEN: u32 = 0xFF00_FF00;
const BLUE: u32 = 0xFF00_00FF;
const WHITE: u32 = 0xFFFF_FFFF;

/// A BMP with a 40-byte info header. `palette` entries are [b, g, r, 0].
fn bmp(w: i32, h: i32, bpp: u16, compression: u32, palette: &[[u8; 4]], pixels: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let offset = 54 + palette.len() as u32 * 4;
    out.extend(b"BM");
    out.extend((offset + pixels.len() as u32).to_le_bytes());
    out.extend([0; 4]);
    out.extend(offset.to_le_bytes());
    out.extend(40u32.to_le_bytes());
    out.extend(w.to_le_bytes());
    out.extend(h.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(bpp.to_le_bytes());
    out.extend(compression.to_le_bytes());
    out.extend((pixels.len() as u32).to_le_bytes());
    out.extend([0; 8]);
    out.extend((palette.len() as u32).to_le_bytes());
    out.extend([0; 4]);
    for p in palette {
        out.extend(p);
    }
    out.extend(pixels);
    out
}

fn set_offset(file: &mut [u8], offset: u32) {
    file[10..14].copy_from_slice(&offset.to_le_bytes());
}

#[test]
fn rgb24_bottom_up_with_row_padding() {
    // Top row red, green; bottom row blue, white. Stored bottom row first, BGR.
    let px = [
        0xFF, 0, 0, 0xFF, 0xFF, 0xFF, 0, 0, // blue, white, pad
        0, 0, 0xFF, 0, 0xFF, 0, 0, 0, // red, green, pad
    ];
    let img = decode(&bmp(2, 2, 24, 0, &[], &px)).unwrap();
    assert_eq!(img.pixels, vec![RED, GREEN, BLUE, WHITE]);
}

#[test]
fn negative_height_is_top_down() {
    let px = [
        0, 0, 0xFF, 0, 0xFF, 0, 0, 0, // red, green, pad
        0xFF, 0, 0, 0xFF, 0xFF, 0xFF, 0, 0, // blue, white, pad
    ];
    let img = decode(&bmp(2, -2, 24, 0, &[], &px)).unwrap();
    assert_eq!(img.pixels, vec![RED, GREEN, BLUE, WHITE]);
}

#[test]
fn one_bit() {
    let pal = [[0, 0, 0, 0], [0xFF, 0xFF, 0xFF, 0]];
    let img = decode(&bmp(3, 1, 1, 0, &pal, &[0b1010_0000, 0, 0, 0])).unwrap();
    assert_eq!(img.pixels, vec![WHITE, BLACK, WHITE]);
}

#[test]
fn four_bit_index_past_palette_is_black() {
    let pal = [[0xFF, 0, 0, 0], [0, 0xFF, 0, 0]];
    let img = decode(&bmp(3, 1, 4, 0, &pal, &[0x01, 0x50, 0, 0])).unwrap();
    assert_eq!(img.pixels, vec![BLUE, GREEN, BLACK]);
}

#[test]
fn eight_bit_short_palette() {
    let pal = [[0, 0, 0, 0], [0, 0, 0xFF, 0], [0, 0xFF, 0, 0]];
    let img = decode(&bmp(2, 1, 8, 0, &pal, &[2, 1, 0, 0])).unwrap();
    assert_eq!(img.pixels, vec![GREEN, RED]);
}

#[test]
fn palette_missing_from_file_is_black() {
    // Claims 4 colours but the file ends after 1; the offset points past the end.
    let mut file = bmp(2, 1, 8, 0, &[[0, 0, 0xFF, 0]], &[]);
    file[46..50].copy_from_slice(&4u32.to_le_bytes());
    set_offset(&mut file, 9999);
    let img = decode(&file).unwrap();
    assert_eq!(img.pixels, vec![BLACK, BLACK]);
}

#[test]
fn sixteen_bit_555_and_bitfields_565() {
    let img = decode(&bmp(1, 1, 16, 0, &[], &[0x00, 0x7C, 0, 0])).unwrap();
    assert_eq!(img.pixels, vec![RED]);
    // BI_BITFIELDS: the masks follow the 40-byte header, then the pixels.
    let mut px = Vec::new();
    for m in [0xF800u32, 0x07E0, 0x001F] {
        px.extend(m.to_le_bytes());
    }
    px.extend([0xE0, 0x07, 0, 0]);
    let mut file = bmp(1, 1, 16, 3, &[], &px);
    set_offset(&mut file, 0); // force the computed offset: right after the masks
    assert_eq!(decode(&file).unwrap().pixels, vec![GREEN]);
}

#[test]
fn sixteen_bit_channels_scale_with_rounding() {
    // 5-bit 31 → 255 and 16 → 132 (16 × 255 / 31, rounded).
    let img = decode(&bmp(2, 1, 16, 0, &[], &[0xFF, 0x7F, 0x10, 0x42])).unwrap();
    assert_eq!(img.pixels, vec![WHITE, 0xFF84_8484]);
}

#[test]
fn thirty_two_bit_ignores_alpha() {
    let img = decode(&bmp(1, 1, 32, 0, &[], &[0x10, 0x20, 0x30, 0x00])).unwrap();
    assert_eq!(img.pixels, vec![0xFF30_2010]);
}

#[test]
fn os2_core_header() {
    let mut file = Vec::new();
    file.extend(b"BM");
    file.extend([0; 8]);
    file.extend(32u32.to_le_bytes()); // 14 + 12 + two 3-byte palette entries
    file.extend(12u32.to_le_bytes());
    file.extend(2u16.to_le_bytes());
    file.extend(1u16.to_le_bytes());
    file.extend(1u16.to_le_bytes());
    file.extend(1u16.to_le_bytes());
    file.extend([0, 0, 0xFF, 0xFF, 0, 0]); // red, blue (BGR)
    file.extend([0b0100_0000, 0, 0, 0]);
    assert_eq!(decode(&file).unwrap().pixels, vec![RED, BLUE]);
}

#[test]
fn bad_offsets_fall_back_to_after_palette() {
    let pal = [[0, 0, 0xFF, 0], [0, 0xFF, 0, 0]];
    for bad in [0, 20, 1_000_000] {
        let mut file = bmp(2, 1, 8, 0, &pal, &[1, 0, 0, 0]);
        set_offset(&mut file, bad);
        assert_eq!(
            decode(&file).unwrap().pixels,
            vec![GREEN, RED],
            "offset {bad}"
        );
    }
}

#[test]
fn truncated_pixels_are_black() {
    // Only the bottom row is present.
    let img = decode(&bmp(
        2,
        2,
        24,
        0,
        &[],
        &[0xFF, 0, 0, 0xFF, 0xFF, 0xFF, 0, 0],
    ))
    .unwrap();
    assert_eq!(img.pixels, vec![BLACK, BLACK, BLUE, WHITE]);
    // A pixel and a half.
    let img = decode(&bmp(2, 1, 24, 0, &[], &[0, 0, 0xFF, 0])).unwrap();
    assert_eq!(img.pixels, vec![RED, BLACK]);
}

#[test]
fn rle8_runs_absolute_delta_and_end() {
    let pal = [
        [0, 0, 0, 0],
        [0, 0, 0xFF, 0],
        [0, 0xFF, 0, 0],
        [0xFF, 0, 0, 0],
    ];
    let rle = [
        3, 1, // 3 × red
        0, 0, // end of line
        0, 3, 2, 3, 1, 0, // absolute: green, blue, red, pad byte
        0, 1, // end of bitmap
    ];
    let img = decode(&bmp(4, 2, 8, 1, &pal, &rle)).unwrap();
    // File row 0 is the bottom row.
    assert_eq!(
        img.pixels,
        vec![GREEN, BLUE, RED, BLACK, RED, RED, RED, BLACK]
    );
    let delta = [0, 2, 2, 0, 1, 2, 0, 1]; // move right 2, then 1 × green
    let img = decode(&bmp(4, 1, 8, 1, &pal, &delta)).unwrap();
    assert_eq!(img.pixels, vec![BLACK, BLACK, GREEN, BLACK]);
}

#[test]
fn rle4_runs_and_absolute() {
    let pal = [
        [0, 0, 0, 0],
        [0, 0, 0xFF, 0],
        [0, 0xFF, 0, 0],
        [0xFF, 0, 0, 0],
    ];
    let img = decode(&bmp(4, 1, 4, 2, &pal, &[4, 0x12, 0, 1])).unwrap();
    assert_eq!(img.pixels, vec![RED, GREEN, RED, GREEN]);
    let img = decode(&bmp(4, 1, 4, 2, &pal, &[0, 3, 0x12, 0x30, 0, 1])).unwrap();
    assert_eq!(img.pixels, vec![RED, GREEN, BLUE, BLACK]);
}

#[test]
fn truncated_rle_stops_quietly() {
    let pal = [[0, 0, 0xFF, 0]];
    let img = decode(&bmp(3, 1, 8, 1, &pal, &[2, 0, 0])).unwrap();
    assert_eq!(img.pixels, vec![RED, RED, BLACK]);
}

#[test]
fn rejects_bad_files() {
    assert_eq!(decode(b"PK\x03\x04"), Err(BmpError::NotBmp));
    assert_eq!(decode(b"BM\0\0"), Err(BmpError::Truncated));
    for file in [
        bmp(0, 1, 24, 0, &[], &[]),
        bmp(5000, 1, 24, 0, &[], &[]),
        bmp(1, 1, 24, 1, &[], &[]),
    ] {
        assert!(matches!(decode(&file), Err(BmpError::Unsupported(_))));
    }
    let mut odd = bmp(1, 1, 24, 0, &[], &[0, 0, 0, 0]);
    odd[14..18].copy_from_slice(&99u32.to_le_bytes());
    assert!(matches!(decode(&odd), Err(BmpError::Unsupported(_))));
}

#[test]
fn encode24_round_trips() {
    let mut img = Image::new(3, 2, RED);
    img.set(2, 0, 0xFF12_3456);
    img.set(0, 1, BLUE);
    assert_eq!(decode(&encode24(&img)).unwrap(), img);
}

#[test]
fn decode_any_reads_png_and_reports_garbage() {
    let png = {
        let rgba = image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 0]));
        let mut out = std::io::Cursor::new(Vec::new());
        rgba.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    };
    assert_eq!(decode_any(&png).unwrap().pixels, vec![0xFF01_0203]);
    assert!(decode_any(b"not an image").is_err());
    let mut odd = bmp(1, 1, 24, 0, &[], &[0, 0, 0, 0]);
    odd[14..18].copy_from_slice(&99u32.to_le_bytes());
    assert!(decode_any(&odd).unwrap_err().contains("header size 99"));
}
