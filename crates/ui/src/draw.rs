//! The compositor: draws the main window at 1× from a skin, a [`MainView`] and the pointer
//! state. Which sprite shows when follows Webamp's `skinSelectors.ts` and CSS at 999f567.

use crate::interaction::Interaction;
use crate::layout::{self, Mode, Widget};
use crate::marquee;
use crate::view::{self, MainView, PlayState};
use winrust_skin::{Image, Rect, Skin, SpriteId as S, SpriteRef};

/// The main window in the mode `view` asks for, as a new image.
pub fn render(skin: &Skin, view: &MainView, ui: &Interaction) -> Image {
    let mode = if view.shade {
        Mode::Shade
    } else {
        Mode::Normal
    };
    let (w, h) = mode.size();
    let mut out = Image::new(w, h, winrust_skin::image::BLACK);
    if view.shade {
        draw_main_shade(skin, view, ui, &mut out);
    } else {
        draw_main(skin, view, ui, &mut out);
    }
    out
}

/// Draws the normal main window into `out` (275×116).
pub fn draw_main(skin: &Skin, view: &MainView, ui: &Interaction, out: &mut Image) {
    put(out, skin.sprite(S::MainWindowBackground), 0, 0);
    let bar = match (view.easter_egg, view.focused) {
        (true, true) => S::MainEasterEggTitleBarSelected,
        (true, false) => S::MainEasterEggTitleBar,
        (false, true) => S::MainTitleBarSelected,
        (false, false) => S::MainTitleBar,
    };
    put(out, skin.sprite(bar), 0, 0);
    title_buttons(skin, view, ui, Mode::Normal, out);
    clutter_bar(skin, view, ui, out);
    play_state(skin, view, out);
    time(skin, view, out);
    if let Some(vis) = view
        .vis
        .as_ref()
        .filter(|v| (v.width, v.height) == (76, 16))
    {
        out.blit(vis, Rect::new(0, 0, 76, 16), 24, 43);
    }
    title_text(skin, view, ui, out);
    media_info(skin, view, out);
    sliders(skin, view, ui, out);
    buttons(skin, view, ui, out);
    seek_bar(skin, view, ui, Mode::Normal, out);
}

/// Draws the shade-mode bar into `out` (275×14).
pub fn draw_main_shade(skin: &Skin, view: &MainView, ui: &Interaction, out: &mut Image) {
    let bar = if view.focused {
        S::MainShadeBackgroundSelected
    } else {
        S::MainShadeBackground
    };
    put(out, skin.sprite(bar), 0, 0);
    title_buttons(skin, view, ui, Mode::Shade, out);
    if let Some(vis) = view.vis.as_ref().filter(|v| (v.width, v.height) == (38, 5)) {
        out.blit(vis, Rect::new(0, 0, 38, 5), 79, 5);
    }
    // Webamp draws five spaces under the mini time, so empty cells show text.bmp too.
    let cells = [1, 7, 12, 20, 25];
    for left in cells {
        glyph(out, skin, ' ', 127 + left, 4, Rect::new(0, 0, 275, 14));
    }
    if time_shown(view) {
        let t = view::time_digits(view.elapsed, view.duration, view.time_mode);
        let digit = |i: usize| char::from(b'0' + t.digits[i]);
        let chars = [
            if t.minus { '-' } else { ' ' },
            digit(0),
            digit(1),
            digit(2),
            digit(3),
        ];
        for (c, left) in chars.into_iter().zip(cells) {
            glyph(out, skin, c, 127 + left, 4, Rect::new(0, 0, 275, 14));
        }
    }
    seek_bar(skin, view, ui, Mode::Shade, out);
}

fn put(out: &mut Image, s: SpriteRef, x: u32, y: u32) {
    out.blit(s.image, s.rect, x, y);
}

/// The `w`×`h` top-left part of a sprite: Webamp shows some sprites in a smaller box.
fn put_part(out: &mut Image, s: SpriteRef, w: u32, h: u32, x: u32, y: u32) {
    let r = Rect::new(s.rect.x, s.rect.y, w.min(s.rect.w), h.min(s.rect.h));
    out.blit(s.image, r, x, y);
}

/// A sprite at its widget's place.
fn put_at(out: &mut Image, s: SpriteRef, mode: Mode, widget: Widget) {
    if let Some(r) = layout::rect(mode, widget) {
        put_part(out, s, r.w, r.h, r.x, r.y);
    }
}

