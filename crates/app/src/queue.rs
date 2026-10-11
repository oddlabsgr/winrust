//! What plays next: the entries from the command line or a drop, the one playing, and the
//! one handed to the engine early for a gapless start. Pure; the model turns its answers into
//! engine commands. Order rules follow Webamp `getNextTrackId`; the playlist crate replaces
//! this in M4.

use winrust_audio::Source;

/// What the engine's queued track should become after a setting changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Requeue {
    /// Send this entry with `QueueNext`.
    Queue(usize),
    /// Send `Unqueue`.
    Unqueue,
    Keep,
}

#[derive(Clone, Debug)]
pub struct Queue {
    entries: Vec<Source>,
    current: Option<usize>,
    /// The entry handed to the engine with `QueueNext`.
    queued: Option<usize>,
    /// The engine reported that the queued entry cannot be opened.
    queued_failed: bool,
    /// `current` was loaded and has not started yet.
    awaiting_start: bool,
    /// The engine has a track loaded (playing, paused or starting).
    playing: bool,
    /// Entries that failed one after another; when every entry has, playback stops.
    failures: usize,
    pub shuffle: bool,
    pub repeat: bool,
    /// Stop when the current track ends (Ctrl+V).
    pub stop_after: bool,
    rng: u64,
}

impl Queue {
    /// `seed` drives the shuffle order.
    pub fn new(seed: u64) -> Self {
        Queue {
            entries: Vec::new(),
            current: None,
            queued: None,
            queued_failed: false,
            awaiting_start: false,
            playing: false,
            failures: 0,
            shuffle: false,
            repeat: false,
            stop_after: false,
            rng: seed | 1,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entry(&self, index: usize) -> Option<&Source> {
        self.entries.get(index)
    }

    pub fn current(&self) -> Option<usize> {
        self.current
    }

    /// Replaces every entry; returns the first one, to load.
    pub fn replace(&mut self, entries: Vec<Source>) -> Option<usize> {
        self.entries = entries;
        self.current = None;
        self.stop();
        self.failures = 0;
        if self.entries.is_empty() {
            None
        } else {
            self.load(0)
        }
    }

    /// The entry to load for Play: the current one, else the first.
    pub fn restart(&mut self) -> Option<usize> {
        let index = self.current.or((!self.entries.is_empty()).then_some(0))?;
        self.failures = 0;
        self.load(index)
    }

    /// The entry `n` steps away (previous, next, ±10 jumps), to load; `None` past either end
    /// without repeat.
    pub fn step(&mut self, n: i32) -> Option<usize> {
        let index = match self.current {
            Some(from) => self.pick(from, n),
            None => (!self.entries.is_empty()).then_some(0),
        }?;
        self.failures = 0;
        self.load(index)
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
    pub fn on_started(&mut self, gapless: bool) -> Option<usize> {
        if gapless && let Some(q) = self.queued.take() {
            self.current = Some(q);
        }
        self.queued = None;
        self.queued_failed = false;
        self.awaiting_start = false;
        self.failures = 0;
        self.queue_next()
    }

    /// The engine reported an error; `is_source` tells whether a source is the one it names.
    /// Returns the entry to load instead.
    pub fn on_error(&mut self, is_source: impl Fn(&Source) -> bool) -> Option<usize> {
        if self.queued.is_some_and(|q| is_source(&self.entries[q])) {
            self.queued_failed = true;
            return None;
        }
        match self.current {
            Some(i) if self.awaiting_start && is_source(&self.entries[i]) => self.skip(i),
            _ => None,
        }
    }

    /// The engine stopped at the end of its last track. Returns the entry to load.
    pub fn on_ended(&mut self) -> Option<usize> {
        if self.stop_after {
            self.stop();
            return None;
        }
        match self.queued.take() {
            Some(q) if self.queued_failed => self.skip(q),
            // The engine finished before it saw the queued track: play it now.
            Some(q) => self.load(q),
            None => {
                self.stop();
                None
            }
        }
    }

    /// Shuffle, repeat or stop-after changed: what the engine's queued track becomes.
    pub fn requeue(&mut self) -> Requeue {
        if !self.playing || self.awaiting_start {
            return Requeue::Keep;
        }
        let before = self.queued;
        match self.queue_next() {
            Some(next) => Requeue::Queue(next),
            None if before.is_some() => Requeue::Unqueue,
            None => Requeue::Keep,
        }
    }

    fn queue_next(&mut self) -> Option<usize> {
        let current = self.current?;
        self.queued = if self.stop_after {
            None
        } else {
            self.pick(current, 1)
        };
        self.queued_failed = false;
        self.queued
    }

    /// Moves past an entry that cannot be opened, unless every entry has failed in a row.
    fn skip(&mut self, failed: usize) -> Option<usize> {
        self.failures += 1;
        let next = if self.failures < self.entries.len() {
            self.pick(failed, 1)
        } else {
            None
        };
        match next {
            Some(i) => self.load(i),
            None => {
                self.stop();
                None
            }
        }
    }

    fn load(&mut self, index: usize) -> Option<usize> {
        self.current = Some(index);
        self.queued = None;
        self.queued_failed = false;
        self.awaiting_start = true;
        self.playing = true;
        Some(index)
    }

    /// The entry `n` steps from `from` (Webamp `getNextTrackId`): shuffle picks any other
    /// entry; repeat wraps; otherwise `None` past either end, and jumps clamp.
    fn pick(&mut self, from: usize, n: i32) -> Option<usize> {
        let len = self.entries.len();
        if len == 0 {
            return None;
        }
        if self.shuffle {
            if len == 1 {
                return Some(0);
            }
            loop {
                let i = self.random(len);
                if i != from {
                    return Some(i);
                }
            }
        }
        let to = from as i64 + i64::from(n);
        if self.repeat {
            return Some(to.rem_euclid(len as i64) as usize);
        }
        if (from + 1 == len && n > 0) || (from == 0 && n < 0) {
            return None;
        }
        Some(to.clamp(0, len as i64 - 1) as usize)
    }

    /// xorshift64: plenty for a shuffle, and repeatable in tests.
    fn random(&mut self, below: usize) -> usize {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % below as u64) as usize
    }
}
