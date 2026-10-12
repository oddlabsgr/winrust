mod common;

use common::{file, names, playlist};
use std::collections::HashSet;
use winrust_playlist::entry::EntryId;
use winrust_playlist::order::PlayOrder;
use winrust_playlist::playlist::Playlist;

fn name(pl: &Playlist, id: Option<EntryId>) -> Option<String> {
    let index = pl.index_of(id?)?;
    Some(names(pl)[index].clone())
}

fn at(pl: &mut Playlist, index: usize) {
    pl.set_current(Some(pl.entries()[index].id()));
}

/// Plays the next entry, as the playback cursor does.
fn advance(order: &mut PlayOrder, pl: &mut Playlist) -> Option<String> {
    let id = order.next(pl)?;
    pl.set_current(Some(id));
    order.played(pl, id);
    name(pl, Some(id))
}

/// Plays on until the order ends.
fn rest(order: &mut PlayOrder, pl: &mut Playlist) -> Vec<String> {
    std::iter::from_fn(|| advance(order, pl))
        .take(1000)
        .collect()
}

fn remove(pl: &mut Playlist, rows: &[usize]) {
    pl.select_none();
    for &r in rows {
        pl.ctrl_click(r);
    }
    pl.remove_selected();
}

#[test]
fn list_order_walks_the_rows_and_stops_at_the_ends() {
    let mut pl = playlist(3);
    let mut order = PlayOrder::new(1);
    assert_eq!(rest(&mut order, &mut pl), ["0.mp3", "1.mp3", "2.mp3"]);
    assert_eq!(order.next(&pl), None);
    at(&mut pl, 0);
    assert_eq!(order.previous(&pl), None);
}

#[test]
fn repeat_wraps_both_ways() {
    let mut pl = playlist(3);
    let mut order = PlayOrder::new(1);
    order.repeat = true;
    at(&mut pl, 2);
    assert_eq!(name(&pl, order.next(&pl)).unwrap(), "0.mp3");
    at(&mut pl, 0);
    assert_eq!(name(&pl, order.previous(&pl)).unwrap(), "2.mp3");
}

#[test]
fn after_the_current_entry_is_removed_play_goes_on_from_its_place() {
    let mut pl = playlist(4);
    let mut order = PlayOrder::new(1);
    at(&mut pl, 1);
    remove(&mut pl, &[1]);
    assert_eq!(name(&pl, order.next(&pl)).unwrap(), "2.mp3");
    assert_eq!(name(&pl, order.previous(&pl)).unwrap(), "0.mp3");
    assert_eq!(name(&pl, order.first(&pl)).unwrap(), "2.mp3");
}

#[test]
fn play_starts_at_the_first_entry_or_the_current_one() {
    let mut pl = playlist(3);
    let mut order = PlayOrder::new(1);
    assert_eq!(name(&pl, order.first(&pl)).unwrap(), "0.mp3");
    at(&mut pl, 2);
    assert_eq!(name(&pl, order.first(&pl)).unwrap(), "2.mp3");
    remove(&mut pl, &[2]);
    assert_eq!(
        name(&pl, order.first(&pl)).unwrap(),
        "0.mp3",
        "nothing followed the removed entry"
    );
}

#[test]
fn jumps_clamp_to_the_ends_and_stop_there() {
    let mut pl = playlist(25);
    let order = PlayOrder::new(1);
    let jump = |pl: &mut Playlist, n| {
        let id = order.jump(pl, n);
        pl.set_current(id.or(pl.current()));
        name(pl, id)
    };
    at(&mut pl, 3);
    assert_eq!(jump(&mut pl, -10).unwrap(), "0.mp3");
    assert_eq!(jump(&mut pl, -10), None, "already at the start");
    assert_eq!(jump(&mut pl, 10).unwrap(), "10.mp3");
    assert_eq!(jump(&mut pl, 10).unwrap(), "20.mp3");
    assert_eq!(jump(&mut pl, 10).unwrap(), "24.mp3");
    assert_eq!(jump(&mut pl, 10), None, "already at the end");
}

#[test]
fn jumps_wrap_with_repeat_and_follow_the_list_with_shuffle() {
    let mut pl = playlist(25);
    let mut order = PlayOrder::new(1);
    order.repeat = true;
    order.set_shuffle(true, &pl);
    at(&mut pl, 20);
    assert_eq!(name(&pl, order.jump(&pl, 10)).unwrap(), "5.mp3");
    assert_eq!(name(&pl, order.jump(&pl, -30)).unwrap(), "15.mp3");
}

#[test]
fn shuffle_plays_every_entry_once_then_stops() {
    let mut pl = playlist(6);
    let mut order = PlayOrder::new(7);
    order.set_shuffle(true, &pl);
    let played = rest(&mut order, &mut pl);
    assert_eq!(played.len(), 6);
    assert_eq!(played.iter().collect::<HashSet<_>>().len(), 6);
    assert_ne!(played, names(&pl), "not list order");
    assert_eq!(order.next(&pl), None);
}

