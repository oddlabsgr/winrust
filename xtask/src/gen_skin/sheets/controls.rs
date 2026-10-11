//! Controls: transport buttons, title bar pieces, sliders and toggle buttons.

use super::*;
use crate::gen_skin::draw::*;
use crate::gen_skin::icons::{self, Transport};
use crate::gen_skin::palette::*;
use winrust_skin::SpriteId as S;

/// 28 frames, 15 pixels apart: a groove whose fill grows with the level.
fn level_frames(img: &mut Image, area: Rect, centred: bool) {
    for i in 0..28u32 {
        let f = rect(area.x, area.y + 15 * i, area.w, 13);
        groove(img, f);
        let inner_w = f.w - 4;
        let fill = (inner_w * i).div_ceil(27).max(2);
        let c = level(i, 27);
        if centred {
            let x = f.x + 2 + (inner_w - fill) / 2;
            img.fill_rect(rect(x, f.y + 4, fill, 5), c);
        } else {
            img.fill_rect(rect(f.x + 2, f.y + 4, fill, 5), c);
        }
    }
}

fn toggle(img: &mut Image, b: Rect, down: bool, on: bool, draw: impl Fn(&mut Image, Rect, u32)) {
    if down {
        pressed(img, b);
    } else {
        raised(img, b);
    }
    let shift = u32::from(down);
    let c = if on { LCD_ON } else { INK_DIM };
    draw(img, rect(b.x + shift, b.y + shift, b.w, b.h), c);
}

pub fn cbuttons() -> Image {
    let mut img = canvas(Sheet::Cbuttons);
    let pairs = [
        (
            S::MainPreviousButton,
            S::MainPreviousButtonActive,
            Transport::Previous,
        ),
        (S::MainPlayButton, S::MainPlayButtonActive, Transport::Play),
        (
            S::MainPauseButton,
            S::MainPauseButtonActive,
            Transport::Pause,
        ),
        (S::MainStopButton, S::MainStopButtonActive, Transport::Stop),
        (S::MainNextButton, S::MainNextButtonActive, Transport::Next),
        (
            S::MainEjectButton,
            S::MainEjectButtonActive,
            Transport::Eject,
        ),
    ];
    for (up, down, kind) in pairs {
        for (id, is_down) in [(up, false), (down, true)] {
            button(&mut img, r(id), is_down, |img, b, c| {
                icons::transport(img, kind, b, 7, c)
            });
        }
    }
    img
}

pub fn titlebar() -> Image {
    let mut img = canvas(Sheet::Titlebar);
    title_bar(
        &mut img,
        r(S::MainTitleBarSelected),
        "WINRUST",
        true,
        18,
        240,
    );
    title_bar(&mut img, r(S::MainTitleBar), "WINRUST", false, 18, 240);
    let egg = "NO LLAMAS WERE HARMED";
    title_bar(
        &mut img,
        r(S::MainEasterEggTitleBarSelected),
        egg,
        true,
        18,
        240,
    );
    title_bar(&mut img, r(S::MainEasterEggTitleBar), egg, false, 18, 240);

    let small_buttons: [(S, S, &[&str]); 5] = [
        (
            S::MainOptionsButton,
            S::MainOptionsButtonDepressed,
            &icons::MENU,
        ),
        (
            S::MainMinimizeButton,
            S::MainMinimizeButtonDepressed,
            &icons::MINIMIZE,
        ),
        (
            S::MainCloseButton,
            S::MainCloseButtonDepressed,
            &icons::CLOSE,
        ),
        (
            S::MainShadeButton,
            S::MainShadeButtonDepressed,
            &icons::SHADE,
        ),
        (
            S::MainShadeButtonSelected,
            S::MainShadeButtonSelectedDepressed,
            &icons::UNSHADE,
        ),
    ];
    for (up, down, icon) in small_buttons {
        for (id, is_down) in [(up, false), (down, true)] {
            button(&mut img, r(id), is_down, |img, b, c| {
                bitmap_centered(img, b, icon, c)
            });
        }
    }

    // Clutter bar: O A I D V at Winamp's offsets.
    let letters = [
        ("O", 3, 8),
        ("A", 11, 7),
        ("I", 18, 7),
        ("D", 25, 8),
        ("V", 33, 7),
    ];
    for (id, ink) in [
        (S::MainClutterBarBackground, INK_DIM),
        (S::MainClutterBarBackgroundDisabled, BEVEL_LIGHT),
    ] {
        let b = r(id);
        pressed(&mut img, b);
        for (l, y, h) in letters {
            text_centered(&mut img, rect(b.x, b.y + y, b.w, h), l, ink);
        }
    }
    let selected = [
        (S::MainClutterBarButtonOSelected, "O"),
        (S::MainClutterBarButtonASelected, "A"),
        (S::MainClutterBarButtonISelected, "I"),
        (S::MainClutterBarButtonDSelected, "D"),
        (S::MainClutterBarButtonVSelected, "V"),
    ];
    for (id, l) in selected {
        let b = r(id);
        img.fill_rect(b, FACE_PRESSED_TOP);
        text_centered(&mut img, b, l, COPPER);
    }

    // Shade mode bar: mini visualizer, mini time, mini transport, seek groove.
    for (id, active) in [
        (S::MainShadeBackgroundSelected, true),
        (S::MainShadeBackground, false),
    ] {
        let b = r(id);
        vgradient(&mut img, b, BODY_TOP, BODY_BOTTOM);
        frame(&mut img, b, BEVEL_LIGHT, BEVEL_DARK);
        title_stripes(&mut img, b.x + 18, b.y + 4, 56, active);
        lcd(&mut img, rect(b.x + 78, b.y + 3, 40, 8));
        lcd(&mut img, rect(b.x + 125, b.y + 2, 31, 10));
        let ink = if active { INK } else { INK_DIM };
        let minis = [
            (169, 7, Transport::Previous),
            (176, 10, Transport::Play),
            (186, 9, Transport::Pause),
            (195, 9, Transport::Stop),
            (204, 10, Transport::Next),
            (215, 10, Transport::Eject),
        ];
        for (x, w, kind) in minis {
            icons::transport(&mut img, kind, rect(b.x + x, b.y + 2, w, 10), 5, ink);
        }
        title_stripes(&mut img, b.x + 158, b.y + 4, 8, active);
    }
    groove(&mut img, r(S::MainShadePositionBackground));
    for id in [
        S::MainShadePositionThumbLeft,
        S::MainShadePositionThumb,
        S::MainShadePositionThumbRight,
    ] {
        let b = r(id);
        vgradient(&mut img, b, COPPER, RUST);
        frame(&mut img, b, COPPER, RUST_DARK);
    }
    img
}

