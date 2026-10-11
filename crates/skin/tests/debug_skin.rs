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

#[test]
fn volume_and_balance_frames_are_told_apart() {
    // Each of the 28 frames carries a stripe as wide as its index + 1, so a wrong frame
    // shows in golden images.
    let skin = Skin::debug();
    for id in [
        SpriteId::MainVolumeBackground,
        SpriteId::MainBalanceBackground,
    ] {
        let s = skin.sprite(id);
        let frames: HashSet<Vec<u32>> = (0..28)
            .map(|i| {
                (0..s.rect.w)
                    .map(|dx| s.image.get(s.rect.x + dx, s.rect.y + 15 * i + 6))
                    .collect()
            })
            .collect();
        assert_eq!(frames.len(), 28, "{id:?}");
    }
}
