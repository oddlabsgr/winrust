# M4 Part 1: Playlist Crate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `crates/playlist` (`winrust-playlist`): entries, selection, editing and sorting, the play order with a real shuffle, the playback cursor, M3U/M3U8/PLS files, title templates, folder and argument sources, tag reading and the tag worker.

**Architecture:**
- **Pure, tested without files:**
  - `entry`: `EntryId`, `Entry`, `Tags`, `Item`;
  - `playlist`: `Playlist`, which holds the entries, the current entry and the selection, and does every edit;
  - `title`: the title template;
  - `rng`: the seeded generator;
  - `order`: `PlayOrder`, list order or a shuffle permutation;
  - `cursor`: `Cursor`, M3's queue bookkeeping, walking the play order.
- **Reading the disk:**
  - `files`: playlist files;
  - `sources`: arguments, drops and folders;
  - `tags`: one file's tags, through `lofty`;
  - `worker`: `TagWorker`, a background thread.
- **No app change:** `crates/app` keeps M3's `queue` and `sources` until part 3 swaps them for this crate.

**Tech Stack:** Rust 2024; `lofty` 0.25.4 (new), `crossbeam-channel` 0.5.17 and `url` 2.5.8 (both already in the tree through `winrust-audio`); `tempfile` for tests.

**Spec:** `docs/superpowers/specs/2026-10-12-m4-playlist-design.md` §4 (this part), §8; umbrella `docs/superpowers/specs/2026-10-10-winrust-design.md` §5.1, §6.3, §9.3, §12.1. Reference implementation: Webamp at commit 999f567 (`packages/webamp`), `reducers/playlist.ts`, `actionCreators/playlist.ts` and `utils.ts`.

**Scope:** M4 part 1 of 3. Part 2 draws the playlist window in `crates/ui`; part 3 wires both into the app.

**Provenance:** every code block below was compiled and tested on Windows 11 with Rust 1.99 before the plan was written, and the plan was then replayed on a fresh clone, checking every RED and GREEN. Final suite: 507 passed, 1 ignored on Windows; `fmt`, `clippy -D warnings` and `cargo deny` clean. Linux and macOS have not built this code yet: CI is its first build there, and runs one more test there (`linked_files_count_and_linked_folders_are_not_followed`, Unix only), so expect 508. A compile error there is a plan defect to rule on, not a sign of a broken step. The new fuzz target builds on stable; its first nightly run is CI's.

## Global Constraints

- `winrust-playlist` has no windowing or OS API dependency (umbrella §5.1). Its dependencies are `winrust-audio` (for `Source`), `lofty` 0.25.4, `crossbeam-channel` 0.5.17 and `url` 2.5.8.
- Every edit keeps the current entry and the selection attached to the same entries (spec §4.4).
- Sorting ignores case, compares paths one component at a time, and is stable (spec §4.4).
- The play order is pure and seeded, so tests repeat exactly (spec §4.5).
- Gapless is for local files only: a stream is never handed to the engine with `QueueNext` (spec §4.6, umbrella §6.3).
- A playlist file never fails as a whole: lines that cannot be used are skipped (spec §4.7).
- The default title template is `[%artist% - ]%title%` (spec §4.8).
- One tag thread; a new request replaces the pending ones; reads already under way finish (spec §4.10).
- Folders expand recursively to `aac flac m4a mp3 mp4 oga ogg wav` files (spec §4.11).
- `crates/app` is not changed in this part.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo deny check` stay clean.
- Work on branch `m4-part1-playlist`. Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Decisions fixed by this plan

These settle what the spec leaves open; each costs little to change later.

**Entries and selection:**
- **Ids** come from a counter in `Playlist`. `Entry` fields are public except its id, so `Playlist::get_mut` cannot break an id.
- **The keyboard cursor is kept apart from the anchor.** The spec makes the anchor the cursor, but Shift+arrows need a moving end while the anchor stays put. Plain moves and clicks set both; Shift moves only the cursor.
- **Shift-click with no anchor does nothing**, as in Webamp.

**Removing the current entry:**
- Playback goes on with the entry that followed it. `Playlist::position()` reports `Position::Before(row)` until another entry becomes current.
- Entries appended after a removed last entry follow it.
- `clear` starts over, like `replace`.

**Play order:**
- **Shuffle without repeat plays each entry once, then stops.** With repeat, a new permutation follows, never starting with the track just played. This differs from M3 and Webamp, where shuffle went on forever. The spec's "Repeat: wraps … in list order or permutation order" is read as: repeat decides the wrap in both orders.
- **Shuffle's history:**
  - Previous walks back through what played, and gives nothing at the start of the history.
  - Choosing an entry that already played (a double-click) rewinds the history to it.
  - Turning shuffle on draws a new permutation that starts with the current entry.
- **The ±10 jumps keep M3's rules:** list order; wrap with repeat; otherwise clamp, and do nothing when already at that end.
- **Seeds:** `Rng::new` mixes the seed. M3's `seed | 1` made seeds 42 and 43 give the same shuffle.

**Playback cursor:**
- **`requeue` answers `Keep` when the queued entry would not change**, since part 3 calls it after every edit.
- **When a track ends with nothing queued, the cursor asks the play order again:** a stream may be next, or entries may have been appended since the track started.
- **An entry that starts playing is no longer dead.**

**Title templates:**
- **Syntax:**
  - field names are letters, digits and `_`, in any case;
  - a `%` not followed by `name%` is literal;
  - `]]` is always a literal `]`.
- **Sections:** a section shows when every field directly inside it has a value; a nested section decides for itself.
- **Fallbacks:**
  - a blank result gives the file name;
  - before the tags are read, the title is the playlist file's title, else the file name;
  - a file whose tags cannot be read keeps the playlist file's title, and counts as read so it is not asked for again.

**Tags and the worker:**
- **What is read:**
  - the primary tag, else the first tag;
  - blank fields, year 0, track 0 and a zero duration count as missing;
  - cover art is not read.
- **`lofty` keeps its default features:** 0.25.4 does not build without them.
- **`deny.toml` ignores RUSTSEC-2024-0436:** `paste` is unmaintained, not vulnerable, and is a build-time macro of `lofty`.
- **Results arrive through a callback**, run on the worker thread (the spec says a channel). Part 3 forwards them to its `EventLoopProxy` without a relay thread; tests pass a channel sender.
- **Dropping the worker does not join its thread,** so quitting never waits on a slow read. `TagWorker::with_reader` lets tests use a slow reader.

**Playlist files:**
- **Format:** `.pls` is read as PLS, anything else as M3U; a `.m3u` that is not valid UTF-8 is Windows-1252.
- **Locations:**
  - `file://` URLs go through the `url` crate (already in the tree through `winrust-audio`);
  - any other `scheme://` with a scheme of two or more characters is a stream, so drive letters never count;
  - on Linux and macOS, `\` in a path is read as `/`.
- **Reading:**
  - `#EXTINF` seconds may be followed by attributes;
  - durations that are zero, negative or out of range count as unknown;
  - PLS keys without a number are ignored;
  - numbers without a `FileN` are skipped.
- **Writing:** durations are rounded to whole seconds, titles are kept on one line, and lines end with LF.

**Sources:**
- Linked files in folders count; linked folders are not followed, so a link loop cannot trap the scan.
- A missing or unreadable playlist file adds nothing.

**Fuzzing:** a `playlist_file` fuzz target parses the same bytes as M3U, PLS and a title template (umbrella §12.1).

## Review Focus

The inputs most likely to bite that the feature tests do not reach. Each has a test in the owning task:

1. **Removing the playing entry, or the queued one, mid-track:** playback goes on with the next entry; it neither stops nor jumps to the top. Tests: Task 3 `a_removed_current_entry_hands_over_to_the_one_after_it`; Task 5 `removing_the_playing_entry_keeps_the_next_one_queued`, `requeue_follows_edits_to_the_list`.
2. **Shuffle over a playlist where most files are gone** (an unplugged drive): playback goes on until every entry has failed, even when the shuffle meets a failed entry twice. Test: Task 5 `with_shuffle_skipping_goes_on_until_every_entry_failed`.
3. **Old Winamp playlists:**
   - Windows-1252 text;
   - backslash paths read on Linux or macOS;
   - `#EXTINF` lines with attributes or absurd numbers.

   Tests: Task 6 `a_legacy_m3u_that_is_not_utf8_is_windows_1252`, `backslashes_separate_folders_on_every_os`, `extinf_attributes_and_odd_numbers_are_tolerated`.
4. **Tags with line breaks or blank fields:** the saved playlist keeps one entry per line, and blank tags fall back to the file name. Tests: Task 6 `m3u8_is_written_with_relative_paths_under_the_base`; Task 8 `blank_fields_count_as_missing`.
5. **A folder containing a link to its own parent:** the scan ends. Test: Task 7 `linked_files_count_and_linked_folders_are_not_followed` (Linux and macOS).

## File Structure

| File | Responsibility |
|---|---|
| `crates/playlist/Cargo.toml` | the `winrust-playlist` package |
| `crates/playlist/src/lib.rs` | module list |
| `crates/playlist/src/entry.rs` | `EntryId`, `Tags`, `Item`, `Entry` (title, file name, tags) |
| `crates/playlist/src/playlist.rs` | `Playlist`, `Position`, `path_key`: entries, current entry, selection, edits, sorting |
| `crates/playlist/src/title.rs` | `Template`, `DEFAULT` |
| `crates/playlist/src/rng.rs` | `Rng` |
| `crates/playlist/src/order.rs` | `PlayOrder` |
| `crates/playlist/src/cursor.rs` | `Cursor`, `Requeue` |
| `crates/playlist/src/files.rs` | `read`, `decode`, `parse_m3u`, `parse_pls`, `write_m3u8`, `write_pls` |
| `crates/playlist/src/sources.rs` | `from_args`, `from_paths`, `is_url`, `AUDIO_EXTENSIONS`, `PLAYLIST_EXTENSIONS` |
| `crates/playlist/src/tags.rs` | `read`, `FileTags`, `TagError` |
| `crates/playlist/src/worker.rs` | `TagWorker`, `TagResult` |
| `crates/playlist/tests/*.rs` | tests, with helpers in `tests/common/mod.rs` |
| `fuzz/fuzz_targets/playlist_file.rs` | fuzzing playlist files and templates |
| `deny.toml` | the `paste` advisory |
| `.github/workflows/fuzz.yml` | the new fuzz target |

## Setup

Branch `m4-part1-playlist` starts from `main` with the spec and this plan as its first commits.

Run: `git switch m4-part1-playlist`, then `cargo test --workspace`
Expected: `402 passed`, `1 ignored` in total, 0 failed.

---

### Task 1: Entries and selection

The crate starts with entries and the selection, following Webamp's reducer: a click selects one row and sets the anchor, Ctrl-click toggles, Shift-click selects from the anchor, plus select all, none and invert. The keyboard cursor moves on its own, so Shift+arrows can extend from the anchor (§7).

**Files:**
- Create: `crates/playlist/Cargo.toml`, `crates/playlist/src/lib.rs`, `crates/playlist/src/entry.rs`, `crates/playlist/src/playlist.rs`
- Test: `crates/playlist/tests/common/mod.rs`, `crates/playlist/tests/selection.rs`

**Interfaces:**
- Consumes: `winrust_audio::Source`.
- Produces:
  - `entry::EntryId` (`Copy`, `Eq`, `Hash`, `Ord`);
  - `entry::Tags { artist, title, album: Option<String>, year, track: Option<u32> }`;
  - `entry::Item { source: Source, hint: Option<String>, duration: Option<Duration> }`, with `From<Source>`;
  - `entry::Entry`, with:
    - public fields `source`, `tags: Option<Tags>`, `hint`, `duration`, `dead`, `selected`;
    - `id() -> EntryId`;
  - `playlist::Playlist` (`Default`), with:
    - `new`, `len`, `is_empty`, `entries() -> &[Entry]`, `get(EntryId)`, `get_mut(EntryId)`, `index_of(EntryId) -> Option<usize>`;
    - `insert(at: usize, Vec<Item>)`, `append(Vec<Item>)`, `replace(Vec<Item>)`;
    - `current() -> Option<EntryId>`, `current_index()`, `set_current(Option<EntryId>)`;
    - `click(row)`, `ctrl_click(row)`, `shift_click(row)`, `select_all`, `select_none`, `invert_selection`;
    - `cursor() -> Option<EntryId>`, `cursor_index()`, `move_cursor(to: usize, extend: bool)`.

- [ ] **Step 1: Write the crate skeleton and the failing tests**

Create `crates/playlist/Cargo.toml`:

```toml
[package]
name = "winrust-playlist"
description = "Winrust playlist: entries, selection, editing, play order, playlist files, title templates and tags"
version.workspace = true
edition.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
winrust-audio = { path = "../audio" }
```

Create `crates/playlist/src/lib.rs`:

```rust
//! The Winrust playlist: entries, selection and editing, play order, playlist files, title
//! templates and tags. No windowing or OS API; only reading playlist files, folders and tags
//! touches the disk.
```

Create `crates/playlist/tests/common/mod.rs`:

```rust
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
```

Create `crates/playlist/tests/selection.rs`:

