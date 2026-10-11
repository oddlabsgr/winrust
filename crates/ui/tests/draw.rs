use std::time::Duration;
use winrust_skin::{Image, Rect, Sheet, Skin, SpriteId as S, bmp, debug};
use winrust_ui::draw;
use winrust_ui::interaction::{Interaction, Pointer};
use winrust_ui::view::{MainView, PlayState, TimeMode};

/// Whether `img` shows the `w`×`h` top-left part of sprite `id` at (x, y).
fn shows_part(img: &Image, x: u32, y: u32, skin: &Skin, id: S, w: u32, h: u32) -> bool {
    let s = skin.sprite(id);
    (0..h).all(|dy| {
        (0..w).all(|dx| img.get(x + dx, y + dy) == s.image.get(s.rect.x + dx, s.rect.y + dy))
    })
}

fn shows(img: &Image, x: u32, y: u32, skin: &Skin, id: S) -> bool {
    let r = id.def().rect;
    shows_part(img, x, y, skin, id, r.w, r.h)
}

/// Whether `img` shows the `text.bmp` glyph for `c` at (x, y).
fn shows_glyph(img: &Image, x: u32, y: u32, skin: &Skin, c: char) -> bool {
    let g = skin.glyph(c);
    (0..g.rect.h).all(|dy| {
        (0..g.rect.w).all(|dx| img.get(x + dx, y + dy) == g.image.get(g.rect.x + dx, g.rect.y + dy))
    })
}

/// Whether `img` shows the main window background throughout `r`.
fn background_at(img: &Image, skin: &Skin, r: Rect) -> bool {
    let bg = skin.sprite(S::MainWindowBackground);
    (0..r.h).all(|dy| {
        (0..r.w).all(|dx| img.get(r.x + dx, r.y + dy) == bg.image.get(r.x + dx, r.y + dy))
    })
}

/// Playing 1:23 of a 4:56 stereo MP3 at 128 kbps and 44 kHz, focused.
fn playing() -> MainView {
    MainView {
        state: PlayState::Playing,
        elapsed: Duration::from_secs(83),
        duration: Some(Duration::from_secs(296)),
        title: "1. Artist - Title (4:56)".into(),
        kbps: Some(128),
        khz: Some(44),
        channels: Some(2),
        volume: 50,
        seekable: true,
        focused: true,
        ..MainView::default()
    }
}

#[test]
fn stopped_shows_the_background_and_the_stopped_indicator() {
    let skin = Skin::debug();
    let view = MainView::default();
    let img = draw::render(&skin, &view, &Interaction::new());
    assert_eq!((img.width, img.height), (275, 116));
    assert!(shows(&img, 0, 0, &skin, S::MainTitleBar));
    assert!(shows(&img, 26, 28, &skin, S::MainStoppedIndicator));
    // No time, bitrate, sample rate or seek thumb while stopped.
    assert!(background_at(&img, &skin, Rect::new(38, 26, 61, 13)));
    assert!(background_at(&img, &skin, Rect::new(111, 43, 15, 6)));
    assert!(background_at(&img, &skin, Rect::new(156, 43, 10, 6)));
    assert!(shows(&img, 16, 72, &skin, S::MainPositionSliderBackground));
    assert!(shows(&img, 212, 41, &skin, S::MainMono));
    assert!(shows(&img, 239, 41, &skin, S::MainStereo));
}

