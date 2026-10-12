//! Ported from M3's `app::queue` tests, plus the spec §4.6 changes.

mod common;

use common::{file, names, playlist};
use winrust_audio::Source;
use winrust_playlist::cursor::{Cursor, Requeue};
use winrust_playlist::entry::EntryId;
use winrust_playlist::playlist::Playlist;

/// The playlist `0.mp3`… and a cursor that has loaded the first entry.
fn started(n: usize) -> (Playlist, Cursor) {
    let mut pl = playlist(n);
    let mut c = Cursor::new(7);
    let first = c.play(&mut pl);
    assert_eq!(first.is_some(), n > 0);
    (pl, c)
}

/// The row of the entry that a cursor call returns.
macro_rules! row {
    ($pl:ident, $call:expr) => {{
        let id: Option<EntryId> = $call;
        id.and_then(|id| $pl.index_of(id))
    }};
}

/// The engine cannot open the entry at `index`.
fn fail(pl: &mut Playlist, c: &mut Cursor, index: usize) -> Option<EntryId> {
    let source = pl.entries()[index].source.clone();
    c.on_error(pl, &source)
}

fn select(pl: &mut Playlist, rows: &[usize]) {
    pl.select_none();
    for &r in rows {
        pl.ctrl_click(r);
    }
}

#[test]
fn plays_through_gaplessly_then_stops() {
    let (mut pl, mut c) = started(3);
    assert_eq!(row!(pl, c.on_started(&mut pl, false)), Some(1));
    assert_eq!(row!(pl, c.on_started(&mut pl, true)), Some(2));
    assert_eq!(pl.current_index(), Some(1));
    assert_eq!(c.on_started(&mut pl, true), None);
    assert_eq!(c.on_ended(&mut pl), None);
    assert_eq!(pl.current_index(), Some(2));
}

#[test]
fn an_empty_list_plays_nothing() {
    let (mut pl, mut c) = started(0);
    assert_eq!(c.play(&mut pl), None);
    assert_eq!(c.next(&mut pl), None);
}

#[test]
fn repeat_wraps_both_ways() {
    let (mut pl, mut c) = started(3);
    c.order.repeat = true;
    assert_eq!(row!(pl, c.previous(&mut pl)), Some(2));
    assert_eq!(row!(pl, c.on_started(&mut pl, false)), Some(0));
    assert_eq!(row!(pl, c.next(&mut pl)), Some(0));
}

#[test]
fn without_repeat_the_ends_stop() {
    // Webamp `getNextTrackId`: no previous before the first entry, no next after the last.
    let (mut pl, mut c) = started(3);
    assert_eq!(c.previous(&mut pl), None);
    let (mut pl, mut c) = started(3);
    c.jump(&mut pl, 2);
    assert_eq!(c.next(&mut pl), None);
}

#[test]
fn jumps_clamp_to_the_ends() {
    let (mut pl, mut c) = started(25);
    c.jump(&mut pl, 3);
    assert_eq!(row!(pl, c.jump(&mut pl, -10)), Some(0));
    assert_eq!(row!(pl, c.jump(&mut pl, 10)), Some(10));
    assert_eq!(row!(pl, c.jump(&mut pl, 10)), Some(20));
    assert_eq!(row!(pl, c.jump(&mut pl, 10)), Some(24));
}

#[test]
fn shuffle_plays_every_entry_before_repeating() {
    let (mut pl, mut c) = started(5);
    c.order.repeat = true;
    c.order.set_shuffle(true, &pl);
    let mut played = vec![pl.current_index().unwrap()];
    for _ in 0..19 {
        let next = c.on_started(&mut pl, false).unwrap();
        played.push(row!(pl, Some(next)).unwrap());
        c.on_ended(&mut pl);
    }
    for round in played.chunks(5) {
        let mut sorted = round.to_vec();
        sorted.sort();
        assert_eq!(sorted, [0, 1, 2, 3, 4], "{played:?}");
    }
}