```rust
mod common;

use common::{file, names, playlist, selected};
use std::collections::HashSet;
use winrust_playlist::playlist::Playlist;

#[test]
fn ids_are_never_reused() {
    let mut pl = playlist(2);
    let mut ids: HashSet<_> = pl.entries().iter().map(|e| e.id()).collect();
    pl.replace(vec![file("a"), file("b")]);
    ids.extend(pl.entries().iter().map(|e| e.id()));
    assert_eq!(ids.len(), 4);
}

#[test]
fn insert_places_entries_before_a_row_and_appends_past_the_end() {
    let mut pl = playlist(3);
    pl.insert(1, vec![file("a"), file("b")]);
    pl.insert(99, vec![file("c")]);
    assert_eq!(names(&pl), ["0.mp3", "a", "b", "1.mp3", "2.mp3", "c"]);
}

#[test]
fn a_click_selects_one_row() {
    let mut pl = playlist(4);
    pl.click(1);
    pl.click(2);
    assert_eq!(selected(&pl), [2]);
}

#[test]
fn ctrl_click_toggles_rows() {
    let mut pl = playlist(4);
    pl.click(0);
    pl.ctrl_click(2);
    assert_eq!(selected(&pl), [0, 2]);
    pl.ctrl_click(0);
    assert_eq!(selected(&pl), [2]);
}

#[test]
fn shift_click_selects_from_the_anchor() {
    let mut pl = playlist(6);
    pl.click(3);
    pl.shift_click(1);
    assert_eq!(selected(&pl), [1, 2, 3]);
    pl.shift_click(4);
    assert_eq!(selected(&pl), [3, 4], "the anchor stays on the clicked row");
}

#[test]
fn shift_click_without_an_anchor_does_nothing() {
    let mut pl = playlist(3);
    pl.shift_click(2);
    assert_eq!(selected(&pl), [] as [usize; 0]);
}

#[test]
fn select_all_none_and_invert() {
    let mut pl = playlist(4);
    pl.click(1);
    pl.invert_selection();
    assert_eq!(selected(&pl), [0, 2, 3]);
    pl.select_all();
    assert_eq!(selected(&pl), [0, 1, 2, 3]);
    pl.select_none();
    assert_eq!(selected(&pl), [] as [usize; 0]);
}

#[test]
fn the_keyboard_cursor_moves_and_extends_from_the_anchor() {
    let mut pl = playlist(6);
    pl.move_cursor(2, false);
    pl.move_cursor(3, true);
    pl.move_cursor(4, true);
    assert_eq!(selected(&pl), [2, 3, 4]);
    pl.move_cursor(1, true);
    assert_eq!(selected(&pl), [1, 2]);
    assert_eq!(pl.cursor_index(), Some(1));
    pl.move_cursor(usize::MAX, false);
    assert_eq!(selected(&pl), [5]);
    assert_eq!(pl.cursor_index(), Some(5));
}

#[test]
fn extending_without_an_anchor_starts_one() {
    let mut pl = playlist(3);
    pl.move_cursor(1, true);
    assert_eq!(selected(&pl), [1]);
    pl.move_cursor(2, true);
    assert_eq!(selected(&pl), [1, 2]);
}

#[test]
fn the_cursor_does_nothing_on_an_empty_list() {
    let mut pl = Playlist::new();
    pl.move_cursor(0, false);
    assert_eq!(pl.cursor(), None);
}

#[test]
fn rows_past_the_end_are_ignored() {
    let mut pl = playlist(2);
    pl.click(0);
    pl.click(5);
    pl.ctrl_click(5);
    pl.shift_click(5);
    assert_eq!(selected(&pl), [0]);
}

#[test]
fn current_and_selection_follow_their_entries_when_rows_are_inserted() {
    let mut pl = playlist(3);
    pl.click(1);
    pl.set_current(Some(pl.entries()[2].id()));
    pl.insert(0, vec![file("a")]);
    assert_eq!(selected(&pl), [2]);
    assert_eq!(pl.current_index(), Some(3));
    assert_eq!(pl.cursor_index(), Some(2));
}

#[test]
fn replace_leaves_nothing_current_or_selected() {
    let mut pl = playlist(3);
    pl.click(1);
    let old = pl.entries()[0].id();
    pl.set_current(Some(old));
    pl.replace(vec![file("a")]);
    assert_eq!(pl.current(), None);
    assert_eq!(pl.cursor(), None);
    assert_eq!(selected(&pl), [] as [usize; 0]);
    pl.set_current(Some(old));
    assert_eq!(
        pl.current(),
        None,
        "an id from the old list is not in this one"
    );
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p winrust-playlist --test selection`
Expected: compile errors ``unresolved import `winrust_playlist::entry` `` and ``unresolved import `winrust_playlist::playlist` ``.

- [ ] **Step 3: Implement**

Replace `crates/playlist/src/lib.rs` with:

```rust
//! The Winrust playlist: entries, selection and editing, play order, playlist files, title
//! templates and tags. No windowing or OS API; only reading playlist files, folders and tags
//! touches the disk.

pub mod entry;
pub mod playlist;
```

Create `crates/playlist/src/entry.rs`:

```rust
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
```

Create `crates/playlist/src/playlist.rs`:

```rust
//! The playlist: entries in order, the current entry and the selection. Edits keep both
//! attached to the same entries. Selection follows Webamp `reducers/playlist.ts`.

use crate::entry::{Entry, EntryId, Item};

#[derive(Clone, Debug, Default)]
pub struct Playlist {
    entries: Vec<Entry>,
    next_id: u64,
    current: Option<EntryId>,
    /// Where Shift-click and Shift+arrows extend the selection from.
    anchor: Option<EntryId>,
    /// The keyboard cursor.
    cursor: Option<EntryId>,
}

impl Playlist {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn get(&self, id: EntryId) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id() == id)
    }

    pub fn get_mut(&mut self, id: EntryId) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.id() == id)
    }

    pub fn index_of(&self, id: EntryId) -> Option<usize> {
        self.entries.iter().position(|e| e.id() == id)
    }

    /// Adds entries before row `at` (a drop between rows); past the end, appends.
    pub fn insert(&mut self, at: usize, items: Vec<Item>) {
        let at = at.min(self.entries.len());
        let new: Vec<Entry> = items
            .into_iter()
            .map(|item| {
                self.next_id += 1;
                Entry::new(EntryId(self.next_id), item)
            })
            .collect();
        self.entries.splice(at..at, new);
    }

    pub fn append(&mut self, items: Vec<Item>) {
        self.insert(self.entries.len(), items);
    }

    /// Replaces every entry; nothing is current or selected afterwards.
    pub fn replace(&mut self, items: Vec<Item>) {
        self.entries.clear();
        self.current = None;
        self.anchor = None;
        self.cursor = None;
        self.append(items);
    }

    /// The entry playing, or that Play starts from.
    pub fn current(&self) -> Option<EntryId> {
        self.current
    }

    pub fn current_index(&self) -> Option<usize> {
        self.index_of(self.current?)
    }

    /// An id not in the playlist clears the current entry.
    pub fn set_current(&mut self, id: Option<EntryId>) {
        self.current = id.filter(|id| self.index_of(*id).is_some());
    }

    /// Selects only this row, which becomes the anchor and the cursor.
    pub fn click(&mut self, index: usize) {
        let Some(id) = self.id_at(index) else {
            return;
        };
        for (i, e) in self.entries.iter_mut().enumerate() {
            e.selected = i == index;
        }
        self.anchor = Some(id);
        self.cursor = Some(id);
    }

    /// Toggles this row, which becomes the anchor and the cursor even when it is unselected
    /// (as Winamp 2 does).
    pub fn ctrl_click(&mut self, index: usize) {
        let Some(id) = self.id_at(index) else {
            return;
        };
        self.entries[index].selected = !self.entries[index].selected;
        self.anchor = Some(id);
        self.cursor = Some(id);
    }

    /// Selects only the rows from the anchor to this one; without an anchor, does nothing.
    pub fn shift_click(&mut self, index: usize) {
        let (Some(id), Some(anchor)) = (self.id_at(index), self.anchor_index()) else {
            return;
        };
        let (from, to) = (anchor.min(index), anchor.max(index));
        for (i, e) in self.entries.iter_mut().enumerate() {
            e.selected = (from..=to).contains(&i);
        }
        self.cursor = Some(id);
    }

    pub fn select_all(&mut self) {
        self.entries.iter_mut().for_each(|e| e.selected = true);
    }

    pub fn select_none(&mut self) {
        self.entries.iter_mut().for_each(|e| e.selected = false);
    }

    pub fn invert_selection(&mut self) {
        self.entries
            .iter_mut()
            .for_each(|e| e.selected = !e.selected);
    }

    /// The keyboard cursor's entry.
    pub fn cursor(&self) -> Option<EntryId> {
        self.cursor
    }

    pub fn cursor_index(&self) -> Option<usize> {
        self.index_of(self.cursor?)
    }

    /// Moves the keyboard cursor to row `to` (clamped to the last row). It selects that row
    /// alone, or with `extend` the rows from the anchor to it.
    pub fn move_cursor(&mut self, to: usize, extend: bool) {
        if self.entries.is_empty() {
            return;
        }
        let to = to.min(self.entries.len() - 1);
        if extend && self.anchor_index().is_some() {
            self.shift_click(to);
        } else {
            self.click(to);
        }
    }

    fn id_at(&self, index: usize) -> Option<EntryId> {
        self.entries.get(index).map(Entry::id)
    }

    fn anchor_index(&self) -> Option<usize> {
        self.index_of(self.anchor?)
    }
}
```

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p winrust-playlist`
Expected: PASS: `13 passed` (selection).

- [ ] **Step 5: Lint and commit**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no output.

Run: `cargo deny check`
Expected: `advisories ok, bans ok, licenses ok, sources ok`.

```bash
git add Cargo.lock crates/playlist
git commit -m "feat(playlist): entries and selection" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Title templates

`[%artist% - ]%title%` and the rest of spec §4.8. Parsing never fails: what is not valid syntax is literal text. `Entry::title` applies the template and its fallbacks.

**Files:**
- Create: `crates/playlist/src/title.rs`
- Modify: `crates/playlist/src/entry.rs`, `crates/playlist/src/lib.rs`
- Test: `crates/playlist/tests/title.rs`

**Interfaces:**
- Consumes: `entry::{Entry, Tags, Item}`, `playlist::Playlist` (Task 1).
- Produces:
  - `title::DEFAULT: &str` (`"[%artist% - ]%title%"`);
  - `title::Template` (`Default` = `DEFAULT`), with `parse(&str) -> Template` and `render(&Tags, filename: &str) -> String`;
  - `Entry::filename() -> String` (the file name without extension, or the URL);
  - `Entry::title(&Template) -> String` (the title every view shows).

- [ ] **Step 1: Write the failing tests**

Create `crates/playlist/tests/title.rs`:

```rust
use std::path::PathBuf;
use winrust_audio::Source;
use winrust_playlist::entry::{Entry, Item, Tags};
use winrust_playlist::playlist::Playlist;
use winrust_playlist::title::Template;

fn tags() -> Tags {
    Tags {
        artist: Some("Artist".into()),
        title: Some("Title".into()),
        album: Some("Album".into()),
        year: Some(1999),
        track: Some(7),
    }
}

fn render(template: &str, tags: &Tags) -> String {
    Template::parse(template).render(tags, "song")
}

fn entry(source: Source, hint: Option<&str>, tags: Option<Tags>) -> Entry {
    let mut pl = Playlist::new();
    pl.append(vec![Item {
        source,
        hint: hint.map(String::from),
        duration: None,
    }]);
    let id = pl.entries()[0].id();
    pl.get_mut(id).unwrap().tags = tags;
    pl.entries()[0].clone()
}

fn song() -> Source {
    Source::File(PathBuf::from("music").join("song.mp3"))
}

#[test]
fn the_default_template_joins_artist_and_title() {
    let t = Template::default();
    assert_eq!(t.render(&tags(), "song"), "Artist - Title");
    let no_artist = Tags {
        artist: None,
        ..tags()
    };
    assert_eq!(t.render(&no_artist, "song"), "Title");
}

#[test]
fn every_field_expands() {
    assert_eq!(
        render(
            "%track%|%artist%|%album%|%year%|%title%|%filename%",
            &tags()
        ),
        "7|Artist|Album|1999|Title|song"
    );
}

#[test]
fn field_names_ignore_case() {
    assert_eq!(render("%ARTIST%/%Title%", &tags()), "Artist/Title");
}

#[test]
fn a_section_vanishes_when_a_field_inside_is_empty() {
    let no_album = Tags {
        album: None,
        ..tags()
    };
    assert_eq!(render("[%album% - ]%title%", &no_album), "Title");
    assert_eq!(render("[%album% - ]%title%", &tags()), "Album - Title");
    assert_eq!(render("[no fields]", &no_album), "no fields");
}

#[test]
fn nested_sections_decide_for_themselves() {
    let t = "[%artist%[ (%year%)] - ]%title%";
    let no_year = Tags {
        year: None,
        ..tags()
    };
    assert_eq!(render(t, &tags()), "Artist (1999) - Title");
    assert_eq!(render(t, &no_year), "Artist - Title");
    let no_artist = Tags {
        artist: None,
        ..tags()
    };
    assert_eq!(render(t, &no_artist), "Title");
}

#[test]
fn doubled_marks_are_literal() {
    assert_eq!(render("%%[[x]]", &tags()), "%[x]");
}

#[test]
fn unknown_fields_expand_to_nothing() {
    assert_eq!(render("%genre%x", &tags()), "x");
    assert_eq!(render("[%genre% - ]%title%", &tags()), "Title");
}

#[test]
fn broken_syntax_is_taken_literally() {
    assert_eq!(render("50% [%title%", &tags()), "50% [Title");
    assert_eq!(render("a]b", &tags()), "a]b");
    assert_eq!(render("%title", &tags()), "%title");
    assert_eq!(render("100%", &tags()), "100%");
    assert_eq!(render("%%%", &tags()), "%%");
    assert_eq!(render("", &tags()), "");
}

#[test]
fn the_title_falls_back_to_the_file_name() {
    let t = Template::default();
    let untitled = Tags {
        title: None,
        ..tags()
    };
    assert_eq!(entry(song(), None, Some(untitled)).title(&t), "song");
    let blank = Template::parse("%genre%");
    assert_eq!(entry(song(), None, Some(tags())).title(&blank), "song");
    assert_eq!(
        entry(song(), None, Some(tags())).title(&t),
        "Artist - Title"
    );
}

#[test]
fn the_playlist_file_title_shows_until_the_tags_are_read() {
    let t = Template::default();
    assert_eq!(
        entry(song(), Some("From the list"), None).title(&t),
        "From the list"
    );
    assert_eq!(entry(song(), None, None).title(&t), "song");
    let read = entry(song(), Some("From the list"), Some(tags()));
    assert_eq!(read.title(&t), "Artist - Title");
}

#[test]
fn a_stream_is_named_by_its_url() {
    let url = Source::Url("http://radio.example/live".into());
    let e = entry(url, None, None);
    assert_eq!(e.filename(), "http://radio.example/live");
    assert_eq!(e.title(&Template::default()), "http://radio.example/live");
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p winrust-playlist --test title`
Expected: compile errors ``unresolved import `winrust_playlist::title` ``, ``no method named `title` found for struct `winrust_playlist::entry::Entry` `` and ``no method named `filename` found``.

