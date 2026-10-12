mod common;

use common::{file, fixture, full_tags, write_tags};
use std::time::Duration;
use winrust_playlist::entry::{Item, Tags};
use winrust_playlist::playlist::Playlist;
use winrust_playlist::tags::{FileTags, TagError, read};

fn about_a_second(d: Option<Duration>) -> bool {
    d.is_some_and(|d| d > Duration::from_millis(900) && d < Duration::from_millis(1200))
}

#[test]
fn tags_and_duration_are_read_from_every_format() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["tone.mp3", "tone.flac", "tone.ogg", "tone.m4a"] {
        let path = fixture(dir.path(), name);
        write_tags(&path, &full_tags());
        let got = read(&path).unwrap();
        assert_eq!(got.tags, full_tags(), "{name}");
        assert!(about_a_second(got.duration), "{name}: {:?}", got.duration);
    }
}

#[test]
fn a_file_without_tags_still_gets_its_duration() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "tone.flac");
    write_tags(&path, &Tags::default());
    let got = read(&path).unwrap();
    assert_eq!(got.tags, Tags::default());
    assert!(about_a_second(got.duration));
}

#[test]
fn blank_fields_count_as_missing() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "tone.ogg");
    let blank = Tags {
        artist: Some("  ".into()),
        title: Some(" Title ".into()),
        ..Tags::default()
    };
    write_tags(&path, &blank);
    let tags = read(&path).unwrap().tags;
    assert_eq!(tags.artist, None);
    assert_eq!(tags.title.as_deref(), Some("Title"));
}

#[test]
fn files_that_cannot_be_read_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let junk = dir.path().join("junk.mp3");
    std::fs::write(&junk, b"not audio at all").unwrap();
    assert!(read(&junk).is_err());
    assert!(read(&dir.path().join("missing.flac")).is_err());
}

#[test]
fn an_entry_takes_the_tags_and_duration_it_is_given() {
    let mut pl = Playlist::new();
    pl.append(vec![Item {
        hint: Some("From the list".into()),
        duration: Some(Duration::from_secs(9)),
        ..file("a.mp3")
    }]);
    let id = pl.entries()[0].id();
    let entry = pl.get_mut(id).unwrap();
    entry.set_tags(Ok(FileTags {
        tags: full_tags(),
        duration: None,
    }));
    assert_eq!(entry.tags, Some(full_tags()));
    assert_eq!(entry.duration, Some(Duration::from_secs(9)), "kept");
    entry.set_tags(Ok(FileTags {
        tags: Tags::default(),
        duration: Some(Duration::from_secs(3)),
    }));
    assert_eq!(entry.duration, Some(Duration::from_secs(3)));
}

#[test]
fn an_unreadable_file_keeps_the_playlist_file_title() {
    let mut pl = Playlist::new();
    pl.append(vec![Item {
        hint: Some("From the list".into()),
        ..file("a.mp3")
    }]);
    let id = pl.entries()[0].id();
    let entry = pl.get_mut(id).unwrap();
    entry.set_tags(Err(TagError("unreadable".into())));
    assert!(entry.tags.is_some(), "counts as read");
    assert_eq!(entry.title(&Default::default()), "From the list");
    assert!(!entry.dead);
}

#[test]
fn the_format_comes_from_the_content_so_oga_files_are_read() {
    let dir = tempfile::tempdir().unwrap();
    let ogg = fixture(dir.path(), "tone.ogg");
    write_tags(&ogg, &full_tags());
    let oga = dir.path().join("tone.oga");
    std::fs::rename(&ogg, &oga).unwrap();
    let got = read(&oga).unwrap();
    assert_eq!(got.tags, full_tags());
    assert!(about_a_second(got.duration));
}
