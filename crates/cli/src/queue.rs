//! What to play next, decided from engine events. Pure so it can be unit-tested.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    Load(usize),
    Enqueue(usize),
    Quit,
    Nothing,
}

#[derive(Debug)]
pub struct Queue {
    len: usize,
    current: Option<usize>,
    queued: Option<usize>,
    /// The engine reported that the queued track could not be opened.
    queued_failed: bool,
    awaiting_start: bool,
}

impl Queue {
    pub fn new(len: usize) -> Self {
        Queue {
            len,
            current: None,
            queued: None,
            queued_failed: false,
            awaiting_start: false,
        }
    }

    pub fn start(&mut self) -> Next {
        self.load(0)
    }

    fn load(&mut self, index: usize) -> Next {
        if index >= self.len {
            return Next::Quit;
        }
        self.current = Some(index);
        self.queued = None;
        self.queued_failed = false;
        self.awaiting_start = true;
        Next::Load(index)
    }

    /// A track became audible.
    pub fn on_started(&mut self, gapless: bool) -> Next {
        if gapless {
            self.current = self.queued.take();
        }
        self.awaiting_start = false;
        match self.current {
            Some(i) if i + 1 < self.len => {
                self.queued = Some(i + 1);
                Next::Enqueue(i + 1)
            }
            _ => Next::Nothing,
        }
    }

    /// The engine reported an error; `is_source(i)` says whether entry `i` is the track it
    /// names.
    pub fn on_error(&mut self, is_source: impl Fn(usize) -> bool) -> Next {
        if self.queued.is_some_and(&is_source) {
            self.queued_failed = true;
            return Next::Nothing;
        }
        match self.current {
            Some(i) if self.awaiting_start && is_source(i) => self.load(i + 1),
            _ => Next::Nothing,
        }
    }

    /// Playback stopped at the end of the last queued track.
    pub fn on_ended(&mut self) -> Next {
        match self.queued.take() {
            Some(q) if self.queued_failed => self.load(q + 1),
            // The engine finished before it saw the queued track: play it now.
            Some(q) => self.load(q),
            None => Next::Quit,
        }
    }

    /// User asked for the next track.
    pub fn skip(&mut self) -> Next {
        match self.current {
            Some(i) => self.load(i + 1),
            None => Next::Quit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plays_through_gaplessly() {
        let mut q = Queue::new(3);
        assert_eq!(q.start(), Next::Load(0));
        assert_eq!(q.on_started(false), Next::Enqueue(1));
        assert_eq!(q.on_started(true), Next::Enqueue(2));
        assert_eq!(q.on_started(true), Next::Nothing);
        assert_eq!(q.on_ended(), Next::Quit);
    }

    #[test]
    fn empty_list_quits() {
        assert_eq!(Queue::new(0).start(), Next::Quit);
    }

    #[test]
    fn unreadable_first_track_is_skipped() {
        let mut q = Queue::new(2);
        q.start();
        assert_eq!(q.on_error(|i| i == 0), Next::Load(1));
        assert_eq!(q.on_started(false), Next::Nothing);
    }

    #[test]
    fn unreadable_queued_track_is_skipped_at_end() {
        let mut q = Queue::new(3);
        q.start();
        assert_eq!(q.on_started(false), Next::Enqueue(1));
        assert_eq!(q.on_error(|i| i == 1), Next::Nothing, "queued track failed");
        assert_eq!(q.on_ended(), Next::Load(2));
    }

    #[test]
    fn skip_moves_on_and_quits_past_the_end() {
        let mut q = Queue::new(2);
        q.start();
        assert_eq!(q.skip(), Next::Load(1));
        assert_eq!(q.skip(), Next::Quit);
    }

    #[test]
    fn track_that_ends_before_the_queue_arrives_does_not_skip_the_next() {
        // Review I7: a header-only file starts and ends at once, before the engine sees
        // the QueueNext; the next track never started but did not fail either.
        let mut q = Queue::new(3);
        q.start();
        assert_eq!(q.on_started(false), Next::Enqueue(1));
        assert_eq!(q.on_ended(), Next::Load(1));
    }

    #[test]
    fn unattributed_errors_change_nothing() {
        let mut q = Queue::new(2);
        q.start();
        assert_eq!(q.on_error(|_| false), Next::Nothing);
        assert_eq!(q.on_started(false), Next::Enqueue(1));
    }
}