- [ ] **Step 3: Implement**

Create `crates/playlist/src/title.rs`:

```rust
//! Title templates such as `[%artist% - ]%title%` (spec §4.8). Parsing never fails: what is
//! not valid syntax is taken literally.

use crate::entry::Tags;

/// The template a new config starts with.
pub const DEFAULT: &str = "[%artist% - ]%title%";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template(Vec<Part>);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Part {
    Text(String),
    Field(Field),
    /// `[...]`: shown only when every field directly inside has a value. A section nested
    /// inside decides for itself.
    Section(Vec<Part>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Artist,
    Title,
    Album,
    Year,
    Track,
    Filename,
    /// Expands to nothing.
    Unknown,
}

impl Default for Template {
    fn default() -> Self {
        Template::parse(DEFAULT)
    }
}

impl Template {
    pub fn parse(text: &str) -> Template {
        let chars: Vec<char> = text.chars().collect();
        let mut pos = 0;
        Template(parse(&chars, &mut pos, false).0)
    }

    /// Expands the template; `filename` is the value of `%filename%`.
    pub fn render(&self, tags: &Tags, filename: &str) -> String {
        let mut out = String::new();
        render(&self.0, tags, filename, &mut out);
        out
    }
}

/// Parses up to the end, or inside a section up to its `]`. Returns the parts and whether
/// the section was closed.
fn parse(chars: &[char], pos: &mut usize, in_section: bool) -> (Vec<Part>, bool) {
    let mut parts = Vec::new();
    let mut text = String::new();
    while let Some(&c) = chars.get(*pos) {
        let doubled = chars.get(*pos + 1) == Some(&c);
        match c {
            '%' | '[' | ']' if doubled => {
                text.push(c);
                *pos += 2;
            }
            '%' => match field_name(&chars[*pos + 1..]) {
                Some(name) => {
                    flush(&mut text, &mut parts);
                    parts.push(Part::Field(field(&name)));
                    *pos += name.chars().count() + 2;
                }
                None => {
                    text.push('%');
                    *pos += 1;
                }
            },
            '[' => {
                *pos += 1;
                flush(&mut text, &mut parts);
                let (inner, closed) = parse(chars, pos, true);
                if closed {
                    parts.push(Part::Section(inner));
                } else {
                    parts.push(Part::Text("[".into()));
                    parts.extend(inner);
                }
            }
            ']' if in_section => {
                *pos += 1;
                flush(&mut text, &mut parts);
                return (parts, true);
            }
            c => {
                text.push(c);
                *pos += 1;
            }
        }
    }
    flush(&mut text, &mut parts);
    (parts, false)
}

/// `name%` at the start of `rest`, with a name of letters, digits and `_`.
fn field_name(rest: &[char]) -> Option<String> {
    let len = rest
        .iter()
        .position(|c| !(c.is_ascii_alphanumeric() || *c == '_'))?;
    (len > 0 && rest[len] == '%').then(|| rest[..len].iter().collect())
}

fn field(name: &str) -> Field {
    match name.to_ascii_lowercase().as_str() {
        "artist" => Field::Artist,
        "title" => Field::Title,
        "album" => Field::Album,
        "year" => Field::Year,
        "track" => Field::Track,
        "filename" => Field::Filename,
        _ => Field::Unknown,
    }
}

fn flush(text: &mut String, parts: &mut Vec<Part>) {
    if !text.is_empty() {
        parts.push(Part::Text(std::mem::take(text)));
    }
}

fn render(parts: &[Part], tags: &Tags, filename: &str, out: &mut String) {
    for part in parts {
        match part {
            Part::Text(t) => out.push_str(t),
            Part::Field(f) => out.push_str(&value(*f, tags, filename)),
            Part::Section(inner) => {
                let filled = inner.iter().all(|p| match p {
                    Part::Field(f) => !value(*f, tags, filename).is_empty(),
                    _ => true,
                });
                if filled {
                    render(inner, tags, filename, out);
                }
            }
        }
    }
}

fn value(field: Field, tags: &Tags, filename: &str) -> String {
    let text = |s: &Option<String>| s.clone().unwrap_or_default();
    let number = |n: Option<u32>| n.map(|n| n.to_string()).unwrap_or_default();
    match field {
        Field::Artist => text(&tags.artist),
        Field::Title => text(&tags.title),
        Field::Album => text(&tags.album),
        Field::Year => number(tags.year),
        Field::Track => number(tags.track),
        Field::Filename => filename.to_string(),
        Field::Unknown => String::new(),
    }
}
```

Patch `crates/playlist/src/entry.rs` (apply with `git apply`):

```diff
diff --git a/crates/playlist/src/entry.rs b/crates/playlist/src/entry.rs
index 8145d37..1dd7623 100644
--- a/crates/playlist/src/entry.rs
+++ b/crates/playlist/src/entry.rs
@@ -1,5 +1,6 @@
 //! One playlist entry and what is known about it.
 
+use crate::title::Template;
 use std::time::Duration;
 use winrust_audio::Source;
 
@@ -65,4 +66,31 @@ impl Entry {
     pub fn id(&self) -> EntryId {
         self.id
     }
+
+    /// `%filename%`: the file name without its extension; for a stream, the URL.
+    pub fn filename(&self) -> String {
+        match &self.source {
+            Source::File(path) => path
+                .file_stem()
+                .unwrap_or(path.as_os_str())
+                .to_string_lossy()
+                .into_owned(),
+            Source::Url(url) => url.clone(),
+        }
+    }
+
+    /// The title the playlist shows (spec §4.8): the template over the tags, or the file
+    /// name when the tags have no title or the template gives nothing. Before the tags are
+    /// read, the playlist file's title, else the file name.
+    pub fn title(&self, template: &Template) -> String {
+        let name = self.filename();
+        match &self.tags {
+            Some(tags) if tags.title.is_some() => {
+                let title = template.render(tags, &name);
+                if title.trim().is_empty() { name } else { title }
+            }
+            Some(_) => name,
+            None => self.hint.clone().unwrap_or(name),
+        }
+    }
 }
```

Patch `crates/playlist/src/lib.rs` (apply with `git apply`):

```diff
diff --git a/crates/playlist/src/lib.rs b/crates/playlist/src/lib.rs
index 3cfa74d..451ed43 100644
--- a/crates/playlist/src/lib.rs
+++ b/crates/playlist/src/lib.rs
@@ -4,3 +4,4 @@
 
 pub mod entry;
 pub mod playlist;
+pub mod title;
```

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p winrust-playlist`
Expected: PASS: `13 passed` (selection), `11 passed` (title).

- [ ] **Step 5: Lint and commit**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no output.

```bash
git add crates/playlist
git commit -m "feat(playlist): title templates" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Editing and sorting

Every edit from spec §4.4: remove selected, crop, clear, remove dead, move the selection (Webamp `dragSelected` and `moveSelected`), sort by title, file name or path, reverse and randomize.

When the current entry is removed, `position()` remembers where play goes on. This fixes M3's folder-sort minor through `path_key`.

**Files:**
- Create: `crates/playlist/src/rng.rs`
- Modify: `crates/playlist/src/playlist.rs`, `crates/playlist/src/lib.rs`
- Test: `crates/playlist/tests/editing.rs`

**Interfaces:**
- Consumes: Task 1's `Playlist`; Task 2's `Template` and `Entry::title`.
- Produces:
  - `rng::Rng`, with `new(seed: u64)`, `below(n: usize) -> usize`, `shuffle(&mut [T])`;
  - `playlist::Position { Start, At(usize), Before(usize) }` and `Playlist::position() -> Position`;
  - `Playlist::remove_selected`, `crop`, `clear`, `remove_dead(exists: impl Fn(&Path) -> bool)`;
  - `Playlist::move_selected(offset: isize) -> isize`, which returns the rows actually moved;
  - `Playlist::sort_by_title(&Template)`, `sort_by_file_name`, `sort_by_path`, `reverse`, `randomize(&mut Rng)`;
  - `playlist::path_key(&Path) -> Vec<String>`.

- [ ] **Step 1: Write the failing tests**

Create `crates/playlist/tests/editing.rs`:

```rust
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
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p winrust-playlist --test editing`
Expected: compile errors, among them ``unresolved import `winrust_playlist::rng` ``, ``unresolved import `winrust_playlist::playlist::Position` `` and ``no method named `remove_selected` found for struct `Playlist` ``.

- [ ] **Step 3: Implement**

Create `crates/playlist/src/rng.rs`:

```rust
//! A small seeded random generator, so shuffles repeat exactly in tests.

/// xorshift64: plenty for a shuffle.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    /// Every seed gives its own sequence.
    pub fn new(seed: u64) -> Self {
        Rng((seed ^ 0x9E37_79B9_7F4A_7C15).max(1))
    }

    /// A number in `0..n`; `n` must not be 0.
    pub fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }

    /// Fisher-Yates.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }
}
```

Replace `crates/playlist/src/playlist.rs` with:

