//! Window backgrounds: main, equalizer (with its shade bar) and playlist.

use super::*;
use crate::gen_skin::draw::*;
use crate::gen_skin::icons::{self, Transport};
use crate::gen_skin::palette::*;
use winrust_skin::SpriteId as S;

/// Top-row piece of the playlist frame: a 14-pixel strip over 6 pixels of frame.
fn playlist_top(img: &mut Image, b: Rect, active: bool, stripes: Option<(u32, u32)>) {
    vgradient(img, rect(b.x, b.y, b.w, 14), BODY_TOP, BODY_BOTTOM);
    img.fill_rect(rect(b.x, b.y + 14, b.w, 6), BODY_BOTTOM);
    hline(img, b.x, b.y, b.w, BEVEL_LIGHT);
    hline(img, b.x, b.y + 19, b.w, SHADOW);
    if let Some((from, w)) = stripes {
        title_stripes(img, b.x + from, b.y + 4, w, active);
    }
}

fn menu_item(img: &mut Image, b: Rect, lines: &[&str], down: bool) {
    if down {
        pressed(img, b);
    } else {
        raised(img, b);
    }
    let c = if down { COPPER } else { INK };
    match lines {
        [one] => label_centered(img, b, one, c),
        [top, bottom] => {
            label_centered(img, rect(b.x, b.y + 2, b.w, 6), top, c);
            label_centered(img, rect(b.x, b.y + 10, b.w, 6), bottom, c);
        }
        _ => {}
    }
}

pub const EQ_BANDS: [&str; 10] = [
    "60", "170", "310", "600", "1K", "3K", "6K", "12K", "14K", "16K",
];

pub fn main() -> Image {
    let mut img = canvas(Sheet::Main);
    let all = r(S::MainWindowBackground);
    body(&mut img, all);
    // Display: time, play state and visualizer.
    lcd(&mut img, rect(20, 22, 84, 41));
    // The time display's colon is part of the background, as in Winamp.
    img.fill_rect(rect(72, 29, 2, 2), LCD_ON);
    img.fill_rect(rect(72, 34, 2, 2), LCD_ON);
    // Song title (Webamp's marquee draws rows 27-32), bitrate and sample rate.
    lcd(&mut img, rect(109, 25, 158, 10));
    lcd(&mut img, rect(109, 41, 19, 10));
    label(&mut img, 130, 43, "KBPS", INK_DIM);
    lcd(&mut img, rect(154, 41, 14, 10));
    label(&mut img, 170, 43, "KHZ", INK_DIM);
    // Grooves between the sliders, the seek bar and the buttons.
    hline(&mut img, 10, 70, 255, SHADOW);
    hline(&mut img, 10, 71, 255, BEVEL_LIGHT);
    hline(&mut img, 10, 84, 255, SHADOW);
    hline(&mut img, 10, 85, 255, BEVEL_LIGHT);
    // Logo plate.
    let logo = rect(252, 90, 15, 17);
    vgradient(&mut img, logo, RUST, RUST_DARK);
    frame(&mut img, logo, COPPER, SHADOW);
    label_centered(&mut img, logo, "W", INK);
    img
}

