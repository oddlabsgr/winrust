//! One playlist entry and what is known about it.

use std::time::Duration;
use winrust_audio::Source;

/// Names an entry for the whole session, wherever edits move it. Never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryId(pub(crate) u64);

/// A file's tags. A field the file lacks, or has empty, is `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tags {
    pub artist: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    pub year: Option<u32>,
    pub track: Option<u32>,
}

/// Something to add: a source, with the title and duration a playlist file gave it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub source: Source,
    pub hint: Option<String>,
    pub duration: Option<Duration>,
}

impl From<Source> for Item {
    fn from(source: Source) -> Self {
        Item {
            source,
            hint: None,
            duration: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    id: EntryId,
    pub source: Source,
    /// `None` until the file's tags are read. A stream's title arrives here too.
    pub tags: Option<Tags>,
    /// The title the playlist file gave, shown until the tags are read.
    pub hint: Option<String>,
    pub duration: Option<Duration>,
    /// The file is missing, or the engine could not open it.
    pub dead: bool,
    pub selected: bool,
}

impl Entry {
    pub(crate) fn new(id: EntryId, item: Item) -> Self {
        Entry {
            id,
            source: item.source,
            tags: None,
            hint: item.hint,
            duration: item.duration,
            dead: false,
            selected: false,
        }
    }

    pub fn id(&self) -> EntryId {
        self.id
    }
}
