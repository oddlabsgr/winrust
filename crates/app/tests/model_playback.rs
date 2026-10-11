mod common;

use common::*;
use std::time::Duration;
use winrust_app::model::{Action, Control, Effect};
use winrust_audio::{Command, EngineError, EngineEvent, PlaybackState, Source};

#[test]
fn opening_replaces_the_queue_and_plays_the_first_entry() {
    let mut h = Harness::new();
    let fx = h.apply(Action::Open(vec![file("a"), file("b")]));
    assert_eq!(commands(&fx), vec![Command::Load(file("a"))]);
    assert_eq!(h.model.marquee.text(), "1. a");
    assert!(fx.contains(&Effect::Redraw));
}

#[test]
fn opening_nothing_keeps_the_queue_and_says_so() {
    // A dropped folder with no audio files in it must not wipe what is playing.
    let mut h = Harness::playing(&["a", "b"], 60);
    let fx = h.apply(Action::Open(vec![]));
    assert_eq!(commands(&fx), vec![]);
    assert_eq!(h.model.marquee.text(), "No audio files found");
    assert_eq!(h.model.queue.len(), 2);
    assert_eq!(
        commands(&h.control(Control::Next)),
        vec![Command::Load(file("b"))]
    );
}

#[test]
fn a_started_track_is_titled_and_the_next_one_queued() {
    let mut h = Harness::new();
    h.apply(Action::Open(vec![file("a"), file("b")]));
    let fx = h.started("a", 185, false);
    assert_eq!(commands(&fx), vec![Command::QueueNext(file("b"))]);
    assert_eq!(h.model.marquee.text(), "1. a (3:05)");
    let track = h.model.track.as_ref().unwrap();
    assert_eq!((track.kbps, track.khz, track.channels), (Some(128), 44, 2));
    let fx = h.started("b", 60, true);
    assert_eq!(commands(&fx), vec![]);
    assert_eq!(h.model.marquee.text(), "2. b (1:00)");
}

#[test]
fn play_resumes_restarts_or_starts_over() {
    let mut h = Harness::playing(&["a", "b"], 60);
    assert_eq!(
        commands(&h.control(Control::Play)),
        vec![Command::Load(file("a"))],
        "playing: restart"
    );
    h.engine(EngineEvent::State(PlaybackState::Paused));
    assert_eq!(commands(&h.control(Control::Play)), vec![Command::Play]);
    h.control(Control::Stop);
    h.engine(EngineEvent::State(PlaybackState::Stopped));
    assert_eq!(
        commands(&h.control(Control::Play)),
        vec![Command::Load(file("a"))]
    );
}

#[test]
fn pause_toggles_and_does_nothing_when_stopped() {
    let mut h = Harness::playing(&["a"], 60);
    assert_eq!(commands(&h.control(Control::Pause)), vec![Command::Pause]);
    h.engine(EngineEvent::State(PlaybackState::Paused));
    assert_eq!(commands(&h.control(Control::Pause)), vec![Command::Play]);
    h.engine(EngineEvent::State(PlaybackState::Stopped));
    assert_eq!(commands(&h.control(Control::Pause)), vec![]);
}

#[test]
fn next_and_previous_load_the_neighbours() {
    let mut h = Harness::playing(&["a", "b", "c"], 60);
    assert_eq!(
        commands(&h.control(Control::Next)),
        vec![Command::Load(file("b"))]
    );
    assert_eq!(
        commands(&h.control(Control::Previous)),
        vec![Command::Load(file("a"))]
    );
    assert_eq!(
        commands(&h.control(Control::Previous)),
        vec![Command::Stop],
        "before the first entry: stop, as Webamp"
    );
}

