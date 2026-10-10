//! Finds a skin's files in a zip archive (`.wsz`) or a directory.
//!
//! Files are matched by name alone, at any folder depth, ignoring case. When a name occurs
//! more than once the last one wins (archive order; sorted path order in a directory), as
//! when Winamp extracts an archive over itself. Only files a skin can use are read, each
//! at most [`MAX_FILE`] bytes; anything unreadable becomes a warning.

use crate::skin::CURSORS;
use crate::sprites::Sheet;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::Path;

pub const MAX_FILE: u64 = 16 << 20;
const MAX_ENTRIES: usize = 10_000;
const MAX_DEPTH: usize = 4;

#[derive(Default)]
pub struct SkinFiles {
    /// Lower-case file name → (arrival order, contents).
    files: HashMap<String, (usize, Vec<u8>)>,
    arrivals: usize,
    pub warnings: Vec<String>,
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

impl SkinFiles {
    pub fn from_zip(bytes: &[u8]) -> Result<SkinFiles, String> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
        if archive.len() > MAX_ENTRIES {
            return Err(format!("{} entries is too many", archive.len()));
        }
        let mut out = SkinFiles::default();
        for i in 0..archive.len() {
            let mut entry = match archive.by_index(i) {
                Ok(entry) => entry,
                Err(e) => {
                    out.warnings.push(format!("archive entry {i}: {e}"));
                    continue;
                }
            };
            let Ok(path) = entry.name().map(|n| n.into_owned()) else {
                out.warnings
                    .push(format!("archive entry {i}: unreadable name"));
                continue;
            };
            let name = path.rsplit(['/', '\\']).next().unwrap_or("").to_lowercase();
            if entry.is_dir() || !wanted(&name) {
                continue;
            }
            let mut data = Vec::new();
            match entry.by_ref().take(MAX_FILE + 1).read_to_end(&mut data) {
                Ok(_) if data.len() as u64 > MAX_FILE => out
                    .warnings
                    .push(format!("{path}: larger than {MAX_FILE} bytes")),
                Ok(_) => out.insert(name, data),
                Err(e) => out.warnings.push(format!("{path}: {e}")),
            }
        }
        Ok(out)
    }

    pub fn from_dir(dir: &Path) -> std::io::Result<SkinFiles> {
        let mut out = SkinFiles::default();
        out.walk(dir, 0, true)?;
        Ok(out)
    }

    fn walk(&mut self, dir: &Path, depth: usize, top: bool) -> std::io::Result<()> {
        let mut entries = match std::fs::read_dir(dir) {
            Ok(rd) => rd
                .filter_map(Result::ok)
                .map(|e| e.path())
                .collect::<Vec<_>>(),
            Err(e) if !top => {
                self.warnings.push(format!("{}: {e}", dir.display()));
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        entries.sort();
        for path in entries {
            if path.is_dir() {
                if depth < MAX_DEPTH {
                    self.walk(&path, depth + 1, false)?;
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
            match std::fs::metadata(&path) {
                Ok(m) if m.len() > MAX_FILE => self
                    .warnings
                    .push(format!("{}: larger than {MAX_FILE} bytes", path.display())),
                Ok(_) => match std::fs::read(&path) {
                    Ok(data) => self.insert(name, data),
                    Err(e) => self.warnings.push(format!("{}: {e}", path.display())),
                },
                Err(e) => self.warnings.push(format!("{}: {e}", path.display())),
            }
        }
        Ok(())
    }

    fn insert(&mut self, name: String, data: Vec<u8>) {
        self.arrivals += 1;
        self.files.insert(name, (self.arrivals, data));
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
