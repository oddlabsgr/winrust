# M2 Part 1: Skin Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Load classic Winamp 2.x skins (`.wsz`, `.zip` or a directory) into decoded, sprite-addressable form with Webamp-compatible quirk handling, plus a debug skin, golden contact-sheet tests and fuzzing.

**Architecture:** A new pure crate, `crates/skin` (`winrust-skin`). A tolerant BMP decoder (falling back to the `image` crate, which also reads PNG) turns each sheet into an `Image` of `0xAARRGGBB` pixels. A sprite table transcribed from Webamp maps 192 named sprites to sheet rectangles. `Skin` pads each sheet to the size its sprites need and resolves every sprite either to its own sheet or to the base skin. Until M2 part 2 generates the Winrust Base skin, the base skin is the debug skin.

**Tech Stack:** Rust 2024; `zip` 9.0.1 (deflate through zlib-rs only); `image` 0.25.10 (`bmp` and `png` only); `thiserror` 2; `tempfile` (tests); `libfuzzer-sys` 0.4.13 with cargo-fuzz (nightly, CI only).

**Spec:** `docs/superpowers/specs/2026-10-10-winrust-design.md`: §4 (legal), §5.1 (pure crates), §7.1–7.4 (loading, files, quirks, sprite map), §7.7 (region parsing only; shaping is M6), §10 (no panics), §11 (testing), §12.1 (nightly fuzz job), §13 (decisions owned by M2).

**Scope:** M2 is split into two plans. This plan (part 1) builds the skin engine. Part 2 builds the Winrust Base generator (`cargo xtask gen-skin`) and its golden tests, and swaps it in as the fallback skin. The split is there because part 2 is original pixel art, best reviewed separately and by eye.

## Decisions fixed by this plan (spec §13: "Full sprite coordinate table and BMP decoder fallback rules")

- **Sprite coordinates:** transcribed from Webamp's `packages/webamp/js/skinSprites.ts` at commit `999f5676143c5696af69e1b27db901f2d35f428b` by the script in Appendix A. That gives 192 sprites on 14 sheets. The 20 `gen.bmp` sprites are left out, because §7.2 ignores `gen.bmp`.
- **`text.bmp` map:** Webamp's `FONT_LOOKUP`, with two exceptions:
  - Å, Ö and Ä use their own glyphs. Webamp strips accents first, so it never shows them.
  - Characters the font lacks use the space cell.
  - Other accented Latin-1 letters show the plain letter.
- **BMP decoding:** the rules are listed in the `bmp.rs` module comment (Task 2). Images larger than 2048 px on either side are rejected. Anything the own decoder rejects goes to the `image` crate, which also reads PNG; Webamp accepts PNG too, following WACUP.
- **Sheet sizes:** each sheet's canonical size is the extent of its sprites.
  - A smaller sheet is padded with black.
  - A sprite whose top-left corner lies outside the decoded sheet comes from the base skin.
  - A sprite only partly covered shows black for the missing part.
- **File lookup:** by file name at any folder depth, ignoring case.
  - When a name occurs more than once, the last one wins: archive order, or sorted path order for a directory.
  - `.bmp` and `.png` of the same sheet compete the same way.
  - Limits: archive 64 MiB, file 16 MiB, 10 000 archive entries, directory depth 4.
- **Derived sheets:**
  - A missing `balance.bmp` is replaced by `volume.bmp`.
  - The time display uses `nums_ex.bmp` when the skin has it, or when it has neither digit sheet.
- **Cursors:** the raw bytes of the 24 cursor files Webamp reads are kept by name. Decoding them is M6.
- **`region.txt`:** parsed as Webamp's `regionParser` does. A polygon with a coordinate that is not an integer is dropped, where Webamp would emit an invalid shape.
- **Text files:** read as UTF-8 (BOM dropped), else as Latin-1.

## Global Constraints

- Edition 2024; `version`, `edition`, `license` (`MIT OR Apache-2.0`) and `publish = false` come from `[workspace.package]`.
- `crates/skin` must not depend on winit, egui or any OS windowing API (§5.1).
- No Nullsoft or third-party skin assets in the repository (§4). Tests build their own bitmaps. Real skins are read only from `WINRUST_SKINS_DIR`.
- No panics on bad skin data (§10). Library errors are typed with `thiserror`.
- Dependencies must pass `cargo deny check` with the existing `deny.toml`.
- Gates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- Golden images: `WINRUST_BLESS=1` regenerates them (§11).
- Webamp is credited in `THIRD_PARTY.md` (§4).

## Review Focus

1. **Files in a subfolder, in odd case, or present twice:** the last copy is used. Covered by Task 6, `last_duplicate_wins_across_folders_case_and_extension`.
2. **Sheets smaller than Winamp 2.9's** (skins made for Winamp 2.0 lack the shade and clutter-bar sprites): sprites the sheet doesn't reach come from the base skin, partly covered ones are padded black. Covered by Task 6, `small_sheet_is_padded_black_and_unreached_sprites_use_base`.
3. **Archives from 1990s zip tools** (implode/shrink methods) or with corrupt entries: that file falls back to the base skin, a warning is kept and loading continues. Covered by Task 6, `entry_with_unsupported_compression_warns_and_falls_back` and `unreadable_bitmap_warns_and_falls_back`.
4. **Malformed BMP headers** (wrong offsets, short palettes, truncated data, RLE): they decode leniently and missing pixels are black. Covered by Task 2.
5. **Hostile input** (zip bombs, huge declared sizes): memory stays bounded and nothing panics. Covered by the limits in Task 2 and Task 6, plus Task 8's mutation tests and fuzzing.

---

### Task 1: Crate scaffold and image types

**Files:**
- Create: `crates/skin/Cargo.toml`
- Create: `crates/skin/src/lib.rs`
- Create: `crates/skin/src/image.rs`
- Test: `crates/skin/tests/image.rs`

**Interfaces:**
- Consumes: workspace `members = ["crates/*", "xtask"]` (picks the crate up automatically).
- Produces:
  - `winrust_skin::Image { width: u32, height: u32, pixels: Vec<u32> }`, with pixels as `0xAARRGGBB`.
  - Methods: `Image::new(w, h, fill)`, `get(x, y) -> u32` (black outside), `set`, `fill_rect(Rect, u32)`, `blit(&Image, Rect, x, y)` and `padded(w, h) -> Image`.
  - `winrust_skin::Rect { x, y, w, h: u32 }`, `const fn Rect::new`.
  - `winrust_skin::image::BLACK`.

- [ ] **Step 1: Create the crate manifest and an empty library**

`crates/skin/Cargo.toml`:

```toml
[package]
name = "winrust-skin"
description = "Winrust skin engine: classic Winamp 2.x skins (.wsz) and the sprite map"
version.workspace = true
edition.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
image = { version = "0.25.10", default-features = false, features = ["bmp", "png"] }
thiserror = "2.0.21"
zip = { version = "9.0.1", default-features = false, features = ["deflate-flate2-zlib-rs"] }

[dev-dependencies]
tempfile = "3"
```

`crates/skin/src/lib.rs`:

```rust
//! Winrust skin engine: loads classic Winamp 2.x skins (`.wsz`, `.zip` or a directory) and
//! maps their sprites. Pure: no windowing or OS dependencies.
```

- [ ] **Step 2: Write the failing test**

`crates/skin/tests/image.rs`:

```rust
use winrust_skin::image::BLACK;
use winrust_skin::{Image, Rect};

#[test]
fn get_outside_is_black() {
    let img = Image::new(2, 2, 0xFF11_2233);
    assert_eq!(img.get(1, 1), 0xFF11_2233);
    assert_eq!(img.get(2, 0), BLACK);
    assert_eq!(img.get(0, 9), BLACK);
}

#[test]
fn blit_clips_and_fills_black_outside_source() {
    let mut src = Image::new(2, 1, 0xFFAA_AAAA);
    src.set(1, 0, 0xFFBB_BBBB);
    let mut dst = Image::new(3, 2, 0xFF00_00FF);
    dst.blit(&src, Rect::new(1, 0, 2, 1), 1, 1);
    assert_eq!(dst.get(1, 1), 0xFFBB_BBBB);
    assert_eq!(dst.get(2, 1), BLACK, "source pixel (2,0) is outside src");
    assert_eq!(dst.get(0, 0), 0xFF00_00FF);
    // Nothing is written past the destination edge (and nothing panics).
    dst.blit(&src, Rect::new(0, 0, 2, 1), 2, 1);
    assert_eq!(dst.get(2, 1), 0xFFAA_AAAA);
}

#[test]
fn padded_grows_with_black() {
    let img = Image::new(1, 1, 0xFF12_3456);
    let p = img.padded(2, 3);
    assert_eq!((p.width, p.height), (2, 3));
    assert_eq!(p.get(0, 0), 0xFF12_3456);
    assert_eq!(p.get(1, 2), BLACK);
    assert_eq!(img.padded(1, 1), img);
}

#[test]
fn fill_rect_is_clipped() {
    let mut img = Image::new(2, 2, BLACK);
    img.fill_rect(Rect::new(1, 1, 5, 5), 0xFFFF_FFFF);
    assert_eq!(img.pixels, vec![BLACK, BLACK, BLACK, 0xFFFF_FFFF]);
}
```

- [ ] **Step 3: Run it to verify it fails**

Run: `cargo test -p winrust-skin --test image`
Expected: compile error `unresolved import `winrust_skin::image`` (E0432).

- [ ] **Step 4: Implement**

`crates/skin/src/image.rs`:

```rust
//! Images and rectangles. Pixels are `u32` in `0xAARRGGBB` order.

/// Opaque black: what a skin shows where its bitmap has no pixels.
pub const BLACK: u32 = 0xFF00_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub const fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Rect { x, y, w, h }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

impl Image {
    pub fn new(width: u32, height: u32, fill: u32) -> Self {
        Image {
            width,
            height,
            pixels: vec![fill; width as usize * height as usize],
        }
    }

    /// Pixel at (x, y); black outside the image.
    pub fn get(&self, x: u32, y: u32) -> u32 {
        if x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize]
        } else {
            BLACK
        }
    }

    /// Sets (x, y); ignored outside the image.
    pub fn set(&mut self, x: u32, y: u32, color: u32) {
        if x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize] = color;
        }
    }

    /// Fills `r`, clipped to the image.
    pub fn fill_rect(&mut self, r: Rect, color: u32) {
        for y in r.y..r.y.saturating_add(r.h).min(self.height) {
            for x in r.x..r.x.saturating_add(r.w).min(self.width) {
                self.set(x, y, color);
            }
        }
    }

    /// Copies the `from` area of `src` to (x, y). Source pixels outside `src` are black;
    /// destination pixels outside `self` are dropped.
    pub fn blit(&mut self, src: &Image, from: Rect, x: u32, y: u32) {
        for dy in 0..from.h {
            for dx in 0..from.w {
                let c = src.get(from.x + dx, from.y + dy);
                self.set(x + dx, y + dy, c);
            }
        }
    }

    /// The image grown to at least `width`×`height`; new pixels are black.
    pub fn padded(&self, width: u32, height: u32) -> Image {
        if self.width >= width && self.height >= height {
            return self.clone();
        }
        let mut out = Image::new(self.width.max(width), self.height.max(height), BLACK);
        out.blit(self, Rect::new(0, 0, self.width, self.height), 0, 0);
        out
    }
}
```

`crates/skin/src/lib.rs`:

```rust
//! Winrust skin engine: loads classic Winamp 2.x skins (`.wsz`, `.zip` or a directory) and
//! maps their sprites. Pure: no windowing or OS dependencies.

pub mod image;

pub use image::{Image, Rect};
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p winrust-skin --test image`
Expected: `test result: ok. 4 passed`

