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