#[test]
fn playing_shows_time_indicators_and_media_info() {
    let skin = Skin::debug();
    let img = draw::render(&skin, &playing(), &Interaction::new());
    assert!(shows(&img, 0, 0, &skin, S::MainTitleBarSelected));
    // The play indicator, its first column under the work indicator.
    let play = skin.sprite(S::MainPlayingIndicator);
    for dx in 1..9 {
        let expected = play.image.get(play.rect.x + dx, play.rect.y + 4);
        assert_eq!(img.get(26 + dx, 32), expected, "play x {dx}");
    }
    // Only the first 3 columns of the work indicator show.
    assert!(shows_part(
        &img,
        24,
        28,
        &skin,
        S::MainNotWorkingIndicator,
        3,
        9
    ));
    assert!(shows(&img, 38, 26, &skin, S::NoMinusSignEx));
    for (x, digit) in [
        (48, S::Digit0Ex),
        (60, S::Digit1Ex),
        (78, S::Digit2Ex),
        (90, S::Digit3Ex),
    ] {
        assert!(shows(&img, x, 26, &skin, digit), "{digit:?}");
    }
    for (i, c) in "128".chars().enumerate() {
        assert!(shows_glyph(&img, 111 + 5 * i as u32, 43, &skin, c));
    }
    assert!(shows_glyph(&img, 156, 43, &skin, '4'));
    assert!(shows_glyph(&img, 161, 43, &skin, '4'));
    assert!(shows_glyph(&img, 111, 27, &skin, '1'));
    assert!(shows_glyph(&img, 116, 27, &skin, '.'));
    assert!(shows(&img, 212, 41, &skin, S::MainMono));
    assert!(shows(&img, 239, 41, &skin, S::MainStereoSelected));
    // 83 s of 296 s is 28%: the thumb sits 61 pixels along.
    assert!(shows(&img, 16 + 61, 72, &skin, S::MainPositionSliderThumb));
}

#[test]
fn remaining_time_shows_the_minus_sign() {
    let skin = Skin::debug();
    let view = MainView {
        time_mode: TimeMode::Remaining,
        ..playing()
    };
    let img = draw::render(&skin, &view, &Interaction::new());
    // 4:56 - 1:23 = 3:33.
    assert!(shows(&img, 38, 26, &skin, S::MinusSignEx));
    assert!(shows(&img, 60, 26, &skin, S::Digit3Ex));
}

