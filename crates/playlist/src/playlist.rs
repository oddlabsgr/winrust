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
