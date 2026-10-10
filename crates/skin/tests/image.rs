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
