use std::time::Duration;
use winrust_ui::view::{self, MainView, PlayState, TimeDigits, TimeMode};

fn secs(s: u64) -> Duration {
    Duration::from_secs(s)
}

#[test]
fn a_new_view_is_stopped_and_empty() {
    let v = MainView::default();
    assert_eq!(v.state, PlayState::Stopped);
    assert_eq!(v.time_mode, TimeMode::Elapsed);
    assert!(!v.shade && !v.blink_hidden && v.vis.is_none());
}

#[test]
fn time_shows_elapsed_or_remaining() {
    let t = |elapsed, duration, mode| view::time_digits(elapsed, duration, mode);
    assert_eq!(
        t(secs(83), Some(secs(296)), TimeMode::Elapsed),
        TimeDigits {
            minus: false,
            digits: [0, 1, 2, 3]
        }
    );
    // 4:56 - 1:23 = 3:33.
    assert_eq!(
        t(secs(83), Some(secs(296)), TimeMode::Remaining),
        TimeDigits {
            minus: true,
            digits: [0, 3, 3, 3]
        }
    );
    // Unknown duration (a stream): elapsed time, without the minus sign.
    assert_eq!(
        t(secs(83), None, TimeMode::Remaining),
        TimeDigits {
            minus: false,
            digits: [0, 1, 2, 3]
        }
    );
    // Partial seconds are dropped.
    let almost = Duration::from_millis(59_999);
    assert_eq!(t(almost, None, TimeMode::Elapsed).digits, [0, 0, 5, 9]);
}

#[test]
fn minutes_above_99_show_as_hours_and_minutes() {
    let digits = |s| view::time_digits(secs(s), None, TimeMode::Elapsed).digits;
    assert_eq!(digits(99 * 60 + 59), [9, 9, 5, 9]);
    assert_eq!(digits(100 * 60), [0, 1, 4, 0]);
    assert_eq!(digits(200 * 3600), [9, 9, 0, 0]);
}

#[test]
fn slider_frames_follow_webamp() {
    // MainVolume.tsx: round(v * 28 / 100) - 1, never below the first frame.
    for (v, frame) in [(0, 0), (5, 0), (6, 1), (50, 13), (100, 27)] {
        assert_eq!(view::volume_frame(v), frame, "volume {v}");
    }
    // MainBalance.tsx: floor(|b| * 27 / 100).
    for (b, frame) in [(0, 0), (3, 0), (4, 1), (-50, 13), (-100, 27), (100, 27)] {
        assert_eq!(view::balance_frame(b), frame, "balance {b}");
    }
}

#[test]
fn thumbs_travel_the_track_less_the_thumb() {
    assert_eq!(view::thumb_offset(68, 14, 0.0), 0);
    assert_eq!(view::thumb_offset(68, 14, 0.5), 27);
    assert_eq!(view::thumb_offset(68, 14, 1.0), 54);
    assert_eq!(view::thumb_offset(68, 14, 2.0), 54);
    assert_eq!(view::thumb_offset(248, 29, 0.28), 61);
}

#[test]
fn fractions_of_volume_balance_and_position() {
    assert_eq!(view::volume_fraction(55), 0.55);
    assert_eq!(view::balance_fraction(-100), 0.0);
    assert_eq!(view::balance_fraction(0), 0.5);
    // Webamp rounds the position to whole percent of whole seconds.
    let elapsed = Duration::from_millis(83_900);
    assert_eq!(view::seek_fraction(elapsed, secs(296)), 0.28);
    assert_eq!(view::seek_fraction(secs(5), secs(0)), 0.0);
}

#[test]
fn balance_snaps_to_the_centre_within_25() {
    for (b, snapped) in [(24, 0), (-24, 0), (25, 25), (-100, -100)] {
        assert_eq!(view::snap_balance(b), snapped, "{b}");
    }
}

#[test]
fn slider_texts_follow_webamp() {
    assert_eq!(view::volume_text(55), "Volume: 55%");
    assert_eq!(view::balance_text(0), "Balance: Center");
    assert_eq!(view::balance_text(-30), "Balance: 30% Left");
    assert_eq!(view::balance_text(30), "Balance: 30% Right");
    assert_eq!(
        view::seek_text(0.28, secs(296)),
        "Seek to: 01:22/04:56 (28%)"
    );
}

#[test]
fn bitrate_and_sample_rate_fill_their_cells() {
    assert_eq!(view::kbps_text(128), "128");
    assert_eq!(view::kbps_text(96), " 96");
    // Wider numbers are cut to the 3-character cell, as Winamp shows "141" for 1411.
    assert_eq!(view::kbps_text(1411), "141");
    assert_eq!(view::khz_text(44), "44");
    assert_eq!(view::khz_text(8), " 8");
    assert_eq!(view::khz_text(192), "19");
}

#[test]
fn out_of_range_values_are_clamped() {
    assert_eq!(view::volume_frame(255), 27);
    assert_eq!(view::volume_fraction(255), 1.0);
    assert_eq!(view::balance_frame(-128), 27);
    assert_eq!(view::balance_fraction(-128), 0.0);
    // Elapsed past the duration: remaining time stops at zero, the seek bar at the end.
    let t = view::time_digits(secs(300), Some(secs(296)), TimeMode::Remaining);
    assert_eq!(t.digits, [0, 0, 0, 0]);
    assert_eq!(view::seek_fraction(secs(300), secs(296)), 1.0);
    assert_eq!(
        view::seek_text(2.0, secs(296)),
        "Seek to: 04:56/04:56 (100%)"
    );
}