```rust
//! The playlist: entries in order, the current entry and the selection. Edits keep both
//! attached to the same entries. Selection follows Webamp `reducers/playlist.ts`.

use crate::entry::{Entry, EntryId, Item};
use crate::rng::Rng;
use crate::title::Template;
use std::path::Path;
use winrust_audio::Source;

/// Where the list order goes on from (spec §4.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    /// Nothing is current.
    Start,
    /// The current entry's row.
    At(usize),
    /// The current entry was removed; the entry that followed it is now at this row, or the
    /// row is the list's length when nothing followed it.
    Before(usize),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Current {
    #[default]
    None,
    At(EntryId),
    /// The current entry was removed; play goes on at this entry, or past the end.
    Removed(Option<EntryId>),
}

#[derive(Clone, Debug, Default)]
pub struct Playlist {
    entries: Vec<Entry>,
    next_id: u64,
    current: Current,
    /// Where Shift-click and Shift+arrows extend the selection from.
    anchor: Option<EntryId>,
    /// The keyboard cursor.
    cursor: Option<EntryId>,
}

impl Playlist {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn get(&self, id: EntryId) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id() == id)
    }

    pub fn get_mut(&mut self, id: EntryId) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.id() == id)
    }

    pub fn index_of(&self, id: EntryId) -> Option<usize> {
        self.entries.iter().position(|e| e.id() == id)
    }

    /// Adds entries before row `at` (a drop between rows); past the end, appends.
    pub fn insert(&mut self, at: usize, items: Vec<Item>) {
        let at = at.min(self.entries.len());
        let new: Vec<Entry> = items
            .into_iter()
            .map(|item| {
                self.next_id += 1;
                Entry::new(EntryId(self.next_id), item)
            })
            .collect();
        if self.current == Current::Removed(None) && at == self.entries.len() {
            // Entries added after the removed last entry are what follows it now.
            self.current = Current::Removed(new.first().map(Entry::id));
        }
        self.entries.splice(at..at, new);
    }

    pub fn append(&mut self, items: Vec<Item>) {
        self.insert(self.entries.len(), items);
    }

    /// Replaces every entry; nothing is current or selected afterwards.
    pub fn replace(&mut self, items: Vec<Item>) {
        self.entries.clear();
        self.current = Current::None;
        self.anchor = None;
        self.cursor = None;
        self.append(items);
    }

    /// The entry playing, or that Play starts from.
    pub fn current(&self) -> Option<EntryId> {
        match self.current {
            Current::At(id) => Some(id),
            _ => None,
        }
    }

    pub fn current_index(&self) -> Option<usize> {
        self.index_of(self.current()?)
    }

    /// An id not in the playlist clears the current entry.
    pub fn set_current(&mut self, id: Option<EntryId>) {
        self.current = match id.filter(|id| self.index_of(*id).is_some()) {
            Some(id) => Current::At(id),
            None => Current::None,
        };
    }

    pub fn position(&self) -> Position {
        let at = |id| self.index_of(id);
        match self.current {
            Current::None => Position::Start,
            Current::At(id) => at(id).map_or(Position::Start, Position::At),
            Current::Removed(Some(id)) => at(id).map_or(Position::Start, Position::Before),
            Current::Removed(None) => Position::Before(self.entries.len()),
        }
    }

    pub fn remove_selected(&mut self) {
        self.retain(|e| !e.selected);
    }

    /// Keeps only the selected entries.
    pub fn crop(&mut self) {
        self.retain(|e| e.selected);
    }

    pub fn clear(&mut self) {
        self.replace(Vec::new());
    }

    /// Removes dead entries, and files that `exists` says are gone (Alt+Del).
    pub fn remove_dead(&mut self, exists: impl Fn(&Path) -> bool) {
        self.retain(|e| {
            !e.dead
                && match &e.source {
                    Source::File(path) => exists(path),
                    Source::Url(_) => true,
                }
        });
    }

    /// Moves the selected entries `offset` rows (up when negative), keeping the gaps between
    /// them; the block stops at the list's ends (Webamp `dragSelected`, `moveSelected`).
    /// Returns the rows actually moved.
    pub fn move_selected(&mut self, offset: isize) -> isize {
        let selected: Vec<bool> = self.entries.iter().map(|e| e.selected).collect();
        let (Some(first), Some(last)) = (
            selected.iter().position(|s| *s),
            selected.iter().rposition(|s| *s),
        ) else {
            return 0;
        };
        let len = selected.len();
        let offset = offset.clamp(-(first as isize), (len - 1 - last) as isize);
        if offset == 0 {
            return 0;
        }
        let mut slots: Vec<Option<Entry>> = self.entries.drain(..).map(Some).collect();
        let mut unselected = (0..len).filter(|i| !selected[*i]);
        for i in 0..len {
            let from = i as isize - offset;
            let from = if (0..len as isize).contains(&from) && selected[from as usize] {
                from as usize
            } else {
                unselected.next().expect("a row for every unselected entry")
            };
            self.entries
                .push(slots[from].take().expect("each entry moves once"));
        }
        offset
    }

    /// Sorts by the title shown, ignoring case. Every sort is stable.
    pub fn sort_by_title(&mut self, template: &Template) {
        self.entries
            .sort_by_cached_key(|e| e.title(template).to_lowercase());
    }

    /// Sorts by file name, or URL for a stream, ignoring case.
    pub fn sort_by_file_name(&mut self) {
        self.entries.sort_by_cached_key(|e| match &e.source {
            Source::File(path) => path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .to_lowercase(),
            Source::Url(url) => url.to_lowercase(),
        });
    }

    pub fn sort_by_path(&mut self) {
        self.entries.sort_by_cached_key(|e| match &e.source {
            Source::File(path) => path_key(path),
            Source::Url(url) => vec![url.to_lowercase()],
        });
    }

    pub fn reverse(&mut self) {
        self.entries.reverse();
    }

    pub fn randomize(&mut self, rng: &mut Rng) {
        rng.shuffle(&mut self.entries);
    }

    /// Selects only this row, which becomes the anchor and the cursor.
    pub fn click(&mut self, index: usize) {
        let Some(id) = self.id_at(index) else {
            return;
        };
        for (i, e) in self.entries.iter_mut().enumerate() {
            e.selected = i == index;
        }
        self.anchor = Some(id);
        self.cursor = Some(id);
    }

    /// Toggles this row, which becomes the anchor and the cursor even when it is unselected
    /// (as Winamp 2 does).
    pub fn ctrl_click(&mut self, index: usize) {
        let Some(id) = self.id_at(index) else {
            return;
        };
        self.entries[index].selected = !self.entries[index].selected;
        self.anchor = Some(id);
        self.cursor = Some(id);
    }

    /// Selects only the rows from the anchor to this one; without an anchor, does nothing.
    pub fn shift_click(&mut self, index: usize) {
        let (Some(id), Some(anchor)) = (self.id_at(index), self.anchor_index()) else {
            return;
        };
        let (from, to) = (anchor.min(index), anchor.max(index));
        for (i, e) in self.entries.iter_mut().enumerate() {
            e.selected = (from..=to).contains(&i);
        }
        self.cursor = Some(id);
    }

    pub fn select_all(&mut self) {
        self.entries.iter_mut().for_each(|e| e.selected = true);
    }

    pub fn select_none(&mut self) {
        self.entries.iter_mut().for_each(|e| e.selected = false);
    }

    pub fn invert_selection(&mut self) {
        self.entries
            .iter_mut()
            .for_each(|e| e.selected = !e.selected);
    }

    /// The keyboard cursor's entry.
    pub fn cursor(&self) -> Option<EntryId> {
        self.cursor
    }

    pub fn cursor_index(&self) -> Option<usize> {
        self.index_of(self.cursor?)
    }

    /// Moves the keyboard cursor to row `to` (clamped to the last row). It selects that row
    /// alone, or with `extend` the rows from the anchor to it.
    pub fn move_cursor(&mut self, to: usize, extend: bool) {
        if self.entries.is_empty() {
            return;
        }
        let to = to.min(self.entries.len() - 1);
        if extend && self.anchor_index().is_some() {
            self.shift_click(to);
        } else {
            self.click(to);
        }
    }

    /// Keeps the entries `keep` accepts. A removed current entry hands over to the next
    /// entry kept after it; a removed anchor or cursor is cleared.
    fn retain(&mut self, keep: impl Fn(&Entry) -> bool) {
        let kept: Vec<bool> = self.entries.iter().map(keep).collect();
        let removed = |id: Option<EntryId>| {
            let i = self.index_of(id?)?;
            (!kept[i]).then_some(i)
        };
        let current = match self.current {
            Current::At(id) | Current::Removed(Some(id)) => removed(Some(id)).map(|i| {
                let after = (i + 1..kept.len()).find(|j| kept[*j]);
                Current::Removed(after.map(|j| self.entries[j].id()))
            }),
            _ => None,
        };
        let anchor_removed = removed(self.anchor).is_some();
        let cursor_removed = removed(self.cursor).is_some();
        if let Some(current) = current {
            self.current = current;
        }
        if anchor_removed {
            self.anchor = None;
        }
        if cursor_removed {
            self.cursor = None;
        }
        let mut kept = kept.into_iter();
        self.entries.retain(|_| kept.next().unwrap_or(true));
    }

    fn id_at(&self, index: usize) -> Option<EntryId> {
        self.entries.get(index).map(Entry::id)
    }

    fn anchor_index(&self) -> Option<usize> {
        self.index_of(self.anchor?)
    }
}

/// A sort key for paths: compared one component at a time, ignoring case, so `Live/` comes
/// before `Live 2001/`.
pub fn path_key(path: &Path) -> Vec<String> {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect()
}
```

Patch `crates/playlist/src/lib.rs` (apply with `git apply`):

```diff
diff --git a/crates/playlist/src/lib.rs b/crates/playlist/src/lib.rs
index 451ed43..089d2be 100644
--- a/crates/playlist/src/lib.rs
+++ b/crates/playlist/src/lib.rs
@@ -4,4 +4,5 @@
 
 pub mod entry;
 pub mod playlist;
+pub mod rng;
 pub mod title;
```

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p winrust-playlist`
Expected: PASS: `14 passed` (editing), `13 passed` (selection), `11 passed` (title).

- [ ] **Step 5: Lint and commit**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no output.

```bash
git add crates/playlist
git commit -m "feat(playlist): editing and sorting" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Play order

Spec §4.5: list order, or a shuffle permutation that plays every entry once. With repeat it is followed by a new one that never starts with the track just played (umbrella §9.3).

Edits keep the permutation valid: removed entries drop out, and new ones spread at random through the part not yet played. The ±10 jumps follow the list.

**Files:**
- Create: `crates/playlist/src/order.rs`
- Modify: `crates/playlist/src/lib.rs`
- Test: `crates/playlist/tests/order.rs`

**Interfaces:**
- Consumes: Task 1's `Playlist` and `EntryId`; Task 3's `Position` and `Rng`.
- Produces `order::PlayOrder` (`Clone`), with:
  - the public field `repeat: bool`;
  - `new(seed: u64)`, `shuffle() -> bool`, `set_shuffle(on: bool, &Playlist)`, `rng() -> &mut Rng`;
  - `first(&Playlist)`, `next(&Playlist)` and `previous(&Playlist)`, each `-> Option<EntryId>`; `next` only looks, and stays the same until something plays;
  - `jump(&Playlist, n: isize) -> Option<EntryId>`;
  - `played(&Playlist, EntryId)`, called whenever an entry becomes current.

- [ ] **Step 1: Write the failing tests**

Create `crates/playlist/tests/order.rs`:

```rust
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
    order.played(&pl, back);
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
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p winrust-playlist --test order`
Expected: compile error ``unresolved import `winrust_playlist::order` ``.

- [ ] **Step 3: Implement**

Create `crates/playlist/src/order.rs`:

```rust
//! The play order (spec §4.5): list order or a shuffle permutation, repeat, and the ±10
//! jumps. Pure and seeded, so tests repeat exactly.

use crate::entry::{Entry, EntryId};
use crate::playlist::{Playlist, Position};
use crate::rng::Rng;
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub struct PlayOrder {
    /// Wrap around the whole list (Winamp 2): in list order, or into a new permutation.
    pub repeat: bool,
    shuffle: bool,
    /// The shuffle permutation. `perm[..played]` have played, the last of them most recently.
    perm: Vec<EntryId>,
    played: usize,
    /// With repeat on, the permutation after this one, drawn when first asked for.
    next_round: Option<Vec<EntryId>>,
    rng: Rng,
}

impl PlayOrder {
    pub fn new(seed: u64) -> Self {
        PlayOrder {
            repeat: false,
            shuffle: false,
            perm: Vec::new(),
            played: 0,
            next_round: None,
            rng: Rng::new(seed),
        }
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    /// Turning shuffle on draws a new permutation that starts with the current entry.
    pub fn set_shuffle(&mut self, on: bool, pl: &Playlist) {
        self.shuffle = on;
        if on {
            self.perm = pl.entries().iter().map(Entry::id).collect();
            self.rng.shuffle(&mut self.perm);
            self.played = 0;
            self.next_round = None;
            if let Some(current) = pl.current() {
                self.played(pl, current);
            }
        }
    }

    /// The generator, for randomizing the list.
    pub fn rng(&mut self) -> &mut Rng {
        &mut self.rng
    }

    /// Where Play starts when nothing is current: the first entry, the entry after a removed
    /// current one, or with shuffle the next in the permutation.
    pub fn first(&mut self, pl: &Playlist) -> Option<EntryId> {
        if self.shuffle {
            return self.next(pl);
        }
        let index = match pl.position() {
            Position::At(i) => i,
            Position::Before(i) if i < pl.len() => i,
            _ => 0,
        };
        pl.entries().get(index).map(Entry::id)
    }

    /// The entry after the current one; `None` at the end without repeat.
    pub fn next(&mut self, pl: &Playlist) -> Option<EntryId> {
        if !self.shuffle {
            return self.list_step(pl, true);
        }
        self.sync(pl);
        if let Some(&id) = self.perm.get(self.played) {
            return Some(id);
        }
        if !self.repeat || self.perm.is_empty() {
            return None;
        }
        let last = self.played.checked_sub(1).map(|i| self.perm[i]);
        let round = self.next_round.get_or_insert_with(|| {
            let mut round = self.perm.clone();
            self.rng.shuffle(&mut round);
            if round.len() > 1 && Some(round[0]) == last {
                // Never the track that just played (umbrella §9.3).
                let other = 1 + self.rng.below(round.len() - 1);
                round.swap(0, other);
            }
            round
        });
        round.first().copied()
    }

    /// The entry before the current one: in list order, or back through what played.
    pub fn previous(&mut self, pl: &Playlist) -> Option<EntryId> {
        if !self.shuffle {
            return self.list_step(pl, false);
        }
        self.sync(pl);
        self.played.checked_sub(2).map(|i| self.perm[i])
    }

    /// The entry `n` rows away in list order, even with shuffle on (the ±10 jumps). With
    /// repeat it wraps; otherwise it clamps, and gives `None` when already at that end.
    pub fn jump(&self, pl: &Playlist, n: isize) -> Option<EntryId> {
        let len = pl.len() as isize;
        let id = |i: isize| Some(pl.entries()[i as usize].id());
        if len == 0 {
            return None;
        }
        let from = match pl.position() {
            Position::Start => return id(0),
            Position::At(i) | Position::Before(i) => (i as isize).min(len - 1),
        };
        let to = from + n;
        if self.repeat {
            id(to.rem_euclid(len))
        } else if (from == len - 1 && n > 0) || (from == 0 && n < 0) {
            None
        } else {
            id(to.clamp(0, len - 1))
        }
    }

    /// An entry became current (it was loaded, or started gaplessly). With shuffle it counts
    /// as played: taken from the part not yet played, or, when it played already, the
    /// history goes back to it.
    pub fn played(&mut self, pl: &Playlist, id: EntryId) {
        if !self.shuffle {
            return;
        }
        self.sync(pl);
        if let Some(round) = self.next_round.take_if(|r| r.first() == Some(&id)) {
            self.perm = round;
            self.played = 1;
            return;
        }
        match self.perm.iter().position(|x| *x == id) {
            Some(i) if i < self.played => self.played = i + 1,
            Some(i) => {
                self.perm[self.played..=i].rotate_right(1);
                self.played += 1;
            }
            None => {}
        }
    }

    fn list_step(&self, pl: &Playlist, forward: bool) -> Option<EntryId> {
        let len = pl.len();
        let target = match pl.position() {
            Position::Start => Some(0),
            Position::At(i) if forward => Some(i + 1),
            Position::Before(i) if forward => Some(i),
            Position::At(i) | Position::Before(i) => i.checked_sub(1),
        };
        let index = match target.filter(|t| *t < len) {
            Some(t) => t,
            None if self.repeat && len > 0 => {
                if forward {
                    0
                } else {
                    len - 1
                }
            }
            None => return None,
        };
        Some(pl.entries()[index].id())
    }

    /// Keeps the permutation valid after edits: removed entries drop out, and new ones are
    /// spread at random through the part not yet played.
    fn sync(&mut self, pl: &Playlist) {
        let ids: HashSet<EntryId> = pl.entries().iter().map(Entry::id).collect();
        let before = self.perm.len();
        let gone_played = self.perm[..self.played]
            .iter()
            .filter(|id| !ids.contains(id))
            .count();
        self.perm.retain(|id| ids.contains(id));
        self.played -= gone_played;
        let known: HashSet<EntryId> = self.perm.iter().copied().collect();
        let mut new: Vec<EntryId> = pl
            .entries()
            .iter()
            .map(Entry::id)
            .filter(|id| !known.contains(id))
            .collect();
        if self.perm.len() == before && new.is_empty() {
            return;
        }
        self.next_round = None;
        self.rng.shuffle(&mut new);
        // A uniformly random interleaving of the unplayed part and the new entries.
        let mut old = self.perm.split_off(self.played).into_iter().peekable();
        let mut new = new.into_iter().peekable();
        while old.peek().is_some() || new.peek().is_some() {
            let (o, n) = (old.len(), new.len());
            let pick_new = self.rng.below(o + n) < n;
            let next = if pick_new { new.next() } else { old.next() };
            self.perm.extend(next);
        }
    }
}
```

