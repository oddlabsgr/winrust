use std::time::Duration;
use winrust_ui::interaction::{Interaction, Pointer, UiEvent};
use winrust_ui::layout::Widget;
use winrust_ui::view::{MainView, PlayState};

fn down(x: i32, y: i32) -> Pointer {
    Pointer::Down {
        x,
        y,
        double: false,
    }
}

fn mv(x: i32, y: i32) -> Pointer {
    Pointer::Move { x, y }
}

fn up(x: i32, y: i32) -> Pointer {
    Pointer::Up { x, y }
}

/// Playing a 200-second track from the start, volume 50.
fn playing() -> MainView {
    MainView {
        state: PlayState::Playing,
        duration: Some(Duration::from_secs(200)),
        seekable: true,
        volume: 50,
        ..MainView::default()
    }
}

#[test]
fn a_button_fires_when_released_over_it() {
    let v = MainView::default();
    let mut ui = Interaction::new();
    assert_eq!(ui.pointer(down(50, 95), &v), None);
    assert_eq!(ui.pressed(), Some(Widget::Play));
    assert_eq!(
        ui.pointer(up(52, 96), &v),
        Some(UiEvent::Clicked(Widget::Play))
    );
    assert_eq!(ui.pressed(), None);
}

#[test]
fn leaving_a_held_button_releases_it_until_the_pointer_returns() {
    let v = MainView::default();
    let mut ui = Interaction::new();
    ui.pointer(down(50, 95), &v);
    ui.pointer(mv(150, 95), &v);
    assert_eq!(ui.pressed(), None);
    ui.pointer(mv(51, 95), &v);
    assert_eq!(ui.pressed(), Some(Widget::Play));
    ui.pointer(mv(150, 95), &v);
    assert_eq!(ui.pointer(up(150, 95), &v), None);
}

#[test]
fn dragging_blank_body_moves_the_window() {
    // Winamp moves the main window from any spot that is not a control.
    let v = MainView::default();
    let mut ui = Interaction::new();
    assert_eq!(ui.pointer(down(132, 95), &v), None);
    assert_eq!(ui.pressed(), None);
    assert_eq!(
        ui.pointer(mv(140, 100), &v),
        Some(UiEvent::MoveWindow { dx: 8, dy: 5 })
    );
    assert_eq!(ui.pointer(up(140, 100), &v), None);
}

#[test]
fn pressing_the_volume_track_jumps_then_drags() {
    // Volume 50 puts the 14-pixel thumb at x 133-146 of the 65-pixel track at 107.
    let v = playing();
    let mut ui = Interaction::new();
    // The thumb's centre jumps to the pointer: (117 - 7 - 107) / 51 = 0.059.
    assert_eq!(ui.pointer(down(117, 63), &v), Some(UiEvent::Volume(6)));
    assert_eq!(ui.sliding().map(|(w, _)| w), Some(Widget::Volume));
    assert_eq!(ui.pointer(mv(161, 63), &v), Some(UiEvent::Volume(92)));
    assert_eq!(ui.pointer(mv(161, 70), &v), None);
    assert_eq!(ui.pointer(mv(300, 63), &v), Some(UiEvent::Volume(100)));
    assert_eq!(ui.pointer(up(300, 63), &v), None);
    assert_eq!(ui.sliding(), None);
}

#[test]
fn grabbing_the_thumb_keeps_the_grab_offset() {
    let v = playing();
    let mut ui = Interaction::new();
    // 3 pixels into the thumb at 133: nothing changes yet.
    assert_eq!(ui.pointer(down(136, 63), &v), None);
    // The thumb follows the pointer, still 3 pixels in: (143 - 107) / 51 = 0.706.
    assert_eq!(ui.pointer(mv(146, 63), &v), Some(UiEvent::Volume(71)));
    assert_eq!(ui.pointer(mv(136, 63), &v), Some(UiEvent::Volume(51)));
}

#[test]
fn balance_snaps_to_the_centre() {
    let v = playing();
    let mut ui = Interaction::new();
    assert_eq!(ui.pointer(down(210, 63), &v), Some(UiEvent::Balance(100)));
    // (199 - 7 - 177) / 24 = 0.625, which is 25% right.
    assert_eq!(ui.pointer(mv(199, 63), &v), Some(UiEvent::Balance(25)));
    // 8% right snaps to the centre.
    assert_eq!(ui.pointer(mv(197, 63), &v), Some(UiEvent::Balance(0)));
}

#[test]
fn the_seek_bar_seeks_on_release_only() {
    let v = playing();
    let mut ui = Interaction::new();
    // The 29-pixel thumb's centre jumps to x 150: (150 - 14 - 16) / 219.
    assert_eq!(ui.pointer(down(150, 76), &v), None);
    let (widget, fraction) = ui.sliding().unwrap();
    assert_eq!(widget, Widget::Seek);
    assert!((fraction - 120.0 / 219.0).abs() < 1e-6);
    assert_eq!(ui.pointer(mv(249, 76), &v), None);
    assert_eq!(ui.pointer(up(400, 76), &v), Some(UiEvent::Seek(1.0)));
}

