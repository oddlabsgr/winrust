//! What command-line arguments and dropped paths add (spec §4.11): a URL as a stream, a
//! playlist file as its entries, a folder as the audio files inside it, and any other file as
//! itself. Moved from M3's `app::sources`.

use crate::entry::Item;
use crate::files;
use crate::playlist::path_key;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use winrust_audio::Source;

/// File extensions a folder is searched for (what the audio engine decodes).
pub const AUDIO_EXTENSIONS: [&str; 8] = ["aac", "flac", "m4a", "mp3", "mp4", "oga", "ogg", "wav"];

/// Playlist files, which stand for their entries when given or dropped.
pub const PLAYLIST_EXTENSIONS: [&str; 3] = ["m3u", "m3u8", "pls"];

/// `winrust [file|folder|playlist|URL]...`
pub fn from_args(args: impl IntoIterator<Item = OsString>) -> Vec<Item> {
    let mut out = Vec::new();
    for arg in args {
        match arg.to_str().filter(|s| is_url(s)) {
            Some(url) => out.push(Source::Url(url.to_string()).into()),
            // Absolute, so the autosave finds the file again from any folder.
            None => expand(std::path::absolute(&arg).unwrap_or(arg.into()), &mut out),
        }
    }
    out
}

/// Files and folders dropped on a window.
pub fn from_paths(paths: Vec<PathBuf>) -> Vec<Item> {
    let mut out = Vec::new();
    for path in paths {
        expand(path, &mut out);
    }
    out
}

pub fn is_url(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    ["http://", "https://", "icy://"]
        .iter()
        .any(|p| lower.starts_with(p))
}

/// A folder as the audio files in it and its subfolders, sorted by path (playlist files
/// inside are ignored); a playlist file as its entries, none when it cannot be read; any
/// other file as it is (a bad one is reported when it plays).
fn expand(path: PathBuf, out: &mut Vec<Item>) {
    if path.is_dir() {
        out.extend(
            audio_files(path)
                .into_iter()
                .map(|p| Source::File(p).into()),
        );
    } else if has_extension(&path, &PLAYLIST_EXTENSIONS) {
        out.extend(files::read(&path).unwrap_or_default());
    } else {
        out.push(Source::File(path).into());
    }
}

/// Unreadable folders add nothing. Linked files count; linked folders are not followed, so a
/// link loop cannot trap the scan.
fn audio_files(folder: PathBuf) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut folders = vec![folder];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(t) if t.is_dir() => folders.push(path),
                Ok(t)
                    if (t.is_file() || (t.is_symlink() && path.is_file()))
                        && has_extension(&path, &AUDIO_EXTENSIONS) =>
                {
                    found.push(path)
                }
                _ => {}
            }
        }
    }
    found.sort_by_cached_key(|p| path_key(p));
    found
}

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| extensions.contains(&e.to_ascii_lowercase().as_str()))
}
