use winrust_skin::sprites::SPRITES;
use winrust_skin::{Image, Rect, Sheet, SpriteId as S};
use xtask::gen_skin::palette::{INK, LCD_BG, LCD_ON, SHADOW};
use xtask::gen_skin::sheets::{self, windows};

fn pixels(img: &Image, r: Rect) -> Vec<u32> {
    let mut out = Vec::new();
    for y in r.y..r.y + r.h {
        for x in r.x..r.x + r.w {
            out.push(img.get(x, y));
        }
    }
    out
}

#[test]
fn every_sheet_has_its_canonical_size() {
    for (sheet, img) in Sheet::ALL.iter().zip(sheets::all()) {
        assert_eq!((img.width, img.height), sheet.size(), "{sheet:?}");
    }
}

#[test]
fn every_sprite_is_drawn() {
    // The canvas starts as SHADOW; a sprite left entirely SHADOW was forgotten.
    let all = sheets::all();
    for def in &SPRITES {
        let sheet = &all[def.sheet as usize];
        assert!(
            pixels(sheet, def.rect).iter().any(|&p| p != SHADOW),
            "{}",
            def.name
        );
    }
}

#[test]
fn time_colon_is_part_of_the_main_background() {
    // Winamp draws only the digits; the colon between them comes from main.bmp.
    let main = windows::main();
    for (x, y) in [(72, 29), (73, 30), (72, 34), (73, 35)] {
        assert_eq!(main.get(x, y), LCD_ON, "({x},{y})");
    }
    assert_eq!(main.get(60, 50), LCD_BG);
}

#[test]
fn eq_graph_colours_run_red_to_green() {
    let eq = windows::eqmain();
    let r = S::EqGraphLineColors.def().rect;
    let [_, tr, tg, _] = eq.get(r.x, r.y).to_be_bytes();
    let [_, br, bg, _] = eq.get(r.x, r.y + r.h - 1).to_be_bytes();
    assert!(tr > tg && bg > br);
}

#[test]
fn eq_title_bar_holds_the_maximize_fallback() {
    // Webamp shows EQ_MAXIMIZE_BUTTON_ACTIVE_FALLBACK when a skin has no eq_ex.bmp.
    let eq = windows::eqmain();
    assert!(pixels(&eq, S::EqMaximizeButtonActiveFallback.def().rect).contains(&INK));
}

#[test]
fn playlist_menu_items_have_two_states() {
    let pl = windows::pledit();
    for (up, down) in [
        (S::PlaylistAddUrl, S::PlaylistAddUrlSelected),
        (S::PlaylistCrop, S::PlaylistCropSelected),
        (S::PlaylistLoadList, S::PlaylistLoadListSelected),
    ] {
        assert_ne!(
            pixels(&pl, up.def().rect),
            pixels(&pl, down.def().rect),
            "{up:?}"
        );
    }
}
