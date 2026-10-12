mod common;

use common::{file, names, playlist, selected};
use std::collections::HashSet;
use winrust_playlist::playlist::Playlist;

#[test]
fn ids_are_never_reused() {
    let mut pl = playlist(2);
    let mut ids: HashSet<_> = pl.entries().iter().map(|e| e.id()).collect();
    pl.replace(vec![file("a"), file("b")]);
    ids.extend(pl.entries().iter().map(|e| e.id()));
    assert_eq!(ids.len(), 4);
}

#[test]
fn insert_places_entries_before_a_row_and_appends_past_the_end() {
    let mut pl = playlist(3);
    pl.insert(1, vec![file("a"), file("b")]);
    pl.insert(99, vec![file("c")]);
    assert_eq!(names(&pl), ["0.mp3", "a", "b", "1.mp3", "2.mp3", "c"]);
}

#[test]
fn a_click_selects_one_row() {
    let mut pl = playlist(4);
    pl.click(1);
    pl.click(2);
    assert_eq!(selected(&pl), [2]);
}

#[test]
fn ctrl_click_toggles_rows() {
    let mut pl = playlist(4);
    pl.click(0);
    pl.ctrl_click(2);
    assert_eq!(selected(&pl), [0, 2]);
    pl.ctrl_click(0);
    assert_eq!(selected(&pl), [2]);
}

#[test]
fn shift_click_selects_from_the_anchor() {
    let mut pl = playlist(6);
    pl.click(3);
    pl.shift_click(1);
    assert_eq!(selected(&pl), [1, 2, 3]);
    pl.shift_click(4);
    assert_eq!(selected(&pl), [3, 4], "the anchor stays on the clicked row");
}

#[test]
fn shift_click_without_an_anchor_does_nothing() {
    let mut pl = playlist(3);
    pl.shift_click(2);
    assert_eq!(selected(&pl), [] as [usize; 0]);
}

#[test]
fn select_all_none_and_invert() {
    let mut pl = playlist(4);
    pl.click(1);
    pl.invert_selection();
    assert_eq!(selected(&pl), [0, 2, 3]);
    pl.select_all();
    assert_eq!(selected(&pl), [0, 1, 2, 3]);
    pl.select_none();
    assert_eq!(selected(&pl), [] as [usize; 0]);
}

#[test]
fn the_keyboard_cursor_moves_and_extends_from_the_anchor() {
    let mut pl = playlist(6);
    pl.move_cursor(2, false);
    pl.move_cursor(3, true);
    pl.move_cursor(4, true);
    assert_eq!(selected(&pl), [2, 3, 4]);
    pl.move_cursor(1, true);
    assert_eq!(selected(&pl), [1, 2]);
    assert_eq!(pl.cursor_index(), Some(1));
    pl.move_cursor(usize::MAX, false);
    assert_eq!(selected(&pl), [5]);
    assert_eq!(pl.cursor_index(), Some(5));
}

#[test]
fn extending_without_an_anchor_starts_one() {
    let mut pl = playlist(3);
    pl.move_cursor(1, true);
    assert_eq!(selected(&pl), [1]);
    pl.move_cursor(2, true);
    assert_eq!(selected(&pl), [1, 2]);
}

#[test]
fn the_cursor_does_nothing_on_an_empty_list() {
    let mut pl = Playlist::new();
    pl.move_cursor(0, false);
    assert_eq!(pl.cursor(), None);
}

#[test]
fn rows_past_the_end_are_ignored() {
    let mut pl = playlist(2);
    pl.click(0);
    pl.click(5);
    pl.ctrl_click(5);
    pl.shift_click(5);
    assert_eq!(selected(&pl), [0]);
}

#[test]
fn current_and_selection_follow_their_entries_when_rows_are_inserted() {
    let mut pl = playlist(3);
    pl.click(1);
    pl.set_current(Some(pl.entries()[2].id()));
    pl.insert(0, vec![file("a")]);
    assert_eq!(selected(&pl), [2]);
    assert_eq!(pl.current_index(), Some(3));
    assert_eq!(pl.cursor_index(), Some(2));
}

#[test]
fn replace_leaves_nothing_current_or_selected() {
    let mut pl = playlist(3);
    pl.click(1);
    let old = pl.entries()[0].id();
    pl.set_current(Some(old));
    pl.replace(vec![file("a")]);
    assert_eq!(pl.current(), None);
    assert_eq!(pl.cursor(), None);
    assert_eq!(selected(&pl), [] as [usize; 0]);
    pl.set_current(Some(old));
    assert_eq!(
        pl.current(),
        None,
        "an id from the old list is not in this one"
    );
}