/// A `text.bmp` glyph with its top-left corner at (x, y), cut to `clip`.
fn glyph(out: &mut Image, skin: &Skin, c: char, x: i32, y: u32, clip: Rect) {
    let g = skin.glyph(c);
    for dy in 0..g.rect.h {
        for dx in 0..g.rect.w {
            let px = x + dx as i32;
            if layout::contains(clip, px, (y + dy) as i32) {
                let color = g.image.get(g.rect.x + dx, g.rect.y + dy);
                out.set(px as u32, y + dy, color);
            }
        }
    }
}

fn text(out: &mut Image, skin: &Skin, s: &str, x: i32, y: u32, clip: Rect) {
    for (i, c) in s.chars().enumerate() {
        glyph(out, skin, c, x + 5 * i as i32, y, clip);
    }
}

/// Title buttons show the title bar until first pressed, and only in the focused window
/// (Webamp `.selected #title-bar #close.clicked`). In shade mode the shade button always
/// shows its "unshade" sprite when focused.
fn title_buttons(skin: &Skin, view: &MainView, ui: &Interaction, mode: Mode, out: &mut Image) {
    let shade_mode = mode == Mode::Shade;
    let buttons = [
        (
            Widget::Options,
            S::MainOptionsButton,
            S::MainOptionsButtonDepressed,
        ),
        (
            Widget::Minimize,
            S::MainMinimizeButton,
            S::MainMinimizeButtonDepressed,
        ),
        (
            Widget::Shade,
            S::MainShadeButton,
            S::MainShadeButtonDepressed,
        ),
        (
            Widget::Close,
            S::MainCloseButton,
            S::MainCloseButtonDepressed,
        ),
    ];
    for (widget, up, down) in buttons {
        let pressed = ui.pressed() == Some(widget);
        let sprite = if shade_mode && widget == Widget::Shade {
            if pressed {
                Some(S::MainShadeButtonSelectedDepressed)
            } else {
                view.focused.then_some(S::MainShadeButtonSelected)
            }
        } else if !view.focused {
            None
        } else if pressed {
            Some(down)
        } else {
            ui.title_button_clicked(widget).then_some(up)
        };
        if let Some(id) = sprite {
            put_at(out, skin.sprite(id), mode, widget);
        }
    }
}

/// The clutter bar: A lit for always-on-top, D for double-size, any letter while pressed.
fn clutter_bar(skin: &Skin, view: &MainView, ui: &Interaction, out: &mut Image) {
    put(out, skin.sprite(S::MainClutterBarBackground), 10, 22);
    let letters = [
        (Widget::ClutterO, S::MainClutterBarButtonOSelected, false),
        (
            Widget::ClutterA,
            S::MainClutterBarButtonASelected,
            view.always_on_top,
        ),
        (Widget::ClutterI, S::MainClutterBarButtonISelected, false),
        (
            Widget::ClutterD,
            S::MainClutterBarButtonDSelected,
            view.double_size,
        ),
        (Widget::ClutterV, S::MainClutterBarButtonVSelected, false),
    ];
    for (widget, id, on) in letters {
        if on || ui.pressed() == Some(widget) {
            put_at(out, skin.sprite(id), Mode::Normal, widget);
        }
    }
}

/// The play/pause/stop indicator and, beside it, the 3-pixel work indicator: "not
/// working" while playing, "working" (with no play indicator) while buffering.
fn play_state(skin: &Skin, view: &MainView, out: &mut Image) {
    let indicator = match view.state {
        PlayState::Playing => Some(S::MainPlayingIndicator),
        PlayState::Paused => Some(S::MainPausedIndicator),
        PlayState::Stopped => Some(S::MainStoppedIndicator),
        PlayState::Buffering => None,
    };
    if let Some(id) = indicator {
        put(out, skin.sprite(id), 26, 28);
    }
    let work = match view.state {
        PlayState::Playing => Some(S::MainNotWorkingIndicator),
        PlayState::Buffering => Some(S::MainWorkingIndicator),
        _ => None,
    };
    if let Some(id) = work {
        put_part(out, skin.sprite(id), 3, 9, 24, 28);
    }
}

/// Hidden while stopped, and during the hidden half of the paused blink.
fn time_shown(view: &MainView) -> bool {
    match view.state {
        PlayState::Stopped => false,
        PlayState::Paused => !view.blink_hidden,
        _ => true,
    }
}

