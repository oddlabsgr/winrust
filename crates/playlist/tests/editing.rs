mod common;

use common::{file, names, playlist, selected};
use std::path::Path;
use winrust_audio::Source;
use winrust_playlist::entry::{Item, Tags};
use winrust_playlist::playlist::{Playlist, Position};
use winrust_playlist::rng::Rng;
use winrust_playlist::title::Template;

fn named(names: &[&str]) -> Playlist {
    let mut pl = Playlist::new();
    pl.append(names.iter().map(|n| file(n)).collect());
    pl
}

fn set_current(pl: &mut Playlist, index: usize) {
    pl.set_current(Some(pl.entries()[index].id()));
}

fn select(pl: &mut Playlist, rows: &[usize]) {
    pl.select_none();
    for &r in rows {
        pl.ctrl_click(r);
    }
}

#[test]
fn removing_the_selection_keeps_the_current_entry() {
    let mut pl = playlist(5);
    set_current(&mut pl, 4);
    select(&mut pl, &[1, 2]);
    pl.remove_selected();
    assert_eq!(names(&pl), ["0.mp3", "3.mp3", "4.mp3"]);
    assert_eq!(pl.current_index(), Some(2));
    assert_eq!(pl.position(), Position::At(2));
}

#[test]
fn crop_keeps_only_the_selection() {
    let mut pl = playlist(5);
    select(&mut pl, &[1, 3]);
    pl.crop();
    assert_eq!(names(&pl), ["1.mp3", "3.mp3"]);
    assert_eq!(selected(&pl), [0, 1]);
}

#[test]
fn clear_empties_the_list() {
    let mut pl = playlist(3);
    set_current(&mut pl, 1);
    pl.clear();
    assert!(pl.is_empty());
    assert_eq!(pl.position(), Position::Start);
}

#[test]
fn a_removed_current_entry_hands_over_to_the_one_after_it() {
    let mut pl = playlist(5);
    set_current(&mut pl, 1);
    select(&mut pl, &[1, 2]);
    pl.remove_selected();
    assert_eq!(pl.current(), None);
    assert_eq!(pl.position(), Position::Before(1), "3.mp3 plays next");
    select(&mut pl, &[1]);
    pl.remove_selected();
    assert_eq!(pl.position(), Position::Before(1), "then 4.mp3");
    select(&mut pl, &[1]);
    pl.remove_selected();
    assert_eq!(pl.position(), Position::Before(1), "past the end");
}

#[test]
fn entries_added_after_a_removed_last_entry_follow_it() {
    let mut pl = playlist(2);
    set_current(&mut pl, 1);
    select(&mut pl, &[1]);
    pl.remove_selected();
    pl.append(vec![file("a")]);
    assert_eq!(pl.position(), Position::Before(1));
    assert_eq!(names(&pl)[1], "a");
}

#[test]
fn edits_keep_the_current_entry_and_the_selection() {
    let mut pl = named(&["c.mp3", "a.mp3", "b.mp3"]);
    set_current(&mut pl, 0);
    pl.click(2);
    pl.sort_by_path();
    assert_eq!(names(&pl), ["a.mp3", "b.mp3", "c.mp3"]);
    assert_eq!(pl.current_index(), Some(2));
    assert_eq!(selected(&pl), [1]);
    assert_eq!(pl.cursor_index(), Some(1));
}

#[test]
fn remove_dead_drops_dead_entries_and_missing_files() {
    let mut pl = named(&["a.mp3", "gone.mp3", "c.mp3"]);
    pl.append(vec![Source::Url("http://radio.example/".into()).into()]);
    let first = pl.entries()[0].id();
    pl.get_mut(first).unwrap().dead = true;
    pl.remove_dead(|p| p != Path::new("gone.mp3"));
    assert_eq!(names(&pl), ["c.mp3", "http://radio.example/"]);
}

#[test]
fn the_selection_moves_as_a_block_and_stops_at_the_ends() {
    let mut pl = playlist(6);
    select(&mut pl, &[1, 3]);
    assert_eq!(pl.move_selected(1), 1);
    assert_eq!(
        names(&pl),
        ["0.mp3", "2.mp3", "1.mp3", "4.mp3", "3.mp3", "5.mp3"]
    );
    assert_eq!(selected(&pl), [2, 4]);
    assert_eq!(pl.move_selected(10), 1, "the block stops at the end");
    assert_eq!(selected(&pl), [3, 5]);
    assert_eq!(pl.move_selected(-10), -3);
    assert_eq!(selected(&pl), [0, 2]);
    assert_eq!(
        names(&pl),
        ["1.mp3", "0.mp3", "3.mp3", "2.mp3", "4.mp3", "5.mp3"]
    );
}

#[test]
fn moving_nothing_selected_does_nothing() {
    let mut pl = playlist(3);
    assert_eq!(pl.move_selected(1), 0);
    assert_eq!(names(&pl), ["0.mp3", "1.mp3", "2.mp3"]);
}

#[test]
fn sorting_by_path_compares_folders_one_at_a_time_ignoring_case() {
    let mut pl = named(&[
        "Live 2001/b.mp3",
        "live/c.mp3",
        "Live/a.mp3",
        "B.mp3",
        "a.mp3",
    ]);
    pl.sort_by_path();
    assert_eq!(
        names(&pl),
        [
            "a.mp3",
            "B.mp3",
            "Live/a.mp3",
            "live/c.mp3",
            "Live 2001/b.mp3"
        ]
    );
}

#[test]
fn sorting_by_file_name_ignores_folders() {
    let mut pl = named(&["z/b.mp3", "a/C.mp3", "y/a.mp3"]);
    pl.sort_by_file_name();
    assert_eq!(names(&pl), ["y/a.mp3", "z/b.mp3", "a/C.mp3"]);
}

#[test]
fn sorting_by_title_uses_the_shown_title_and_is_stable() {
    let mut pl = Playlist::new();
    let titles = [Some("beta"), Some("Alpha"), None, Some("alpha")];
    for (i, title) in titles.iter().enumerate() {
        pl.append(vec![Item {
            hint: Some(format!("unused {i}")),
            ..file(&format!("{i}.mp3"))
        }]);
        let id = pl.entries()[i].id();
        pl.get_mut(id).unwrap().tags = Some(Tags {
            title: title.map(String::from),
            ..Tags::default()
        });
    }
    pl.sort_by_title(&Template::default());
    // Entry 2 has no title, so it shows and sorts as its file name, "2".
    assert_eq!(names(&pl), ["2.mp3", "1.mp3", "3.mp3", "0.mp3"]);
}

#[test]
fn reverse_turns_the_list_around() {
    let mut pl = playlist(3);
    pl.reverse();
    assert_eq!(names(&pl), ["2.mp3", "1.mp3", "0.mp3"]);
}

#[test]
fn randomize_shuffles_every_entry_repeatably() {
    let shuffled = |seed| {
        let mut pl = playlist(10);
        set_current(&mut pl, 0);
        pl.randomize(&mut Rng::new(seed));
        let at = pl.current_index().unwrap();
        assert_eq!(
            names(&pl)[at],
            "0.mp3",
            "the current entry is still current"
        );
        names(&pl)
    };
    let a = shuffled(1);
    assert_eq!(a, shuffled(1));
    assert_ne!(a, names(&playlist(10)));
    let mut sorted = a.clone();
    sorted.sort();
    assert_eq!(sorted, names(&playlist(10)));
}