- [ ] **Step 6: Check licenses of the new dependencies**

Run: `cargo deny check`
Expected: ends with `advisories ok, bans ok, licenses ok, sources ok` (warnings allowed).

- [ ] **Step 7: Commit**

```bash
git add crates/skin Cargo.lock
git commit -m "feat(skin): add winrust-skin crate with image and rect types"
```

---

### Task 2: Tolerant BMP decoder

**Files:**
- Create: `crates/skin/src/bmp.rs`
- Modify: `crates/skin/src/lib.rs`
- Test: `crates/skin/tests/bmp.rs`

**Interfaces:**
- Consumes: `Image`, `image::BLACK` (Task 1).
- Produces:
  - `bmp::decode(&[u8]) -> Result<Image, BmpError>`, where `BmpError` is `NotBmp`, `Truncated` or `Unsupported(String)`.
  - `bmp::decode_any(&[u8]) -> Result<Image, String>`: own decoder, then the `image` crate (BMP/PNG).
  - `bmp::encode24(&Image) -> Vec<u8>`.
  - `bmp::MAX_SIDE: u32 = 2048`.

- [ ] **Step 1: Write the failing tests**

`crates/skin/tests/bmp.rs`:

```rust
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
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p winrust-skin --test bmp`
Expected: compile error `unresolved import `winrust_skin::bmp`` (E0432).

- [ ] **Step 3: Implement**

`crates/skin/src/bmp.rs`:

```rust
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
```

`crates/skin/src/lib.rs`:

```rust
//! Winrust skin engine: loads classic Winamp 2.x skins (`.wsz`, `.zip` or a directory) and
//! maps their sprites. Pure: no windowing or OS dependencies.

pub mod bmp;
pub mod image;

pub use image::{Image, Rect};
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p winrust-skin --test bmp`
Expected: `test result: ok. 18 passed`

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy -p winrust-skin --all-targets -- -D warnings`
Expected: no warnings.

```bash
git add crates/skin
git commit -m "feat(skin): add tolerant BMP decoder with image-crate fallback"
```

---

### Task 3: Sprite table and text glyphs

**Files:**
- Create: `crates/skin/src/sprites.rs` (generated by Appendix A)
- Create: `crates/skin/src/text.rs`
- Modify: `crates/skin/src/lib.rs`, `THIRD_PARTY.md`
- Test: `crates/skin/tests/sprites.rs`

**Interfaces:**
- Consumes: `Rect` (Task 1).
- Produces:
  - `Sheet`, with the 15 variants `Main, Cbuttons, Titlebar, Numbers, NumsEx, Text, Posbar, Volume, Balance, Monoster, Playpaus, Shufrep, Eqmain, EqEx, Pledit`.
  - `Sheet::ALL` (in that order, so `sheet as usize` indexes it), `Sheet::stem() -> &'static str` (lower-case file stem) and `Sheet::size() -> (u32, u32)`.
  - `SpriteId` (192 variants in table order) and `SpriteId::def() -> &'static SpriteDef { id, name, sheet, rect }`.
  - `sprites::SPRITES: [SpriteDef; 192]`.
  - `text::glyph(char) -> Rect`, plus `text::GLYPH_W = 5` and `text::GLYPH_H = 6`.

- [ ] **Step 1: Write the failing tests**

`crates/skin/tests/sprites.rs`:

```rust
use std::collections::HashSet;
use winrust_skin::sprites::SPRITES;
use winrust_skin::text::glyph;
use winrust_skin::{Rect, Sheet, SpriteId};

#[test]
fn table_matches_webamp_counts() {
    let count = |sheet| SPRITES.iter().filter(|d| d.sheet == sheet).count();
    let expected = [
        (Sheet::Main, 1),
        (Sheet::Cbuttons, 12),
        (Sheet::Titlebar, 27),
        (Sheet::Numbers, 12),
        (Sheet::NumsEx, 12),
        (Sheet::Text, 0),
        (Sheet::Posbar, 3),
        (Sheet::Volume, 3),
        (Sheet::Balance, 3),
        (Sheet::Monoster, 4),
        (Sheet::Playpaus, 5),
        (Sheet::Shufrep, 16),
        (Sheet::Eqmain, 22),
        (Sheet::EqEx, 12),
        (Sheet::Pledit, 60),
    ];
    for (sheet, n) in expected {
        assert_eq!(count(sheet), n, "{sheet:?}");
    }
    assert_eq!(SPRITES.len(), 192);
}

#[test]
fn ids_index_the_table_and_names_are_unique() {
    let mut names = HashSet::new();
    for def in &SPRITES {
        assert_eq!(def.id.def().name, def.name);
        assert!(names.insert(def.name), "duplicate {}", def.name);
    }
}

#[test]
fn spot_checks_against_webamp() {
    let rect = |id: SpriteId| (id.def().sheet, id.def().rect);
    assert_eq!(
        rect(SpriteId::MainPlayButton),
        (Sheet::Cbuttons, Rect::new(23, 0, 23, 18))
    );
    assert_eq!(
        rect(SpriteId::MainTitleBarSelected),
        (Sheet::Titlebar, Rect::new(27, 0, 275, 14))
    );
    assert_eq!(
        rect(SpriteId::MainVolumeBackground),
        (Sheet::Volume, Rect::new(0, 0, 68, 420))
    );
    assert_eq!(
        rect(SpriteId::MainBalanceBackground),
        (Sheet::Balance, Rect::new(9, 0, 38, 420))
    );
    assert_eq!(
        rect(SpriteId::EqGraphLineColors),
        (Sheet::Eqmain, Rect::new(115, 294, 1, 19))
    );
    assert_eq!(
        rect(SpriteId::Digit9Ex),
        (Sheet::NumsEx, Rect::new(81, 0, 9, 13))
    );
}

#[test]
fn every_sprite_fits_its_sheet_size() {
    for def in &SPRITES {
        let (w, h) = def.sheet.size();
        assert!(
            def.rect.x + def.rect.w <= w && def.rect.y + def.rect.h <= h,
            "{}",
            def.name
        );
    }
    assert_eq!(Sheet::Main.size(), (275, 116));
    assert_eq!(Sheet::Text.size(), (155, 18));
}

#[test]
fn letters_digits_and_symbols() {
    assert_eq!(glyph('a'), Rect::new(0, 0, 5, 6));
    assert_eq!(glyph('Z'), Rect::new(125, 0, 5, 6));
    assert_eq!(glyph('0'), Rect::new(0, 6, 5, 6));
    assert_eq!(glyph('#'), Rect::new(150, 6, 5, 6));
    assert_eq!(glyph('*'), Rect::new(20, 12, 5, 6));
}

#[test]
fn brackets_share_cells() {
    assert_eq!(glyph('<'), glyph('['));
    assert_eq!(glyph('}'), glyph(']'));
}

#[test]
fn nordic_letters_have_glyphs_other_accents_fold() {
    assert_eq!(glyph('Å'), Rect::new(0, 12, 5, 6));
    assert_eq!(glyph('ö'), Rect::new(5, 12, 5, 6));
    assert_eq!(glyph('é'), glyph('e'));
    assert_eq!(glyph('Ñ'), glyph('n'));
}

#[test]
fn unknown_is_space() {
    assert_eq!(glyph('€'), glyph(' '));
    assert_eq!(glyph('漢'), Rect::new(150, 0, 5, 6));
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p winrust-skin --test sprites`
Expected: compile error `unresolved import `winrust_skin::sprites`` (E0432).

- [ ] **Step 3: Generate the sprite table**

Save Appendix A's script as `<scratch>/gen_sprites.py`, outside the repository; the script is not committed. Download the pinned source into a new empty scratch directory, then run the script:

```bash
mkdir -p <scratch>/webamp && curl -fsSL https://raw.githubusercontent.com/captbaritone/webamp/999f5676143c5696af69e1b27db901f2d35f428b/packages/webamp/js/skinSprites.ts -o <scratch>/webamp/skinSprites.ts
python -I <scratch>/gen_sprites.py <scratch>/webamp/skinSprites.ts crates/skin/src/sprites.rs
```

Expected output: `192 sprites; {'Balance': (47, 433), 'Cbuttons': (136, 36), 'Main': (275, 116), 'Monoster': (56, 24), 'Numbers': (90, 13), 'NumsEx': (108, 13), 'Playpaus': (48, 9), 'Pledit': (280, 186), 'EqEx': (275, 56), 'Eqmain': (275, 315), 'Posbar': (307, 10), 'Shufrep': (92, 85), 'Titlebar': (344, 86), 'Volume': (68, 433), 'Text': (155, 18)}`

- [ ] **Step 4: Write the glyph map**

`crates/skin/src/text.rs`:

```rust
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
```

`crates/skin/src/lib.rs`:

```rust
//! Winrust skin engine: loads classic Winamp 2.x skins (`.wsz`, `.zip` or a directory) and
//! maps their sprites. Pure: no windowing or OS dependencies.

pub mod bmp;
pub mod image;
pub mod sprites;
pub mod text;

pub use image::{Image, Rect};
pub use sprites::{Sheet, SpriteId};
```

- [ ] **Step 5: Run the tests**

Run: `cargo fmt --all && cargo test -p winrust-skin --test sprites`
Expected: `test result: ok. 8 passed`. `sprites.rs` keeps one sprite per line (the table is `#[rustfmt::skip]`).

- [ ] **Step 6: Credit Webamp**

In `THIRD_PARTY.md`, replace the `## Assets and references` section (heading plus its "None yet" paragraph) with:

````markdown
## Assets and references

### Webamp skin sprite map

