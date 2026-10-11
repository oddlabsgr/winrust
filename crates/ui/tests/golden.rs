mod common;

use common::assert_golden;
use std::time::Duration;
use winrust_skin::{Image, Rect, Skin};
use winrust_ui::draw;
use winrust_ui::interaction::{Interaction, Pointer};
use winrust_ui::view::{MainView, PlayState, TimeMode};

/// A fixed visualizer frame in the skin's colours. The real analyzer is tested in `vis.rs`;
/// its floating-point maths may differ in the last bit between platforms.
fn vis_pattern(skin: &Skin, w: u32, h: u32) -> Image {
    let mut img = Image::new(w, h, skin.vis_colors[0]);
    for x in 0..w {
        let height = (x * 5 + 3) % (h + 1);
        for y in h - height..h {
            img.set(x, y, skin.vis_colors[(2 + y * 16 / h) as usize]);
        }
    }
    img
}

fn press(view: &MainView, x: i32, y: i32) -> Interaction {
    let mut ui = Interaction::new();
    ui.pointer(
        Pointer::Down {
            x,
            y,
            double: false,
        },
        view,
    );
    ui
}

/// The main window in a range of states, stacked with a 2-pixel gap.
fn states(skin: &Skin) -> Image {
    let playing = MainView {
        state: PlayState::Playing,
        elapsed: Duration::from_secs(83),
        duration: Some(Duration::from_secs(296)),
        title: "1. Artist - Title (4:56)".into(),
        kbps: Some(128),
        khz: Some(44),
        channels: Some(2),
        volume: 75,
        balance: -40,
        seekable: true,
        focused: true,
        vis: Some(vis_pattern(skin, 76, 16)),
        ..MainView::default()
    };
    let remaining = MainView {
        time_mode: TimeMode::Remaining,
        shuffle: true,
        repeat: true,
        eq: true,
        playlist: true,
        ..playing.clone()
    };
    let paused = MainView {
        state: PlayState::Paused,
        title: "2. A song title much longer than thirty characters (3:45)".into(),
        title_offset: 37,
        channels: Some(1),
        double_size: true,
        always_on_top: true,
        ..playing.clone()
    };
    let buffering = MainView {
        state: PlayState::Buffering,
        title: "3. Radio stream".into(),
        duration: None,
        seekable: false,
        kbps: Some(96),
        khz: Some(22),
        vis: None,
        ..playing.clone()
    };
    let stopped = MainView {
        title: "Winrust".into(),
        ..MainView::default()
    };
    let shade = MainView {
        shade: true,
        vis: Some(vis_pattern(skin, 38, 5)),
        ..playing.clone()
    };
    let shade_stopped = MainView {
        shade: true,
        ..MainView::default()
    };
    let windows = [
        (stopped.clone(), Interaction::new()),
        (playing.clone(), press(&playing, 50, 95)),
        (remaining.clone(), press(&remaining, 268, 7)),
        (paused.clone(), Interaction::new()),
        (buffering.clone(), press(&buffering, 150, 63)),
        (shade.clone(), Interaction::new()),
        (shade_stopped.clone(), Interaction::new()),
    ];
    let images: Vec<Image> = windows
        .iter()
        .map(|(view, ui)| draw::render(skin, view, ui))
        .collect();
    let height = images.iter().map(|i| i.height + 2).sum::<u32>() - 2;
    let mut out = Image::new(275, height, 0xFFFF_00FF);
    let mut y = 0;
    for img in &images {
        out.blit(img, Rect::new(0, 0, img.width, img.height), 0, y);
        y += img.height + 2;
    }
    out
}

#[test]
fn debug_skin_main_window() {
    assert_golden("main-debug", &states(&Skin::debug()));
}

#[test]
fn base_skin_main_window() {
    assert_golden("main-base", &states(&Skin::base()));
}
