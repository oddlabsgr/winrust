//! Finds a skin's files in a zip archive (`.wsz`) or a directory.
//!
//! Files are matched by name alone, at any folder depth, ignoring case. When a name occurs
//! more than once the last one wins (archive order; sorted path order in a directory), as
//! when Winamp extracts an archive over itself; only that copy is read. Only files a skin
//! can use are read: bitmaps up to [`MAX_FILE`] bytes, cursor and text files up to
//! [`MAX_SMALL_FILE`], and at most [`MAX_TOTAL`] bytes in all. Anything unreadable or over
//! a limit becomes a warning.

use crate::skin::CURSORS;
use crate::sprites::Sheet;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::Path;

pub const MAX_FILE: u64 = 16 << 20;
/// Cursor and text files: real ones are a few KB.
pub const MAX_SMALL_FILE: u64 = 1 << 20;
/// All files of one skin together.
pub const MAX_TOTAL: u64 = 64 << 20;
const MAX_ENTRIES: usize = 10_000;
const MAX_DEPTH: usize = 4;

pub struct SkinFiles {
    /// Lower-case file name → (arrival order, contents).
    files: HashMap<String, (usize, Vec<u8>)>,
    arrivals: usize,
    /// Bytes that may still be read.
    budget: u64,
    pub warnings: Vec<String>,
}

impl Default for SkinFiles {
    fn default() -> Self {
        SkinFiles {
            files: HashMap::new(),
            arrivals: 0,
            budget: MAX_TOTAL,
            warnings: Vec::new(),
        }
    }
}

fn wanted(name: &str) -> bool {
    if let Some(stem) = name
        .strip_suffix(".bmp")
        .or_else(|| name.strip_suffix(".png"))
    {
        return Sheet::ALL.iter().any(|s| s.stem() == stem);
    }
    if let Some(stem) = name.strip_suffix(".cur") {
        return CURSORS.iter().any(|c| c.eq_ignore_ascii_case(stem));
    }
    matches!(name, "pledit.txt" | "viscolor.txt" | "region.txt")
}

fn limit(name: &str) -> u64 {
    if name.ends_with(".bmp") || name.ends_with(".png") {
        MAX_FILE
    } else {
        MAX_SMALL_FILE
    }
}

fn base_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or("").to_lowercase()
}

/// What happened to one file.
enum Outcome {
    Kept,
    Skipped,
    /// The skin's byte budget is spent: stop reading.
    OutOfBudget,
}

impl SkinFiles {
    pub fn from_zip(bytes: &[u8]) -> Result<SkinFiles, String> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
        if archive.len() > MAX_ENTRIES {
            return Err(format!("{} entries is too many", archive.len()));
        }
        let mut out = SkinFiles::default();
        // Only the last entry of each name is used, so only those are decompressed.
        let mut last: HashMap<String, usize> = HashMap::new();
        for i in 0..archive.len() {
            match archive.name_for_index(i) {
                Some(Ok(path)) => {
                    let name = base_name(&path);
                    if wanted(&name) {
                        last.insert(name, i);
                    }
                }
                _ => out
                    .warnings
                    .push(format!("archive entry {i}: unreadable name")),
            }
        }
        let mut chosen: Vec<(usize, String)> = last.into_iter().map(|(n, i)| (i, n)).collect();
        chosen.sort();
        for (i, name) in chosen {
            let mut entry = match archive.by_index(i) {
                Ok(entry) => entry,
                Err(e) => {
                    out.warnings.push(format!("archive entry {i}: {e}"));
                    continue;
                }
            };
            if entry.is_dir() {
                continue;
            }
            let path = entry
                .name()
                .map(|n| n.into_owned())
                .unwrap_or_else(|_| name.clone());
            if let Outcome::OutOfBudget = out.read(name, &path, &mut entry) {
                break;
            }
        }
        Ok(out)
    }

    /// Reads one file within its own limit and the remaining budget.
    fn read(&mut self, name: String, path: &str, source: &mut impl Read) -> Outcome {
        let limit = limit(&name);
        let cap = limit.min(self.budget);
        let mut data = Vec::new();
        let result = source.take(cap + 1).read_to_end(&mut data);
        self.budget = self.budget.saturating_sub(data.len() as u64);
        match result {
            Err(e) => {
                self.warnings.push(format!("{path}: {e}"));
                Outcome::Skipped
            }
            Ok(_) if data.len() as u64 <= cap => {
                self.insert(name, data);
                Outcome::Kept
            }
            Ok(_) if cap < limit => {
                self.warnings.push(format!(
                    "{path}: skipped; the skin's files exceed {MAX_TOTAL} bytes"
                ));
                Outcome::OutOfBudget
            }
            Ok(_) => {
                self.warnings
                    .push(format!("{path}: larger than {limit} bytes"));
                Outcome::Skipped
            }
        }
    }

    pub fn from_dir(dir: &Path) -> std::io::Result<SkinFiles> {
        let mut out = SkinFiles::default();
        out.walk(dir, 0, true)?;
        Ok(out)
    }

    /// Returns false once the budget is spent.
    fn walk(&mut self, dir: &Path, depth: usize, top: bool) -> std::io::Result<bool> {
        let mut entries = match std::fs::read_dir(dir) {
            Ok(rd) => rd
                .filter_map(Result::ok)
                .map(|e| e.path())
                .collect::<Vec<_>>(),
            Err(e) if !top => {
                self.warnings.push(format!("{}: {e}", dir.display()));
                return Ok(true);
            }
            Err(e) => return Err(e),
        };
        entries.sort();
        for path in entries {
            if path.is_dir() {
                if depth < MAX_DEPTH && !self.walk(&path, depth + 1, false)? {
                    return Ok(false);
                }
                continue;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if !wanted(&name) {
                continue;
            }
            let shown = path.display().to_string();
            match std::fs::File::open(&path) {
                Ok(mut file) => {
                    if let Outcome::OutOfBudget = self.read(name, &shown, &mut file) {
                        return Ok(false);
                    }
                }
                Err(e) => self.warnings.push(format!("{shown}: {e}")),
            }
        }
        Ok(true)
    }

    fn insert(&mut self, name: String, data: Vec<u8>) {
        self.arrivals += 1;
        self.files.insert(name, (self.arrivals, data));
    }

    /// Removes and returns the file with this lower-case name.
    pub fn take(&mut self, name: &str) -> Option<Vec<u8>> {
        self.files.remove(name).map(|(_, data)| data)
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Contents of the file with this lower-case name.
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.files.get(name).map(|(_, data)| data.as_slice())
    }

    /// The `<stem>.bmp` or `<stem>.png` that arrived last, with its name.
    pub fn image(&self, stem: &str) -> Option<(String, &[u8])> {
        ["bmp", "png"]
            .iter()
            .filter_map(|ext| {
                let name = format!("{stem}.{ext}");
                let (order, data) = self.files.get(&name)?;
                Some((*order, name, data.as_slice()))
            })
            .max_by_key(|(order, _, _)| *order)
            .map(|(_, name, data)| (name, data))
    }
}