pub fn eqmain() -> Image {
    let mut img = canvas(Sheet::Eqmain);
    let win = r(S::EqWindowBackground);
    body(&mut img, win);
    label(&mut img, 45, 37, "+12DB", INK_DIM);
    label(&mut img, 50, 65, "0DB", INK_DIM);
    label(&mut img, 45, 96, "-12DB", INK_DIM);
    label_centered(&mut img, rect(14, 103, 28, 5), "PREAMP", INK_DIM);
    for (i, band) in EQ_BANDS.iter().enumerate() {
        label_centered(
            &mut img,
            rect(78 + 18 * i as u32 - 2, 103, 18, 5),
            band,
            INK_DIM,
        );
    }
    vline(&mut img, 70, 36, 70, SHADOW);
    vline(&mut img, 71, 36, 70, BEVEL_LIGHT);

    title_bar(
        &mut img,
        r(S::EqTitleBarSelected),
        "EQUALIZER",
        true,
        6,
        248,
    );
    title_bar(&mut img, r(S::EqTitleBar), "EQUALIZER", false, 6, 248);
    // The title buttons are part of the bars; Webamp also falls back to the shade button
    // for its pressed state.
    for bar in [S::EqTitleBarSelected, S::EqTitleBar] {
        for (x, icon) in [(254, &icons::SHADE), (264, &icons::CLOSE)] {
            bar_button(&mut img, r(bar), x, icon);
        }
    }
    for (id, down) in [(S::EqCloseButton, false), (S::EqCloseButtonActive, true)] {
        button(&mut img, r(id), down, |img, b, c| {
            bitmap_centered(img, b, &icons::CLOSE, c)
        });
    }

    let led_button = |img: &mut Image, b: Rect, down: bool, on: bool, word: &str| {
        if down {
            pressed(img, b);
        } else {
            raised(img, b);
        }
        let s = u32::from(down);
        img.fill_rect(
            rect(b.x + 3 + s, b.y + 4 + s, 3, 3),
            if on { LCD_ON } else { LCD_DIM },
        );
        let text_area = rect(b.x + 7 + s, b.y + s, b.w - 8, b.h);
        label_centered(img, text_area, word, if on { INK } else { INK_DIM });
    };
    let leds = [
        (S::EqOnButton, false, false, "ON"),
        (S::EqOnButtonDepressed, true, false, "ON"),
        (S::EqOnButtonSelected, false, true, "ON"),
        (S::EqOnButtonSelectedDepressed, true, true, "ON"),
        (S::EqAutoButton, false, false, "AUTO"),
        (S::EqAutoButtonDepressed, true, false, "AUTO"),
        (S::EqAutoButtonSelected, false, true, "AUTO"),
        (S::EqAutoButtonSelectedDepressed, true, true, "AUTO"),
    ];
    for (id, down, on, word) in leds {
        led_button(&mut img, r(id), down, on, word);
    }

    // 28 slider frames, 14 per row: the groove takes the colour of the slider's level.
    let sliders = r(S::EqSliderBackground);
    for i in 0..28u32 {
        let x = sliders.x + 15 * (i % 14);
        let y = sliders.y + 65 * (i / 14);
        img.fill_rect(rect(x, y, 14, 63), BODY_BOTTOM);
        let g = rect(x + 4, y, 6, 63);
        groove(&mut img, g);
        img.fill_rect(
            rect(g.x + 2, g.y + 2, 2, g.h - 4),
            mix(GROOVE, level(i, 27), 0.8),
        );
        hline(&mut img, x, y + 31, 3, INK_DIM);
        hline(&mut img, x + 11, y + 31, 3, INK_DIM);
    }
    for (id, active) in [(S::EqSliderThumb, false), (S::EqSliderThumbSelected, true)] {
        knob(&mut img, r(id), active, false);
    }
    for (id, down) in [
        (S::EqPresetsButton, false),
        (S::EqPresetsButtonSelected, true),
    ] {
        button(&mut img, r(id), down, |img, b, c| {
            label_centered(img, b, "PRESETS", c)
        });
    }

    let graph = r(S::EqGraphBackground);
    lcd(&mut img, graph);
    hline(&mut img, graph.x + 1, graph.y + 9, graph.w - 2, LCD_GRID);
    let colors = r(S::EqGraphLineColors);
    for dy in 0..colors.h {
        img.set(
            colors.x,
            colors.y + dy,
            level(colors.h - 1 - dy, colors.h - 1),
        );
    }
    let preamp = r(S::EqPreampLine);
    hline(
        &mut img,
        preamp.x,
        preamp.y,
        preamp.w,
        mix(LCD_BG, COPPER, 0.6),
    );
    img
}

