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
