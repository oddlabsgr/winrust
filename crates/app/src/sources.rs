//! Turns command-line arguments and dropped paths into sources. A folder stands for the
//! audio files inside it, in name order.

use std::ffi::OsString;
use std::path::PathBuf;
use winrust_audio::Source;

/// File extensions a folder is searched for (what the audio engine decodes).
pub const AUDIO_EXTENSIONS: [&str; 8] = ["aac", "flac", "m4a", "mp3", "mp4", "oga", "ogg", "wav"];

/// `winrust [file|folder|URL]...`
pub fn from_args(args: impl IntoIterator<Item = OsString>) -> Vec<Source> {
    let mut out = Vec::new();
    for arg in args {
        match arg.to_str().filter(|s| is_url(s)) {
            Some(url) => out.push(Source::Url(url.to_string())),
            None => expand(PathBuf::from(arg), &mut out),
        }
    }
    out
}

/// Files and folders dropped on the window.
pub fn from_paths(paths: Vec<PathBuf>) -> Vec<Source> {
    let mut out = Vec::new();
    for path in paths {
        expand(path, &mut out);
    }
    out
}

fn is_url(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    ["http://", "https://", "icy://"]
        .iter()
        .any(|p| lower.starts_with(p))
}

/// A file as it is (a bad one is reported when it plays); a folder as the audio files in it
/// and its subfolders, sorted by path ignoring case. Unreadable folders add nothing.
fn expand(path: PathBuf, out: &mut Vec<Source>) {
    if !path.is_dir() {
        out.push(Source::File(path));
        return;
    }
    let mut files = Vec::new();
    let mut folders = vec![path];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(t) if t.is_dir() => folders.push(path),
                Ok(t) if t.is_file() && is_audio(&path) => files.push(path),
                _ => {}
            }
        }
    }
    files.sort_by_cached_key(|p| p.to_string_lossy().to_lowercase());
    out.extend(files.into_iter().map(Source::File));
}

fn is_audio(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}