const DIGITS: [S; 10] = [
    S::Digit0,
    S::Digit1,
    S::Digit2,
    S::Digit3,
    S::Digit4,
    S::Digit5,
    S::Digit6,
    S::Digit7,
    S::Digit8,
    S::Digit9,
];

const DIGITS_EX: [S; 10] = [
    S::Digit0Ex,
    S::Digit1Ex,
    S::Digit2Ex,
    S::Digit3Ex,
    S::Digit4Ex,
    S::Digit5Ex,
    S::Digit6Ex,
    S::Digit7Ex,
    S::Digit8Ex,
    S::Digit9Ex,
];

/// The time: `nums_ex` draws a whole 9×13 minus cell at (38, 26); `numbers` a 5×1 minus
/// at (38, 32). The colon is part of the background.
fn time(skin: &Skin, view: &MainView, out: &mut Image) {
    if !time_shown(view) {
        return;
    }
    let t = view::time_digits(view.elapsed, view.duration, view.time_mode);
    let ex = skin.uses_nums_ex();
    match (ex, t.minus) {
        (true, true) => put(out, skin.sprite(S::MinusSignEx), 38, 26),
        (true, false) => put(out, skin.sprite(S::NoMinusSignEx), 38, 26),
        (false, true) => put(out, skin.sprite(S::MinusSign), 38, 32),
        (false, false) => put(out, skin.sprite(S::NoMinusSign), 38, 32),
    }
    let digits = if ex { &DIGITS_EX } else { &DIGITS };
    for (d, x) in t.digits.iter().zip([48, 60, 78, 90]) {
        put(out, skin.sprite(digits[*d as usize]), x, 26);
    }
}

/// The scrolling title, or a slider's value while it is dragged.
fn title_text(skin: &Skin, view: &MainView, ui: &Interaction, out: &mut Image) {
    let clip = Rect::new(111, 27, 154, 6);
    let (s, offset) = match ui.sliding() {
        Some((Widget::Volume, _)) => (view::volume_text(view.volume), 0),
        Some((Widget::Balance, _)) => (view::balance_text(view.balance), 0),
        Some((Widget::Seek, f)) => (view::seek_text(f, view.duration.unwrap_or_default()), 0),
        _ => (view.title.clone(), view.title_offset),
    };
    text(
        out,
        skin,
        &marquee::looped(&s),
        111 - offset as i32,
        27,
        clip,
    );
}

/// Bitrate, sample rate and mono/stereo; all unlit while stopped.
fn media_info(skin: &Skin, view: &MainView, out: &mut Image) {
    let playing = view.state != PlayState::Stopped;
    if playing {
        if let Some(kbps) = view.kbps {
            let clip = Rect::new(111, 43, 15, 6);
            text(out, skin, &view::kbps_text(kbps), 111, 43, clip);
        }
        if let Some(khz) = view.khz {
            let clip = Rect::new(156, 43, 10, 6);
            text(out, skin, &view::khz_text(khz), 156, 43, clip);
        }
    }
    let channels = view.channels.filter(|_| playing).unwrap_or(0);
    let mono = if channels == 1 {
        S::MainMonoSelected
    } else {
        S::MainMono
    };
    let stereo = if channels >= 2 {
        S::MainStereoSelected
    } else {
        S::MainStereo
    };
    put(out, skin.sprite(mono), 212, 41);
    put(out, skin.sprite(stereo), 239, 41);
}

/// Volume and balance: the background frame for the value, and the thumb.
fn sliders(skin: &Skin, view: &MainView, ui: &Interaction, out: &mut Image) {
    let dragging = |w| ui.sliding().is_some_and(|(s, _)| s == w);
    let sliders = [
        (
            Widget::Volume,
            S::MainVolumeBackground,
            view::volume_frame(view.volume),
            view::volume_fraction(view.volume),
            [S::MainVolumeThumb, S::MainVolumeThumbSelected],
        ),
        (
            Widget::Balance,
            S::MainBalanceBackground,
            view::balance_frame(view.balance),
            view::balance_fraction(view.balance),
            [S::MainBalanceThumb, S::MainBalanceThumbActive],
        ),
    ];
    for (widget, background, frame, fraction, thumbs) in sliders {
        let Some(track) = layout::rect(Mode::Normal, widget) else {
            continue;
        };
        let bg = skin.sprite(background);
        // The background keeps its sprite's width; the volume track is 3 pixels shorter.
        let from = Rect::new(bg.rect.x, bg.rect.y + 15 * frame, bg.rect.w, 13);
        out.blit(bg.image, from, track.x, track.y);
        let thumb = thumbs[usize::from(dragging(widget))];
        let width = layout::thumb_width(Mode::Normal, widget);
        let x = track.x + view::thumb_offset(track.w, width, fraction);
        put(out, skin.sprite(thumb), x, track.y + 1);
    }
}