Patch `crates/playlist/src/lib.rs` (apply with `git apply`):

```diff
diff --git a/crates/playlist/src/lib.rs b/crates/playlist/src/lib.rs
index 089d2be..4979b80 100644
--- a/crates/playlist/src/lib.rs
+++ b/crates/playlist/src/lib.rs
@@ -3,6 +3,7 @@
 //! touches the disk.
 
 pub mod entry;
+pub mod order;
 pub mod playlist;
 pub mod rng;
 pub mod title;
```

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p winrust-playlist`
Expected: PASS: `14 passed` (editing), `14 passed` (order), `13 passed` (selection), `11 passed` (title).

- [ ] **Step 5: Lint and commit**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no output.

```bash
git add crates/playlist
git commit -m "feat(playlist): play order with a real shuffle" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Playback cursor

M3's `app::queue` bookkeeping, walking the play order (spec §4.6):
- the current track and the gapless `QueueNext` track;
- `Unqueue` when settings or the list change;
- stop-after-current;
- skipping entries the engine cannot open.

Three changes from M3:
- streams are never queued gaplessly;
- the skip guard counts distinct failed entries;
- failed entries are marked dead.

The tests port M3's queue tests.

**Files:**
- Create: `crates/playlist/src/cursor.rs`
- Modify: `crates/playlist/src/lib.rs`
- Test: `crates/playlist/tests/cursor.rs`

**Interfaces:**
- Consumes: Tasks 1, 3 and 4 (`Playlist`, `PlayOrder`); `winrust_audio::Source`.
- Produces:
  - `cursor::Requeue { Queue(EntryId), Unqueue, Keep }`;
  - `cursor::Cursor`, with:
    - public fields `order: PlayOrder` and `stop_after: bool`;
    - `new(seed: u64)`;
    - `play`, `next` and `previous`, each `(&mut Playlist) -> Option<EntryId>`;
    - `play_entry(&mut Playlist, EntryId)` and `jump(&mut Playlist, n: isize)`, each `-> Option<EntryId>`;
    - `stop()`;
    - `on_started(&mut Playlist, gapless: bool)`, `on_error(&mut Playlist, &Source)` and `on_ended(&mut Playlist)`, each `-> Option<EntryId>`;
    - `requeue(&Playlist) -> Requeue`;
  - every `Option<EntryId>` names an entry to `Load`, except `on_started`, whose entry goes to `QueueNext`. Each call that loads also makes the entry current.

- [ ] **Step 1: Write the failing tests**

Create `crates/playlist/tests/cursor.rs`:

```rust
//! Ported from M3's `app::queue` tests, plus the spec §4.6 changes.

mod common;

use common::{file, names, playlist};
use winrust_audio::Source;
use winrust_playlist::cursor::{Cursor, Requeue};
use winrust_playlist::entry::EntryId;
use winrust_playlist::playlist::Playlist;

/// The playlist `0.mp3`… and a cursor that has loaded the first entry.
fn started(n: usize) -> (Playlist, Cursor) {
    let mut pl = playlist(n);
    let mut c = Cursor::new(7);
    let first = c.play(&mut pl);
    assert_eq!(first.is_some(), n > 0);
    (pl, c)
}

/// The row of the entry that a cursor call returns.
macro_rules! row {
    ($pl:ident, $call:expr) => {{
        let id: Option<EntryId> = $call;
        id.and_then(|id| $pl.index_of(id))
    }};
}

/// The engine cannot open the entry at `index`.
fn fail(pl: &mut Playlist, c: &mut Cursor, index: usize) -> Option<EntryId> {
    let source = pl.entries()[index].source.clone();
    c.on_error(pl, &source)
}

fn select(pl: &mut Playlist, rows: &[usize]) {
    pl.select_none();
    for &r in rows {
        pl.ctrl_click(r);
    }
}

#[test]
fn plays_through_gaplessly_then_stops() {
    let (mut pl, mut c) = started(3);
    assert_eq!(row!(pl, c.on_started(&mut pl, false)), Some(1));
    assert_eq!(row!(pl, c.on_started(&mut pl, true)), Some(2));
    assert_eq!(pl.current_index(), Some(1));
    assert_eq!(c.on_started(&mut pl, true), None);
    assert_eq!(c.on_ended(&mut pl), None);
    assert_eq!(pl.current_index(), Some(2));
}

#[test]
fn an_empty_list_plays_nothing() {
    let (mut pl, mut c) = started(0);
    assert_eq!(c.play(&mut pl), None);
    assert_eq!(c.next(&mut pl), None);
}

#[test]
fn repeat_wraps_both_ways() {
    let (mut pl, mut c) = started(3);
    c.order.repeat = true;
    assert_eq!(row!(pl, c.previous(&mut pl)), Some(2));
    assert_eq!(row!(pl, c.on_started(&mut pl, false)), Some(0));
    assert_eq!(row!(pl, c.next(&mut pl)), Some(0));
}

#[test]
fn without_repeat_the_ends_stop() {
    // Webamp `getNextTrackId`: no previous before the first entry, no next after the last.
    let (mut pl, mut c) = started(3);
    assert_eq!(c.previous(&mut pl), None);
    let (mut pl, mut c) = started(3);
    c.jump(&mut pl, 2);
    assert_eq!(c.next(&mut pl), None);
}

#[test]
fn jumps_clamp_to_the_ends() {
    let (mut pl, mut c) = started(25);
    c.jump(&mut pl, 3);
    assert_eq!(row!(pl, c.jump(&mut pl, -10)), Some(0));
    assert_eq!(row!(pl, c.jump(&mut pl, 10)), Some(10));
    assert_eq!(row!(pl, c.jump(&mut pl, 10)), Some(20));
    assert_eq!(row!(pl, c.jump(&mut pl, 10)), Some(24));
}

#[test]
fn shuffle_plays_every_entry_before_repeating() {
    let (mut pl, mut c) = started(5);
    c.order.repeat = true;
    c.order.set_shuffle(true, &pl);
    let mut played = vec![pl.current_index().unwrap()];
    for _ in 0..19 {
        let next = c.on_started(&mut pl, false).unwrap();
        played.push(row!(pl, Some(next)).unwrap());
        c.on_ended(&mut pl);
    }
    for round in played.chunks(5) {
        let mut sorted = round.to_vec();
        sorted.sort();
        assert_eq!(sorted, [0, 1, 2, 3, 4], "{played:?}");
    }
}

#[test]
fn shuffle_and_repeat_with_one_entry_replay_it() {
    let (mut pl, mut c) = started(1);
    c.order.repeat = true;
    c.order.set_shuffle(true, &pl);
    assert_eq!(row!(pl, c.on_started(&mut pl, false)), Some(0));
}

#[test]
fn an_unreadable_current_entry_is_skipped_and_marked_dead() {
    let (mut pl, mut c) = started(3);
    assert_eq!(row!(pl, fail(&mut pl, &mut c, 0)), Some(1));
    assert!(pl.entries()[0].dead);
    assert_eq!(row!(pl, c.on_started(&mut pl, false)), Some(2));
}

#[test]
fn an_unreadable_queued_entry_is_skipped_at_the_end() {
    let (mut pl, mut c) = started(3);
    assert_eq!(row!(pl, c.on_started(&mut pl, false)), Some(1));
    assert_eq!(fail(&mut pl, &mut c, 1), None, "the current track plays on");
    assert_eq!(row!(pl, c.on_ended(&mut pl)), Some(2));
    assert!(pl.entries()[1].dead);
}

#[test]
fn a_dead_entry_that_plays_after_all_comes_back_to_life() {
    let (mut pl, mut c) = started(2);
    fail(&mut pl, &mut c, 0);
    let first = pl.entries()[0].id();
    c.play_entry(&mut pl, first);
    c.on_started(&mut pl, false);
    assert!(!pl.entries()[0].dead);
}

#[test]
fn errors_about_other_sources_change_nothing() {
    let (mut pl, mut c) = started(2);
    let other = Source::File("9.mp3".into());
    assert_eq!(c.on_error(&mut pl, &other), None);
    assert_eq!(row!(pl, c.on_started(&mut pl, false)), Some(1));
}

#[test]
fn every_entry_unreadable_stops_even_with_repeat() {
    let (mut pl, mut c) = started(3);
    c.order.repeat = true;
    assert_eq!(row!(pl, fail(&mut pl, &mut c, 0)), Some(1));
    assert_eq!(row!(pl, fail(&mut pl, &mut c, 1)), Some(2));
    assert_eq!(fail(&mut pl, &mut c, 2), None, "all three failed");
}

#[test]
fn with_shuffle_skipping_goes_on_until_every_entry_failed() {
    // M3 counted failures, so a shuffle revisiting a bad entry could stop before the good one.
    for seed in 0..40 {
        let mut pl = playlist(5);
        let mut c = Cursor::new(seed);
        c.order.repeat = true;
        c.order.set_shuffle(true, &pl);
        let good = pl.entries()[2].id();
        let mut id = c.play(&mut pl);
        for step in 0..100 {
            let Some(now) = id else {
                panic!("seed {seed}: stopped at step {step} although 2.mp3 plays");
            };
            id = if now == good {
                c.on_started(&mut pl, false);
                c.on_ended(&mut pl)
            } else {
                let bad = pl.get(now).unwrap().source.clone();
                c.on_error(&mut pl, &bad)
            };
        }
    }
}

#[test]
fn streams_are_never_queued_gaplessly() {
    let mut pl = playlist(1);
    pl.append(vec![
        Source::Url("http://radio.example/".into()).into(),
        file("2.mp3"),
    ]);
    let mut c = Cursor::new(7);
    c.play(&mut pl);
    assert_eq!(c.on_started(&mut pl, false), None, "the stream is next");
    assert_eq!(row!(pl, c.on_ended(&mut pl)), Some(1), "loaded at the end");
    assert_eq!(row!(pl, c.on_started(&mut pl, false)), Some(2));
}

#[test]
fn a_track_ending_before_the_queue_arrives_plays_the_queued_one() {
    let (mut pl, mut c) = started(3);
    assert_eq!(row!(pl, c.on_started(&mut pl, false)), Some(1));
    assert_eq!(row!(pl, c.on_ended(&mut pl)), Some(1));
}

#[test]
fn entries_added_after_the_last_play_when_it_ends() {
    let (mut pl, mut c) = started(1);
    assert_eq!(c.on_started(&mut pl, false), None);
    pl.append(vec![file("new.mp3")]);
    assert_eq!(row!(pl, c.on_ended(&mut pl)), Some(1));
}

#[test]
fn stop_after_current_queues_nothing_and_clears_itself() {
    let (mut pl, mut c) = started(3);
    c.stop_after = true;
    assert_eq!(c.on_started(&mut pl, false), None);
    assert_eq!(c.on_ended(&mut pl), None);
    assert!(!c.stop_after);
    assert_eq!(row!(pl, c.play(&mut pl)), Some(0));
}

#[test]
fn requeue_follows_repeat_and_stop_after() {
    let (mut pl, mut c) = started(2);
    c.next(&mut pl);
    assert_eq!(c.on_started(&mut pl, false), None, "last entry, no repeat");
    c.order.repeat = true;
    let first = pl.entries()[0].id();
    assert_eq!(c.requeue(&pl), Requeue::Queue(first));
    assert_eq!(c.requeue(&pl), Requeue::Keep, "already queued");
    c.stop_after = true;
    assert_eq!(c.requeue(&pl), Requeue::Unqueue);
    assert_eq!(c.requeue(&pl), Requeue::Keep);
}

#[test]
fn requeue_waits_for_the_track_to_start() {
    let (pl, mut c) = started(2);
    c.order.repeat = true;
    assert_eq!(c.requeue(&pl), Requeue::Keep);
}

#[test]
fn requeue_follows_edits_to_the_list() {
    let (mut pl, mut c) = started(3);
    c.on_started(&mut pl, false);
    select(&mut pl, &[1]);
    pl.remove_selected();
    let two = pl.entries()[1].id();
    assert_eq!(c.requeue(&pl), Requeue::Queue(two));
    select(&mut pl, &[1]);
    pl.remove_selected();
    assert_eq!(c.requeue(&pl), Requeue::Unqueue);
}

#[test]
fn removing_the_playing_entry_keeps_the_next_one_queued() {
    let (mut pl, mut c) = started(4);
    c.jump(&mut pl, 1);
    let two = c.on_started(&mut pl, false);
    select(&mut pl, &[1]);
    pl.remove_selected();
    assert_eq!(c.requeue(&pl), Requeue::Keep);
    c.on_started(&mut pl, true);
    assert_eq!(pl.current(), two);
    assert_eq!(names(&pl)[pl.current_index().unwrap()], "2.mp3");
}

#[test]
fn stop_keeps_the_current_entry_for_play() {
    let (mut pl, mut c) = started(3);
    c.next(&mut pl);
    c.on_started(&mut pl, false);
    c.stop();
    assert_eq!(c.requeue(&pl), Requeue::Keep, "nothing plays");
    assert_eq!(row!(pl, c.play(&mut pl)), Some(1));
}

#[test]
fn play_entry_plays_that_entry() {
    let (mut pl, mut c) = started(3);
    let two = pl.entries()[2].id();
    assert_eq!(c.play_entry(&mut pl, two), Some(two));
    assert_eq!(pl.current(), Some(two));
}

#[test]
fn play_after_replacing_the_list_starts_at_its_first_entry() {
    let (mut pl, mut c) = started(3);
    c.jump(&mut pl, 2);
    pl.replace(vec![file("a"), file("b")]);
    assert_eq!(row!(pl, c.play(&mut pl)), Some(0));
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p winrust-playlist --test cursor`
Expected: compile error ``unresolved import `winrust_playlist::cursor` ``.

