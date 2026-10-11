mod common;

use common::*;
use std::time::Duration;
use winrust_app::model::{Action, Control, Effect, FRAME};
use winrust_audio::{Command, EngineEvent, PlaybackState};
use winrust_skin::Image;
use winrust_ui::interaction::Pointer;
use winrust_ui::layout::{self, Mode, Widget};
use winrust_ui::view::{PlayState, TimeMode};
use winrust_ui::vis::VisMode;

fn centre(h: &Harness, widget: Widget) -> (i32, i32) {
    let mode = if h.model.shade {
        Mode::Shade
    } else {
        Mode::Normal
    };
    let r = layout::rect(mode, widget).unwrap();
    ((r.x + r.w / 2) as i32, (r.y + r.h / 2) as i32)
}

fn down(h: &mut Harness, x: i32, y: i32) -> Vec<Effect> {
    h.apply(Action::Pointer(Pointer::Down {
        x,
        y,
        double: false,
    }))
}

fn click(h: &mut Harness, widget: Widget) -> Vec<Effect> {
    let (x, y) = centre(h, widget);
    let mut fx = down(h, x, y);
    fx.extend(h.apply(Action::Pointer(Pointer::Up { x, y })));
    fx
}

#[test]
fn transport_buttons_drive_the_engine() {
    let mut h = Harness::playing(&["a", "b"], 60);
    assert_eq!(
        commands(&click(&mut h, Widget::Pause)),
        vec![Command::Pause]
    );
    assert_eq!(
        commands(&click(&mut h, Widget::Next)),
        vec![Command::Load(file("b"))]
    );
    assert_eq!(commands(&click(&mut h, Widget::Stop)), vec![Command::Stop]);
}

#[test]
fn a_held_button_redraws_pressed() {
    let mut h = Harness::playing(&["a"], 60);
    let (x, y) = centre(&h, Widget::Play);
    assert_eq!(down(&mut h, x, y), vec![Effect::Redraw]);
    assert_eq!(h.model.interaction.pressed(), Some(Widget::Play));
    assert_eq!(
        h.apply(Action::Pointer(Pointer::Move { x: 0, y: 115 })),
        vec![Effect::Redraw],
        "leaving the button un-presses it"
    );
    assert_eq!(
        h.apply(Action::Pointer(Pointer::Move { x: 1, y: 115 })),
        vec![],
        "nothing changed"
    );
}

#[test]
fn title_bar_buttons_minimize_shade_and_close() {
    let mut h = Harness::new();
    assert!(click(&mut h, Widget::Minimize).contains(&Effect::Minimize));
    assert!(click(&mut h, Widget::Close).contains(&Effect::Exit));
    assert!(click(&mut h, Widget::Shade).contains(&Effect::Resize));
    assert!(h.model.shade);
    assert!(click(&mut h, Widget::Shade).contains(&Effect::Resize));
    assert!(!h.model.shade);
}

#[test]
fn dragging_the_title_bar_moves_the_window() {
    let mut h = Harness::new();
    down(&mut h, 100, 5);
    let fx = h.apply(Action::Pointer(Pointer::Move { x: 130, y: 25 }));
    assert!(
        fx.contains(&Effect::MoveWindow { dx: 30, dy: 20 }),
        "{fx:?}"
    );
}

#[test]
fn double_clicking_the_title_bar_shades_the_window() {
    let mut h = Harness::new();
    let fx = h.apply(Action::Pointer(Pointer::Down {
        x: 100,
        y: 5,
        double: true,
    }));
    assert!(fx.contains(&Effect::Resize));
    assert!(h.model.shade);
}

#[test]
fn shade_mode_has_its_own_transport() {
    let mut h = Harness::playing(&["a"], 60);
    h.control(Control::ToggleShade);
    assert_eq!(commands(&click(&mut h, Widget::Stop)), vec![Command::Stop]);
}

#[test]
fn clutter_bar_toggles_always_on_top_and_double_size() {
    let mut h = Harness::new();
    let fx = click(&mut h, Widget::ClutterA);
    assert!(fx.contains(&Effect::SetAlwaysOnTop(true)) && fx.contains(&Effect::SaveConfig));
    assert!(h.model.always_on_top);
    let fx = click(&mut h, Widget::ClutterD);
    assert!(fx.contains(&Effect::Resize) && fx.contains(&Effect::SaveConfig));
    assert!(h.model.double_size);
    assert!(
        commands(&click(&mut h, Widget::ClutterO)).is_empty(),
        "menus arrive in M7"
    );
}

#[test]
fn clicks_on_the_time_and_visualizer_switch_their_modes() {
    let mut h = Harness::new();
    assert!(click(&mut h, Widget::Time).contains(&Effect::SaveConfig));
    assert_eq!(h.model.time_mode, TimeMode::Remaining);
    assert!(click(&mut h, Widget::Vis).contains(&Effect::SaveConfig));
    assert_eq!(h.model.vis_mode, VisMode::Oscilloscope);
}