pub fn eq_ex() -> Image {
    let mut img = canvas(Sheet::EqEx);
    for (id, active) in [
        (S::EqShadeBackgroundSelected, true),
        (S::EqShadeBackground, false),
    ] {
        let b = r(id);
        vgradient(&mut img, b, BODY_TOP, BODY_BOTTOM);
        frame(&mut img, b, BEVEL_LIGHT, BEVEL_DARK);
        label(
            &mut img,
            b.x + 7,
            b.y + 4,
            "EQUALIZER",
            if active { COPPER } else { INK_DIM },
        );
        groove(&mut img, rect(b.x + 61, b.y + 4, 97, 7));
        groove(&mut img, rect(b.x + 164, b.y + 4, 43, 7));
        title_stripes(&mut img, b.x + 212, b.y + 4, 38, active);
        for (x, icon) in [(254, &icons::UNSHADE), (264, &icons::CLOSE)] {
            bar_button(&mut img, b, x, icon);
        }
    }
    let thumbs = [
        (S::EqShadeVolumeSliderLeft, 0),
        (S::EqShadeVolumeSliderCenter, 14),
        (S::EqShadeVolumeSliderRight, 27),
        (S::EqShadeBalanceSliderLeft, 27),
        (S::EqShadeBalanceSliderCenter, 0),
        (S::EqShadeBalanceSliderRight, 27),
    ];
    for (id, lvl) in thumbs {
        let b = r(id);
        img.fill_rect(b, level(lvl, 27));
        frame(&mut img, b, INK, SHADOW);
    }
    let buttons: [(S, bool, &[&str]); 4] = [
        // Pressed shade button of the normal EQ, then of the shaded EQ.
        (S::EqMaximizeButtonActive, true, &icons::SHADE),
        (S::EqMinimizeButtonActive, true, &icons::UNSHADE),
        (S::EqShadeCloseButton, false, &icons::CLOSE),
        (S::EqShadeCloseButtonActive, true, &icons::CLOSE),
    ];
    for (id, down, icon) in buttons {
        button(&mut img, r(id), down, |img, b, c| {
            bitmap_centered(img, b, icon, c)
        });
    }
    img
}