/// Transport buttons, EQ/PL toggles, shuffle and repeat.
fn buttons(skin: &Skin, view: &MainView, ui: &Interaction, out: &mut Image) {
    let pressed = |w| ui.pressed() == Some(w);
    let transport = [
        (
            Widget::Previous,
            S::MainPreviousButton,
            S::MainPreviousButtonActive,
        ),
        (Widget::Play, S::MainPlayButton, S::MainPlayButtonActive),
        (Widget::Pause, S::MainPauseButton, S::MainPauseButtonActive),
        (Widget::Stop, S::MainStopButton, S::MainStopButtonActive),
        (Widget::Next, S::MainNextButton, S::MainNextButtonActive),
        (Widget::Eject, S::MainEjectButton, S::MainEjectButtonActive),
    ];
    for (widget, up, down) in transport {
        let id = if pressed(widget) { down } else { up };
        put_at(out, skin.sprite(id), Mode::Normal, widget);
    }
    // [off, off pressed, on, on pressed]
    let toggles = [
        (
            Widget::Eq,
            view.eq,
            [
                S::MainEqButton,
                S::MainEqButtonDepressed,
                S::MainEqButtonSelected,
                S::MainEqButtonDepressedSelected,
            ],
        ),
        (
            Widget::Playlist,
            view.playlist,
            [
                S::MainPlaylistButton,
                S::MainPlaylistButtonDepressed,
                S::MainPlaylistButtonSelected,
                S::MainPlaylistButtonDepressedSelected,
            ],
        ),
        (
            Widget::Shuffle,
            view.shuffle,
            [
                S::MainShuffleButton,
                S::MainShuffleButtonDepressed,
                S::MainShuffleButtonSelected,
                S::MainShuffleButtonSelectedDepressed,
            ],
        ),
        // Repeat after shuffle: it overlaps shuffle by a column and sits on top.
        (
            Widget::Repeat,
            view.repeat,
            [
                S::MainRepeatButton,
                S::MainRepeatButtonDepressed,
                S::MainRepeatButtonSelected,
                S::MainRepeatButtonSelectedDepressed,
            ],
        ),
    ];
    for (widget, on, sprites) in toggles {
        let id = sprites[usize::from(on) * 2 + usize::from(pressed(widget))];
        put_at(out, skin.sprite(id), Mode::Normal, widget);
    }
}

/// The seek bar; its thumb shows only while a seekable track plays or pauses. In shade mode
/// the thumb has left, middle and right sprites (Webamp `Position.tsx`).
fn seek_bar(skin: &Skin, view: &MainView, ui: &Interaction, mode: Mode, out: &mut Image) {
    let shade = mode == Mode::Shade;
    let background = if shade {
        S::MainShadePositionBackground
    } else {
        S::MainPositionSliderBackground
    };
    put_at(out, skin.sprite(background), mode, Widget::Seek);
    let Some(duration) = view
        .duration
        .filter(|_| view.seekable && view.state != PlayState::Stopped)
    else {
        return;
    };
    let dragged = ui.sliding().filter(|(w, _)| *w == Widget::Seek);
    let fraction = dragged.map_or(view::seek_fraction(view.elapsed, duration), |(_, f)| f);
    let thumb = match (shade, dragged.is_some()) {
        (false, false) => S::MainPositionSliderThumb,
        (false, true) => S::MainPositionSliderThumbSelected,
        (true, _) if fraction <= 0.33 => S::MainShadePositionThumbLeft,
        (true, _) if fraction >= 0.66 => S::MainShadePositionThumbRight,
        (true, _) => S::MainShadePositionThumb,
    };
    let Some(track) = layout::rect(mode, Widget::Seek) else {
        return;
    };
    let width = layout::thumb_width(mode, Widget::Seek);
    let x = track.x + view::thumb_offset(track.w, width, fraction);
    put(out, skin.sprite(thumb), x, track.y);
}