#[test]
fn the_seek_bar_ignores_streams_and_stopped_playback() {
    let mut ui = Interaction::new();
    let stream = MainView {
        seekable: false,
        duration: None,
        ..playing()
    };
    assert_eq!(ui.pointer(down(150, 76), &stream), None);
    assert_eq!(ui.sliding(), None);
    assert_eq!(ui.pointer(up(150, 76), &stream), None);
    let stopped = MainView {
        state: PlayState::Stopped,
        ..playing()
    };
    ui.pointer(down(150, 76), &stopped);
    assert_eq!(ui.pointer(up(150, 76), &stopped), None);
}

#[test]
fn dragging_the_title_bar_moves_the_window() {
    let v = MainView::default();
    let mut ui = Interaction::new();
    assert_eq!(ui.pointer(down(100, 5), &v), None);
    assert_eq!(
        ui.pointer(mv(110, 2), &v),
        Some(UiEvent::MoveWindow { dx: 10, dy: -3 })
    );
    assert_eq!(ui.pointer(up(110, 2), &v), None);
}

#[test]
fn double_clicking_the_title_bar_toggles_shade() {
    let v = MainView::default();
    let mut ui = Interaction::new();
    let double = Pointer::Down {
        x: 100,
        y: 5,
        double: true,
    };
    assert_eq!(ui.pointer(double, &v), Some(UiEvent::ToggleShade));
    assert_eq!(ui.pointer(mv(120, 5), &v), None);
    assert_eq!(ui.pointer(up(120, 5), &v), None);
}

#[test]
fn title_buttons_show_through_until_first_pressed() {
    let v = MainView::default();
    let mut ui = Interaction::new();
    assert!(!ui.title_button_clicked(Widget::Close));
    ui.pointer(down(268, 7), &v);
    assert!(ui.title_button_clicked(Widget::Close));
    assert!(!ui.title_button_clicked(Widget::Minimize));
    assert_eq!(
        ui.pointer(up(268, 7), &v),
        Some(UiEvent::Clicked(Widget::Close))
    );
    assert!(ui.title_button_clicked(Widget::Close));
}

#[test]
fn dragging_the_title_text_scrubs_it() {
    let v = MainView::default();
    let mut ui = Interaction::new();
    assert_eq!(ui.pointer(down(150, 28), &v), Some(UiEvent::DragTitle(0)));
    assert_eq!(ui.pointer(mv(140, 28), &v), Some(UiEvent::DragTitle(-10)));
    assert_eq!(ui.pointer(mv(140, 40), &v), None);
    assert_eq!(ui.pointer(up(140, 40), &v), Some(UiEvent::DragTitleEnd));
}

#[test]
fn shade_mode_has_its_own_widgets() {
    let v = MainView {
        shade: true,
        ..playing()
    };
    let mut ui = Interaction::new();
    ui.pointer(down(180, 6), &v);
    assert_eq!(
        ui.pointer(up(180, 6), &v),
        Some(UiEvent::Clicked(Widget::Play))
    );
    // The 17-pixel seek bar has a 3-pixel thumb: (240 - 1 - 226) / 14.
    ui.pointer(down(240, 6), &v);
    assert_eq!(ui.pointer(up(240, 6), &v), Some(UiEvent::Seek(13.0 / 14.0)));
}

#[test]
fn drags_far_outside_the_window_clamp() {
    let v = playing();
    let mut ui = Interaction::new();
    ui.pointer(down(140, 63), &v);
    assert_eq!(ui.pointer(mv(-100_000, 63), &v), Some(UiEvent::Volume(0)));
    assert_eq!(
        ui.pointer(mv(100_000, -100_000), &v),
        Some(UiEvent::Volume(100))
    );
    ui.pointer(up(100_000, -100_000), &v);
    ui.pointer(down(100, 5), &v);
    assert_eq!(
        ui.pointer(mv(-100_000, 100_000), &v),
        Some(UiEvent::MoveWindow {
            dx: -100_100,
            dy: 99_995
        })
    );
}

#[test]
fn window_drag_reports_the_offset_from_the_grab_point() {
    // Pointer positions are window-relative: once the window has followed a move, the
    // next event reports only the new travel.
    let v = MainView::default();
    let mut ui = Interaction::new();
    ui.pointer(down(100, 5), &v);
    assert_eq!(
        ui.pointer(mv(110, 5), &v),
        Some(UiEvent::MoveWindow { dx: 10, dy: 0 })
    );
    assert_eq!(
        ui.pointer(mv(101, 5), &v),
        Some(UiEvent::MoveWindow { dx: 1, dy: 0 })
    );
}

#[test]
fn cancel_ends_any_grab_without_its_effect() {
    let v = playing();
    let mut ui = Interaction::new();
    // A volume drag whose button-up never arrives must not follow the pointer afterwards.
    ui.pointer(down(117, 63), &v);
    assert_eq!(ui.cancel(), None);
    assert_eq!(ui.sliding(), None);
    assert_eq!(ui.pointer(mv(160, 63), &v), None);
    // A cancelled button press does not click; a cancelled seek does not seek.
    ui.pointer(down(50, 95), &v);
    assert_eq!(ui.cancel(), None);
    assert_eq!(ui.pressed(), None);
    assert_eq!(ui.pointer(up(50, 95), &v), None);
    ui.pointer(down(150, 76), &v);
    assert_eq!(ui.cancel(), None);
    assert_eq!(ui.pointer(up(150, 76), &v), None);
    // A title-text drag still ends, so the title resumes scrolling.
    ui.pointer(down(150, 28), &v);
    assert_eq!(ui.cancel(), Some(UiEvent::DragTitleEnd));
    assert_eq!(ui.cancel(), None);
}