#[test]
fn with_repeat_new_permutations_follow_and_never_repeat_a_track_at_once() {
    let mut pl = playlist(4);
    let mut order = PlayOrder::new(3);
    order.repeat = true;
    order.set_shuffle(true, &pl);
    let played: Vec<String> = (0..400)
        .map(|_| advance(&mut order, &mut pl).unwrap())
        .collect();
    for round in played.chunks(4) {
        assert_eq!(round.iter().collect::<HashSet<_>>().len(), 4, "{round:?}");
    }
    for pair in played.windows(2) {
        assert_ne!(pair[0], pair[1]);
    }
}

#[test]
fn turning_shuffle_on_keeps_the_current_entry_first() {
    let mut pl = playlist(5);
    let mut order = PlayOrder::new(9);
    at(&mut pl, 2);
    order.set_shuffle(true, &pl);
    let played = rest(&mut order, &mut pl);
    assert_eq!(played.len(), 4);
    assert!(!played.contains(&"2.mp3".to_string()));
}

#[test]
fn previous_walks_back_through_what_played() {
    let mut pl = playlist(5);
    let mut order = PlayOrder::new(5);
    order.set_shuffle(true, &pl);
    let first: Vec<String> = (0..3)
        .map(|_| advance(&mut order, &mut pl).unwrap())
        .collect();
    let back = order.previous(&pl).unwrap();
    assert_eq!(name(&pl, Some(back)).unwrap(), first[1]);
    pl.set_current(Some(back));
    order.back_to(&pl, back);
    assert_eq!(
        advance(&mut order, &mut pl).unwrap(),
        first[2],
        "forward again"
    );
    let mut all = first;
    all.extend(rest(&mut order, &mut pl));
    assert_eq!(all.iter().collect::<HashSet<_>>().len(), 5);
}

#[test]
fn entries_added_during_shuffle_join_the_part_not_yet_played() {
    let mut pl = playlist(3);
    let mut order = PlayOrder::new(11);
    order.set_shuffle(true, &pl);
    let mut played: Vec<String> = (0..2)
        .map(|_| advance(&mut order, &mut pl).unwrap())
        .collect();
    pl.append(vec![file("a"), file("b"), file("c")]);
    played.extend(rest(&mut order, &mut pl));
    assert_eq!(played.len(), 6);
    assert_eq!(played.iter().collect::<HashSet<_>>().len(), 6);
}

#[test]
fn removed_entries_drop_out_of_the_permutation() {
    let mut pl = playlist(5);
    let mut order = PlayOrder::new(13);
    order.set_shuffle(true, &pl);
    let first = advance(&mut order, &mut pl).unwrap();
    let others: Vec<usize> = (0..5).filter(|i| names(&pl)[*i] != first).collect();
    remove(&mut pl, &others[..3]);
    let left: Vec<String> = names(&pl).into_iter().filter(|n| *n != first).collect();
    assert_eq!(left.len(), 1);
    assert_eq!(rest(&mut order, &mut pl), left);
}

#[test]
fn an_empty_list_has_no_order_and_one_entry_repeats_alone() {
    let mut pl = Playlist::new();
    let mut order = PlayOrder::new(1);
    order.repeat = true;
    order.set_shuffle(true, &pl);
    assert_eq!(order.next(&pl), None);
    pl.append(vec![file("only")]);
    assert_eq!(advance(&mut order, &mut pl).unwrap(), "only");
    assert_eq!(advance(&mut order, &mut pl).unwrap(), "only");
}

#[test]
fn the_same_seed_gives_the_same_order() {
    let order = |seed| {
        let mut pl = playlist(8);
        let mut order = PlayOrder::new(seed);
        order.set_shuffle(true, &pl);
        rest(&mut order, &mut pl)
    };
    assert_eq!(order(42), order(42));
    assert_ne!(order(42), order(43));
}

#[test]
fn choosing_an_entry_that_already_played_does_not_replay_the_ones_after_it() {
    let mut pl = playlist(10);
    let mut order = PlayOrder::new(21);
    order.set_shuffle(true, &pl);
    let heard: Vec<String> = (0..6)
        .map(|_| advance(&mut order, &mut pl).unwrap())
        .collect();
    // A double-click on the second track heard.
    let row = names(&pl).iter().position(|n| *n == heard[1]).unwrap();
    let again = pl.entries()[row].id();
    pl.set_current(Some(again));
    order.played(&pl, again);
    let after = rest(&mut order, &mut pl);
    assert_eq!(after.len(), 4, "{after:?}");
    assert!(after.iter().all(|n| !heard.contains(n)), "{after:?}");
}
