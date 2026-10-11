use std::path::PathBuf;
use winrust_app::queue::{Queue, Requeue};
use winrust_audio::Source;

fn entries(n: usize) -> Vec<Source> {
    (0..n)
        .map(|i| Source::File(PathBuf::from(format!("{i}.mp3"))))
        .collect()
}

fn queue(n: usize) -> Queue {
    let mut q = Queue::new(7);
    assert_eq!(q.replace(entries(n)), if n > 0 { Some(0) } else { None });
    q
}

fn is(i: usize) -> impl Fn(&Source) -> bool {
    move |s| *s == Source::File(PathBuf::from(format!("{i}.mp3")))
}

#[test]
fn plays_through_gaplessly_then_stops() {
    let mut q = queue(3);
    assert_eq!(q.on_started(false), Some(1));
    assert_eq!(q.on_started(true), Some(2));
    assert_eq!(q.current(), Some(1));
    assert_eq!(q.on_started(true), None);
    assert_eq!(q.on_ended(), None);
    assert_eq!(q.current(), Some(2));
}

#[test]
fn empty_queue_plays_nothing() {
    let mut q = queue(0);
    assert_eq!(q.restart(), None);
    assert_eq!(q.step(1), None);
}

#[test]
fn repeat_wraps_both_ways() {
    let mut q = queue(3);
    q.repeat = true;
    assert_eq!(q.step(-1), Some(2));
    assert_eq!(q.on_started(false), Some(0));
    assert_eq!(q.step(1), Some(0));
}

#[test]
fn without_repeat_the_ends_stop() {
    // Webamp `getNextTrackId`: no previous before the first entry, no next after the last.
    let mut q = queue(3);
    assert_eq!(q.step(-1), None);
    let mut q = queue(3);
    q.step(2);
    assert_eq!(q.step(1), None);
}

#[test]
fn jumps_clamp_to_the_ends() {
    let mut q = queue(25);
    q.step(3);
    assert_eq!(q.step(-10), Some(0));
    assert_eq!(q.step(10), Some(10));
    assert_eq!(q.step(10), Some(20));
    assert_eq!(q.step(10), Some(24));
}

#[test]
fn shuffle_picks_other_entries() {
    let mut q = queue(5);
    q.shuffle = true;
    let mut seen = [false; 5];
    for _ in 0..200 {
        let current = q.current().unwrap();
        let next = q.on_started(false).unwrap();
        assert_ne!(next, current);
        seen[next] = true;
        q.step(1);
    }
    assert!(seen.iter().all(|s| *s), "{seen:?}");
}

#[test]
fn shuffle_with_one_entry_replays_it() {
    let mut q = queue(1);
    q.shuffle = true;
    assert_eq!(q.on_started(false), Some(0));
}

#[test]
fn unreadable_current_entry_is_skipped() {
    let mut q = queue(3);
    assert_eq!(q.on_error(is(0)), Some(1));
    assert_eq!(q.on_started(false), Some(2));
}

#[test]
fn unreadable_queued_entry_is_skipped_at_the_end() {
    let mut q = queue(3);
    assert_eq!(q.on_started(false), Some(1));
    assert_eq!(q.on_error(is(1)), None, "the current track plays on");
    assert_eq!(q.on_ended(), Some(2));
}

#[test]
fn errors_about_other_sources_change_nothing() {
    let mut q = queue(2);
    assert_eq!(q.on_error(is(9)), None);
    assert_eq!(q.on_started(false), Some(1));
}

#[test]
fn every_entry_unreadable_stops_even_with_repeat() {
    let mut q = queue(3);
    q.repeat = true;
    assert_eq!(q.on_error(is(0)), Some(1));
    assert_eq!(q.on_error(is(1)), Some(2));
    assert_eq!(q.on_error(is(2)), None, "all three failed in a row");
}

#[test]
fn track_ending_before_the_queue_arrives_plays_the_queued_one() {
    let mut q = queue(3);
    assert_eq!(q.on_started(false), Some(1));
    assert_eq!(q.on_ended(), Some(1));
}

#[test]
fn stop_after_current_queues_nothing_and_clears_itself() {
    let mut q = queue(3);
    q.stop_after = true;
    assert_eq!(q.on_started(false), None);
    assert_eq!(q.on_ended(), None);
    assert!(!q.stop_after);
    assert_eq!(q.restart(), Some(0));
}

#[test]
fn requeue_follows_repeat_and_stop_after() {
    let mut q = queue(2);
    q.step(1);
    assert_eq!(q.on_started(false), None, "last entry, no repeat");
    q.repeat = true;
    assert_eq!(q.requeue(), Requeue::Queue(0));
    q.stop_after = true;
    assert_eq!(q.requeue(), Requeue::Unqueue);
    assert_eq!(q.requeue(), Requeue::Keep);
}

#[test]
fn requeue_waits_for_the_track_to_start() {
    let mut q = queue(2);
    q.repeat = true;
    assert_eq!(q.requeue(), Requeue::Keep);
}

#[test]
fn stop_keeps_the_current_entry_for_play() {
    let mut q = queue(3);
    q.step(1);
    q.on_started(false);
    q.stop();
    assert_eq!(q.requeue(), Requeue::Keep, "nothing plays");
    assert_eq!(q.restart(), Some(1));
}

#[test]
fn replace_starts_over() {
    let mut q = queue(3);
    q.step(2);
    assert_eq!(q.replace(entries(2)), Some(0));
    assert_eq!(q.len(), 2);
    assert_eq!(q.entry(1), Some(&Source::File(PathBuf::from("1.mp3"))));
}