- [ ] **Step 3: Implement**

Create `crates/playlist/src/cursor.rs`:

```rust
//! Playback bookkeeping (spec §4.6): the entry playing, the one handed to the engine early
//! with `QueueNext` for a gapless start, stop-after-current, and skipping entries the engine
//! cannot open. Ported from M3's `app::queue`, walking the play order. Pure: the app turns
//! its answers into engine commands.

use crate::entry::EntryId;
use crate::order::PlayOrder;
use crate::playlist::Playlist;
use std::collections::HashSet;
use winrust_audio::Source;

/// What the engine's queued track should become after a setting or the list changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Requeue {
    /// Send this entry with `QueueNext`.
    Queue(EntryId),
    /// Send `Unqueue`.
    Unqueue,
    Keep,
}

#[derive(Clone, Debug)]
pub struct Cursor {
    /// Shuffle and repeat.
    pub order: PlayOrder,
    /// The entry handed to the engine with `QueueNext`.
    queued: Option<EntryId>,
    /// The engine reported that the queued entry cannot be opened.
    queued_failed: bool,
    /// The current entry was loaded and has not started yet.
    awaiting_start: bool,
    /// The engine has a track loaded (playing, paused or starting).
    playing: bool,
    /// Entries that failed since one last started; when every entry has, playback stops.
    failed: HashSet<EntryId>,
    /// Stop when the current track ends (Ctrl+V).
    pub stop_after: bool,
}

impl Cursor {
    /// `seed` drives the shuffle order.
    pub fn new(seed: u64) -> Self {
        Cursor {
            order: PlayOrder::new(seed),
            queued: None,
            queued_failed: false,
            awaiting_start: false,
            playing: false,
            failed: HashSet::new(),
            stop_after: false,
        }
    }

    /// Play: the current entry, else where the play order starts. Returns the entry to load.
    pub fn play(&mut self, pl: &mut Playlist) -> Option<EntryId> {
        let id = pl.current().or_else(|| self.order.first(pl))?;
        self.failed.clear();
        self.load(pl, id)
    }

    /// Plays this entry (a double-click, Enter).
    pub fn play_entry(&mut self, pl: &mut Playlist, id: EntryId) -> Option<EntryId> {
        pl.index_of(id)?;
        self.failed.clear();
        self.load(pl, id)
    }

    /// The next entry in play order, to load; `None` past the end without repeat.
    pub fn next(&mut self, pl: &mut Playlist) -> Option<EntryId> {
        let id = self.order.next(pl)?;
        self.failed.clear();
        self.load(pl, id)
    }

    pub fn previous(&mut self, pl: &mut Playlist) -> Option<EntryId> {
        let id = self.order.previous(pl)?;
        self.failed.clear();
        self.load(pl, id)
    }

    /// The ±10 jumps, in list order.
    pub fn jump(&mut self, pl: &mut Playlist, n: isize) -> Option<EntryId> {
        let id = self.order.jump(pl, n)?;
        self.failed.clear();
        self.load(pl, id)
    }

    /// Playback stopped; the current entry stays for Play.
    pub fn stop(&mut self) {
        self.queued = None;
        self.queued_failed = false;
        self.awaiting_start = false;
        self.playing = false;
        self.stop_after = false;
    }

    /// A track became audible. Returns the entry to hand to the engine with `QueueNext`.
    pub fn on_started(&mut self, pl: &mut Playlist, gapless: bool) -> Option<EntryId> {
        if gapless && let Some(q) = self.queued.take() {
            pl.set_current(Some(q));
            self.order.played(pl, q);
        }
        if let Some(entry) = pl.current().and_then(|id| pl.get_mut(id)) {
            entry.dead = false;
        }
        self.awaiting_start = false;
        self.failed.clear();
        self.queue_next(pl)
    }

    /// The engine reported an error about `source`. Returns the entry to load instead.
    pub fn on_error(&mut self, pl: &mut Playlist, source: &Source) -> Option<EntryId> {
        let is = |id: Option<EntryId>| {
            id.and_then(|id| pl.get(id))
                .is_some_and(|e| e.source == *source)
        };
        if is(self.queued) {
            self.queued_failed = true;
            return None;
        }
        match pl.current() {
            Some(id) if self.awaiting_start && is(Some(id)) => self.skip(pl, id),
            _ => None,
        }
    }

    /// The engine stopped at the end of its last track. Returns the entry to load.
    pub fn on_ended(&mut self, pl: &mut Playlist) -> Option<EntryId> {
        if self.stop_after {
            self.stop();
            return None;
        }
        let next = match self.queued.take() {
            Some(q) if self.queued_failed => return self.skip(pl, q),
            // The engine finished before it saw the queued track: play it now.
            Some(q) => Some(q),
            // Nothing was queued: a stream is next, or entries were added since.
            None => self.order.next(pl),
        };
        match next {
            Some(id) => self.load(pl, id),
            None => {
                self.stop();
                None
            }
        }
    }

    /// Shuffle, repeat, stop-after or the list changed: what the engine's queued track
    /// becomes.
    pub fn requeue(&mut self, pl: &Playlist) -> Requeue {
        if !self.playing || self.awaiting_start {
            return Requeue::Keep;
        }
        let (before, failed) = (self.queued, self.queued_failed);
        match self.queue_next(pl) {
            Some(next) if before == Some(next) => {
                self.queued_failed = failed;
                Requeue::Keep
            }
            Some(next) => Requeue::Queue(next),
            None if before.is_some() => Requeue::Unqueue,
            None => Requeue::Keep,
        }
    }

    fn queue_next(&mut self, pl: &Playlist) -> Option<EntryId> {
        self.queued = None;
        self.queued_failed = false;
        if self.stop_after {
            return None;
        }
        let next = self.order.next(pl)?;
        // Gapless is for local files (umbrella §6.3): a stream loads when this track ends.
        if matches!(pl.get(next)?.source, Source::Url(_)) {
            return None;
        }
        self.queued = Some(next);
        self.queued
    }

    /// Marks an entry the engine cannot open as dead and moves past it, unless every entry
    /// has failed since one last played.
    fn skip(&mut self, pl: &mut Playlist, failed: EntryId) -> Option<EntryId> {
        if let Some(entry) = pl.get_mut(failed) {
            entry.dead = true;
        }
        self.failed.insert(failed);
        pl.set_current(Some(failed));
        self.order.played(pl, failed);
        let all_failed = pl.entries().iter().all(|e| self.failed.contains(&e.id()));
        match self.order.next(pl).filter(|_| !all_failed) {
            Some(next) => self.load(pl, next),
            None => {
                self.stop();
                None
            }
        }
    }

    fn load(&mut self, pl: &mut Playlist, id: EntryId) -> Option<EntryId> {
        pl.set_current(Some(id));
        self.order.played(pl, id);
        self.queued = None;
        self.queued_failed = false;
        self.awaiting_start = true;
        self.playing = true;
        Some(id)
    }
}
```

Patch `crates/playlist/src/lib.rs` (apply with `git apply`):

```diff
diff --git a/crates/playlist/src/lib.rs b/crates/playlist/src/lib.rs
index 4979b80..4c31913 100644
--- a/crates/playlist/src/lib.rs
+++ b/crates/playlist/src/lib.rs
@@ -2,6 +2,7 @@
 //! templates and tags. No windowing or OS API; only reading playlist files, folders and tags
 //! touches the disk.
 
+pub mod cursor;
 pub mod entry;
 pub mod order;
 pub mod playlist;
```

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p winrust-playlist`
Expected: PASS: `24 passed` (cursor), `14 passed` (editing), `14 passed` (order), `13 passed` (selection), `11 passed` (title).

- [ ] **Step 5: Lint and commit**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no output.

```bash
git add crates/playlist
git commit -m "feat(playlist): playback cursor" -m "Ported from M3's queue: streams are never queued gaplessly, the skip guard counts distinct failed entries, and failed entries are marked dead." -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Playlist files

Spec §4.7: read M3U, M3U8 and PLS; write M3U8 and PLS version 2, with paths relative to an optional base folder. A fuzz target covers the parsers and templates.

**Files:**
- Create: `crates/playlist/src/files.rs`, `fuzz/fuzz_targets/playlist_file.rs`
- Modify: `crates/playlist/Cargo.toml`, `crates/playlist/src/lib.rs`, `fuzz/Cargo.toml`, `.github/workflows/fuzz.yml`
- Test: `crates/playlist/tests/files.rs`

**Interfaces:**
- Consumes: Task 1's `Item`, `Entry` and `Playlist`; Task 2's `Template` and `Entry::title`.
- Produces:
  - `files::read(&Path) -> io::Result<Vec<Item>>`;
  - `files::decode(&[u8], legacy: bool) -> String`;
  - `files::parse_m3u(&str, base: &Path) -> Vec<Item>` and `files::parse_pls(&str, base: &Path) -> Vec<Item>`;
  - `files::write_m3u8(&[Entry], &Template, base: Option<&Path>) -> String` and `files::write_pls(…) -> String`, with the same arguments.

- [ ] **Step 1: Write the failing tests**

Patch `crates/playlist/Cargo.toml` (apply with `git apply`):

```diff
diff --git a/crates/playlist/Cargo.toml b/crates/playlist/Cargo.toml
index 4551ab2..4900b26 100644
--- a/crates/playlist/Cargo.toml
+++ b/crates/playlist/Cargo.toml
@@ -7,4 +7,8 @@ license.workspace = true
 publish.workspace = true
 
 [dependencies]
+url = "2.5.8"
 winrust-audio = { path = "../audio" }
+
+[dev-dependencies]
+tempfile = "3"
```

Create `crates/playlist/tests/files.rs`:

```rust
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use winrust_audio::Source;
use winrust_playlist::entry::{Item, Tags};
use winrust_playlist::files::{decode, parse_m3u, parse_pls, read, write_m3u8, write_pls};
use winrust_playlist::playlist::Playlist;
use winrust_playlist::title::Template;

fn base() -> PathBuf {
    PathBuf::from("lists")
}

fn item(source: Source, hint: Option<&str>, secs: Option<u64>) -> Item {
    Item {
        source,
        hint: hint.map(String::from),
        duration: secs.map(Duration::from_secs),
    }
}

fn local(name: &str) -> Source {
    Source::File(base().join(name))
}

#[test]
fn m3u_reads_extinf_titles_and_durations() {
    let text = "#EXTM3U\n#EXTINF:123,Artist - Song\nsong.mp3\n#EXTINF:-1,Radio\nhttp://radio.example/live\nplain.mp3\n";
    assert_eq!(
        parse_m3u(text, &base()),
        [
            item(local("song.mp3"), Some("Artist - Song"), Some(123)),
            item(
                Source::Url("http://radio.example/live".into()),
                Some("Radio"),
                None
            ),
            item(local("plain.mp3"), None, None),
        ]
    );
}

#[test]
fn an_extinf_line_describes_only_the_next_location() {
    let text = "#EXTINF:5,A\r\n\r\n# a comment\r\na.mp3\r\nb.mp3\r\n";
    assert_eq!(
        parse_m3u(text, &base()),
        [
            item(local("a.mp3"), Some("A"), Some(5)),
            item(local("b.mp3"), None, None),
        ]
    );
}

#[test]
fn extinf_attributes_and_odd_numbers_are_tolerated() {
    let text = "#extinf:-1 tvg-id=\"x\",Name\nhttp://tv.example/\n#EXTINF:1e300,Huge\nhuge.mp3\n#EXTINF:abc\nbad.mp3\n";
    assert_eq!(
        parse_m3u(text, &base()),
        [
            item(Source::Url("http://tv.example/".into()), Some("Name"), None),
            item(local("huge.mp3"), Some("Huge"), None),
            item(local("bad.mp3"), None, None),
        ]
    );
}

#[test]
fn pls_reads_entries_in_number_order_whatever_the_key_case() {
    let text = "[playlist]\nfile2=b.mp3\nFILE1=a.mp3\nTitle1=A\nlength1=60\nFile4=d.mp3\nTitle3=no file\nNumberOfEntries=3\nVersion=2\n";
    assert_eq!(
        parse_pls(text, &base()),
        [
            item(local("a.mp3"), Some("A"), Some(60)),
            item(local("b.mp3"), None, None),
            item(local("d.mp3"), None, None),
        ]
    );
}

#[test]
fn absolute_paths_stay_and_relative_ones_join_the_playlist_folder() {
    let dir = tempfile::tempdir().unwrap();
    let absolute = dir.path().join("x.mp3");
    let text = format!("{}\nsub/a.mp3\n", absolute.display());
    assert_eq!(
        parse_m3u(&text, &base()),
        [
            item(Source::File(absolute), None, None),
            item(Source::File(base().join("sub").join("a.mp3")), None, None),
        ]
    );
}

#[test]
fn backslashes_separate_folders_on_every_os() {
    assert_eq!(
        parse_m3u("sub\\a.mp3", &base()),
        [item(
            Source::File(base().join("sub").join("a.mp3")),
            None,
            None
        )]
    );
}

#[test]
fn file_urls_become_paths_and_stream_urls_pass_through() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a b.mp3");
    let url = url::Url::from_file_path(&path).unwrap();
    let text = format!("{url}\nHTTPS://radio.example/x\nicy://radio.example/y\n");
    assert_eq!(
        parse_m3u(&text, &base()),
        [
            item(Source::File(path), None, None),
            item(Source::Url("HTTPS://radio.example/x".into()), None, None),
            item(Source::Url("icy://radio.example/y".into()), None, None),
        ]
    );
}

#[test]
fn text_is_utf8_after_a_byte_order_mark() {
    assert_eq!(decode(b"\xEF\xBB\xBFcaf\xC3\xA9", true), "café");
}

#[test]
fn a_legacy_m3u_that_is_not_utf8_is_windows_1252() {
    assert_eq!(decode(b"caf\xE9 \x80 \x93x\x94", true), "café € “x”");
    assert_eq!(decode(b"caf\xE9", false), "caf\u{FFFD}");
}

#[test]
fn read_picks_the_format_and_encoding_by_extension() {
    let dir = tempfile::tempdir().unwrap();
    let m3u = dir.path().join("old.M3U");
    fs::write(&m3u, b"#EXTINF:1,Caf\xE9\nsong.mp3\n").unwrap();
    let pls = dir.path().join("list.pls");
    fs::write(&pls, "[playlist]\nFile1=a.mp3\n").unwrap();
    assert_eq!(
        read(&m3u).unwrap(),
        [item(
            Source::File(dir.path().join("song.mp3")),
            Some("Café"),
            Some(1)
        )]
    );
    assert_eq!(
        read(&pls).unwrap(),
        [item(Source::File(dir.path().join("a.mp3")), None, None)]
    );
    assert!(read(&dir.path().join("missing.m3u8")).is_err());
}

/// A playlist with a tagged file in `dir`, an untagged one in a subfolder, a file elsewhere
/// and a stream.
fn sample(dir: &Path, elsewhere: &Path) -> Playlist {
    let mut pl = Playlist::new();
    pl.append(vec![
        item(Source::File(dir.join("a.mp3")), None, Some(61)),
        item(Source::File(dir.join("sub").join("b.mp3")), None, None),
        item(Source::File(elsewhere.join("c.mp3")), None, Some(5)),
        item(Source::Url("http://radio.example/".into()), None, None),
    ]);
    let first = pl.entries()[0].id();
    pl.get_mut(first).unwrap().tags = Some(Tags {
        artist: Some("Artist".into()),
        title: Some("Line\nbreak".into()),
        ..Tags::default()
    });
    pl
}

#[test]
fn m3u8_is_written_with_relative_paths_under_the_base() {
    let dir = PathBuf::from("music");
    let pl = sample(&dir, Path::new("/elsewhere"));
    let text = write_m3u8(&pl.entries()[..1], &Template::default(), Some(&dir));
    assert_eq!(text, "#EXTM3U\n#EXTINF:61,Artist - Line break\na.mp3\n");
    let text = write_m3u8(&pl.entries()[3..], &Template::default(), None);
    assert_eq!(
        text,
        "#EXTM3U\n#EXTINF:-1,http://radio.example/\nhttp://radio.example/\n"
    );
}

#[test]
fn m3u8_and_pls_read_back_what_was_written() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let pl = sample(dir.path(), elsewhere.path());
    let t = Template::default();
    let expected: Vec<Item> = pl
        .entries()
        .iter()
        .map(|e| Item {
            source: e.source.clone(),
            hint: Some(e.title(&t).replace('\n', " ")),
            duration: e.duration,
        })
        .collect();
    for base in [Some(dir.path()), None] {
        let m3u8 = write_m3u8(pl.entries(), &t, base);
        assert_eq!(parse_m3u(&m3u8, dir.path()), expected, "{m3u8}");
        let pls = write_pls(pl.entries(), &t, base);
        assert_eq!(parse_pls(&pls, dir.path()), expected, "{pls}");
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p winrust-playlist --test files`
Expected: compile error ``unresolved import `winrust_playlist::files` ``.