#[test]
fn jumps_move_ten_entries() {
    let names: Vec<String> = (0..30).map(|i| format!("t{i}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut h = Harness::playing(&names, 60);
    assert_eq!(
        commands(&h.control(Control::Jump(10))),
        vec![Command::Load(file("t10"))]
    );
    assert_eq!(
        commands(&h.control(Control::Jump(-10))),
        vec![Command::Load(file("t0"))]
    );
}

#[test]
fn seeking_moves_five_seconds_inside_the_track() {
    let mut h = Harness::playing(&["a"], 60);
    h.engine(EngineEvent::Position(Duration::from_secs(3)));
    assert_eq!(
        commands(&h.control(Control::SeekBy(-5))),
        vec![Command::Seek(Duration::ZERO)]
    );
    h.engine(EngineEvent::Position(Duration::from_secs(58)));
    assert_eq!(
        commands(&h.control(Control::SeekBy(5))),
        vec![Command::Seek(Duration::from_secs(60))]
    );
    assert_eq!(h.model.elapsed, Duration::from_secs(60));
}

#[test]
fn streams_and_stopped_players_do_not_seek() {
    let mut h = Harness::new();
    h.apply(Action::Open(vec![Source::Url("http://radio/".into())]));
    let mut stream = info(&Source::Url("http://radio/".into()), 0);
    stream.duration = None;
    stream.seekable = false;
    h.engine(EngineEvent::TrackStarted {
        info: stream,
        gapless: false,
    });
    assert_eq!(commands(&h.control(Control::SeekBy(5))), vec![]);
    let mut h = Harness::playing(&["a"], 60);
    h.engine(EngineEvent::State(PlaybackState::Stopped));
    assert_eq!(commands(&h.control(Control::SeekBy(5))), vec![]);
}

#[test]
fn volume_steps_clamp_and_save() {
    let mut h = Harness::new();
    h.model.volume = 100;
    assert!(h.control(Control::VolumeBy(1)).is_empty());
    let fx = h.control(Control::VolumeBy(-1));
    assert_eq!(commands(&fx), vec![Command::SetVolume(99)]);
    assert!(fx.contains(&Effect::SaveConfig));
    h.control(Control::VolumeBy(-500));
    assert_eq!(h.model.volume, 0);
}

#[test]
fn engine_settings_carry_volume_and_balance() {
    let mut h = Harness::new();
    h.model.volume = 40;
    h.model.balance = -50;
    assert_eq!(
        h.model.engine_settings(),
        vec![Command::SetVolume(40), Command::SetBalance(-0.5)]
    );
}

#[test]
fn repeat_on_the_last_entry_queues_the_first() {
    let mut h = Harness::playing(&["a", "b"], 60);
    h.started("b", 60, true);
    let fx = h.control(Control::ToggleRepeat);
    assert_eq!(commands(&fx), vec![Command::QueueNext(file("a"))]);
    assert!(fx.contains(&Effect::SaveConfig));
    let fx = h.control(Control::ToggleRepeat);
    assert_eq!(commands(&fx), vec![Command::Unqueue]);
}

#[test]
fn stop_after_current_unqueues_and_stays_stopped_at_the_end() {
    let mut h = Harness::playing(&["a", "b"], 60);
    assert_eq!(
        commands(&h.control(Control::StopAfterCurrent)),
        vec![Command::Unqueue]
    );
    assert_eq!(commands(&h.engine(EngineEvent::TrackEnded)), vec![]);
    assert!(!h.model.queue.stop_after, "used up");
}

#[test]
fn stop_after_current_pressed_twice_queues_again() {
    let mut h = Harness::playing(&["a", "b"], 60);
    h.control(Control::StopAfterCurrent);
    assert_eq!(
        commands(&h.control(Control::StopAfterCurrent)),
        vec![Command::QueueNext(file("b"))]
    );
}

#[test]
fn stop_after_current_stops_a_gapless_start_it_was_too_late_for() {
    let mut h = Harness::playing(&["a", "b"], 60);
    h.control(Control::StopAfterCurrent);
    let fx = h.started("b", 60, true);
    assert_eq!(commands(&fx), vec![Command::Stop]);
}

#[test]
fn unreadable_file_shows_the_error_and_plays_the_next() {
    let mut h = Harness::new();
    h.apply(Action::Open(vec![file("a"), file("b")]));
    let error = EngineError::Open {
        path: "a.mp3".into(),
        reason: "not found".into(),
    };
    let fx = h.engine(EngineEvent::Error {
        source: Some(file("a")),
        error: error.clone(),
    });
    assert_eq!(commands(&fx), vec![Command::Load(file("b"))]);
    assert_eq!(h.model.marquee.text(), error.to_string());
}

#[test]
fn device_errors_only_show_a_message() {
    let mut h = Harness::playing(&["a"], 60);
    let fx = h.engine(EngineEvent::Error {
        source: None,
        error: EngineError::Device("unplugged".into()),
    });
    assert_eq!(commands(&fx), vec![]);
    assert_eq!(h.model.marquee.text(), "audio device error: unplugged");
}

#[test]
fn the_end_of_the_queue_leaves_the_player_stopped() {
    let mut h = Harness::playing(&["a"], 60);
    assert_eq!(commands(&h.engine(EngineEvent::TrackEnded)), vec![]);
    assert_eq!(
        commands(&h.control(Control::Play)),
        vec![Command::Load(file("a"))]
    );
}

#[test]
fn stream_titles_and_bitrates_update_the_display() {
    let mut h = Harness::new();
    let url = Source::Url("http://radio/".into());
    h.apply(Action::Open(vec![url.clone()]));
    assert_eq!(h.model.marquee.text(), "1. http://radio/");
    let mut stream = info(&url, 0);
    stream.duration = None;
    h.engine(EngineEvent::TrackStarted {
        info: stream,
        gapless: false,
    });
    h.engine(EngineEvent::StreamTitle("Artist - Song".into()));
    assert_eq!(h.model.marquee.text(), "1. Artist - Song");
    h.engine(EngineEvent::Bitrate(96));
    assert_eq!(h.model.track.as_ref().unwrap().kbps, Some(96));
}

#[test]
fn nothing_loaded_shows_the_name_and_version() {
    let h = Harness::new();
    assert_eq!(
        h.model.marquee.text(),
        format!("Winrust {}", env!("CARGO_PKG_VERSION"))
    );
}
