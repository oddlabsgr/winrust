#![allow(dead_code)]

use std::path::PathBuf;
use winrust_audio::Source;
use winrust_playlist::entry::Item;
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