- [ ] **Step 3: Implement**

Create `crates/playlist/src/files.rs`:

```rust
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
```

Patch `crates/playlist/src/lib.rs` (apply with `git apply`):

```diff
diff --git a/crates/playlist/src/lib.rs b/crates/playlist/src/lib.rs
index 4c31913..8a0e062 100644
--- a/crates/playlist/src/lib.rs
+++ b/crates/playlist/src/lib.rs
@@ -4,6 +4,7 @@
 
 pub mod cursor;
 pub mod entry;
+pub mod files;
 pub mod order;
 pub mod playlist;
 pub mod rng;
```

Create `fuzz/fuzz_targets/playlist_file.rs`:

```rust
#![no_main]

use libfuzzer_sys::fuzz_target;
use std::path::Path;
use winrust_playlist::entry::Tags;
use winrust_playlist::files::{decode, parse_m3u, parse_pls};
use winrust_playlist::title::Template;

// The same bytes as an M3U file, a PLS file and a title template.
fuzz_target!(|data: &[u8]| {
    let text = decode(data, true);
    let _ = parse_m3u(&text, Path::new("base"));
    let _ = parse_pls(&text, Path::new("base"));
    let _ = Template::parse(&text).render(&Tags::default(), "name");
});
```

Patch `fuzz/Cargo.toml` (apply with `git apply`):

```diff
diff --git a/fuzz/Cargo.toml b/fuzz/Cargo.toml
index 80bc060..cb10998 100644
--- a/fuzz/Cargo.toml
+++ b/fuzz/Cargo.toml
@@ -11,6 +11,7 @@ cargo-fuzz = true
 crossbeam-channel = "0.5.17"
 libfuzzer-sys = "0.4.13"
 winrust-audio = { path = "../crates/audio" }
+winrust-playlist = { path = "../crates/playlist" }
 winrust-skin = { path = "../crates/skin" }
 
 # Not part of the main workspace: cargo-fuzz needs nightly.
@@ -36,3 +37,10 @@ path = "fuzz_targets/icy.rs"
 test = false
 doc = false
 bench = false
+
+[[bin]]
+name = "playlist_file"
+path = "fuzz_targets/playlist_file.rs"
+test = false
+doc = false
+bench = false
```

Patch `.github/workflows/fuzz.yml` (apply with `git apply`):

```diff
diff --git a/.github/workflows/fuzz.yml b/.github/workflows/fuzz.yml
index 61c5a0d..afcb623 100644
--- a/.github/workflows/fuzz.yml
+++ b/.github/workflows/fuzz.yml
@@ -9,6 +9,8 @@ on:
       - "fuzz/**"
       - "crates/skin/**"
       - "crates/audio/src/icy.rs"
+      - "crates/playlist/src/files.rs"
+      - "crates/playlist/src/title.rs"
       - ".github/workflows/fuzz.yml"
 
 env:
@@ -21,7 +23,7 @@ jobs:
     strategy:
       fail-fast: false
       matrix:
-        target: [bmp, skin_archive, icy]
+        target: [bmp, skin_archive, icy, playlist_file]
     steps:
       - uses: actions/checkout@v5
       # rust-toolchain.toml pins stable; `cargo +nightly` below overrides it.
```

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p winrust-playlist`
Expected: PASS: `24 passed` (cursor), `14 passed` (editing), `12 passed` (files), `14 passed` (order), `13 passed` (selection), `11 passed` (title).

Run: `cargo check --manifest-path fuzz/Cargo.toml --bin playlist_file`
Expected: `Finished`. The fuzz crate is outside the workspace and builds on stable for a check; the nightly fuzz run is CI's.

- [ ] **Step 5: Lint and commit**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no output.

Run: `cargo deny check`
Expected: `advisories ok, bans ok, licenses ok, sources ok`.

```bash
git add Cargo.lock crates/playlist fuzz/Cargo.toml fuzz/fuzz_targets/playlist_file.rs .github/workflows/fuzz.yml
git commit -m "feat(playlist): M3U, M3U8 and PLS files" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Folders, arguments and playlist files as sources

Spec §4.11; moved from M3's `app::sources`, which part 3 deletes:
- a URL is a stream;
- a playlist file is its entries;
- a folder is the audio files inside it, recursively, sorted with `path_key`;
- any other file is itself.

**Files:**
- Create: `crates/playlist/src/sources.rs`
- Modify: `crates/playlist/src/lib.rs`
- Test: `crates/playlist/tests/sources.rs`

**Interfaces:**
- Consumes: Task 1's `Item`; Task 3's `path_key`; Task 6's `files::read`.
- Produces:
  - `sources::from_args(impl IntoIterator<Item = OsString>) -> Vec<Item>`;
  - `sources::from_paths(Vec<PathBuf>) -> Vec<Item>`;
  - `sources::is_url(&str) -> bool`;
  - `sources::AUDIO_EXTENSIONS` and `sources::PLAYLIST_EXTENSIONS`.

- [ ] **Step 1: Write the failing tests**

Create `crates/playlist/tests/sources.rs`:

```rust
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use winrust_audio::Source;
use winrust_playlist::entry::Item;
use winrust_playlist::sources::{from_args, from_paths};

fn files(paths: &[PathBuf]) -> Vec<Item> {
    paths
        .iter()
        .map(|p| Source::File(p.clone()).into())
        .collect()
}

fn touch(root: &Path, names: &[&str]) {
    for name in names {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"").unwrap();
    }
}

#[test]
fn urls_and_files_are_told_apart() {
    let got = from_args([
        OsString::from("HTTPS://radio.example/live"),
        OsString::from("icy://radio.example/x"),
        OsString::from("song.mp3"),
    ]);
    assert_eq!(
        got,
        [
            Source::Url("HTTPS://radio.example/live".into()).into(),
            Source::Url("icy://radio.example/x".into()).into(),
            Source::File(PathBuf::from("song.mp3")).into(),
        ]
    );
}

#[test]
fn folders_expand_to_their_audio_files_in_name_order() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(
        root,
        &["b.mp3", "A.FLAC", "cover.jpg", "sub/c.ogg", "notes.txt"],
    );
    assert_eq!(
        from_paths(vec![root.to_path_buf()]),
        files(&[
            root.join("A.FLAC"),
            root.join("b.mp3"),
            root.join("sub").join("c.ogg")
        ])
    );
}

#[test]
fn folders_sort_one_name_at_a_time() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, &["Live 2001/a.mp3", "live/b.mp3", "Live/c.mp3"]);
    let got = from_paths(vec![root.to_path_buf()]);
    let names: Vec<_> = got
        .iter()
        .map(|i| match &i.source {
            Source::File(p) => p.strip_prefix(root).unwrap().to_path_buf(),
            Source::Url(_) => unreachable!(),
        })
        .collect();
    let first_folder = names[0].parent().unwrap().to_string_lossy().to_lowercase();
    assert_eq!(first_folder, "live", "{names:?}");
    assert_eq!(names[2], Path::new("Live 2001").join("a.mp3"));
}

#[test]
fn files_named_directly_are_kept_whatever_they_are() {
    let dir = tempfile::tempdir().unwrap();
    let odd = dir.path().join("track.xyz");
    fs::write(&odd, b"").unwrap();
    let missing = dir.path().join("missing.mp3");
    assert_eq!(
        from_paths(vec![odd.clone(), missing.clone()]),
        files(&[odd, missing])
    );
}

#[test]
fn folder_arguments_expand_too() {
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), &["a.wav"]);
    assert_eq!(
        from_args([dir.path().as_os_str().to_owned()]),
        files(&[dir.path().join("a.wav")])
    );
}

#[test]
fn playlist_files_expand_to_their_entries() {
    let dir = tempfile::tempdir().unwrap();
    let list = dir.path().join("mix.M3U8");
    fs::write(&list, "#EXTINF:3,Three\nsub/a.mp3\nhttp://radio.example/\n").unwrap();
    let expected = vec![
        Item {
            source: Source::File(dir.path().join("sub").join("a.mp3")),
            hint: Some("Three".into()),
            duration: Some(std::time::Duration::from_secs(3)),
        },
        Source::Url("http://radio.example/".into()).into(),
    ];
    assert_eq!(from_paths(vec![list.clone()]), expected);
    assert_eq!(from_args([list.into_os_string()]), expected);
}

#[test]
fn a_missing_playlist_file_adds_nothing() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(from_paths(vec![dir.path().join("gone.pls")]), []);
}

#[test]
fn playlist_files_inside_folders_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), &["a.mp3"]);
    fs::write(dir.path().join("list.m3u"), "a.mp3\na.mp3\n").unwrap();
    assert_eq!(
        from_paths(vec![dir.path().to_path_buf()]),
        files(&[dir.path().join("a.mp3")])
    );
}

#[cfg(unix)]
#[test]
fn linked_files_count_and_linked_folders_are_not_followed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, &["real/a.mp3"]);
    std::os::unix::fs::symlink(root.join("real").join("a.mp3"), root.join("link.mp3")).unwrap();
    std::os::unix::fs::symlink(root, root.join("real").join("loop")).unwrap();
    assert_eq!(
        from_paths(vec![root.to_path_buf()]),
        files(&[root.join("link.mp3"), root.join("real").join("a.mp3")])
    );
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p winrust-playlist --test sources`
Expected: compile error ``unresolved import `winrust_playlist::sources` ``.

- [ ] **Step 3: Implement**

Create `crates/playlist/src/sources.rs`:

```rust
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
            None => expand(PathBuf::from(arg), &mut out),
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
```

Patch `crates/playlist/src/lib.rs` (apply with `git apply`):

```diff
diff --git a/crates/playlist/src/lib.rs b/crates/playlist/src/lib.rs
index 8a0e062..001136d 100644
--- a/crates/playlist/src/lib.rs
+++ b/crates/playlist/src/lib.rs
@@ -8,4 +8,5 @@ pub mod files;
 pub mod order;
 pub mod playlist;
 pub mod rng;
+pub mod sources;
 pub mod title;
```

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p winrust-playlist`
Expected: PASS: `24 passed` (cursor), `14 passed` (editing), `12 passed` (files), `14 passed` (order), `13 passed` (selection), `8 passed` (sources; 9 on Linux and macOS), `11 passed` (title).

- [ ] **Step 5: Lint and commit**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no output.

```bash
git add crates/playlist
git commit -m "feat(playlist): folders, arguments and playlist files as sources" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Tags and the tag worker

