#![allow(dead_code)]

use lofty::config::WriteOptions;
use lofty::file::TaggedFileExt;
use lofty::tag::items::Timestamp;
use lofty::tag::{Accessor, Tag, TagExt};
use std::path::{Path, PathBuf};
use winrust_audio::Source;
use winrust_playlist::entry::{Item, Tags};
use winrust_playlist::playlist::Playlist;

pub fn file(name: &str) -> Item {
    Source::File(PathBuf::from(name)).into()
}

/// Entries `0.mp3`, `1.mp3`, … `n-1.mp3`.
pub fn playlist(n: usize) -> Playlist {
    let mut pl = Playlist::new();
    pl.append((0..n).map(|i| file(&format!("{i}.mp3"))).collect());
    pl
}

/// Each entry's file name or URL, in order.
pub fn names(pl: &Playlist) -> Vec<String> {
    pl.entries()
        .iter()
        .map(|e| match &e.source {
            Source::File(p) => p.to_string_lossy().into_owned(),
            Source::Url(u) => u.clone(),
        })
        .collect()
}

pub fn selected(pl: &Playlist) -> Vec<usize> {
    let rows = pl.entries().iter().enumerate();
    rows.filter(|(_, e)| e.selected).map(|(i, _)| i).collect()
}

/// A copy of an audio fixture (`crates/audio/tests/fixtures`) in `dir`.
pub fn fixture(dir: &Path, name: &str) -> PathBuf {
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../audio/tests/fixtures");
    let to = dir.join(name);
    std::fs::copy(from.join(name), &to).unwrap();
    to
}

/// Replaces the file's tags with these, in its own tag format.
pub fn write_tags(path: &Path, tags: &Tags) {
    let kind = lofty::read_from_path(path).unwrap().primary_tag_type();
    let mut tag = Tag::new(kind);
    if let Some(v) = &tags.artist {
        tag.set_artist(v.clone());
    }
    if let Some(v) = &tags.title {
        tag.set_title(v.clone());
    }
    if let Some(v) = &tags.album {
        tag.set_album(v.clone());
    }
    if let Some(year) = tags.year {
        tag.set_date(Timestamp {
            year: year as u16,
            ..Timestamp::default()
        });
    }
    if let Some(track) = tags.track {
        tag.set_track(track);
    }
    tag.save_to_path(path, WriteOptions::default()).unwrap();
}

pub fn full_tags() -> Tags {
    Tags {
        artist: Some("Ärtist".into()),
        title: Some("Tïtle".into()),
        album: Some("Album".into()),
        year: Some(1999),
        track: Some(7),
    }
}