#[test]
fn numbers_bmp_draws_a_one_pixel_minus_sign() {
    // A skin with numbers.bmp but no nums_ex.bmp: the 5×1 minus sits at (38, 32).
    let dir = std::env::temp_dir().join(format!("winrust-ui-numbers-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let numbers = &debug::sheets()[Sheet::Numbers as usize];
    std::fs::write(dir.join("numbers.bmp"), bmp::encode24(numbers)).unwrap();
    let skin = Skin::load(&dir).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(!skin.uses_nums_ex());
    let view = MainView {
        time_mode: TimeMode::Remaining,
        ..playing()
    };
    let img = draw::render(&skin, &view, &Interaction::new());
    assert!(shows(&img, 38, 32, &skin, S::MinusSign));
    assert!(shows(&img, 60, 26, &skin, S::Digit3));
}

#[test]
fn paused_time_blinks() {
    let skin = Skin::debug();
    let paused = MainView {
        state: PlayState::Paused,
        ..playing()
    };
    let img = draw::render(&skin, &paused, &Interaction::new());
    assert!(shows(&img, 26, 28, &skin, S::MainPausedIndicator));
    assert!(shows(&img, 48, 26, &skin, S::Digit0Ex));
    let hidden = MainView {
        blink_hidden: true,
        ..paused
    };
    let img = draw::render(&skin, &hidden, &Interaction::new());
    assert!(background_at(&img, &skin, Rect::new(38, 26, 61, 13)));
}

#[test]
fn buffering_shows_the_working_indicator_instead_of_play() {
    let skin = Skin::debug();
    let view = MainView {
        state: PlayState::Buffering,
        ..playing()
    };
    let img = draw::render(&skin, &view, &Interaction::new());
    assert!(shows_part(
        &img,
        24,
        28,
        &skin,
        S::MainWorkingIndicator,
        3,
        9
    ));
    assert!(background_at(&img, &skin, Rect::new(27, 28, 8, 9)));
}

#[test]
fn title_buttons_show_the_bar_until_first_pressed() {
    let skin = Skin::debug();
    let view = playing();
    let mut ui = Interaction::new();
    let img = draw::render(&skin, &view, &ui);
    assert!(shows(&img, 0, 0, &skin, S::MainTitleBarSelected));
    ui.pointer(
        Pointer::Down {
            x: 268,
            y: 7,
            double: false,
        },
        &view,
    );
    let img = draw::render(&skin, &view, &ui);
    assert!(shows(&img, 264, 3, &skin, S::MainCloseButtonDepressed));
    ui.pointer(Pointer::Move { x: 100, y: 50 }, &view);
    let img = draw::render(&skin, &view, &ui);
    assert!(shows(&img, 264, 3, &skin, S::MainCloseButton));
    // An unfocused window shows its title bar only.
    let unfocused = MainView {
        focused: false,
        ..playing()
    };
    let img = draw::render(&skin, &unfocused, &ui);
    assert!(shows(&img, 0, 0, &skin, S::MainTitleBar));
}

#[test]
fn the_easter_egg_swaps_the_title_bar() {
    let skin = Skin::debug();
    let view = MainView {
        easter_egg: true,
        ..playing()
    };
    let img = draw::render(&skin, &view, &Interaction::new());
    assert!(shows(&img, 0, 0, &skin, S::MainEasterEggTitleBarSelected));
}

#[test]
fn sliders_show_their_frame_and_thumb() {
    let skin = Skin::debug();
    let view = MainView {
        balance: -50,
        ..playing()
    };
    let img = draw::render(&skin, &view, &Interaction::new());
    // Volume 50 is frame 13; its thumb sits 27 pixels along.
    let vol = skin.sprite(S::MainVolumeBackground);
    for dx in 0..68 {
        let frame_px = vol.image.get(vol.rect.x + dx, vol.rect.y + 15 * 13 + 5);
        if !(27..41).contains(&dx) {
            assert_eq!(img.get(107 + dx, 62), frame_px, "volume x {dx}");
        }
    }
    assert!(shows(&img, 107 + 27, 58, &skin, S::MainVolumeThumb));
    // Balance -50 is frame 13 too; its thumb sits 6 pixels along.
    let bal = skin.sprite(S::MainBalanceBackground);
    assert_eq!(
        img.get(177, 62),
        bal.image.get(bal.rect.x, bal.rect.y + 15 * 13 + 5)
    );
    assert!(shows(&img, 177 + 6, 58, &skin, S::MainBalanceThumb));
}

#[test]
fn dragging_a_slider_shows_its_value_in_the_title() {
    let skin = Skin::debug();
    let view = playing();
    let mut ui = Interaction::new();
    ui.pointer(
        Pointer::Down {
            x: 140,
            y: 63,
            double: false,
        },
        &view,
    );
    let img = draw::render(&skin, &view, &ui);
    assert!(shows(&img, 107 + 27, 58, &skin, S::MainVolumeThumbSelected));
    for (i, c) in "Volume: 50%".chars().enumerate() {
        assert!(shows_glyph(&img, 111 + 5 * i as u32, 27, &skin, c), "{c}");
    }
}

#[test]
fn toggles_and_pressed_buttons() {
    let skin = Skin::debug();
    let view = MainView {
        shuffle: true,
        eq: true,
        double_size: true,
        ..playing()
    };
    let mut ui = Interaction::new();
    ui.pointer(
        Pointer::Down {
            x: 50,
            y: 95,
            double: false,
        },
        &view,
    );
    let img = draw::render(&skin, &view, &ui);
    assert!(shows(&img, 39, 88, &skin, S::MainPlayButtonActive));
    assert!(shows(&img, 16, 88, &skin, S::MainPreviousButton));
    // Next is 22 pixels wide in the window.
    assert!(shows_part(&img, 108, 88, &skin, S::MainNextButton, 22, 18));
    // Repeat covers shuffle's last column.
    assert!(shows_part(
        &img,
        164,
        89,
        &skin,
        S::MainShuffleButtonSelected,
        46,
        15
    ));
    assert!(shows(&img, 210, 89, &skin, S::MainRepeatButton));
    assert!(shows(&img, 219, 58, &skin, S::MainEqButtonSelected));
    assert!(shows(&img, 242, 58, &skin, S::MainPlaylistButton));
    assert!(shows(&img, 10, 47, &skin, S::MainClutterBarButtonDSelected));
}

#[test]
fn shade_mode_draws_the_bar() {
    let skin = Skin::debug();
    let view = MainView {
        shade: true,
        ..playing()
    };
    let img = draw::render(&skin, &view, &Interaction::new());
    assert_eq!((img.width, img.height), (275, 14));
    assert!(shows_part(
        &img,
        0,
        0,
        &skin,
        S::MainShadeBackgroundSelected,
        6,
        14
    ));
    assert!(shows(&img, 254, 3, &skin, S::MainShadeButtonSelected));
    // Mini time " 01:23" in text.bmp cells at 128, 134, 139, 147, 152.
    for (x, c) in [(128, ' '), (134, '0'), (139, '1'), (147, '2'), (152, '3')] {
        assert!(shows_glyph(&img, x, 4, &skin, c), "{c} at {x}");
    }
    assert!(shows_part(
        &img,
        226,
        4,
        &skin,
        S::MainShadePositionBackground,
        4,
        7
    ));
    // 28%: the left-hand thumb, 4 pixels along the 14-pixel travel.
    assert!(shows(
        &img,
        226 + 4,
        4,
        &skin,
        S::MainShadePositionThumbLeft
    ));
}

#[test]
fn the_visualizer_frame_is_drawn_in_place() {
    let skin = Skin::debug();
    let view = MainView {
        vis: Some(Image::new(76, 16, 0xFF12_3456)),
        ..playing()
    };
    let img = draw::render(&skin, &view, &Interaction::new());
    assert_eq!(img.get(24, 43), 0xFF12_3456);
    assert_eq!(img.get(99, 58), 0xFF12_3456);
    let shade = MainView {
        shade: true,
        vis: Some(Image::new(38, 5, 0xFF65_4321)),
        ..playing()
    };
    let img = draw::render(&skin, &shade, &Interaction::new());
    assert_eq!(img.get(79, 5), 0xFF65_4321);
    assert_eq!(img.get(116, 9), 0xFF65_4321);
}

#[test]
fn long_titles_scroll_within_the_marquee() {
    let skin = Skin::debug();
    let view = MainView {
        title: "1. A song title much longer than thirty characters (3:45)".into(),
        title_offset: 7,
        ..playing()
    };
    let img = draw::render(&skin, &view, &Interaction::new());
    // Offset 7: the '.' (5..10) starts at 111 - 2, cut to its last 3 columns; ' ' follows.
    assert!(shows_glyph(&img, 114, 27, &skin, ' '));
    let dot = skin.glyph('.');
    for dx in 2..5 {
        assert_eq!(
            img.get(109 + dx, 27),
            dot.image.get(dot.rect.x + dx, dot.rect.y)
        );
    }
    // Nothing is drawn outside the marquee.
    let bg = skin.sprite(S::MainWindowBackground);
    for x in [110, 265] {
        assert_eq!(img.get(x, 27), bg.image.get(x, 27), "x {x}");
    }
}

#[test]
fn unmapped_characters_draw_as_spaces() {
    let skin = Skin::debug();
    let view = MainView {
        title: "日本 🎵".into(),
        ..playing()
    };
    let img = draw::render(&skin, &view, &Interaction::new());
    for i in 0..4 {
        assert!(shows_glyph(&img, 111 + 5 * i, 27, &skin, ' '), "cell {i}");
    }
}

#[test]
fn tiny_sheets_draw_black_or_base_pixels() {
    // A skin whose main.bmp and cbuttons.bmp are far smaller than Winamp's.
    let dir = std::env::temp_dir().join(format!("winrust-ui-tiny-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("main.bmp"),
        bmp::encode24(&Image::new(20, 20, 0xFFFF_0000)),
    )
    .unwrap();
    std::fs::write(
        dir.join("cbuttons.bmp"),
        bmp::encode24(&Image::new(5, 5, 0xFF00_FF00)),
    )
    .unwrap();
    let skin = Skin::load(&dir).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    let img = draw::render(&skin, &playing(), &Interaction::new());
    assert_eq!((img.width, img.height), (275, 116));
    // Inside the tiny bitmap its own colour; past it, black (as Winamp pads small sheets).
    assert_eq!(img.get(2, 16), 0xFFFF_0000);
    assert_eq!(img.get(150, 110), winrust_skin::image::BLACK);
    // MAIN_PREVIOUS_BUTTON starts inside the 5×5 cbuttons.bmp; MAIN_PLAY_BUTTON does not
    // and comes from the base skin.
    assert_eq!(img.get(16, 88), 0xFF00_FF00);
    assert!(shows(&img, 39, 88, &Skin::base(), S::MainPlayButton));
}
