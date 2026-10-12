//! A file's tags and duration, read with `lofty` (spec §4.9).

use crate::entry::Tags;
use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{Accessor, Tag};
use std::borrow::Cow;
use std::fmt;
use std::path::Path;
use std::time::Duration;

/// What `read` finds in a file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileTags {
    pub tags: Tags,
    pub duration: Option<Duration>,
}

/// The file cannot be read. That does not make it dead: playing it may still work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagError(pub String);

impl fmt::Display for TagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TagError {}

/// The primary tag, or else the first tag the file has, and the duration. A file without
/// tags still gets its duration.
pub fn read(path: &Path) -> Result<FileTags, TagError> {
    let file = Probe::open(path)
        .and_then(|p| p.options(ParseOptions::new().read_cover_art(false)).read())
        .map_err(|e| TagError(e.to_string()))?;
    let duration = Some(file.properties().duration()).filter(|d| !d.is_zero());
    let tags = file
        .primary_tag()
        .or_else(|| file.first_tag())
        .map(tags_of)
        .unwrap_or_default();
    Ok(FileTags { tags, duration })
}

fn tags_of(tag: &Tag) -> Tags {
    let text = |value: Option<Cow<'_, str>>| {
        value
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    Tags {
        artist: text(tag.artist()),
        title: text(tag.title()),
        album: text(tag.album()),
        year: tag.date().map(|d| u32::from(d.year)).filter(|y| *y > 0),
        track: tag.track().filter(|t| *t > 0),
    }
}
