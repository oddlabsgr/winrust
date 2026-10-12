//! The tag worker (spec §4.10): one background thread that reads tags in the order asked,
//! visible rows first. The caller applies results by id, so edits made meanwhile are harmless.

use crate::entry::EntryId;
use crate::tags::{self, FileTags, TagError};
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};
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
            // A tag library panicking on one bad file must not end tag reading for the
            // session (umbrella §10: no panics on bad user data).
            let result = panic::catch_unwind(AssertUnwindSafe(|| read(&path)))
                .unwrap_or_else(|_| Err(TagError("the file's tags could not be read".into())));
            deliver(id, result);
        }
    }
}
