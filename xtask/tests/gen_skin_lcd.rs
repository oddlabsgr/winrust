use winrust_skin::{Image, Rect, Sheet, SpriteId as S, text};
use xtask::gen_skin::palette::{BEVEL_LIGHT, LCD_BG, LCD_DIM, LCD_ON, RUST, SHADOW};
use xtask::gen_skin::{font, sheets::lcd};

fn pixels(img: &Image, r: Rect) -> Vec<u32> {
    let mut out = Vec::new();
    for y in r.y..r.y + r.h {
        for x in r.x..r.x + r.w {
            out.push(img.get(x, y));
        }
    }
    out
}

fn lit(img: &Image, r: Rect) -> usize {
    pixels(img, r).into_iter().filter(|&p| p == LCD_ON).count()
}

#[test]
fn sheets_have_canonical_sizes() {
    for (img, sheet) in [
        (lcd::numbers(), Sheet::Numbers),
        (lcd::nums_ex(), Sheet::NumsEx),
        (lcd::text(), Sheet::Text),
        (lcd::monoster(), Sheet::Monoster),
        (lcd::playpaus(), Sheet::Playpaus),
    ] {
        assert_eq!((img.width, img.height), sheet.size(), "{sheet:?}");
    }
}

#[test]
fn minus_sign_strips_follow_winamp() {
    // Winamp reads numbers.bmp's minus from row 6 of the `2` and the blank from the `1`.
    let numbers = lcd::numbers();
    assert!(
        pixels(&numbers, S::MinusSign.def().rect)
            .iter()
            .all(|&p| p == LCD_ON)
    );
    assert!(
        pixels(&numbers, S::NoMinusSign.def().rect)
            .iter()
            .all(|&p| p == LCD_BG)
    );
    let ex = lcd::nums_ex();
    let minus = S::MinusSignEx.def().rect;
    assert_eq!(lit(&ex, minus), 5);
    assert_eq!(lit(&ex, Rect::new(minus.x + 2, minus.y + 6, 5, 1)), 5);
    assert_eq!(lit(&ex, S::NoMinusSignEx.def().rect), 0);
}

#[test]
fn digits_are_seven_segment() {
    let numbers = lcd::numbers();
    let cell = |d: u32| Rect::new(9 * d, 0, 9, 13);
    let counts: Vec<usize> = (0..10).map(|d| lit(&numbers, cell(d))).collect();
    assert_eq!(counts.iter().max(), Some(&counts[8]));
    assert_eq!(
        lit(&numbers, cell(1)),
        lit(&numbers, Rect::new(9 + 7, 0, 2, 13))
    );
}

#[test]
fn every_mapped_character_has_its_glyph() {
    let sheet = lcd::text();
    for c in lcd::TEXT_CHARSET.chars() {
        let want: usize = font::glyph(c)
            .rows
            .iter()
            .map(|r| r.matches('#').count())
            .sum();
        assert!(want > 0, "{c:?} has no glyph");
        assert_eq!(lit(&sheet, text::glyph(c)), want, "{c:?}");
    }
    assert_eq!(lit(&sheet, text::glyph(' ')), 0);
}

#[test]
fn mono_and_stereo_light_up_when_selected() {
    let m = lcd::monoster();
    for (on, off) in [
        (S::MainStereoSelected, S::MainStereo),
        (S::MainMonoSelected, S::MainMono),
    ] {
        assert!(lit(&m, on.def().rect) > 0);
        assert_eq!(lit(&m, off.def().rect), 0);
        assert!(pixels(&m, off.def().rect).contains(&LCD_DIM));
    }
}

#[test]
fn indicators_show_play_state_and_work() {
    let p = lcd::playpaus();
    assert!(lit(&p, S::MainPlayingIndicator.def().rect) > 0);
    // Only the first 3 columns of the work indicators are ever shown. Webamp shows
    // "not working" beside the play triangle during playback, "working" while buffering.
    let idle = S::MainNotWorkingIndicator.def().rect;
    assert!(lit(&p, Rect::new(idle.x, idle.y, 3, 9)) > 0);
    let busy = S::MainWorkingIndicator.def().rect;
    assert!(pixels(&p, Rect::new(busy.x, busy.y, 3, 9)).contains(&RUST));
}

#[test]
fn mono_and_stereo_cells_form_one_panel() {
    // In the window MONO (x 212) sits left of STEREO (x 239): one panel, bevelled outside only.
    let m = lcd::monoster();
    for id in [
        S::MainMono,
        S::MainMonoSelected,
        S::MainStereo,
        S::MainStereoSelected,
    ] {
        let b = id.def().rect;
        let left: Vec<u32> = (1..b.h - 1).map(|y| m.get(b.x, b.y + y)).collect();
        let right: Vec<u32> = (1..b.h - 1)
            .map(|y| m.get(b.x + b.w - 1, b.y + y))
            .collect();
        if matches!(id, S::MainMono | S::MainMonoSelected) {
            assert!(left.iter().all(|&p| p == SHADOW), "{id:?}");
            assert!(!right.contains(&BEVEL_LIGHT), "{id:?}");
        } else {
            assert!(!left.contains(&SHADOW), "{id:?}");
            assert!(right.iter().all(|&p| p == BEVEL_LIGHT), "{id:?}");
        }
    }
}