#[test]
fn eq_and_playlist_buttons_only_light_up() {
    let mut h = Harness::new();
    click(&mut h, Widget::Eq);
    click(&mut h, Widget::Playlist);
    assert!(h.model.eq && h.model.playlist);
    let fx = h.control(Control::ToggleEq);
    assert_eq!(fx, vec![Effect::Redraw]);
    assert!(!h.model.eq);
}

#[test]
fn volume_and_balance_sliders_set_the_engine_and_save() {
    let mut h = Harness::new();
    let (_, y) = centre(&h, Widget::Volume);
    let fx = down(&mut h, 171, y);
    assert_eq!(commands(&fx), vec![Command::SetVolume(100)]);
    assert!(fx.contains(&Effect::SaveConfig));
    h.apply(Action::Pointer(Pointer::Up { x: 171, y }));
    let fx = down(&mut h, 177, y);
    assert_eq!(commands(&fx), vec![Command::SetBalance(-1.0)]);
    assert_eq!(h.model.balance, -100);
}

#[test]
fn the_seek_bar_seeks_on_release() {
    let mut h = Harness::playing(&["a"], 60);
    let (x, y) = centre(&h, Widget::Seek);
    assert_eq!(commands(&down(&mut h, x, y)), vec![]);
    let fx = h.apply(Action::Pointer(Pointer::Up { x, y }));
    let [Command::Seek(to)] = commands(&fx)[..] else {
        panic!("{fx:?}")
    };
    assert!(to.as_secs() == 30, "{to:?}");
}

#[test]
fn losing_focus_drops_a_drag() {
    let mut h = Harness::new();
    let (_, y) = centre(&h, Widget::Volume);
    down(&mut h, 110, y);
    assert!(h.apply(Action::Focus(false)).contains(&Effect::Redraw));
    assert!(!h.model.focused);
    assert_eq!(
        commands(&h.apply(Action::Pointer(Pointer::Move { x: 171, y }))),
        vec![]
    );
}

#[test]
fn fade_stop_lowers_the_volume_over_a_second_then_stops() {
    let mut h = Harness::playing(&["a"], 60);
    h.model.volume = 80;
    h.apply(Action::Tick);
    assert_eq!(commands(&h.control(Control::FadeStop)), vec![]);
    h.advance(Duration::from_millis(500));
    assert_eq!(
        commands(&h.apply(Action::Tick)),
        vec![Command::SetVolume(40)]
    );
    h.advance(Duration::from_millis(500));
    assert_eq!(commands(&h.apply(Action::Tick)), vec![Command::Stop]);
    // The volume comes back only once playback has stopped: restored any sooner, the last
    // of the audio would play at full volume.
    assert_eq!(
        commands(&h.engine(EngineEvent::State(PlaybackState::Stopped))),
        vec![Command::SetVolume(80)]
    );
    assert_eq!(h.model.volume, 80, "the setting is kept");
}

#[test]
fn stopping_during_a_fade_restores_the_volume_once_stopped() {
    let mut h = Harness::playing(&["a"], 60);
    h.model.volume = 80;
    h.control(Control::FadeStop);
    h.advance(Duration::from_millis(300));
    h.apply(Action::Tick);
    assert_eq!(commands(&h.control(Control::Stop)), vec![Command::Stop]);
    assert_eq!(
        commands(&h.engine(EngineEvent::State(PlaybackState::Stopped))),
        vec![Command::SetVolume(80)]
    );
    h.advance(Duration::from_secs(1));
    assert_eq!(commands(&h.apply(Action::Tick)), vec![], "the fade is over");
}

#[test]
fn pausing_during_a_fade_restores_the_volume_once_paused() {
    let mut h = Harness::playing(&["a"], 60);
    h.model.volume = 80;
    h.control(Control::FadeStop);
    h.advance(Duration::from_millis(300));
    h.apply(Action::Tick);
    assert_eq!(commands(&h.control(Control::Pause)), vec![Command::Pause]);
    assert_eq!(
        commands(&h.engine(EngineEvent::State(PlaybackState::Paused))),
        vec![Command::SetVolume(80)]
    );
}

#[test]
fn a_track_ending_during_a_fade_restores_the_volume() {
    let mut h = Harness::playing(&["a"], 60);
    h.model.volume = 80;
    h.control(Control::FadeStop);
    h.advance(Duration::from_millis(300));
    h.apply(Action::Tick);
    h.engine(EngineEvent::TrackEnded);
    assert_eq!(
        commands(&h.engine(EngineEvent::State(PlaybackState::Stopped))),
        vec![Command::SetVolume(80)]
    );
    h.advance(Duration::from_secs(1));
    assert_eq!(
        commands(&h.apply(Action::Tick)),
        vec![],
        "no Stop for a stopped player"
    );
}

