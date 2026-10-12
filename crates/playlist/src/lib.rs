//! The Winrust playlist: entries, selection and editing, play order, playlist files, title
//! templates and tags. No windowing or OS API; only reading playlist files, folders and tags
//! touches the disk.

pub mod cursor;
pub mod entry;
pub mod order;
pub mod playlist;
pub mod rng;
pub mod title;