`crates/skin/src/sprites.rs` and `crates/skin/src/text.rs` transcribe the sprite coordinate
table and the `text.bmp` character map from [Webamp](https://github.com/captbaritone/webamp)
(`packages/webamp/js/skinSprites.ts`, commit `999f567`). The skin loader also follows the
parsing rules of Webamp's skin, `pledit.txt`, `viscolor.txt` and `region.txt` parsers. Webamp is
licensed under the MIT License:

```text
The MIT License (MIT)

Copyright (c) [2015] [Jordan Eldredge]

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
````

- [ ] **Step 7: Commit**

```bash
git add crates/skin THIRD_PARTY.md
git commit -m "feat(skin): add Winamp 2.x sprite table from Webamp and text.bmp glyph map"
```

---

### Task 4: Text-file parsers

**Files:**
- Create: `crates/skin/src/ini.rs`, `crates/skin/src/pledit.rs`, `crates/skin/src/viscolor.rs`, `crates/skin/src/region.rs`
- Modify: `crates/skin/src/lib.rs`
- Test: `crates/skin/tests/parsers.rs`

**Interfaces:**
- Produces:
  - `ini::parse(&str) -> Ini` (`HashMap<String, HashMap<String, String>>`) and `ini::decode_text(&[u8]) -> String`.
  - `pledit::parse(&str) -> PlaylistStyle { normal, current, normal_bg, selected_bg: u32, font: String }` (`Default`).
  - `viscolor::parse(&str) -> [u32; 24]` and `viscolor::DEFAULT_COLORS`.
  - `region::parse(&str) -> Regions { normal, window_shade, equalizer, equalizer_ws: Vec<Polygon> }`, where `Polygon = Vec<(i32, i32)>`.

- [ ] **Step 1: Write the failing tests**

`crates/skin/tests/parsers.rs`:

```rust
use winrust_skin::ini::{decode_text, parse as parse_ini};
use winrust_skin::pledit::{self, PlaylistStyle};
use winrust_skin::region::{self, Polygon};
use winrust_skin::viscolor::{self, DEFAULT_COLORS};

// ini

#[test]
fn sections_keys_and_values() {
    let ini = parse_ini(
        "ignored=1\r\n[Text]\r\nNormal = #00FF00\n ; Font=no\nFont=\"Arial\"\nbad line\n",
    );
    let text = &ini["text"];
    assert_eq!(text["normal"], "#00FF00");
    assert_eq!(text["font"], "Arial");
    assert_eq!(text.len(), 2);
    assert_eq!(ini.len(), 1);
}

#[test]
fn value_stops_at_second_equals_and_loses_one_quote_each_side() {
    let ini = parse_ini("[a]\nk='x=y'\nq=\"\"v\"\"\n");
    assert_eq!(ini["a"]["k"], "x");
    assert_eq!(ini["a"]["q"], "\"v\"");
}

#[test]
fn repeated_section_starts_over() {
    let ini = parse_ini("[a]\nx=1\n[A]\ny=2\n");
    assert_eq!(ini["a"].get("x"), None);
    assert_eq!(ini["a"]["y"], "2");
}

#[test]
fn text_decoding() {
    assert_eq!(decode_text(b"\xEF\xBB\xBFabc"), "abc");
    assert_eq!(decode_text("Ärial".as_bytes()), "Ärial");
    assert_eq!(decode_text(b"\xC4rial"), "Ärial");
}

// pledit.txt

#[test]
fn reads_colors_and_font() {
    let style = pledit::parse(
        "[Text]\nNormal=#00FF00\nCurrent=FFFFFF\nNormalBG=#000000\nSelectedBG=#0000C6\nFont=Tahoma\n",
    );
    assert_eq!(
        style,
        PlaylistStyle {
            normal: 0xFF00_FF00,
            current: 0xFFFF_FFFF,
            normal_bg: 0xFF00_0000,
            selected_bg: 0xFF00_00C6,
            font: "Tahoma".into(),
        }
    );
}

#[test]
fn tolerates_junk_and_keeps_defaults() {
    let style = pledit::parse("[text]\nnormal=#12345678\ncurrent=#FFF\nnormalbg=green\nfont=\n");
    assert_eq!(style.normal, 0xFF12_3456);
    assert_eq!(style.current, 0xFFFF_FFFF);
    assert_eq!(style.normal_bg, PlaylistStyle::default().normal_bg);
    assert_eq!(style.font, "Arial");
    assert_eq!(pledit::parse("no sections"), PlaylistStyle::default());
}

// viscolor.txt

#[test]
fn reads_lines_in_order_with_comments() {
    let colors = viscolor::parse("0,0,0, // background\n 24 , 33,41 // dots\n255 255 255\n");
    assert_eq!(colors[0], 0xFF00_0000);
    assert_eq!(colors[1], 0xFF18_2129);
    assert_eq!(colors[2], 0xFFFF_FFFF);
    assert_eq!(colors[3], DEFAULT_COLORS[3]);
}

#[test]
fn non_matching_lines_do_not_use_an_entry() {
    let colors = viscolor::parse("// header\n\n1,2,3\n");
    assert_eq!(colors[0], 0xFF01_0203);
    assert_eq!(colors[1], DEFAULT_COLORS[1]);
}

#[test]
fn regex_quirks_and_clamping() {
    let first = |line: &str| viscolor::parse(line)[0];
    assert_eq!(first("12,34"), 0xFF0C_0304);
    assert_eq!(first("123"), 0xFF01_0203);
    assert_eq!(first("300,0,99999999999"), 0xFFFF_00FF);
    assert_eq!(first("1,2"), DEFAULT_COLORS[0]);
    assert_eq!(first("x 1,2,3"), DEFAULT_COLORS[0]);
}

#[test]
fn extra_lines_are_ignored() {
    let text = "1,1,1\n".repeat(30);
    assert!(viscolor::parse(&text).iter().all(|&c| c == 0xFF01_0101));
}

// region.txt

#[test]
fn reads_sections_with_mixed_separators() {
    let r = region::parse(
        "[Normal]\nNumPoints=4, 3\nPointList=0,0, 275,0 275 116,0,116  1,1,2,2,3,3\n\
         [WindowShade]\nNumPoints=4\nPointList=0,0,275,0,275,14,0,14\n",
    );
    assert_eq!(
        r.normal,
        vec![
            vec![(0, 0), (275, 0), (275, 116), (0, 116)],
            vec![(1, 1), (2, 2), (3, 3)],
        ]
    );
    assert_eq!(r.window_shade.len(), 1);
    assert!(r.equalizer.is_empty());
}

#[test]
fn small_polygons_skip_their_points() {
    let r = region::parse("[Equalizer]\nNumPoints=2,3\nPointList=9,9,9,9,1,1,2,2,3,3\n");
    assert_eq!(r.equalizer, vec![vec![(1, 1), (2, 2), (3, 3)]]);
}

#[test]
fn short_point_list_keeps_partial_then_drops() {
    let r = region::parse("[Normal]\nNumPoints=4,3\nPointList=0,0,1,0,1,1\n");
    assert_eq!(r.normal, vec![vec![(0, 0), (1, 0), (1, 1)]]);
}

#[test]
fn bad_numbers() {
    let r = region::parse("[Normal]\nNumPoints=3,x,3\nPointList=0,0,1,0,1,1,5,5,6,6,7,7\n");
    assert_eq!(
        r.normal,
        vec![vec![(0, 0), (1, 0), (1, 1)], vec![(5, 5), (6, 6), (7, 7)]]
    );
    let r = region::parse("[Normal]\nNumPoints=3,3\nPointList=0,0,1,0,1,z,5,5,6,6,7,7\n");
    assert_eq!(r.normal, vec![vec![(5, 5), (6, 6), (7, 7)]]);
    assert_eq!(
        region::parse("[Normal]\nNumPoints=3\n").normal,
        Vec::<Polygon>::new()
    );
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p winrust-skin --test parsers`
Expected: compile error `unresolved import `winrust_skin::ini`` (E0432).

- [ ] **Step 3: Implement**

`crates/skin/src/ini.rs`:

```rust
//! Lenient INI parsing for `pledit.txt` and `region.txt`, following Webamp's `parseIni`.
//!
//! - Section and key names are trimmed and lower-cased; a repeated section starts over.
//! - Lines whose first non-blank character is `;` are comments; lines before the first
//!   section and lines without `=` are ignored.
//! - A value ends at a second `=`, is trimmed, and loses one leading and one trailing quote.

use std::collections::HashMap;

pub type Ini = HashMap<String, HashMap<String, String>>;

pub fn parse(text: &str) -> Ini {
    let mut ini = Ini::new();
    let mut section: Option<String> = None;
    for line in text.split(['\r', '\n']) {
        let trimmed = line.trim();
        if let Some(name) = &section
            && let Some((key, value)) = property(trimmed)
        {
            ini.entry(name.clone()).or_default().insert(key, value);
        } else if trimmed.len() > 2 && trimmed.starts_with('[') && trimmed.ends_with(']') {
            let name = trimmed[1..trimmed.len() - 1].trim().to_lowercase();
            ini.insert(name.clone(), HashMap::new());
            section = Some(name);
        }
    }
    ini
}

fn property(line: &str) -> Option<(String, String)> {
    if line.starts_with(';') {
        return None;
    }
    let (key, rest) = line.split_once('=')?;
    let key = key.trim().to_lowercase();
    if key.is_empty() {
        return None;
    }
    let value = rest.split('=').next().unwrap_or("").trim();
    let value = value.strip_prefix(['"', '\'']).unwrap_or(value);
    let value = value.strip_suffix(['"', '\'']).unwrap_or(value);
    Some((key, value.to_string()))
}

/// Decodes a skin text file: UTF-8 (BOM dropped) or, failing that, Latin-1.
pub fn decode_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}
```

`crates/skin/src/pledit.rs`:

```rust
//! `pledit.txt`: playlist colours and font.

use crate::ini;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistStyle {
    pub normal: u32,
    pub current: u32,
    pub normal_bg: u32,
    pub selected_bg: u32,
    pub font: String,
}

impl Default for PlaylistStyle {
    fn default() -> Self {
        PlaylistStyle {
            normal: 0xFF00_FF00,
            current: 0xFFFF_FFFF,
            normal_bg: 0xFF00_0000,
            selected_bg: 0xFF00_00FF,
            font: "Arial".into(),
        }
    }
}

/// Reads the `[Text]` section; missing or unreadable entries keep their defaults.
pub fn parse(text: &str) -> PlaylistStyle {
    let mut style = PlaylistStyle::default();
    let ini = ini::parse(text);
    let Some(section) = ini.get("text") else {
        return style;
    };
    let colors = [
        ("normal", &mut style.normal),
        ("current", &mut style.current),
        ("normalbg", &mut style.normal_bg),
        ("selectedbg", &mut style.selected_bg),
    ];
    for (key, slot) in colors {
        if let Some(color) = section.get(key).and_then(|v| color(v)) {
            *slot = color;
        }
    }
    if let Some(font) = section.get("font").filter(|f| !f.is_empty()) {
        style.font = font.clone();
    }
    style
}

/// `#RRGGBB` with the `#` optional and anything past six digits ignored (skins in the wild
/// carry extra characters, which Winamp tolerates); `#RGB` shorthand as in CSS.
fn color(value: &str) -> Option<u32> {
    let hex = value.trim();
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    let digits: String = hex.chars().take(6).collect();
    let full = match digits.len() {
        6 => digits,
        3 => digits.chars().flat_map(|c| [c, c]).collect(),
        _ => return None,
    };
    u32::from_str_radix(&full, 16)
        .ok()
        .map(|rgb| 0xFF00_0000 | rgb)
}
```

`crates/skin/src/viscolor.rs`:

```rust
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
```

`crates/skin/src/region.rs`:

```rust
//! `region.txt`: window shapes as polygons, following Webamp's `regionParser`.
//!
//! - `NumPoints` is a comma-separated list of polygon sizes; `PointList` holds x,y pairs
//!   separated by commas and/or blanks.
//! - A size below 3 skips that many points and drops the polygon.
//! - A size that is not a number drops that polygon only.
//! - A polygon cut short by the end of the list keeps the points it has; once no points
//!   are left, the remaining polygons are dropped.
//! - A polygon containing a coordinate that is not an integer is dropped (ruling: Webamp
//!   would emit an invalid shape).

use crate::ini;

pub type Polygon = Vec<(i32, i32)>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Regions {
    pub normal: Vec<Polygon>,
    pub window_shade: Vec<Polygon>,
    pub equalizer: Vec<Polygon>,
    pub equalizer_ws: Vec<Polygon>,
}

pub fn parse(text: &str) -> Regions {
    let ini = ini::parse(text);
    let section = |name: &str| {
        ini.get(name)
            .and_then(|s| Some(polygons(s.get("numpoints")?, s.get("pointlist")?)))
            .unwrap_or_default()
    };
    Regions {
        normal: section("normal"),
        window_shade: section("windowshade"),
        equalizer: section("equalizer"),
        equalizer_ws: section("equalizerws"),
    }
}

fn polygons(num_points: &str, point_list: &str) -> Vec<Polygon> {
    let tokens: Vec<&str> = point_list
        .split([',', ' ', '\t'])
        .filter(|t| !t.is_empty())
        .collect();
    let points: Vec<Option<(i32, i32)>> = tokens
        .chunks(2)
        .map(|pair| match pair {
            [x, y] => Some((x.parse().ok()?, y.parse().ok()?)),
            _ => None,
        })
        .collect();
    let mut out = Vec::new();
    let mut next = 0usize;
    for count in num_points
        .split(',')
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        let Ok(count) = count.parse::<usize>() else {
            continue;
        };
        if count < 3 {
            next += count;
            continue;
        }
        let Some(slice) = points.get(next..(next + count).min(points.len())) else {
            break;
        };
        if slice.is_empty() {
            break;
        }
        next += count;
        if let Some(polygon) = slice.iter().copied().collect::<Option<Polygon>>() {
            out.push(polygon);
        }
    }
    out
}
```

`crates/skin/src/lib.rs`:

```rust
//! Winrust skin engine: loads classic Winamp 2.x skins (`.wsz`, `.zip` or a directory) and
//! maps their sprites. Pure: no windowing or OS dependencies.

