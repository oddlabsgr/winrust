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
