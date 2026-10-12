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
