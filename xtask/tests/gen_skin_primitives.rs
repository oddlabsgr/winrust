use winrust_skin::{Image, Rect};
use xtask::gen_skin::icons::{self, Transport};
use xtask::gen_skin::palette::{SHADOW, level};
use xtask::gen_skin::{draw, font};

const A: u32 = 0xFF11_1111;
const B: u32 = 0xFF22_2222;

fn count(img: &Image, color: u32) -> usize {
    img.pixels.iter().filter(|&&p| p == color).count()
}

#[test]
fn mix_blends_each_channel() {
    assert_eq!(draw::mix(0xFF00_0000, 0xFFFF_FFFF, 0.0), 0xFF00_0000);
    assert_eq!(draw::mix(0xFF00_0000, 0xFFFF_FFFF, 1.0), 0xFFFF_FFFF);
    assert_eq!(draw::mix(0xFF00_0000, 0xFFFF_FFFF, 0.5), 0xFF80_8080);
    assert_eq!(draw::mix(0xFF10_2030, 0xFF30_2010, 0.5), 0xFF20_2020);
}

#[test]
fn frame_lights_top_left_and_darkens_bottom_right() {
    let mut img = Image::new(3, 3, 0);
    draw::frame(&mut img, Rect::new(0, 0, 3, 3), A, B);
    assert_eq!(img.get(0, 0), A);
    assert_eq!(img.get(1, 0), A);
    assert_eq!(img.get(2, 2), B);
    assert_eq!(img.get(2, 0), B, "right edge wins the corner");
    assert_eq!(img.get(1, 1), 0);
}

#[test]
fn text_width_follows_glyph_widths() {
    assert_eq!(draw::text_width("AB"), 9);
    assert_eq!(draw::text_width("I"), 3);
    assert_eq!(draw::text_width(""), 0);
    let mut img = Image::new(5, 5, 0);
    assert_eq!(draw::text(&mut img, 0, 0, "I", A), 3);
    assert_eq!((img.get(0, 0), img.get(2, 0), img.get(1, 2)), (A, A, A));
    assert_eq!(img.get(0, 2), 0);
}

#[test]
fn labels_have_a_shadow() {
    let mut img = Image::new(3, 7, 0);
    draw::label(&mut img, 0, 0, ".", A);
    assert_eq!(img.get(0, 4), A);
    assert_eq!(img.get(1, 5), SHADOW);
}

#[test]
fn every_glyph_fits_four_by_five() {
    let chars = (' '..='~').chain("…ÅÖÄ".chars());
    for c in chars {
        let g = font::glyph(c);
        assert!(g.width <= 4, "{c:?}");
        for row in g.rows {
            assert!(row.len() as u32 <= g.width, "{c:?}");
            assert!(row.bytes().all(|b| b == b'#' || b == b'.'), "{c:?}");
        }
    }
    assert_eq!(font::glyph('a').rows, font::glyph('A').rows);
    assert!(font::glyph('€').rows.iter().all(|r| !r.contains('#')));
}

#[test]
fn triangles_have_the_expected_shape() {
    let mut img = Image::new(10, 10, 0);
    icons::tri_right(&mut img, 0, 0, 7, A);
    assert_eq!(count(&img, A), 16);
    assert_eq!(img.get(3, 3), A, "tip");
    assert_eq!(img.get(3, 2), 0);
    let mut img = Image::new(10, 10, 0);
    icons::tri_up(&mut img, 0, 0, 7, A);
    assert_eq!(count(&img, A), 16);
    assert_eq!(img.get(3, 0), A, "tip");
}

#[test]
fn transport_icons_are_centred() {
    let mut img = Image::new(23, 18, 0);
    icons::transport(&mut img, Transport::Stop, Rect::new(0, 0, 23, 18), 7, A);
    assert_eq!(count(&img, A), 49);
    assert_eq!(img.get(8, 5), A);
    assert_eq!(img.get(7, 5), 0);
}

#[test]
fn levels_run_green_to_red() {
    let [_, r0, g0, _] = level(0, 27).to_be_bytes();
    let [_, r1, g1, _] = level(27, 27).to_be_bytes();
    assert!(g0 > r0 && r1 > g1);
}