pub fn posbar() -> Image {
    let mut img = canvas(Sheet::Posbar);
    let track = r(S::MainPositionSliderBackground);
    groove(&mut img, track);
    hline(&mut img, track.x + 2, track.y + 4, track.w - 4, BEVEL_DARK);
    hline(&mut img, track.x + 2, track.y + 5, track.w - 4, BODY_BOTTOM);
    knob(&mut img, r(S::MainPositionSliderThumb), false, true);
    knob(&mut img, r(S::MainPositionSliderThumbSelected), true, true);
    img
}

pub fn volume() -> Image {
    let mut img = canvas(Sheet::Volume);
    let bg = r(S::MainVolumeBackground);
    level_frames(&mut img, rect(bg.x, bg.y, bg.w, 13), false);
    knob(&mut img, r(S::MainVolumeThumb), false, true);
    knob(&mut img, r(S::MainVolumeThumbSelected), true, true);
    img
}

pub fn balance() -> Image {
    let mut img = canvas(Sheet::Balance);
    let bg = r(S::MainBalanceBackground);
    level_frames(&mut img, rect(bg.x, bg.y, bg.w, 13), true);
    knob(&mut img, r(S::MainBalanceThumb), false, true);
    knob(&mut img, r(S::MainBalanceThumbActive), true, true);
    img
}

pub fn shufrep() -> Image {
    let mut img = canvas(Sheet::Shufrep);
    let states = |n: S, d: S, s: S, sd: S| {
        [
            (n, false, false),
            (d, true, false),
            (s, false, true),
            (sd, true, true),
        ]
    };
    for (id, down, on) in states(
        S::MainRepeatButton,
        S::MainRepeatButtonDepressed,
        S::MainRepeatButtonSelected,
        S::MainRepeatButtonSelectedDepressed,
    ) {
        toggle(&mut img, r(id), down, on, |img, b, c| {
            bitmap_centered(img, b, &icons::REPEAT, c)
        });
    }
    for (id, down, on) in states(
        S::MainShuffleButton,
        S::MainShuffleButtonDepressed,
        S::MainShuffleButtonSelected,
        S::MainShuffleButtonSelectedDepressed,
    ) {
        toggle(&mut img, r(id), down, on, |img, b, c| {
            text_centered(img, b, "SHUFFLE", c)
        });
    }
    for (id, down, on) in states(
        S::MainEqButton,
        S::MainEqButtonDepressed,
        S::MainEqButtonSelected,
        S::MainEqButtonDepressedSelected,
    ) {
        toggle(&mut img, r(id), down, on, |img, b, c| {
            text_centered(img, b, "EQ", c)
        });
    }
    for (id, down, on) in states(
        S::MainPlaylistButton,
        S::MainPlaylistButtonDepressed,
        S::MainPlaylistButtonSelected,
        S::MainPlaylistButtonDepressedSelected,
    ) {
        toggle(&mut img, r(id), down, on, |img, b, c| {
            text_centered(img, b, "PL", c)
        });
    }
    img
}
