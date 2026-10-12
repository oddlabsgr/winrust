//! Playlist files (spec §4.7): M3U, M3U8 and PLS. Reading never fails on content: lines it
//! cannot use are skipped.

use crate::entry::{Entry, Item};
use crate::title::Template;
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;
use winrust_audio::Source;

/// Reads a playlist file: `.pls` by its extension, anything else as M3U. Relative paths
/// resolve against the file's folder. Only I/O fails.
pub fn read(path: &Path) -> io::Result<Vec<Item>> {
    let bytes = std::fs::read(path)?;
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let text = decode(&bytes, ext == "m3u");
    let base = path.parent().unwrap_or(Path::new(""));
    Ok(if ext == "pls" {
        parse_pls(&text, base)
    } else {
        parse_m3u(&text, base)
    })
}

/// A playlist file's text: UTF-8, after any byte-order mark. A `legacy` file (`.m3u`) that
/// is not valid UTF-8 is Windows-1252; other invalid bytes become U+FFFD.
pub fn decode(bytes: &[u8], legacy: bool) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) if legacy => bytes.iter().map(|&b| windows_1252(b)).collect(),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// M3U and M3U8: one location per line; `#EXTINF:<seconds>,<title>` describes the next one.
/// Other `#` lines are ignored.
pub fn parse_m3u(text: &str, base: &Path) -> Vec<Item> {
    let mut items = Vec::new();
    let mut info = None;
    for line in text.lines().map(str::trim) {
        if let Some(rest) = strip_prefix_ignore_case(line, "#EXTINF:") {
            info = Some(extinf(rest));
        } else if !line.is_empty() && !line.starts_with('#') {
            let (duration, hint) = info.take().unwrap_or_default();
            if let Some(source) = location(line, base) {
                items.push(Item {
                    source,
                    hint,
                    duration,
                });
            }
        }
    }
    items
}

/// PLS: `FileN`, `TitleN` and `LengthN`, keys in any case, taken in N order. Numbers
/// without a `FileN` are skipped.
pub fn parse_pls(text: &str, base: &Path) -> Vec<Item> {
    type Fields = (Option<Source>, Option<String>, Option<Duration>);
    let mut entries: BTreeMap<u32, Fields> = BTreeMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        let Some(digits) = key.find(|c: char| c.is_ascii_digit()) else {
            continue;
        };
        let Ok(n) = key[digits..].parse::<u32>() else {
            continue;
        };
        let entry = entries.entry(n).or_default();
        match &key[..digits] {
            "file" => entry.0 = location(value, base),
            "title" => entry.1 = Some(value.to_string()).filter(|v| !v.is_empty()),
            "length" => entry.2 = seconds(value),
            _ => {}
        }
    }
    entries
        .into_values()
        .filter_map(|(source, hint, duration)| {
            Some(Item {
                source: source?,
                hint,
                duration,
            })
        })
        .collect()
}

/// M3U8: `#EXTM3U`, then `#EXTINF:<seconds>,<title>` and the location of each entry, with
/// `-1` for an unknown duration. Paths under `base` are written relative to it.
pub fn write_m3u8(entries: &[Entry], template: &Template, base: Option<&Path>) -> String {
    let mut out = String::from("#EXTM3U\n");
    for e in entries {
        let (secs, title) = (written_seconds(e), written_title(e, template));
        let location = written_location(&e.source, base);
        out.push_str(&format!("#EXTINF:{secs},{title}\n{location}\n"));
    }
    out
}

/// PLS version 2.
pub fn write_pls(entries: &[Entry], template: &Template, base: Option<&Path>) -> String {
    let mut out = String::from("[playlist]\n");
    for (i, e) in entries.iter().enumerate() {
        let n = i + 1;
        let location = written_location(&e.source, base);
        out.push_str(&format!("File{n}={location}\n"));
        out.push_str(&format!("Title{n}={}\n", written_title(e, template)));
        out.push_str(&format!("Length{n}={}\n", written_seconds(e)));
    }
    out.push_str(&format!("NumberOfEntries={}\nVersion=2\n", entries.len()));
    out
}

/// `<seconds>,<title>`, where the seconds may be followed by attributes.
fn extinf(rest: &str) -> (Option<Duration>, Option<String>) {
    let (secs, title) = rest.split_once(',').unwrap_or((rest, ""));
    let duration = secs.split_whitespace().next().and_then(seconds);
    let title = Some(title.trim().to_string()).filter(|t| !t.is_empty());
    (duration, title)
}

/// A duration in seconds; zero, negative (`-1`: unknown) or unreadable gives `None`.
fn seconds(text: &str) -> Option<Duration> {
    let secs: f64 = text.trim().parse().ok()?;
    if secs > 0.0 {
        Duration::try_from_secs_f64(secs).ok()
    } else {
        None
    }
}

/// A line's location: a stream URL, a `file://` URL, or a path, relative to `base` unless
/// absolute.
fn location(text: &str, base: &Path) -> Option<Source> {
    if let Some((scheme, _)) = text.split_once("://")
        && is_scheme(scheme)
    {
        if scheme.eq_ignore_ascii_case("file") {
            let path = url::Url::parse(text).ok()?.to_file_path().ok()?;
            return Some(Source::File(path));
        }
        return Some(Source::Url(text.to_string()));
    }
    Some(Source::File(base.join(local_path(text))))
}

/// A URL scheme of two or more characters, so a drive letter never counts.
fn is_scheme(s: &str) -> bool {
    s.len() > 1
        && s.starts_with(|c: char| c.is_ascii_alphabetic())
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Windows playlists separate folders with `\`; elsewhere it is read as `/`.
fn local_path(text: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(text)
    } else {
        PathBuf::from(text.replace('\\', "/"))
    }
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}

fn written_seconds(e: &Entry) -> i64 {
    e.duration.map_or(-1, |d| d.as_secs_f64().round() as i64)
}

/// The shown title, kept on one line.
fn written_title(e: &Entry, template: &Template) -> String {
    e.title(template).replace(['\r', '\n'], " ")
}

fn written_location(source: &Source, base: Option<&Path>) -> String {
    match source {
        Source::Url(url) => url.clone(),
        Source::File(path) => base
            .and_then(|b| path.strip_prefix(b).ok())
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned(),
    }
}

/// Windows-1252: Latin-1, except 0x80..=0x9F.
fn windows_1252(byte: u8) -> char {
    const HIGH: [char; 32] = [
        '\u{20AC}', '\u{81}', '\u{201A}', '\u{192}', '\u{201E}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{2C6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8D}', '\u{17D}',
        '\u{8F}', '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}',
        '\u{2014}', '\u{2DC}', '\u{2122}', '\u{161}', '\u{203A}', '\u{153}', '\u{9D}', '\u{17E}',
        '\u{178}',
    ];
    match byte {
        0x80..=0x9F => HIGH[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}
