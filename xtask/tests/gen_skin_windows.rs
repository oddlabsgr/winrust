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
fn song_title_cells_sit_inside_their_panel() {
    // Webamp's marquee (top 24, 3 px padding) draws text rows 27-32 from x = 111.
    let main = windows::main();
    let text = Rect::new(111, 27, 154, 6);
    assert!(pixels(&main, text).iter().all(|&p| p == LCD_BG));
}

#[test]
fn playlist_mini_time_fits_its_panel_and_has_a_colon() {
    // Webamp draws five 5×6 cells from x = 66 + 1, 7, 12, 20, 25 at y = 23, and no colon.
    let pl = windows::pledit();
    let br = S::PlaylistBottomRightCorner.def().rect;
    for left in [1, 7, 12, 20, 25] {
        let cell = Rect::new(br.x + 66 + left, br.y + 23, 5, 6);
        assert!(
            pixels(&pl, cell).iter().all(|&p| p == LCD_BG),
            "cell {left}"
        );
    }
    for y in [24, 26] {
        assert_eq!(pl.get(br.x + 84, br.y + y), LCD_ON, "colon row {y}");
    }
}

#[test]
fn eq_title_bars_hold_their_buttons() {
    // Like the main window's, the EQ's shade and close buttons show the bar until clicked.
    let (eq, ex) = (windows::eqmain(), windows::eq_ex());
    for (img, bar) in [
        (&eq, S::EqTitleBarSelected),
        (&eq, S::EqTitleBar),
        (&ex, S::EqShadeBackgroundSelected),
        (&ex, S::EqShadeBackground),
    ] {
        let b = bar.def().rect;
        for x in [254, 264] {
            let button = Rect::new(b.x + x, b.y + 3, 9, 9);
            assert!(pixels(img, button).contains(&INK), "{bar:?} at {x}");
        }
    }
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