#[test]
fn shuffle_and_repeat_with_one_entry_replay_it() {
    let (mut pl, mut c) = started(1);
    c.order.repeat = true;
    c.order.set_shuffle(true, &pl);
    assert_eq!(row!(pl, c.on_started(&mut pl, false)), Some(0));
}

#[test]
fn an_unreadable_current_entry_is_skipped_and_marked_dead() {
    let (mut pl, mut c) = started(3);
    assert_eq!(row!(pl, fail(&mut pl, &mut c, 0)), Some(1));
    assert!(pl.entries()[0].dead);
    assert_eq!(row!(pl, c.on_started(&mut pl, false)), Some(2));
}

#[test]
fn an_unreadable_queued_entry_is_skipped_at_the_end() {
    let (mut pl, mut c) = started(3);
    assert_eq!(row!(pl, c.on_started(&mut pl, false)), Some(1));
    assert_eq!(fail(&mut pl, &mut c, 1), None, "the current track plays on");
    assert_eq!(row!(pl, c.on_ended(&mut pl)), Some(2));
    assert!(pl.entries()[1].dead);
}

#[test]
fn a_dead_entry_that_plays_after_all_comes_back_to_life() {
    let (mut pl, mut c) = started(2);
    fail(&mut pl, &mut c, 0);
    let first = pl.entries()[0].id();
    c.play_entry(&mut pl, first);
    c.on_started(&mut pl, false);
    assert!(!pl.entries()[0].dead);
}

#[test]
fn errors_about_other_sources_change_nothing() {
    let (mut pl, mut c) = started(2);
    let other = Source::File("9.mp3".into());
    assert_eq!(c.on_error(&mut pl, &other), None);
    assert_eq!(row!(pl, c.on_started(&mut pl, false)), Some(1));
}

#[test]
fn every_entry_unreadable_stops_even_with_repeat() {
    let (mut pl, mut c) = started(3);
    c.order.repeat = true;
    assert_eq!(row!(pl, fail(&mut pl, &mut c, 0)), Some(1));
    assert_eq!(row!(pl, fail(&mut pl, &mut c, 1)), Some(2));
    assert_eq!(fail(&mut pl, &mut c, 2), None, "all three failed");
}

#[test]
fn with_shuffle_skipping_goes_on_until_every_entry_failed() {
    // M3 counted failures, so a shuffle revisiting a bad entry could stop before the good one.
    for seed in 0..40 {
        let mut pl = playlist(5);
        let mut c = Cursor::new(seed);
        c.order.repeat = true;
        c.order.set_shuffle(true, &pl);
        let good = pl.entries()[2].id();
        let mut id = c.play(&mut pl);
        for step in 0..100 {
            let Some(now) = id else {
                panic!("seed {seed}: stopped at step {step} although 2.mp3 plays");
            };
            id = if now == good {
                c.on_started(&mut pl, false);
                c.on_ended(&mut pl)
            } else {
                let bad = pl.get(now).unwrap().source.clone();
                c.on_error(&mut pl, &bad)
            };
        }
    }
}

#[test]
fn streams_are_never_queued_gaplessly() {
    let mut pl = playlist(1);
    pl.append(vec![
        Source::Url("http://radio.example/".into()).into(),
        file("2.mp3"),
    ]);
    let mut c = Cursor::new(7);
    c.play(&mut pl);
    assert_eq!(c.on_started(&mut pl, false), None, "the stream is next");
    assert_eq!(row!(pl, c.on_ended(&mut pl)), Some(1), "loaded at the end");
    assert_eq!(row!(pl, c.on_started(&mut pl, false)), Some(2));
}

#[test]
fn a_track_ending_before_the_queue_arrives_plays_the_queued_one() {
    let (mut pl, mut c) = started(3);
    assert_eq!(row!(pl, c.on_started(&mut pl, false)), Some(1));
    assert_eq!(row!(pl, c.on_ended(&mut pl)), Some(1));
}