#[test]
fn fade_stop_when_not_playing_just_stops() {
    let mut h = Harness::playing(&["a"], 60);
    h.engine(EngineEvent::State(PlaybackState::Paused));
    assert_eq!(commands(&h.control(Control::FadeStop)), vec![Command::Stop]);
}

#[test]
fn the_view_shows_the_model() {
    let mut h = Harness::playing(&["a"], 185);
    h.engine(EngineEvent::Position(Duration::from_secs(30)));
    h.control(Control::ToggleEasterEgg);
    let v = h.model.view(h.now);
    assert_eq!(v.state, PlayState::Playing);
    assert_eq!(v.elapsed, Duration::from_secs(30));
    assert_eq!(v.duration, Some(Duration::from_secs(185)));
    assert_eq!(v.title, "1. a (3:05)");
    assert_eq!((v.kbps, v.khz, v.channels), (Some(128), Some(44), Some(2)));
    assert_eq!((v.volume, v.balance), (75, 0));
    assert!(v.seekable && v.easter_egg && v.focused && !v.shade);
    assert!(v.vis.is_none(), "the app adds the visualizer");
}

#[test]
fn the_paused_time_blinks_every_second() {
    let mut h = Harness::playing(&["a"], 60);
    h.engine(EngineEvent::State(PlaybackState::Paused));
    let t0 = h.now;
    let hidden = |s: f64| h.model.view(t0 + Duration::from_secs_f64(s)).blink_hidden;
    assert!(!hidden(0.5));
    assert!(hidden(1.5));
    assert!(!hidden(2.5));
}

#[test]
fn the_model_says_when_to_wake() {
    let mut h = Harness::new();
    h.apply(Action::Tick);
    assert_eq!(h.model.next_wake(h.now), None, "idle: sleep");
    let mut h = Harness::playing(&["a"], 60);
    h.apply(Action::Tick);
    assert_eq!(h.model.next_wake(h.now), Some(h.now + FRAME));
    h.engine(EngineEvent::State(PlaybackState::Paused));
    h.advance(Duration::from_millis(300));
    h.apply(Action::Tick);
    assert_eq!(
        h.model.next_wake(h.now),
        Some(h.now + Duration::from_millis(700)),
        "the next blink"
    );
    let mut h = Harness::new();
    h.apply(Action::Open(vec![file(
        "a song title much longer than thirty characters",
    )]));
    h.engine(EngineEvent::State(PlaybackState::Stopped));
    h.apply(Action::Tick);
    assert_eq!(
        h.model.next_wake(h.now),
        Some(h.now + Duration::from_millis(220)),
        "the next marquee step"
    );
}

#[test]
fn messages_last_three_seconds() {
    let mut h = Harness::playing(&["a"], 60);
    h.apply(Action::Tick);
    assert!(
        h.apply(Action::Message("config reset".into()))
            .contains(&Effect::Redraw)
    );
    assert_eq!(h.model.marquee.text(), "config reset");
    h.advance(Duration::from_secs(3));
    h.apply(Action::Tick);
    assert_eq!(h.model.marquee.text(), "1. a (1:00)");
}

#[test]
fn a_quiet_spell_does_not_cut_the_next_message_short() {
    let mut h = Harness::new();
    h.apply(Action::Tick);
    h.advance(Duration::from_secs(60));
    h.apply(Action::Message("hello".into()));
    assert_eq!(
        h.model.next_wake(h.now),
        Some(h.now + Duration::from_secs(3))
    );
    h.advance(Duration::from_millis(16));
    h.apply(Action::Tick);
    assert_eq!(h.model.marquee.text(), "hello");
}

#[test]
fn window_moves_are_saved() {
    let mut h = Harness::new();
    assert_eq!(
        h.apply(Action::Moved { x: 10, y: -20 }),
        vec![Effect::SaveConfig]
    );
    assert_eq!(h.model.position, Some((10, -20)));
}

#[test]
fn the_minimized_parking_spot_is_not_saved() {
    // Windows reports a minimized window at (-32000, -32000); saving that would lose the
    // real position for the next start.
    let mut h = Harness::new();
    h.apply(Action::Moved { x: 10, y: 20 });
    assert_eq!(
        h.apply(Action::Moved {
            x: -32000,
            y: -32000
        }),
        vec![]
    );
    assert_eq!(h.model.position, Some((10, 20)));
}

#[test]
fn the_visualizer_frame_shows_until_playback_stops() {
    let mut h = Harness::playing(&["a"], 60);
    let frame = Image::new(76, 16, 0);
    assert!(h.model.vis_for(Some(&frame)).is_some());
    assert!(
        h.model.vis_for(Some(&Image::new(38, 5, 0))).is_none(),
        "a shade-mode frame does not fit the normal window"
    );
    h.engine(EngineEvent::State(PlaybackState::Paused));
    assert!(h.model.vis_for(Some(&frame)).is_some(), "kept while paused");
    h.engine(EngineEvent::State(PlaybackState::Stopped));
    assert!(h.model.vis_for(Some(&frame)).is_none(), "gone when stopped");
}