Spec §4.9 and §4.10:
- `tags::read` gives a file's tags and duration through `lofty`;
- `Entry::set_tags` takes a result;
- `TagWorker` reads on one background thread, a new request replacing the pending ones.

The tests tag temporary copies of the audio fixtures, so no fixture is added.

**Files:**
- Create: `crates/playlist/src/tags.rs`, `crates/playlist/src/worker.rs`
- Modify: `crates/playlist/Cargo.toml`, `crates/playlist/src/entry.rs`, `crates/playlist/src/lib.rs`, `deny.toml`
- Test: `crates/playlist/tests/common/mod.rs`, `crates/playlist/tests/tags.rs`, `crates/playlist/tests/worker.rs`

**Interfaces:**
- Consumes: Task 1's `Entry`, `Tags` and `EntryId`.
- Produces:
  - `tags::FileTags { tags: Tags, duration: Option<Duration> }`;
  - `tags::TagError(pub String)` (`Error`);
  - `tags::read(&Path) -> Result<FileTags, TagError>`;
  - `Entry::set_tags(Result<FileTags, TagError>)`;
  - `worker::TagResult = Result<FileTags, TagError>`;
  - `worker::TagWorker`, with:
    - `new(deliver: impl FnMut(EntryId, TagResult) + Send + 'static)`;
    - `with_reader(read: impl Fn(&Path) -> TagResult + Send + 'static, deliver)`;
    - `request(Vec<(EntryId, PathBuf)>)`.

- [ ] **Step 1: Write the failing tests**

Patch `crates/playlist/Cargo.toml` (apply with `git apply`):

```diff
diff --git a/crates/playlist/Cargo.toml b/crates/playlist/Cargo.toml
index 4900b26..dacc770 100644
--- a/crates/playlist/Cargo.toml
+++ b/crates/playlist/Cargo.toml
@@ -7,6 +7,8 @@ license.workspace = true
 publish.workspace = true
 
 [dependencies]
+crossbeam-channel = "0.5.17"
+lofty = "0.25.4"
 url = "2.5.8"
 winrust-audio = { path = "../audio" }
 
```

Patch `crates/playlist/tests/common/mod.rs` (apply with `git apply`):

```diff
diff --git a/crates/playlist/tests/common/mod.rs b/crates/playlist/tests/common/mod.rs
index 26fe303..df2377b 100644
--- a/crates/playlist/tests/common/mod.rs
+++ b/crates/playlist/tests/common/mod.rs
@@ -1,8 +1,12 @@
 #![allow(dead_code)]
 
-use std::path::PathBuf;
+use lofty::config::WriteOptions;
+use lofty::file::TaggedFileExt;
+use lofty::tag::items::Timestamp;
+use lofty::tag::{Accessor, Tag, TagExt};
+use std::path::{Path, PathBuf};
 use winrust_audio::Source;
-use winrust_playlist::entry::Item;
+use winrust_playlist::entry::{Item, Tags};
 use winrust_playlist::playlist::Playlist;
 
 pub fn file(name: &str) -> Item {
@@ -31,3 +35,46 @@ pub fn selected(pl: &Playlist) -> Vec<usize> {
     let rows = pl.entries().iter().enumerate();
     rows.filter(|(_, e)| e.selected).map(|(i, _)| i).collect()
 }
+
+/// A copy of an audio fixture (`crates/audio/tests/fixtures`) in `dir`.
+pub fn fixture(dir: &Path, name: &str) -> PathBuf {
+    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../audio/tests/fixtures");
+    let to = dir.join(name);
+    std::fs::copy(from.join(name), &to).unwrap();
+    to
+}
+
+/// Replaces the file's tags with these, in its own tag format.
+pub fn write_tags(path: &Path, tags: &Tags) {
+    let kind = lofty::read_from_path(path).unwrap().primary_tag_type();
+    let mut tag = Tag::new(kind);
+    if let Some(v) = &tags.artist {
+        tag.set_artist(v.clone());
+    }
+    if let Some(v) = &tags.title {
+        tag.set_title(v.clone());
+    }
+    if let Some(v) = &tags.album {
+        tag.set_album(v.clone());
+    }
+    if let Some(year) = tags.year {
+        tag.set_date(Timestamp {
+            year: year as u16,
+            ..Timestamp::default()
+        });
+    }
+    if let Some(track) = tags.track {
+        tag.set_track(track);
+    }
+    tag.save_to_path(path, WriteOptions::default()).unwrap();
+}
+
+pub fn full_tags() -> Tags {
+    Tags {
+        artist: Some("Ärtist".into()),
+        title: Some("Tïtle".into()),
+        album: Some("Album".into()),
+        year: Some(1999),
+        track: Some(7),
+    }
+}
```

Create `crates/playlist/tests/tags.rs`:

```rust
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
```

Create `crates/playlist/tests/worker.rs`:

```rust
mod common;

use common::{fixture, full_tags, playlist, write_tags};
use crossbeam_channel::{RecvTimeoutError, unbounded};
use std::path::PathBuf;
use std::time::Duration;
use winrust_playlist::entry::EntryId;
use winrust_playlist::tags::FileTags;
use winrust_playlist::worker::{TagResult, TagWorker};

const WAIT: Duration = Duration::from_secs(10);

fn ids(n: usize) -> Vec<EntryId> {
    playlist(n).entries().iter().map(|e| e.id()).collect()
}

#[test]
fn the_worker_reads_tags_and_reports_them_by_id() {
    let dir = tempfile::tempdir().unwrap();
    let tagged = fixture(dir.path(), "tone.mp3");
    write_tags(&tagged, &full_tags());
    let id = ids(2);
    let (tx, rx) = unbounded();
    let worker = TagWorker::new(move |id, result| tx.send((id, result)).unwrap());
    worker.request(vec![
        (id[0], tagged),
        (id[1], dir.path().join("missing.mp3")),
    ]);
    let (first, result) = rx.recv_timeout(WAIT).unwrap();
    assert_eq!(first, id[0]);
    assert_eq!(result.unwrap().tags, full_tags());
    let (second, result) = rx.recv_timeout(WAIT).unwrap();
    assert_eq!(second, id[1]);
    assert!(result.is_err());
}

#[test]
fn a_new_request_replaces_the_ones_still_waiting() {
    let id = ids(5);
    let (started_tx, started) = unbounded::<()>();
    let (release, gate) = unbounded::<()>();
    let read = move |path: &std::path::Path| -> TagResult {
        if path.ends_with("slow") {
            started_tx.send(()).unwrap();
            gate.recv_timeout(WAIT).unwrap();
        }
        Ok(FileTags::default())
    };
    let (tx, rx) = unbounded();
    let worker = TagWorker::with_reader(read, move |id, _| tx.send(id).unwrap());
    let path = |name: &str| PathBuf::from(name);
    worker.request(vec![
        (id[0], path("slow")),
        (id[1], path("a")),
        (id[2], path("b")),
    ]);
    started.recv_timeout(WAIT).unwrap();
    worker.request(vec![(id[3], path("c")), (id[4], path("d"))]);
    release.send(()).unwrap();
    let got: Vec<EntryId> = (0..3).map(|_| rx.recv_timeout(WAIT).unwrap()).collect();
    assert_eq!(got, [id[0], id[3], id[4]], "the read under way finishes");
    assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
}

#[test]
fn dropping_the_worker_stops_its_thread() {
    let (tx, rx) = unbounded::<EntryId>();
    let worker = TagWorker::new(move |id, _| tx.send(id).unwrap());
    drop(worker);
    // The thread owned `tx`; once it ends, the channel disconnects.
    assert_eq!(rx.recv_timeout(WAIT), Err(RecvTimeoutError::Disconnected));
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p winrust-playlist --test tags --test worker`
Expected: compile errors ``unresolved import `winrust_playlist::tags` ``, ``unresolved import `winrust_playlist::worker` `` and ``no method named `set_tags` found``.

- [ ] **Step 3: Implement**

Create `crates/playlist/src/tags.rs`:

```rust
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
```

Create `crates/playlist/src/worker.rs`:

```rust
//! The tag worker (spec §4.10): one background thread that reads tags in the order asked,
//! visible rows first. The caller applies results by id, so edits made meanwhile are harmless.

use crate::entry::EntryId;
use crate::tags::{self, FileTags, TagError};
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

pub type TagResult = Result<FileTags, TagError>;

/// Dropping the worker stops its thread once the read under way finishes; nothing waits for
/// that read.
pub struct TagWorker {
    requests: Sender<Vec<(EntryId, PathBuf)>>,
}

impl TagWorker {
    /// Starts the thread. `deliver` receives each result on it.
    pub fn new(deliver: impl FnMut(EntryId, TagResult) + Send + 'static) -> Self {
        Self::with_reader(tags::read, deliver)
    }

    /// As `new`, reading files with `read`.
    pub fn with_reader(
        read: impl Fn(&Path) -> TagResult + Send + 'static,
        mut deliver: impl FnMut(EntryId, TagResult) + Send + 'static,
    ) -> Self {
        let (requests, rx) = crossbeam_channel::unbounded();
        std::thread::Builder::new()
            .name("winrust-tags".into())
            .spawn(move || run(&rx, &read, &mut deliver))
            .expect("start the tag thread");
        TagWorker { requests }
    }

    /// Reads these files' tags in this order, in place of any still waiting. A read already
    /// under way finishes first.
    pub fn request(&self, batch: Vec<(EntryId, PathBuf)>) {
        // The thread only ends when this worker is dropped.
        let _ = self.requests.send(batch);
    }
}

fn run(
    requests: &Receiver<Vec<(EntryId, PathBuf)>>,
    read: &dyn Fn(&Path) -> TagResult,
    deliver: &mut dyn FnMut(EntryId, TagResult),
) {
    let mut pending = VecDeque::new();
    loop {
        if pending.is_empty() {
            match requests.recv() {
                Ok(batch) => pending = batch.into(),
                Err(_) => return,
            }
        }
        loop {
            match requests.try_recv() {
                Ok(batch) => pending = batch.into(),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        if let Some((id, path)) = pending.pop_front() {
            deliver(id, read(&path));
        }
    }
}
```

Patch `crates/playlist/src/entry.rs` (apply with `git apply`):

```diff
diff --git a/crates/playlist/src/entry.rs b/crates/playlist/src/entry.rs
index 1dd7623..1bf0ddc 100644
--- a/crates/playlist/src/entry.rs
+++ b/crates/playlist/src/entry.rs
@@ -1,5 +1,6 @@
 //! One playlist entry and what is known about it.
 
+use crate::tags::{FileTags, TagError};
 use crate::title::Template;
 use std::time::Duration;
 use winrust_audio::Source;
@@ -67,6 +68,23 @@ impl Entry {
         self.id
     }
 
+    /// Takes the result of reading the file's tags. A file that cannot be read keeps the
+    /// title the playlist file gave it.
+    pub fn set_tags(&mut self, result: Result<FileTags, TagError>) {
+        match result {
+            Ok(read) => {
+                self.tags = Some(read.tags);
+                self.duration = read.duration.or(self.duration);
+            }
+            Err(_) => {
+                self.tags = Some(Tags {
+                    title: self.hint.clone(),
+                    ..Tags::default()
+                })
+            }
+        }
+    }
+
     /// `%filename%`: the file name without its extension; for a stream, the URL.
     pub fn filename(&self) -> String {
         match &self.source {
```

Patch `crates/playlist/src/lib.rs` (apply with `git apply`):

```diff
diff --git a/crates/playlist/src/lib.rs b/crates/playlist/src/lib.rs
index 001136d..a655893 100644
--- a/crates/playlist/src/lib.rs
+++ b/crates/playlist/src/lib.rs
@@ -9,4 +9,6 @@ pub mod order;
 pub mod playlist;
 pub mod rng;
 pub mod sources;
+pub mod tags;
 pub mod title;
+pub mod worker;
```

Patch `deny.toml` (apply with `git apply`):

```diff
diff --git a/deny.toml b/deny.toml
index 5cc5e58..466867d 100644
--- a/deny.toml
+++ b/deny.toml
@@ -5,6 +5,11 @@ all-features = false
 
 [advisories]
 yanked = "deny"
+ignore = [
+    # paste is unmaintained, not vulnerable: a build-time macro crate that lofty (tag reading)
+    # uses. Nothing of it runs in Winrust.
+    { id = "RUSTSEC-2024-0436", reason = "unmaintained build-time macro used by lofty" },
+]
 
 [licenses]
 # Permissive licenses only. MPL-2.0 (symphonia, triple_buffer) is file-level copyleft and
```

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p winrust-playlist`
Expected: PASS: `24 passed` (cursor), `14 passed` (editing), `12 passed` (files), `14 passed` (order), `13 passed` (selection), `8 passed` (sources), `6 passed` (tags), `11 passed` (title), `3 passed` (worker).

- [ ] **Step 5: The whole branch**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no output.

Run: `cargo test --workspace`
Expected: `507 passed`, `1 ignored` in total, 0 failed (508 on Linux and macOS).

Run: `cargo deny check`
Expected: `advisories ok, bans ok, licenses ok, sources ok`.

- [ ] **Step 6: Commit**

```bash
git add Cargo.lock deny.toml crates/playlist
git commit -m "feat(playlist): tags and the tag worker" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## After this part

- Nothing user-visible changes yet. The M4 manual checklist (spec §6.8) runs after part 3.
- Part 2 consumes:
  - `Entry::title`, `Entry::duration` and `Entry::dead`;
  - `Playlist::entries`, `current_index` and the selection flags.
- Part 3 consumes everything else:
  - `Cursor` replaces `app::queue`, and `sources` replaces `app::sources`;
  - the tag worker's results go through the `EventLoopProxy`;
  - `files::write_m3u8` is the autosave.