pub mod bmp;
pub mod image;
pub mod ini;
pub mod pledit;
pub mod region;
pub mod sprites;
pub mod text;
pub mod viscolor;

pub use image::{Image, Rect};
pub use sprites::{Sheet, SpriteId};
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p winrust-skin --test parsers`
Expected: `test result: ok. 14 passed`

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy -p winrust-skin --all-targets -- -D warnings`
Expected: no warnings.

```bash
git add crates/skin
git commit -m "feat(skin): parse pledit.txt, viscolor.txt and region.txt like Webamp"
```

---

### Task 5: Skin type and debug skin

**Files:**
- Create: `crates/skin/src/skin.rs`, `crates/skin/src/debug.rs`
- Modify: `crates/skin/src/lib.rs`
- Test: `crates/skin/tests/debug_skin.rs`

**Interfaces:**
- Consumes: `Image`, `Rect`, `Sheet`, `SpriteId`, `SPRITES`, `PlaylistStyle`, `Regions` and `viscolor::DEFAULT_COLORS` (Tasks 1–4).
- Produces:
  - `Skin`, with pub fields `playlist`, `vis_colors`, `regions`, `cursors: BTreeMap<&'static str, Vec<u8>>` and `warnings: Vec<String>`.
  - Methods: `Skin::sprite(SpriteId) -> SpriteRef { image: &Image, rect: Rect }`, `Skin::sheet(Sheet) -> &Image`, `Skin::has_sheet(Sheet) -> bool`, `Skin::uses_nums_ex() -> bool`, `Skin::base() -> Arc<Skin>` and `Skin::debug() -> Skin`.
  - `pub(crate) Skin::complete(Vec<Image>)`.
  - `debug::{color(usize) -> u32, BACKGROUND, sheets() -> Vec<Image>}`.

- [ ] **Step 1: Write the failing tests**

`crates/skin/tests/debug_skin.rs`:

```rust
use std::collections::HashSet;
use std::sync::Arc;
use winrust_skin::debug::{self, BACKGROUND, color};
use winrust_skin::image::BLACK;
use winrust_skin::sprites::SPRITES;
use winrust_skin::{Sheet, Skin, SpriteId};

#[test]
fn colors_are_unique_and_not_background() {
    let n = SPRITES.len() + 3 * 31;
    let colors: HashSet<u32> = (0..n).map(color).collect();
    assert_eq!(colors.len(), n);
    assert!(!colors.contains(&BACKGROUND) && !colors.contains(&BLACK));
}

#[test]
fn sprites_point_at_their_table_rectangles() {
    let skin = Skin::debug();
    let sheets = debug::sheets();
    for def in &SPRITES {
        let s = skin.sprite(def.id);
        assert_eq!(s.rect, def.rect, "{}", def.name);
        assert_eq!(s.image, &sheets[def.sheet as usize], "{}", def.name);
    }
}

#[test]
fn sprite_shows_its_own_color_unless_overlapped() {
    let skin = Skin::debug();
    let s = skin.sprite(SpriteId::MainPlayButton);
    let index = SPRITES
        .iter()
        .position(|d| d.id == SpriteId::MainPlayButton)
        .unwrap();
    assert_eq!(s.image.get(s.rect.x, s.rect.y), color(index));
    // MAIN_WORKING_INDICATOR (39,0) is painted over MAIN_NOT_WORKING_INDICATOR (36,0).
    let s = skin.sprite(SpriteId::MainNotWorkingIndicator);
    let later = SPRITES
        .iter()
        .position(|d| d.id == SpriteId::MainWorkingIndicator)
        .unwrap();
    assert_eq!(s.image.get(s.rect.x + 3, s.rect.y), color(later));
}

#[test]
fn text_cells_are_colored() {
    let skin = Skin::debug();
    let text = skin.sheet(Sheet::Text);
    assert_eq!(text.get(0, 0), color(SPRITES.len()));
    assert_eq!(text.get(154, 17), color(SPRITES.len() + 92));
}

#[test]
fn debug_skin_is_complete() {
    let skin = Skin::debug();
    assert!(Sheet::ALL.iter().all(|&s| skin.has_sheet(s)));
    assert!(skin.uses_nums_ex());
    assert!(skin.warnings.is_empty());
}

#[test]
fn base_is_shared() {
    assert!(Arc::ptr_eq(&Skin::base(), &Skin::base()));
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p winrust-skin --test debug_skin`
Expected: compile error `unresolved import `winrust_skin::debug`` (E0432).

- [ ] **Step 3: Implement**

`crates/skin/src/skin.rs` (loading is added in Task 6):

```rust
//! A loaded skin: decoded sheets, playlist style, visualization colours, window regions and
//! cursors. Whatever a skin lacks comes from the base skin.

use crate::image::{Image, Rect};
use crate::pledit::PlaylistStyle;
use crate::region::Regions;
use crate::sprites::{Sheet, SpriteId};
use crate::viscolor;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

struct SheetImage {
    /// Padded to at least `Sheet::size()`, so every sprite rectangle is inside it.
    image: Image,
    /// Size as decoded, before padding.
    width: u32,
    height: u32,
}

/// Where to blit a sprite from.
#[derive(Clone, Copy, Debug)]
pub struct SpriteRef<'a> {
    pub image: &'a Image,
    pub rect: Rect,
}

pub struct Skin {
    sheets: Vec<Option<Arc<SheetImage>>>,
    fallback: Option<Arc<Skin>>,
    pub playlist: PlaylistStyle,
    pub vis_colors: [u32; 24],
    pub regions: Regions,
    /// Raw `.cur` file contents by cursor name (decoded by the app).
    pub cursors: BTreeMap<&'static str, Vec<u8>>,
    /// Problems that did not stop loading (unreadable bitmaps, bad archive entries).
    pub warnings: Vec<String>,
}

impl Skin {
    /// A skin with every sheet given, in `Sheet::ALL` order, and nothing to fall back on.
    pub(crate) fn complete(sheets: Vec<Image>) -> Skin {
        Skin {
            sheets: sheets
                .into_iter()
                .map(|image| {
                    Some(Arc::new(SheetImage {
                        width: image.width,
                        height: image.height,
                        image,
                    }))
                })
                .collect(),
            fallback: None,
            playlist: PlaylistStyle::default(),
            vis_colors: viscolor::DEFAULT_COLORS,
            regions: Regions::default(),
            cursors: BTreeMap::new(),
            warnings: Vec::new(),
        }
    }

    /// The built-in skin used for anything a skin lacks. Until the Winrust Base skin is
    /// generated (M2 part 2) this is the debug skin.
    pub fn base() -> Arc<Skin> {
        static BASE: OnceLock<Arc<Skin>> = OnceLock::new();
        BASE.get_or_init(|| Arc::new(Skin::debug())).clone()
    }

    /// The sprite's pixels: from this skin when its sheet reaches the sprite (missing
    /// pixels of a partly covered sprite are black), otherwise from the base skin.
    pub fn sprite(&self, id: SpriteId) -> SpriteRef<'_> {
        let def = id.def();
        match &self.sheets[def.sheet as usize] {
            Some(s) if def.rect.x < s.width && def.rect.y < s.height => SpriteRef {
                image: &s.image,
                rect: def.rect,
            },
            _ => self
                .fallback
                .as_ref()
                .expect("only complete skins have no fallback")
                .sprite(id),
        }
    }

    /// A whole sheet (e.g. `text.bmp` for [`crate::text::glyph`] cells).
    pub fn sheet(&self, sheet: Sheet) -> &Image {
        match &self.sheets[sheet as usize] {
            Some(s) => &s.image,
            None => self
                .fallback
                .as_ref()
                .expect("only complete skins have no fallback")
                .sheet(sheet),
        }
    }

    /// Whether the skin supplied this sheet (balance counts when derived from volume).
    pub fn has_sheet(&self, sheet: Sheet) -> bool {
        self.sheets[sheet as usize].is_some()
    }

    /// The time display uses `nums_ex.bmp` (which has a minus sign) when the skin has it,
    /// or when it has neither digit sheet; otherwise `numbers.bmp`.
    pub fn uses_nums_ex(&self) -> bool {
        self.has_sheet(Sheet::NumsEx) || !self.has_sheet(Sheet::Numbers)
    }
}
```

`crates/skin/src/debug.rs` (`wsz()` is added in Task 6):

```rust
//! The debug skin: every sprite a flat colour of its own, so a wrong sprite or offset is
//! obvious in golden images. Sprites are painted in table order, so where sprites overlap
//! the later one shows. Each `text.bmp` cell gets a colour too.

use crate::image::{Image, Rect};
use crate::skin::Skin;
use crate::sprites::{SPRITES, Sheet};
use crate::text::{GLYPH_H, GLYPH_W};

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
```

`crates/skin/src/lib.rs`:

```rust
//! Winrust skin engine: loads classic Winamp 2.x skins (`.wsz`, `.zip` or a directory) and
//! maps their sprites. Pure: no windowing or OS dependencies.

pub mod bmp;
pub mod debug;
pub mod image;
pub mod ini;
pub mod pledit;
pub mod region;
pub mod skin;
pub mod sprites;
pub mod text;
pub mod viscolor;

pub use image::{Image, Rect};
pub use skin::{Skin, SpriteRef};
pub use sprites::{Sheet, SpriteId};
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p winrust-skin --test debug_skin`
Expected: `test result: ok. 6 passed`

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy -p winrust-skin --all-targets -- -D warnings`
Expected: no warnings.

```bash
git add crates/skin
git commit -m "feat(skin): add Skin with sprite lookup and the debug skin"
```

---

### Task 6: Loading skins from archives and directories

**Files:**
- Create: `crates/skin/src/files.rs`
- Modify: `crates/skin/src/skin.rs` (full replacement below), `crates/skin/src/debug.rs` (add `wsz()`), `crates/skin/src/lib.rs`
- Test: `crates/skin/tests/common/mod.rs`, `crates/skin/tests/loading.rs`

**Interfaces:**
- Consumes: `bmp::decode_any`, `bmp::encode24`, the parsers, `Skin::complete` and `debug::sheets`.
- Produces:
  - `Skin::load(&Path) -> Result<Skin, SkinError>` and `Skin::from_zip_bytes(&[u8]) -> Result<Skin, SkinError>`.
  - `SkinError::{Io { path, source }, NotASkin(String)}`.
  - `skin::CURSORS: [&str; 24]` and `skin::MAX_ARCHIVE`.
  - `debug::wsz() -> Vec<u8>`.
  - Test helpers `common::{zip, solid_bmp}`.

- [ ] **Step 1: Write the test helpers and failing tests**

`crates/skin/tests/common/mod.rs`:

```rust
#![allow(dead_code)]

use std::io::{Cursor, Write};
use winrust_skin::{Image, bmp};

pub fn solid_bmp(w: u32, h: u32, color: u32) -> Vec<u8> {
    bmp::encode24(&Image::new(w, h, color))
}

/// A deflated zip of (name, contents) entries, in order.
pub fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
    out.into_inner()
}
```

`crates/skin/tests/loading.rs`:

```rust
mod common;

use common::*;
use winrust_skin::sprites::SPRITES;
use winrust_skin::{Sheet, Skin, SkinError, SpriteId, debug};

const RED: u32 = 0xFFFF_0000;
const GREEN: u32 = 0xFF00_FF00;
const BLUE: u32 = 0xFF00_00FF;