#[test]
fn entries_added_after_the_last_play_when_it_ends() {
    let (mut pl, mut c) = started(1);
    assert_eq!(c.on_started(&mut pl, false), None);
    pl.append(vec![file("new.mp3")]);
    assert_eq!(row!(pl, c.on_ended(&mut pl)), Some(1));
}

#[test]
fn stop_after_current_queues_nothing_and_clears_itself() {
    let (mut pl, mut c) = started(3);
    c.stop_after = true;
    assert_eq!(c.on_started(&mut pl, false), None);
    assert_eq!(c.on_ended(&mut pl), None);
    assert!(!c.stop_after);
    assert_eq!(row!(pl, c.play(&mut pl)), Some(0));
}

#[test]
fn requeue_follows_repeat_and_stop_after() {
    let (mut pl, mut c) = started(2);
    c.next(&mut pl);
    assert_eq!(c.on_started(&mut pl, false), None, "last entry, no repeat");
    c.order.repeat = true;
    let first = pl.entries()[0].id();
    assert_eq!(c.requeue(&pl), Requeue::Queue(first));
    assert_eq!(c.requeue(&pl), Requeue::Keep, "already queued");
    c.stop_after = true;
    assert_eq!(c.requeue(&pl), Requeue::Unqueue);
    assert_eq!(c.requeue(&pl), Requeue::Keep);
}

#[test]
fn requeue_waits_for_the_track_to_start() {
    let (pl, mut c) = started(2);
    c.order.repeat = true;
    assert_eq!(c.requeue(&pl), Requeue::Keep);
}

#[test]
fn requeue_follows_edits_to_the_list() {
    let (mut pl, mut c) = started(3);
    c.on_started(&mut pl, false);
    select(&mut pl, &[1]);
    pl.remove_selected();
    let two = pl.entries()[1].id();
    assert_eq!(c.requeue(&pl), Requeue::Queue(two));
    select(&mut pl, &[1]);
    pl.remove_selected();
    assert_eq!(c.requeue(&pl), Requeue::Unqueue);
}

#[test]
fn removing_the_playing_entry_keeps_the_next_one_queued() {
    let (mut pl, mut c) = started(4);
    c.jump(&mut pl, 1);
    let two = c.on_started(&mut pl, false);
    select(&mut pl, &[1]);
    pl.remove_selected();
    assert_eq!(c.requeue(&pl), Requeue::Keep);
    c.on_started(&mut pl, true);
    assert_eq!(pl.current(), two);
    assert_eq!(names(&pl)[pl.current_index().unwrap()], "2.mp3");
}

#[test]
fn stop_keeps_the_current_entry_for_play() {
    let (mut pl, mut c) = started(3);
    c.next(&mut pl);
    c.on_started(&mut pl, false);
    c.stop();
    assert_eq!(c.requeue(&pl), Requeue::Keep, "nothing plays");
    assert_eq!(row!(pl, c.play(&mut pl)), Some(1));
}

#[test]
fn play_entry_plays_that_entry() {
    let (mut pl, mut c) = started(3);
    let two = pl.entries()[2].id();
    assert_eq!(c.play_entry(&mut pl, two), Some(two));
    assert_eq!(pl.current(), Some(two));
}

#[test]
fn play_after_replacing_the_list_starts_at_its_first_entry() {
    let (mut pl, mut c) = started(3);
    c.jump(&mut pl, 2);
    pl.replace(vec![file("a"), file("b")]);
    assert_eq!(row!(pl, c.play(&mut pl)), Some(0));
}

#[test]
fn previous_and_next_walk_the_shuffle_history() {
    let (mut pl, mut c) = started(5);
    c.order.set_shuffle(true, &pl);
    let first = pl.current();
    let second = c.next(&mut pl);
    let third = c.next(&mut pl);
    assert_eq!(c.previous(&mut pl), second);
    assert_eq!(c.previous(&mut pl), first);
    assert_eq!(c.next(&mut pl), second);
    assert_eq!(c.next(&mut pl), third);
}
