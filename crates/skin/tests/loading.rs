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

#[test]
fn sheets_are_kept_at_their_canonical_size() {
    // Review I1: a sheet declared 2048×2048 kept 16 MiB alive for the skin's lifetime.
    let skin = Skin::from_zip_bytes(&zip(&[
        ("main.bmp", &solid_bmp(400, 300, RED)),
        ("cbuttons.bmp", &solid_bmp(10, 10, GREEN)),
    ]))
    .unwrap();
    let main = skin.sprite(SpriteId::MainWindowBackground).image;
    assert_eq!((main.width, main.height), Sheet::Main.size());
    let buttons = skin.sprite(SpriteId::MainPreviousButton).image;
    assert_eq!((buttons.width, buttons.height), Sheet::Cbuttons.size());
}