fn is_base(skin: &Skin, id: SpriteId) -> bool {
    std::ptr::eq(skin.sprite(id).image, Skin::base().sprite(id).image)
}

fn pixel(skin: &Skin, id: SpriteId, dx: u32, dy: u32) -> u32 {
    let s = skin.sprite(id);
    s.image.get(s.rect.x + dx, s.rect.y + dy)
}

#[test]
fn debug_skin_round_trips_through_a_wsz() {
    let skin = Skin::from_zip_bytes(&debug::wsz()).unwrap();
    assert!(skin.warnings.is_empty(), "{:?}", skin.warnings);
    let expected = debug::sheets();
    for def in &SPRITES {
        assert!(
            !is_base(&skin, def.id),
            "{} came from the base skin",
            def.name
        );
        let sheet = &expected[def.sheet as usize];
        for dy in 0..def.rect.h {
            for dx in 0..def.rect.w {
                assert_eq!(
                    pixel(&skin, def.id, dx, dy),
                    sheet.get(def.rect.x + dx, def.rect.y + dy),
                    "{} at +{dx},+{dy}",
                    def.name
                );
            }
        }
    }
    assert_eq!(skin.sheet(Sheet::Text), &expected[Sheet::Text as usize]);
}

#[test]
fn missing_sheets_come_from_the_base_skin() {
    let skin = Skin::from_zip_bytes(&zip(&[("main.bmp", &solid_bmp(275, 116, RED))])).unwrap();
    assert_eq!(pixel(&skin, SpriteId::MainWindowBackground, 10, 10), RED);
    assert!(!is_base(&skin, SpriteId::MainWindowBackground));
    assert!(is_base(&skin, SpriteId::MainPlayButton));
    assert!(!skin.has_sheet(Sheet::Cbuttons));
    assert_eq!(skin.sheet(Sheet::Text), Skin::base().sheet(Sheet::Text));
}

#[test]
fn small_sheet_is_padded_black_and_unreached_sprites_use_base() {
    // A 30×20 titlebar.bmp reaches MAIN_TITLE_BAR (27,15) but not MAIN_SHADE_BACKGROUND (27,42).
    let skin = Skin::from_zip_bytes(&zip(&[("titlebar.bmp", &solid_bmp(30, 20, GREEN))])).unwrap();
    assert!(!is_base(&skin, SpriteId::MainTitleBar));
    assert_eq!(pixel(&skin, SpriteId::MainTitleBar, 0, 0), GREEN);
    assert_eq!(
        pixel(&skin, SpriteId::MainTitleBar, 3, 0),
        winrust_skin::image::BLACK
    );
    assert_eq!(
        pixel(&skin, SpriteId::MainTitleBar, 0, 5),
        winrust_skin::image::BLACK
    );
    assert!(is_base(&skin, SpriteId::MainShadeBackground));
}

#[test]
fn balance_comes_from_volume_when_missing() {
    let skin = Skin::from_zip_bytes(&zip(&[("volume.bmp", &solid_bmp(68, 433, BLUE))])).unwrap();
    assert!(skin.has_sheet(Sheet::Balance));
    assert_eq!(pixel(&skin, SpriteId::MainBalanceBackground, 0, 0), BLUE);
    assert!(std::ptr::eq(
        skin.sprite(SpriteId::MainBalanceBackground).image,
        skin.sprite(SpriteId::MainVolumeBackground).image
    ));
}

#[test]
fn nums_ex_preferred_over_numbers() {
    let load = |names: &[&str]| {
        let bmp = solid_bmp(108, 13, RED);
        let entries: Vec<(&str, &[u8])> = names.iter().map(|n| (*n, bmp.as_slice())).collect();
        Skin::from_zip_bytes(&zip(&entries)).unwrap().uses_nums_ex()
    };
    assert!(!load(&["numbers.bmp"]));
    assert!(load(&["numbers.bmp", "nums_ex.bmp"]));
    assert!(load(&["main.bmp"]));
}

#[test]
fn last_duplicate_wins_across_folders_case_and_extension() {
    let png = {
        let mut out = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(275, 116, image::Rgba([0, 0, 255, 255]))
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    };
    let red = solid_bmp(275, 116, RED);
    let green = solid_bmp(275, 116, GREEN);
    let skin =
        Skin::from_zip_bytes(&zip(&[("main.bmp", &red), ("Skin/MAIN.BMP", &green)])).unwrap();
    assert_eq!(pixel(&skin, SpriteId::MainWindowBackground, 0, 0), GREEN);
    let skin = Skin::from_zip_bytes(&zip(&[("Main.bmp", &red), ("x\\main.png", &png)])).unwrap();
    assert_eq!(pixel(&skin, SpriteId::MainWindowBackground, 0, 0), BLUE);
}

#[test]
fn reads_text_files_and_cursors_from_any_folder() {
    let skin = Skin::from_zip_bytes(&zip(&[
        (
            "a/b/PLEdit.TXT",
            b"[Text]\r\nNormal=#112233\r\nFont=\xC4rial\r\n",
        ),
        ("VISCOLOR.TXT", b"1,2,3\n"),
        (
            "region.txt",
            b"[Normal]\nNumPoints=3\nPointList=0,0,1,0,1,1\n",
        ),
        ("Close.cur", b"RIFF...."),
        ("readme.txt", b"hello"),
    ]))
    .unwrap();
    assert_eq!(skin.playlist.normal, 0xFF11_2233);
    assert_eq!(skin.playlist.font, "Ärial");
    assert_eq!(skin.vis_colors[0], 0xFF01_0203);
    assert_eq!(skin.regions.normal, vec![vec![(0, 0), (1, 0), (1, 1)]]);
    assert_eq!(
        skin.cursors.get("CLOSE").map(Vec::as_slice),
        Some(&b"RIFF...."[..])
    );
    assert_eq!(skin.cursors.len(), 1);
}

#[test]
fn unreadable_bitmap_warns_and_falls_back() {
    let skin = Skin::from_zip_bytes(&zip(&[
        ("main.bmp", b"BM garbage"),
        ("cbuttons.bmp", &solid_bmp(136, 36, RED)),
    ]))
    .unwrap();
    assert!(is_base(&skin, SpriteId::MainWindowBackground));
    assert!(
        skin.warnings.iter().any(|w| w.contains("main.bmp")),
        "{:?}",
        skin.warnings
    );
}

/// Rewrites every entry's compression method (local and central headers).
fn set_method(zip: &mut [u8], method: u16) {
    for i in 0..zip.len().saturating_sub(4) {
        let at = match &zip[i..i + 4] {
            b"PK\x03\x04" => i + 8,
            b"PK\x01\x02" => i + 10,
            _ => continue,
        };
        zip[at..at + 2].copy_from_slice(&method.to_le_bytes());
    }
}

