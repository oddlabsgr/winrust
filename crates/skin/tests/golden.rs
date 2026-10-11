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

#[test]
fn base_skin_contact_sheet() {
    assert_golden("base-contact", &contact_sheet(&Skin::base()));
}