pub fn pledit() -> Image {
    let mut img = canvas(Sheet::Pledit);
    let tops = [
        (S::PlaylistTopLeftSelected, S::PlaylistTopLeftCorner),
        (S::PlaylistTopTileSelected, S::PlaylistTopTile),
        (S::PlaylistTitleBarSelected, S::PlaylistTitleBar),
        (S::PlaylistTopRightCornerSelected, S::PlaylistTopRightCorner),
    ];
    for (active_id, idle_id) in tops {
        for (id, active) in [(active_id, true), (idle_id, false)] {
            let b = r(id);
            match id {
                S::PlaylistTitleBarSelected | S::PlaylistTitleBar => {
                    playlist_top(&mut img, b, active, None);
                    let c = if active { COPPER } else { INK_DIM };
                    label_centered(&mut img, rect(b.x, b.y + 4, b.w, 5), "PLAYLIST", c);
                }
                S::PlaylistTopRightCornerSelected | S::PlaylistTopRightCorner => {
                    playlist_top(&mut img, b, active, Some((0, 3)));
                    for (x, icon) in [(4, &icons::SHADE), (14, &icons::CLOSE)] {
                        let k = rect(b.x + x, b.y + 3, 9, 9);
                        raised(&mut img, k);
                        bitmap_centered(&mut img, k, icon, INK);
                    }
                }
                _ => playlist_top(&mut img, b, active, Some((0, b.w))),
            }
        }
    }

    // Side tiles: frame around the list, scroll groove on the right.
    let left = r(S::PlaylistLeftTile);
    img.fill_rect(left, BODY_BOTTOM);
    vline(&mut img, left.x, left.y, left.h, BEVEL_LIGHT);
    vline(&mut img, left.x + 11, left.y, left.h, SHADOW);
    let right = r(S::PlaylistRightTile);
    img.fill_rect(right, BODY_BOTTOM);
    vline(&mut img, right.x, right.y, right.h, BEVEL_LIGHT);
    vline(&mut img, right.x + 19, right.y, right.h, BEVEL_DARK);
    img.fill_rect(rect(right.x + 5, right.y, 8, right.h), GROOVE);
    vline(&mut img, right.x + 4, right.y, right.h, SHADOW);
    vline(&mut img, right.x + 13, right.y, right.h, BEVEL_LIGHT);
    knob(&mut img, r(S::PlaylistScrollHandle), false, false);
    knob(&mut img, r(S::PlaylistScrollHandleSelected), true, false);

    let pressed_buttons: [(S, &[&str]); 3] = [
        (S::PlaylistCloseSelected, &icons::CLOSE),
        (S::PlaylistCollapseSelected, &icons::SHADE),
        (S::PlaylistExpandSelected, &icons::UNSHADE),
    ];
    for (id, icon) in pressed_buttons {
        button(&mut img, r(id), true, |img, b, c| {
            bitmap_centered(img, b, icon, c)
        });
    }

    // Shade mode.
    for (id, active) in [
        (S::PlaylistShadeBackgroundLeft, true),
        (S::PlaylistShadeBackground, true),
        (S::PlaylistShadeBackgroundRight, false),
        (S::PlaylistShadeBackgroundRightSelected, true),
    ] {
        let b = r(id);
        vgradient(&mut img, b, BODY_TOP, BODY_BOTTOM);
        hline(&mut img, b.x, b.y, b.w, BEVEL_LIGHT);
        hline(&mut img, b.x, b.y + b.h - 1, b.w, BEVEL_DARK);
        if matches!(
            id,
            S::PlaylistShadeBackgroundRight | S::PlaylistShadeBackgroundRightSelected
        ) {
            for (x, icon) in [(29, &icons::UNSHADE), (39, &icons::CLOSE)] {
                let k = rect(b.x + x, b.y + 3, 9, 9);
                raised(&mut img, k);
                bitmap_centered(&mut img, k, icon, if active { INK } else { INK_DIM });
            }
            // Webamp's shade time covers x 0-19; the stripes fill the gap up to the buttons.
            title_stripes(&mut img, b.x + 20, b.y + 4, 8, active);
        }
    }

    // Bottom row.
    let bottom_body = |img: &mut Image, b: Rect| {
        img.fill_rect(b, BODY_BOTTOM);
        vgradient(img, rect(b.x, b.y + 2, b.w, b.h - 2), BODY_BOTTOM, BODY_TOP);
        hline(img, b.x, b.y, b.w, BEVEL_LIGHT);
        hline(img, b.x, b.y + b.h - 1, b.w, BEVEL_DARK);
    };
    bottom_body(&mut img, r(S::PlaylistBottomTile));
    let bl = r(S::PlaylistBottomLeftCorner);
    bottom_body(&mut img, bl);
    vline(&mut img, bl.x, bl.y, bl.h, BEVEL_LIGHT);
    for (x, word) in [(14, "ADD"), (43, "REM"), (72, "SEL"), (101, "MISC")] {
        menu_item(&mut img, rect(bl.x + x, bl.y + 8, 22, 18), &[word], false);
    }
    let br = r(S::PlaylistBottomRightCorner);
    bottom_body(&mut img, br);
    vline(&mut img, br.x + br.w - 1, br.y, br.h, BEVEL_DARK);
    lcd(&mut img, rect(br.x + 5, br.y + 8, 62, 10));
    let minis = [
        Transport::Previous,
        Transport::Play,
        Transport::Pause,
        Transport::Stop,
        Transport::Next,
        Transport::Eject,
    ];
    for (i, kind) in minis.into_iter().enumerate() {
        let area = rect(br.x + 3 + 10 * i as u32, br.y + 22, 10, 10);
        icons::transport(&mut img, kind, area, 5, INK);
    }
    lcd(&mut img, rect(br.x + 65, br.y + 21, 32, 10));
    // Webamp draws only the mini time's digits; the colon comes from the background.
    for dy in [24, 26] {
        img.set(br.x + 84, br.y + dy, LCD_ON);
    }
    menu_item(
        &mut img,
        rect(br.x + 106, br.y + 8, 22, 18),
        &["LIST"],
        false,
    );
    icons::tri_up(&mut img, br.x + 136, br.y + 3, 7, INK_DIM);
    icons::tri_down(&mut img, br.x + 136, br.y + 9, 7, INK_DIM);
    for i in 0..4u32 {
        let x = br.x + br.w - 4 - 3 * i;
        for d in 0..=(3 * i) {
            img.set(x + d, br.y + br.h - 3 - d, BEVEL_LIGHT);
        }
    }
    let vis = r(S::PlaylistVisualizerBackground);
    bottom_body(&mut img, vis);
    // Webamp draws the playlist visualizer 72×16 at (2, 12); the panel frames it.
    lcd(&mut img, rect(vis.x + 1, vis.y + 11, 74, 18));

    // Popup menus.
    let items: [(S, S, &[&str]); 16] = [
        (
            S::PlaylistAddUrl,
            S::PlaylistAddUrlSelected,
            &["ADD", "URL"],
        ),
        (
            S::PlaylistAddDir,
            S::PlaylistAddDirSelected,
            &["ADD", "DIR"],
        ),
        (
            S::PlaylistAddFile,
            S::PlaylistAddFileSelected,
            &["ADD", "FILE"],
        ),
        (
            S::PlaylistRemoveAll,
            S::PlaylistRemoveAllSelected,
            &["REM", "ALL"],
        ),
        (S::PlaylistCrop, S::PlaylistCropSelected, &["CROP"]),
        (
            S::PlaylistRemoveSelected,
            S::PlaylistRemoveSelectedSelected,
            &["REM", "SEL"],
        ),
        (
            S::PlaylistRemoveMisc,
            S::PlaylistRemoveMiscSelected,
            &["REM", "MISC"],
        ),
        (
            S::PlaylistInvertSelection,
            S::PlaylistInvertSelectionSelected,
            &["SEL", "INV"],
        ),
        (
            S::PlaylistSelectZero,
            S::PlaylistSelectZeroSelected,
            &["SEL", "ZERO"],
        ),
        (
            S::PlaylistSelectAll,
            S::PlaylistSelectAllSelected,
            &["SEL", "ALL"],
        ),
        (
            S::PlaylistSortList,
            S::PlaylistSortListSelected,
            &["SORT", "LIST"],
        ),
        (
            S::PlaylistFileInfo,
            S::PlaylistFileInfoSelected,
            &["FILE", "INFO"],
        ),
        (
            S::PlaylistMiscOptions,
            S::PlaylistMiscOptionsSelected,
            &["MISC", "OPTS"],
        ),
        (
            S::PlaylistNewList,
            S::PlaylistNewListSelected,
            &["NEW", "LIST"],
        ),
        (
            S::PlaylistSaveList,
            S::PlaylistSaveListSelected,
            &["SAVE", "LIST"],
        ),
        (
            S::PlaylistLoadList,
            S::PlaylistLoadListSelected,
            &["LOAD", "LIST"],
        ),
    ];
    for (up, down, lines) in items {
        menu_item(&mut img, r(up), lines, false);
        menu_item(&mut img, r(down), lines, true);
    }
    for id in [
        S::PlaylistAddMenuBar,
        S::PlaylistRemoveMenuBar,
        S::PlaylistSelectMenuBar,
        S::PlaylistMiscMenuBar,
        S::PlaylistListBar,
    ] {
        let b = r(id);
        img.fill_rect(b, RUST);
        vline(&mut img, b.x, b.y, b.h, COPPER);
        vline(&mut img, b.x + 2, b.y, b.h, RUST_DARK);
    }
    img
}