#[test]
fn entry_with_unsupported_compression_warns_and_falls_back() {
    let mut out = std::io::Cursor::new(Vec::new());
    {
        use std::io::Write;
        let mut z = zip::ZipWriter::new(&mut out);
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        z.start_file("main.bmp", stored).unwrap();
        z.write_all(&solid_bmp(275, 116, RED)).unwrap();
        z.finish().unwrap();
    }
    let mut bytes = out.into_inner();
    set_method(&mut bytes, 6); // PKZIP "implode", used by some 1990s archivers
    match Skin::from_zip_bytes(&bytes) {
        Ok(skin) => {
            assert!(is_base(&skin, SpriteId::MainWindowBackground));
            assert!(!skin.warnings.is_empty());
        }
        Err(SkinError::NotASkin(_)) => {}
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn rejects_non_skins() {
    assert!(matches!(
        Skin::from_zip_bytes(b"hello"),
        Err(SkinError::NotASkin(_))
    ));
    let only_readme = zip(&[("readme.txt", b"hi")]);
    assert!(matches!(
        Skin::from_zip_bytes(&only_readme),
        Err(SkinError::NotASkin(_))
    ));
}

#[test]
fn loads_files_and_directories() {
    let dir = tempfile::tempdir().unwrap();
    let wsz = dir.path().join("test.wsz");
    std::fs::write(&wsz, zip(&[("main.bmp", &solid_bmp(275, 116, RED))])).unwrap();
    let skin = Skin::load(&wsz).unwrap();
    assert_eq!(pixel(&skin, SpriteId::MainWindowBackground, 0, 0), RED);

    let folder = dir.path().join("Folder Skin").join("Inner");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("MAIN.BMP"), solid_bmp(275, 116, GREEN)).unwrap();
    let skin = Skin::load(&dir.path().join("Folder Skin")).unwrap();
    assert_eq!(pixel(&skin, SpriteId::MainWindowBackground, 0, 0), GREEN);

    assert!(matches!(
        Skin::load(&dir.path().join("missing.wsz")),
        Err(SkinError::Io { .. })
    ));
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p winrust-skin --test loading`
Expected: compile errors, starting with `unresolved import `winrust_skin::SkinError`` (E0432) and `cannot find function `wsz` in module `debug`` (E0425).

- [ ] **Step 3: Implement**

`crates/skin/src/files.rs`:

```rust
//! Finds a skin's files in a zip archive (`.wsz`) or a directory.
//!
//! Files are matched by name alone, at any folder depth, ignoring case. When a name occurs
//! more than once the last one wins (archive order; sorted path order in a directory), as
//! when Winamp extracts an archive over itself. Only files a skin can use are read, each
//! at most [`MAX_FILE`] bytes; anything unreadable becomes a warning.

use crate::skin::CURSORS;
use crate::sprites::Sheet;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::Path;

pub const MAX_FILE: u64 = 16 << 20;
const MAX_ENTRIES: usize = 10_000;
const MAX_DEPTH: usize = 4;

#[derive(Default)]
pub struct SkinFiles {
    /// Lower-case file name → (arrival order, contents).
    files: HashMap<String, (usize, Vec<u8>)>,
    arrivals: usize,
    pub warnings: Vec<String>,
}

fn wanted(name: &str) -> bool {
    if let Some(stem) = name
        .strip_suffix(".bmp")
        .or_else(|| name.strip_suffix(".png"))
    {
        return Sheet::ALL.iter().any(|s| s.stem() == stem);
    }
    if let Some(stem) = name.strip_suffix(".cur") {
        return CURSORS.iter().any(|c| c.eq_ignore_ascii_case(stem));
    }
    matches!(name, "pledit.txt" | "viscolor.txt" | "region.txt")
}

impl SkinFiles {
    pub fn from_zip(bytes: &[u8]) -> Result<SkinFiles, String> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
        if archive.len() > MAX_ENTRIES {
            return Err(format!("{} entries is too many", archive.len()));
        }
        let mut out = SkinFiles::default();
        for i in 0..archive.len() {
            let mut entry = match archive.by_index(i) {
                Ok(entry) => entry,
                Err(e) => {
                    out.warnings.push(format!("archive entry {i}: {e}"));
                    continue;
                }
            };
            let Ok(path) = entry.name().map(|n| n.into_owned()) else {
                out.warnings
                    .push(format!("archive entry {i}: unreadable name"));
                continue;
            };
            let name = path.rsplit(['/', '\\']).next().unwrap_or("").to_lowercase();
            if entry.is_dir() || !wanted(&name) {
                continue;
            }
            let mut data = Vec::new();
            match entry.by_ref().take(MAX_FILE + 1).read_to_end(&mut data) {
                Ok(_) if data.len() as u64 > MAX_FILE => out
                    .warnings
                    .push(format!("{path}: larger than {MAX_FILE} bytes")),
                Ok(_) => out.insert(name, data),
                Err(e) => out.warnings.push(format!("{path}: {e}")),
            }
        }
        Ok(out)
    }

    pub fn from_dir(dir: &Path) -> std::io::Result<SkinFiles> {
        let mut out = SkinFiles::default();
        out.walk(dir, 0, true)?;
        Ok(out)
    }

    fn walk(&mut self, dir: &Path, depth: usize, top: bool) -> std::io::Result<()> {
        let mut entries = match std::fs::read_dir(dir) {
            Ok(rd) => rd
                .filter_map(Result::ok)
                .map(|e| e.path())
                .collect::<Vec<_>>(),
            Err(e) if !top => {
                self.warnings.push(format!("{}: {e}", dir.display()));
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        entries.sort();
        for path in entries {
            if path.is_dir() {
                if depth < MAX_DEPTH {
                    self.walk(&path, depth + 1, false)?;
                }
                continue;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if !wanted(&name) {
                continue;
            }
            match std::fs::metadata(&path) {
                Ok(m) if m.len() > MAX_FILE => self
                    .warnings
                    .push(format!("{}: larger than {MAX_FILE} bytes", path.display())),
                Ok(_) => match std::fs::read(&path) {
                    Ok(data) => self.insert(name, data),
                    Err(e) => self.warnings.push(format!("{}: {e}", path.display())),
                },
                Err(e) => self.warnings.push(format!("{}: {e}", path.display())),
            }
        }
        Ok(())
    }

    fn insert(&mut self, name: String, data: Vec<u8>) {
        self.arrivals += 1;
        self.files.insert(name, (self.arrivals, data));
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Contents of the file with this lower-case name.
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.files.get(name).map(|(_, data)| data.as_slice())
    }

    /// The `<stem>.bmp` or `<stem>.png` that arrived last, with its name.
    pub fn image(&self, stem: &str) -> Option<(String, &[u8])> {
        ["bmp", "png"]
            .iter()
            .filter_map(|ext| {
                let name = format!("{stem}.{ext}");
                let (order, data) = self.files.get(&name)?;
                Some((*order, name, data.as_slice()))
            })
            .max_by_key(|(order, _, _)| *order)
            .map(|(_, name, data)| (name, data))
    }
}
```

Replace `crates/skin/src/skin.rs` with:

```rust
//! A loaded skin: decoded sheets, playlist style, visualization colours, window regions and
//! cursors. Whatever a skin lacks comes from the base skin.

use crate::bmp;
use crate::files::SkinFiles;
use crate::image::{Image, Rect};
use crate::ini::decode_text;
use crate::pledit::{self, PlaylistStyle};
use crate::region::{self, Regions};
use crate::sprites::{Sheet, SpriteId};
use crate::viscolor;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// Largest skin archive accepted.
pub const MAX_ARCHIVE: u64 = 64 << 20;

/// Cursor files Winamp 2.x reads (`<NAME>.cur`; may hold animated RIFF data).
pub const CURSORS: [&str; 24] = [
    "CLOSE", "EQCLOSE", "EQNORMAL", "EQSLID", "EQTITLE", "MAINMENU", "MMENU", "MIN", "NORMAL",
    "PCLOSE", "PNORMAL", "POSBAR", "PSIZE", "PTBAR", "PVSCROLL", "PWINBUT", "PWSNORM", "PWSSIZE",
    "SONGNAME", "TITLEBAR", "VOLBAL", "WINBUT", "WSNORMAL", "WSPOSBAR",
];

#[derive(Debug, thiserror::Error)]
pub enum SkinError {
    #[error("cannot read {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("not a skin: {0}")]
    NotASkin(String),
}

struct SheetImage {
    /// Padded to at least `Sheet::size()`, so every sprite rectangle is inside it.
    image: Image,
    /// Size as decoded, before padding.
    width: u32,
    height: u32,
}

/// Where to blit a sprite from.
#[derive(Clone, Copy, Debug)]
pub struct SpriteRef<'a> {
    pub image: &'a Image,
    pub rect: Rect,
}

pub struct Skin {
    sheets: Vec<Option<Arc<SheetImage>>>,
    fallback: Option<Arc<Skin>>,
    pub playlist: PlaylistStyle,
    pub vis_colors: [u32; 24],
    pub regions: Regions,
    /// Raw `.cur` file contents by cursor name (decoded by the app).
    pub cursors: BTreeMap<&'static str, Vec<u8>>,
    /// Problems that did not stop loading (unreadable bitmaps, bad archive entries).
    pub warnings: Vec<String>,
}

impl Skin {
    /// Loads a `.wsz`/`.zip` file or a skin directory.
    pub fn load(path: &Path) -> Result<Skin, SkinError> {
        let io = |source| SkinError::Io {
            path: path.to_path_buf(),
            source,
        };
        if path.is_dir() {
            return Self::from_files(SkinFiles::from_dir(path).map_err(io)?);
        }
        let len = std::fs::metadata(path).map_err(io)?.len();
        if len > MAX_ARCHIVE {
            return Err(SkinError::NotASkin(format!("{len} bytes is too large")));
        }
        Self::from_zip_bytes(&std::fs::read(path).map_err(io)?)
    }

    pub fn from_zip_bytes(bytes: &[u8]) -> Result<Skin, SkinError> {
        Self::from_files(SkinFiles::from_zip(bytes).map_err(SkinError::NotASkin)?)
    }

    fn from_files(files: SkinFiles) -> Result<Skin, SkinError> {
        if files.is_empty() {
            return Err(SkinError::NotASkin("no skin files found".into()));
        }
        let mut warnings = files.warnings.clone();
        let mut sheets: Vec<Option<Arc<SheetImage>>> = vec![None; Sheet::ALL.len()];
        for sheet in Sheet::ALL {
            let Some((name, data)) = files.image(sheet.stem()) else {
                continue;
            };
            match bmp::decode_any(data) {
                Ok(img) => {
                    let (w, h) = sheet.size();
                    sheets[sheet as usize] = Some(Arc::new(SheetImage {
                        width: img.width,
                        height: img.height,
                        image: img.padded(w, h),
                    }));
                }
                Err(e) => warnings.push(format!("{name}: {e}")),
            }
        }
        // Without balance.bmp, Winamp draws the balance slider from volume.bmp.
        if sheets[Sheet::Balance as usize].is_none() {
            sheets[Sheet::Balance as usize] = sheets[Sheet::Volume as usize].clone();
        }
        let text = |name: &str| files.get(name).map(decode_text);
        Ok(Skin {
            sheets,
            fallback: Some(Skin::base()),
            playlist: text("pledit.txt")
                .map(|t| pledit::parse(&t))
                .unwrap_or_default(),
            vis_colors: text("viscolor.txt")
                .map(|t| viscolor::parse(&t))
                .unwrap_or(viscolor::DEFAULT_COLORS),
            regions: text("region.txt")
                .map(|t| region::parse(&t))
                .unwrap_or_default(),
            cursors: CURSORS
                .iter()
                .filter_map(|&c| {
                    let data = files.get(&format!("{}.cur", c.to_ascii_lowercase()))?;
                    Some((c, data.to_vec()))
                })
                .collect(),
            warnings,
        })
    }

    /// A skin with every sheet given, in `Sheet::ALL` order, and nothing to fall back on.
    pub(crate) fn complete(sheets: Vec<Image>) -> Skin {
        Skin {
            sheets: sheets
                .into_iter()
                .map(|image| {
                    Some(Arc::new(SheetImage {
                        width: image.width,
                        height: image.height,
                        image,
                    }))
                })
                .collect(),
            fallback: None,
            playlist: PlaylistStyle::default(),
            vis_colors: viscolor::DEFAULT_COLORS,
            regions: Regions::default(),
            cursors: BTreeMap::new(),
            warnings: Vec::new(),
        }
    }

    /// The built-in skin used for anything a skin lacks. Until the Winrust Base skin is
    /// generated (M2 part 2) this is the debug skin.
    pub fn base() -> Arc<Skin> {
        static BASE: OnceLock<Arc<Skin>> = OnceLock::new();
        BASE.get_or_init(|| Arc::new(Skin::debug())).clone()
    }

    /// The sprite's pixels: from this skin when its sheet reaches the sprite (missing
    /// pixels of a partly covered sprite are black), otherwise from the base skin.
    pub fn sprite(&self, id: SpriteId) -> SpriteRef<'_> {
        let def = id.def();
        match &self.sheets[def.sheet as usize] {
            Some(s) if def.rect.x < s.width && def.rect.y < s.height => SpriteRef {
                image: &s.image,
                rect: def.rect,
            },
            _ => self
                .fallback
                .as_ref()
                .expect("only complete skins have no fallback")
                .sprite(id),
        }
    }

    /// A whole sheet (e.g. `text.bmp` for [`crate::text::glyph`] cells).
    pub fn sheet(&self, sheet: Sheet) -> &Image {
        match &self.sheets[sheet as usize] {
            Some(s) => &s.image,
            None => self
                .fallback
                .as_ref()
                .expect("only complete skins have no fallback")
                .sheet(sheet),
        }
    }

    /// Whether the skin supplied this sheet (balance counts when derived from volume).
    pub fn has_sheet(&self, sheet: Sheet) -> bool {
        self.sheets[sheet as usize].is_some()
    }

    /// The time display uses `nums_ex.bmp` (which has a minus sign) when the skin has it,
    /// or when it has neither digit sheet; otherwise `numbers.bmp`.
    pub fn uses_nums_ex(&self) -> bool {
        self.has_sheet(Sheet::NumsEx) || !self.has_sheet(Sheet::Numbers)
    }
}
```

Replace `crates/skin/src/debug.rs` with:

```rust
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
```

`crates/skin/src/lib.rs`:

```rust
//! Winrust skin engine: loads classic Winamp 2.x skins (`.wsz`, `.zip` or a directory) and
//! maps their sprites. Pure: no windowing or OS dependencies.

pub mod bmp;
pub mod debug;
mod files;
pub mod image;
pub mod ini;
pub mod pledit;
pub mod region;
pub mod skin;
pub mod sprites;
pub mod text;
pub mod viscolor;

pub use image::{Image, Rect};
pub use skin::{Skin, SkinError, SpriteRef};
pub use sprites::{Sheet, SpriteId};
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p winrust-skin`
Expected: `loading` reports `test result: ok. 11 passed`. Every other test file passes as in earlier tasks.

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy -p winrust-skin --all-targets -- -D warnings`
Expected: no warnings.

```bash
git add crates/skin
git commit -m "feat(skin): load skins from .wsz/.zip archives and directories with base fallback"
```

---

### Task 7: Contact sheet, golden test and skin tool

**Files:**
- Create: `crates/skin/src/contact.rs`, `crates/skin/examples/skin-tool.rs`, `crates/skin/tests/golden.rs`, `crates/skin/tests/golden/debug-contact.png` (blessed)
- Modify: `crates/skin/src/image.rs` (add `to_png`), `crates/skin/src/lib.rs`, `crates/skin/tests/common/mod.rs` (add `assert_golden`)
- Test: `crates/skin/tests/golden.rs`

**Interfaces:**
- Produces:
  - `contact_sheet(&Skin) -> Image`: every sprite in table order, then the text sheet, 640 px wide on magenta.
  - `Image::to_png() -> Vec<u8>`.
  - Test helper `common::assert_golden(name, &Image)`, which compares with `tests/golden/<name>.png` and is regenerated by `WINRUST_BLESS=1`. M3's compositor golden tests reuse this pattern.

- [ ] **Step 1: Write the failing tests**

Replace `crates/skin/tests/common/mod.rs` with:

```rust
#![allow(dead_code)]

use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use winrust_skin::{Image, bmp};

pub fn solid_bmp(w: u32, h: u32, color: u32) -> Vec<u8> {
    bmp::encode24(&Image::new(w, h, color))
}

/// A deflated zip of (name, contents) entries, in order.
pub fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
    out.into_inner()
}

/// Compares `img` with `tests/golden/<name>.png`. `WINRUST_BLESS=1` rewrites the file.
pub fn assert_golden(name: &str, img: &Image) {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "golden",
        &format!("{name}.png"),
    ]
    .iter()
    .collect();
    if std::env::var_os("WINRUST_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, img.to_png()).unwrap();
        return;
    }
    let expected = std::fs::read(&path)
        .ok()
        .and_then(|png| bmp::decode_any(&png).ok())
        .unwrap_or_else(|| panic!("missing {}; run with WINRUST_BLESS=1", path.display()));
    if expected != *img {
        let actual = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.actual.png"));
        std::fs::write(&actual, img.to_png()).unwrap();
        let differ = if (expected.width, expected.height) == (img.width, img.height) {
            let n = expected
                .pixels
                .iter()
                .zip(&img.pixels)
                .filter(|(a, b)| a != b)
                .count();
            format!("{n} pixels differ")
        } else {
            format!(
                "size {}x{} != {}x{}",
                img.width, img.height, expected.width, expected.height
            )
        };
        panic!(
            "golden {name}: {differ}; actual image at {}",
            actual.display()
        );
    }
}
```

`crates/skin/tests/golden.rs`:

```rust
mod common;

use common::assert_golden;
use winrust_skin::{Image, Skin, bmp, contact_sheet};

#[test]
fn png_round_trip() {
    let mut img = Image::new(2, 1, 0xFF01_0203);
    img.set(1, 0, 0xFFFF_FFFF);
    assert_eq!(bmp::decode_any(&img.to_png()).unwrap(), img);
}

#[test]
fn debug_skin_contact_sheet() {
    assert_golden("debug-contact", &contact_sheet(&Skin::debug()));
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p winrust-skin --test golden`
Expected: compile errors `no method named `to_png`` and `unresolved import `winrust_skin::contact_sheet``.

- [ ] **Step 3: Implement**

In `crates/skin/src/image.rs`, add this method at the end of `impl Image` (after `padded`):

```rust
    /// The image as a PNG file (for golden tests and dev tools).
    pub fn to_png(&self) -> Vec<u8> {
        let rgba: Vec<u8> = self
            .pixels
            .iter()
            .flat_map(|p| {
                let [a, r, g, b] = p.to_be_bytes();
                [r, g, b, a]
            })
            .collect();
        let buffer = image::RgbaImage::from_raw(self.width, self.height, rgba)
            .expect("pixel count matches the size");
        let mut out = std::io::Cursor::new(Vec::new());
        buffer
            .write_to(&mut out, image::ImageFormat::Png)
            .expect("encoding to memory");
        out.into_inner()
    }
```

`crates/skin/src/contact.rs`:

```rust
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
```

`crates/skin/src/lib.rs`:

```rust
//! Winrust skin engine: loads classic Winamp 2.x skins (`.wsz`, `.zip` or a directory) and
//! maps their sprites. Pure: no windowing or OS dependencies.

pub mod bmp;
pub mod contact;
pub mod debug;
mod files;
pub mod image;
pub mod ini;
pub mod pledit;
pub mod region;
pub mod skin;
pub mod sprites;
pub mod text;
pub mod viscolor;

pub use contact::contact_sheet;
pub use image::{Image, Rect};
pub use skin::{Skin, SkinError, SpriteRef};
pub use sprites::{Sheet, SpriteId};
```

- [ ] **Step 4: Run, bless, and look at the snapshot**

Run: `cargo test -p winrust-skin --test golden`
Expected: `png_round_trip` passes; `debug_skin_contact_sheet` fails with `missing ...debug-contact.png; run with WINRUST_BLESS=1`.

Run: `WINRUST_BLESS=1 cargo test -p winrust-skin --test golden`, then `cargo test -p winrust-skin --test golden`
Expected: `test result: ok. 2 passed`. Open `crates/skin/tests/golden/debug-contact.png` (640×1514). It should show flat coloured blocks on magenta:
- The 38×420 balance strip in the top-left.
- The 275×116 main background.
- The playlist, EQ and title-bar pieces.
- The 155×18 multi-coloured text sheet near the bottom, beside the 68×420 volume strip.

- [ ] **Step 5: Add the skin tool**

`crates/skin/examples/skin-tool.rs`:

```rust
//! Skin development tools.
//!
//! `cargo run -p winrust-skin --example skin-tool -- contact <skin.wsz|dir> <out.png>`
//! `cargo run -p winrust-skin --example skin-tool -- debug-wsz <out.wsz>`

use std::path::Path;
use std::process::ExitCode;
use winrust_skin::{Skin, contact_sheet, debug};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["contact", skin, out] => contact(Path::new(skin), Path::new(out)),
        ["debug-wsz", out] => std::fs::write(out, debug::wsz()).map_err(|e| e.to_string()),
        _ => Err("usage: skin-tool contact <skin> <out.png> | debug-wsz <out.wsz>".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn contact(skin: &Path, out: &Path) -> Result<(), String> {
    let skin = Skin::load(skin).map_err(|e| e.to_string())?;
    for warning in &skin.warnings {
        eprintln!("warning: {warning}");
    }
    std::fs::write(out, contact_sheet(&skin).to_png()).map_err(|e| e.to_string())
}
```

Run: `cargo run -p winrust-skin --example skin-tool -- debug-wsz <scratch>/debug.wsz && cargo run -p winrust-skin --example skin-tool -- contact <scratch>/debug.wsz <scratch>/debug.png`
Expected: both succeed with no warnings printed, and `debug.png` is pixel-identical to the golden file. Check with `cmp`, or by comparing the decoded images.

- [ ] **Step 6: Lint and commit**

Run: `cargo clippy -p winrust-skin --all-targets -- -D warnings`
Expected: no warnings.

```bash
git add crates/skin
git commit -m "feat(skin): add contact sheets, golden image test and skin-tool example"
```

---

### Task 8: Robustness: mutation tests and fuzzing

**Files:**
- Create: `crates/skin/tests/robustness.rs`, `fuzz/Cargo.toml`, `fuzz/fuzz_targets/bmp.rs`, `fuzz/fuzz_targets/skin_archive.rs`, `fuzz/fuzz_targets/icy.rs`, `.github/workflows/fuzz.yml`
- Modify: `.gitignore`
- Test: `crates/skin/tests/robustness.rs`

**Interfaces:**
- Consumes: `bmp::decode_any`, `Skin::from_zip_bytes`, and `winrust_audio::icy::{IcyReader, parse_stream_title}`. The `icy` functions are already public; the `IcyReader` titles sender is a `crossbeam_channel::Sender<String>`.

- [ ] **Step 1: Write the mutation tests**

`crates/skin/tests/robustness.rs`:

```rust
//! Damaged bitmaps and archives must load or fail cleanly, never panic. (Deeper fuzzing
//! runs nightly in CI: see `fuzz/`.)

mod common;

use common::*;
use winrust_skin::{Image, Skin, bmp};

/// xorshift64: deterministic and dependency-free.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn mutate(rng: &mut Rng, data: &[u8]) -> Vec<u8> {
    let mut out = data.to_vec();
    for _ in 0..1 + rng.below(8) {
        let i = rng.below(out.len());
        out[i] = rng.next() as u8;
    }
    if rng.below(4) == 0 {
        out.truncate(rng.below(out.len() + 1));
    }
    out
}

#[test]
fn damaged_bitmaps_never_panic() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let mut img = Image::new(7, 5, 0xFF12_3456);
    img.set(3, 2, 0xFFFF_FFFF);
    let seed = bmp::encode24(&img);
    for _ in 0..5000 {
        let data = mutate(&mut rng, &seed);
        let _ = bmp::decode_any(&data);
    }
}

#[test]
fn damaged_archives_never_panic() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let seed = zip(&[
        ("main.bmp", &solid_bmp(8, 8, 0xFF00_FF00)),
        ("pledit.txt", b"[Text]\nNormal=#00FF00\n"),
        (
            "region.txt",
            b"[Normal]\nNumPoints=3\nPointList=0,0,1,0,1,1\n",
        ),
        ("viscolor.txt", b"1,2,3\n"),
    ]);
    for _ in 0..1000 {
        let data = mutate(&mut rng, &seed);
        let _ = Skin::from_zip_bytes(&data);
    }
}
```

- [ ] **Step 2: Prove the tests can fail**

The code under test already exists, so these guard tests pass at once. To show they bite, temporarily change the offset read at the top of `parse_header` in `crates/skin/src/bmp.rs` from `u32_at(data, 10).ok_or(BmpError::Truncated)?` to `u32_at(data, 10).unwrap()`.

Run: `cargo test -p winrust-skin --test robustness`
Expected: FAIL. `damaged_bitmaps_never_panic` panics: `called `Option::unwrap()` on a `None` value`.

Revert the change (`git checkout -- crates/skin/src/bmp.rs`).

- [ ] **Step 3: Run the tests**

Run: `cargo test -p winrust-skin --test robustness`
Expected: `test result: ok. 2 passed`

- [ ] **Step 4: Add the fuzz targets**

`fuzz/Cargo.toml`:

```toml
[package]
name = "winrust-fuzz"
version = "0.0.0"
edition = "2024"
publish = false

[package.metadata]
cargo-fuzz = true

[dependencies]
crossbeam-channel = "0.5.17"
libfuzzer-sys = "0.4.13"
winrust-audio = { path = "../crates/audio" }
winrust-skin = { path = "../crates/skin" }

# Not part of the main workspace: cargo-fuzz needs nightly.
[workspace]

[[bin]]
name = "bmp"
path = "fuzz_targets/bmp.rs"
test = false
doc = false
bench = false

[[bin]]
name = "skin_archive"
path = "fuzz_targets/skin_archive.rs"
test = false
doc = false
bench = false

[[bin]]
name = "icy"
path = "fuzz_targets/icy.rs"
test = false
doc = false
bench = false
```

`fuzz/fuzz_targets/bmp.rs`:

```rust
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = winrust_skin::bmp::decode_any(data);
});
```

`fuzz/fuzz_targets/skin_archive.rs`:

```rust
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = winrust_skin::Skin::from_zip_bytes(data);
});
```

`fuzz/fuzz_targets/icy.rs`:

```rust
#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Read;
use winrust_audio::icy::{IcyReader, parse_stream_title};

// First byte picks the metadata interval; the rest is the stream.
fuzz_target!(|data: &[u8]| {
    let Some((&interval, stream)) = data.split_first() else {
        return;
    };
    let (titles, _rx) = crossbeam_channel::unbounded();
    let mut reader = IcyReader::new(stream, usize::from(interval) + 1, titles);
    let _ = reader.read_to_end(&mut Vec::new());
    let _ = parse_stream_title(stream);
});
```

Append to `.gitignore`:

```
/fuzz/Cargo.lock
```

Run: `cargo check --manifest-path fuzz/Cargo.toml`
Expected: finishes without errors. Actual fuzzing needs nightly and runs in CI.

- [ ] **Step 5: Add the nightly fuzz workflow (§12.1)**

`.github/workflows/fuzz.yml`:

```yaml
name: Fuzz

on:
  schedule:
    - cron: "17 3 * * *"
  workflow_dispatch:
  pull_request:
    paths:
      - "fuzz/**"
      - "crates/skin/**"
      - "crates/audio/src/icy.rs"
      - ".github/workflows/fuzz.yml"

env:
  CARGO_TERM_COLOR: always

jobs:
  fuzz:
    name: fuzz (${{ matrix.target }})
    runs-on: ubuntu-latest
    strategy:
      fail-fast: false
      matrix:
        target: [bmp, skin_archive, icy]
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@nightly
      - name: Install ALSA headers
        run: sudo apt-get update && sudo apt-get install -y libasound2-dev pkg-config
      - run: cargo install cargo-fuzz --locked
      - name: Fuzz
        run: cargo fuzz run ${{ matrix.target }} -- -max_total_time=${{ github.event_name == 'pull_request' && 60 || 600 }}
      - uses: actions/upload-artifact@v4
        if: failure()
        with:
          name: fuzz-${{ matrix.target }}
          path: fuzz/artifacts
```

- [ ] **Step 6: Commit**

```bash
git add crates/skin/tests/robustness.rs fuzz .github/workflows/fuzz.yml .gitignore
git commit -m "test(skin): add mutation tests and cargo-fuzz targets with a nightly fuzz job"
```

The fuzz workflow is verified when the branch's pull request runs it (Final Verification).

---

### Task 9: Real-skin check and documentation

**Files:**
- Create: `crates/skin/tests/real_skins.rs`
- Modify: `README.md`
- Test: `crates/skin/tests/real_skins.rs` (ignored unless `WINRUST_SKINS_DIR` is set)

- [ ] **Step 1: Write the real-skin test**

`crates/skin/tests/real_skins.rs`:

```rust
//! Loads every skin in `WINRUST_SKINS_DIR` and writes a contact sheet for each, to compare
//! with Webamp by eye. No real skins are committed: they belong to their authors. Run:
//!
//! `WINRUST_SKINS_DIR=<dir> cargo test -p winrust-skin --test real_skins -- --ignored --nocapture`

use std::path::Path;
use winrust_skin::{Skin, contact_sheet};

#[test]
#[ignore = "needs WINRUST_SKINS_DIR"]
fn real_skins_load() {
    let dir = std::env::var_os("WINRUST_SKINS_DIR").expect("set WINRUST_SKINS_DIR");
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("real-skins");
    std::fs::create_dir_all(&out).unwrap();
    let (mut count, mut failures) = (0, Vec::new());
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let is_skin = path.is_dir()
            || path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("wsz") || e.eq_ignore_ascii_case("zip"));
        if !is_skin {
            continue;
        }
        count += 1;
        match Skin::load(&path) {
            Ok(skin) => {
                for w in &skin.warnings {
                    println!("{}: warning: {w}", path.display());
                }
                let name = path.file_stem().unwrap().to_string_lossy();
                let png = contact_sheet(&skin).to_png();
                std::fs::write(out.join(format!("{name}.png")), png).unwrap();
            }
            Err(e) => failures.push(format!("{}: {e}", path.display())),
        }
    }
    println!("{count} skins; contact sheets in {}", out.display());
    assert!(count > 0, "no skins found");
    assert!(failures.is_empty(), "{failures:#?}");
}
```

Run: `cargo test -p winrust-skin --test real_skins`
Expected: `test result: ok. 0 passed; 0 failed; 1 ignored`

- [ ] **Step 2: Download real skins to a scratch directory (never into the repository)**

Fetch 60 skins from the Winamp Skin Museum: the 40 most popular, which include `base-2.91.wsz` (md5 `5e4f10275dcb1fb211d4a8b4f1bda236`), and 20 from offset 5000 for older archives. Strip `\r` from the list on Windows.

```bash
mkdir -p <scratch>/realskins && cd <scratch>/realskins
for q in '{ skins(first: 40, offset: 0, sort: MUSEUM) { nodes { md5 download_url } } }' '{ skins(first: 20, offset: 5000, sort: MUSEUM) { nodes { md5 download_url } } }'; do
  curl -s -X POST https://api.webamp.org/graphql -H 'content-type: application/json' -d "{\"query\":\"$q\"}" \
    | python -I -c "import json,sys; [print(n['md5'], n['download_url']) for n in json.load(sys.stdin)['data']['skins']['nodes']]"
done | tr -d '\r' > list.txt
while read md5 url; do curl -fsSL "$url" -o "$md5.wsz"; done < list.txt
```

Expected: 60 `.wsz` files.

- [ ] **Step 3: Load them all**

Run: `WINRUST_SKINS_DIR=<scratch>/realskins cargo test -p winrust-skin --test real_skins -- --ignored --nocapture`
Expected: `60 skins; contact sheets in ...` and `test result: ok. 1 passed`. Warnings may be printed and should be recorded in the ledger. A load failure is a bug: debug it.

- [ ] **Step 4: Check the coordinates by eye**

Open `target/tmp/real-skins/5e4f10275dcb1fb211d4a8b4f1bda236.png` (base-2.91) and check that:
- The transport buttons, title bars (including the shade bars and the "IT REALLY WHIPS THE LLAMA'S ASS!" easter-egg bar), EQ background and sliders, and playlist frame pieces are each cut out whole, with no neighbouring art bleeding in.
- The text font strip and the 28-frame volume and balance strips appear intact.

Record the result in the ledger.

- [ ] **Step 5: Document**

In `README.md`, change the status block to:

```markdown
> **Status:** early development (milestone M2). The audio engine works and can be driven from
> the terminal with `winrust-cli`, and the skin engine loads classic `.wsz` skins. The skinned
> player window is not built yet.
```

and insert this section before `## License`:

````markdown
## Checking skins

The skin engine can lay out every sprite of a skin on one image, for comparing against
[Webamp](https://webamp.org):

```sh
cargo run -p winrust-skin --example skin-tool -- contact path/to/skin.wsz sheet.png
```

To load a whole folder of skins and write a contact sheet for each (no skins are committed;
they belong to their authors):

```sh
WINRUST_SKINS_DIR=path/to/skins cargo test -p winrust-skin --test real_skins -- --ignored --nocapture
```
````

- [ ] **Step 6: Full gates and commit**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo deny check`
Expected: all succeed. The workspace test total grows by 65 over M1's 115 (`winrust-skin`: 4 + 18 + 8 + 14 + 6 + 11 + 2 + 2 = 65, plus 1 ignored).

```bash
git add crates/skin/tests/real_skins.rs README.md
git commit -m "test(skin): add opt-in real-skin loading check; document skin tools"
```

---

## Final Verification

- Push the branch and open the pull request. CI (fmt, test on 3 OSes, deny) must pass.
- The **Fuzz** workflow runs on the pull request because `crates/skin/**` and `fuzz/**` changed. All three targets (`bmp`, `skin_archive`, `icy`) must run for 60 s each without a crash. A crash is a bug: reproduce it with a unit test in `robustness.rs` and fix it.

---

## Appendix A: `gen_sprites.py`

Transcribes Webamp's sprite table into `crates/skin/src/sprites.rs`. Kept here rather than in the repository so the repository holds no Python. Re-run it only to re-transcribe from a newer Webamp commit, and update the commit hash in its header when you do.

```python
"""Transcribes Webamp's skinSprites.ts into crates/skin/src/sprites.rs.

usage: python -I gen_sprites.py <skinSprites.ts> <out.rs>
Webamp commit: 999f5676143c5696af69e1b27db901f2d35f428b (MIT, (c) Jordan Eldredge).
"""
import re, sys

SHEETS = {  # Webamp key -> (Rust variant, file stem)
    "MAIN": ("Main", "main"), "CBUTTONS": ("Cbuttons", "cbuttons"),
    "TITLEBAR": ("Titlebar", "titlebar"), "NUMBERS": ("Numbers", "numbers"),
    "NUMS_EX": ("NumsEx", "nums_ex"), "TEXT": ("Text", "text"),
    "POSBAR": ("Posbar", "posbar"), "VOLUME": ("Volume", "volume"),
    "BALANCE": ("Balance", "balance"), "MONOSTER": ("Monoster", "monoster"),
    "PLAYPAUS": ("Playpaus", "playpaus"), "SHUFREP": ("Shufrep", "shufrep"),
    "EQMAIN": ("Eqmain", "eqmain"), "EQ_EX": ("EqEx", "eq_ex"), "PLEDIT": ("Pledit", "pledit"),
}
src = open(sys.argv[1], encoding="utf-8").read()
body = re.sub(r"/\*.*?\*/", "", src[src.index("const sprites: SpriteMap = {"):], flags=re.S)
pat = re.compile(r'\{\s*name:\s*"([A-Z0-9_]+)",\s*x:\s*(\d+),\s*y:\s*(\d+),\s*'
                 r'(width|height):\s*(\d+),\s*(width|height):\s*(\d+),?\s*\}')
sprites, sizes = [], {}
for key, content in re.findall(r"\n  ([A-Z_]+): \[(.*?)\n  \],", body, flags=re.S):
    if key == "GEN":
        continue  # gen.bmp is not used in v1 (spec §7.2)
    assert len(pat.findall(content)) == content.count("name:"), key
    for name, x, y, k1, v1, k2, v2 in pat.findall(content):
        d = {k1: int(v1), k2: int(v2)}
        sprites.append((name, key, int(x), int(y), d["width"], d["height"]))
for name, key, x, y, w, h in sprites:
    sw, sh = sizes.get(key, (0, 0))
    sizes[key] = (max(sw, x + w), max(sh, y + h))
sizes["TEXT"] = (31 * 5, 3 * 6)
names = [s[0] for s in sprites]
assert len(names) == len(set(names))
camel = lambda n: "".join(p.capitalize() for p in n.lower().split("_"))

out = []
w = out.append
w("//! Sprite coordinates of Winamp 2.x classic skins. Generated; do not edit by hand.")
w("//!")
w("//! Transcribed from Webamp's `skinSprites.ts` (MIT, Copyright (c) 2015 Jordan Eldredge),")
w("//! commit 999f5676143c5696af69e1b27db901f2d35f428b. `gen.bmp` sprites are left out.")
w("")
w("use crate::image::Rect;")
w("")
w("/// A bitmap file of a classic skin.")
w("#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]")
w("pub enum Sheet {")
for key, (v, _) in SHEETS.items():
    w(f"    {v},")
w("}")
w("")
w("impl Sheet {")
w(f"    pub const ALL: [Sheet; {len(SHEETS)}] = [")
for key, (v, _) in SHEETS.items():
    w(f"        Sheet::{v},")
w("    ];")
w("")
w("    /// File name without extension, lower case (`.bmp` or `.png` in the skin).")
w("    pub fn stem(self) -> &'static str {")
w("        match self {")
for key, (v, stem) in SHEETS.items():
    w(f'            Sheet::{v} => "{stem}",')
w("        }")
w("    }")
w("")
w("    /// Size that covers every sprite on the sheet. Smaller sheets are padded to it.")
w("    pub fn size(self) -> (u32, u32) {")
w("        match self {")
for key, (v, _) in SHEETS.items():
    sw, sh = sizes[key]
    w(f"            Sheet::{v} => ({sw}, {sh}),")
w("        }")
w("    }")
w("}")
w("")
w("/// A named sprite. Names follow Webamp's (`MAIN_PLAY_BUTTON` is `MainPlayButton`).")
w("#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]")
w("pub enum SpriteId {")
for n in names:
    w(f"    {camel(n)},")
w("}")
w("")
w("pub struct SpriteDef {")
w("    pub id: SpriteId,")
w("    pub name: &'static str,")
w("    pub sheet: Sheet,")
w("    pub rect: Rect,")
w("}")
w("")
w("impl SpriteId {")
w("    pub fn def(self) -> &'static SpriteDef {")
w("        &SPRITES[self as usize]")
w("    }")
w("}")
w("")
w("#[rustfmt::skip]")
w(f"pub static SPRITES: [SpriteDef; {len(sprites)}] = [")
for name, key, x, y, ww, hh in sprites:
    w(f'    SpriteDef {{ id: SpriteId::{camel(name)}, name: "{name}", sheet: Sheet::{SHEETS[key][0]}, rect: Rect::new({x}, {y}, {ww}, {hh}) }},')
w("];")
open(sys.argv[2], "w", encoding="utf-8", newline="\n").write("\n".join(out) + "\n")
print(len(sprites), "sprites;", {SHEETS[k][0]: v for k, v in sizes.items()})
```
