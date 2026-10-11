use winrust_skin::{Image, Rect, Sheet, SpriteId as S};
use xtask::gen_skin::palette::{
    BEVEL_DARK, BEVEL_LIGHT, GROOVE, INK, LCD_BG, LCD_ON, RUST, SHADOW,
};
use xtask::gen_skin::sheets::controls;

fn pixels(img: &Image, r: Rect) -> Vec<u32> {
    let mut out = Vec::new();
    for y in r.y..r.y + r.h {
        for x in r.x..r.x + r.w {
            out.push(img.get(x, y));
        }
    }
    out
}

fn has(img: &Image, id: S, color: u32) -> bool {
    pixels(img, id.def().rect).contains(&color)
}

#[test]
fn sheets_have_canonical_sizes() {
    for (img, sheet) in [
        (controls::cbuttons(), Sheet::Cbuttons),
        (controls::titlebar(), Sheet::Titlebar),
        (controls::posbar(), Sheet::Posbar),
        (controls::volume(), Sheet::Volume),
        (controls::balance(), Sheet::Balance),
        (controls::shufrep(), Sheet::Shufrep),
    ] {
        assert_eq!((img.width, img.height), sheet.size(), "{sheet:?}");
    }
}

#[test]
fn pressed_buttons_differ_from_released_ones() {
    let img = controls::cbuttons();
    for (up, down) in [
        (S::MainPlayButton, S::MainPlayButtonActive),
        (S::MainStopButton, S::MainStopButtonActive),
        (S::MainEjectButton, S::MainEjectButtonActive),
    ] {
        assert_ne!(
            pixels(&img, up.def().rect),
            pixels(&img, down.def().rect),
            "{up:?}"
        );
    }
}

#[test]
fn volume_fill_grows_with_the_frame() {
    let img = controls::volume();
    let filled = |frame: u32| {
        pixels(&img, Rect::new(0, 15 * frame, 68, 13))
            .into_iter()
            .filter(|p| ![GROOVE, SHADOW, BEVEL_LIGHT].contains(p))
            .count()
    };
    let counts: Vec<usize> = (0..28).map(filled).collect();
    assert!(counts.windows(2).all(|w| w[0] < w[1]), "{counts:?}");
}

#[test]
fn balance_fill_is_centred() {
    let img = controls::balance();
    let bg = S::MainBalanceBackground.def().rect;
    let row = 15 * 10 + 6;
    let coloured: Vec<u32> = (bg.x..bg.x + bg.w)
        .filter(|&x| ![GROOVE, SHADOW, BEVEL_LIGHT].contains(&img.get(x, row)))
        .collect();
    let (first, last) = (coloured[0], *coloured.last().unwrap());
    let centre2 = 2 * bg.x + bg.w - 1;
    assert!((first + last).abs_diff(centre2) <= 1, "{first}..{last}");
}

#[test]
fn active_title_bars_use_rust() {
    let img = controls::titlebar();
    assert!(has(&img, S::MainTitleBarSelected, RUST));
    assert!(!has(&img, S::MainTitleBar, RUST));
    assert!(has(&img, S::MainShadeBackgroundSelected, RUST));
}

#[test]
fn shade_bar_shows_the_mini_transport() {
    let img = controls::titlebar();
    let bar = S::MainShadeBackgroundSelected.def().rect;
    assert!(pixels(&img, Rect::new(bar.x + 169, bar.y, 56, 14)).contains(&INK));
}

#[test]
fn title_bars_hold_their_buttons() {
    // Winamp shows the bar's own pixels until a title button is first clicked.
    let img = controls::titlebar();
    for bar in [
        S::MainTitleBarSelected,
        S::MainTitleBar,
        S::MainEasterEggTitleBarSelected,
        S::MainEasterEggTitleBar,
        S::MainShadeBackgroundSelected,
        S::MainShadeBackground,
    ] {
        let b = bar.def().rect;
        for x in [6, 244, 254, 264] {
            let button = Rect::new(b.x + x, b.y + 3, 9, 9);
            assert!(pixels(&img, button).contains(&INK), "{bar:?} at {x}");
        }
    }
}

#[test]
fn shade_bar_mini_time_fits_its_panel_and_has_a_colon() {
    // Webamp draws five 5×6 cells from x = 127 + 1, 7, 12, 20, 25 at y = 4, and no colon.
    let img = controls::titlebar();
    for bar in [S::MainShadeBackgroundSelected, S::MainShadeBackground] {
        let b = bar.def().rect;
        for left in [1, 7, 12, 20, 25] {
            let cell = Rect::new(b.x + 127 + left, b.y + 4, 5, 6);
            assert!(
                pixels(&img, cell).iter().all(|&p| p == LCD_BG),
                "{bar:?} cell {left}"
            );
        }
        for y in [5, 7] {
            assert_eq!(img.get(b.x + 145, b.y + y), LCD_ON, "{bar:?} colon row {y}");
        }
    }
}

#[test]
fn toggles_light_up_when_selected() {
    let img = controls::shufrep();
    for (on, off) in [
        (S::MainShuffleButtonSelected, S::MainShuffleButton),
        (S::MainRepeatButtonSelected, S::MainRepeatButton),
        (S::MainEqButtonSelected, S::MainEqButton),
        (S::MainPlaylistButtonSelected, S::MainPlaylistButton),
    ] {
        assert!(has(&img, on, LCD_ON), "{on:?}");
        assert!(!has(&img, off, LCD_ON), "{off:?}");
    }
}

#[test]
fn next_button_shows_its_edge_within_22_pixels() {
    // Webamp shows MAIN_NEXT_BUTTON 22 pixels wide, so its dark right edge is column 21.
    let img = controls::cbuttons();
    let b = S::MainNextButton.def().rect;
    for dy in 1..b.h - 1 {
        assert_eq!(img.get(b.x + 21, b.y + dy), BEVEL_DARK, "row {dy}");
    }
}

#[test]
fn shade_bars_share_a_dark_seam() {
    // MAIN_SHADE_BACKGROUND_SELECTED (y 29) and MAIN_SHADE_BACKGROUND (y 42) share row 42: the
    // selected bar's dark bottom edge, with the other bar's light edge just below it.
    let img = controls::titlebar();
    for x in [27 + 120, 27 + 235] {
        assert_eq!(img.get(x, 42), BEVEL_DARK, "x {x}");
        assert_eq!(img.get(x, 43), BEVEL_LIGHT, "x {x}");
    }
}
