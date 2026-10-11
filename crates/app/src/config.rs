//! The settings kept between runs (M3 spec §5.9): `config.toml` in the OS config folder,
//! written whole through a temporary file a second after the last change, and on exit.

use crate::model::Model;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use winrust_ui::view::TimeMode;
use winrust_ui::vis::VisMode;

pub const VERSION: u32 = 1;
const FILE: &str = "config.toml";
const BACKUP: &str = "config.toml.bak";
const TEMP: &str = "config.toml.tmp";
/// Saves wait this long after the last change.
pub const SAVE_AFTER: Duration = Duration::from_secs(1);

#[derive(Serialize, Deserialize)]
#[serde(remote = "TimeMode", rename_all = "lowercase")]
enum TimeModeDef {
    Elapsed,
    Remaining,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "VisMode", rename_all = "lowercase")]
enum VisModeDef {
    Spectrum,
    Oscilloscope,
    Off,
}

/// Unknown fields are ignored and missing ones take their defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    /// The main window's outer position, in physical pixels.
    pub position: Option<[i32; 2]>,
    pub double_size: bool,
    pub always_on_top: bool,
    pub volume: u8,
    pub balance: i8,
    #[serde(with = "TimeModeDef")]
    pub time_mode: TimeMode,
    #[serde(with = "VisModeDef")]
    pub vis_mode: VisMode,
    pub shuffle: bool,
    pub repeat: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: VERSION,
            position: None,
            double_size: false,
            always_on_top: false,
            volume: 75,
            balance: 0,
            time_mode: TimeMode::Elapsed,
            vis_mode: VisMode::Spectrum,
            shuffle: false,
            repeat: false,
        }
    }
}

impl Config {
    pub fn from_model(m: &Model) -> Self {
        Config {
            version: VERSION,
            position: m.position.map(|(x, y)| [x, y]),
            double_size: m.double_size,
            always_on_top: m.always_on_top,
            volume: m.volume,
            balance: m.balance,
            time_mode: m.time_mode,
            vis_mode: m.vis_mode,
            shuffle: m.queue.shuffle,
            repeat: m.queue.repeat,
        }
    }

    /// Puts the settings into `m`, clamping values out of range.
    pub fn apply(&self, m: &mut Model) {
        m.position = self.position.map(|[x, y]| (x, y));
        m.double_size = self.double_size;
        m.always_on_top = self.always_on_top;
        m.volume = self.volume.min(100);
        m.balance = self.balance.clamp(-100, 100);
        m.time_mode = self.time_mode;
        m.vis_mode = self.vis_mode;
        m.queue.shuffle = self.shuffle;
        m.queue.repeat = self.repeat;
    }
}

/// The config folder: `%APPDATA%\winrust`, `~/Library/Application Support/winrust` or
/// `~/.config/winrust` (umbrella spec §9.4).
pub fn dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|d| d.config_dir().join("winrust"))
}

/// Reads `config.toml` from `dir`. A missing file gives the defaults. An unreadable or
/// invalid one is renamed `config.toml.bak` and gives the defaults, with a message saying so.
pub fn load(dir: &Path) -> (Config, Option<String>) {
    let path = dir.join(FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return (Config::default(), None),
        Err(_) => String::new(),
    };
    match toml::from_str(&text) {
        Ok(config) if !text.is_empty() => (config, None),
        _ => {
            let _ = std::fs::rename(&path, dir.join(BACKUP));
            let warning = "Settings reset: config.toml was unreadable".to_string();
            (Config::default(), Some(warning))
        }
    }
}

/// Writes `config.toml` into `dir`, creating the folder. The text goes to a temporary file
/// first and is renamed over the old one, so a crash never leaves half a file.
pub fn save(dir: &Path, config: &Config) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let text = toml::to_string(config).map_err(io::Error::other)?;
    let temp = dir.join(TEMP);
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, dir.join(FILE))
}

/// When to save: [`SAVE_AFTER`] past the last change.
#[derive(Clone, Debug, Default)]
pub struct Debounce {
    due: Option<Instant>,
}

impl Debounce {
    /// A setting changed.
    pub fn poke(&mut self, now: Instant) {
        self.due = Some(now + SAVE_AFTER);
    }

    pub fn due(&self) -> Option<Instant> {
        self.due
    }

    /// True once, when the save is due.
    pub fn take(&mut self, now: Instant) -> bool {
        if self.due.is_some_and(|due| now >= due) {
            self.due = None;
            return true;
        }
        false
    }

    /// True if a save is waiting at all (on exit).
    pub fn take_pending(&mut self) -> bool {
        self.due.take().is_some()
    }
}

/// A monitor's area, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// How much of the title bar must show to count as on screen: its full height, this wide.
const MIN_VISIBLE: i64 = 32;

/// Where the main window opens: the saved position if enough of its title bar shows on a
/// monitor to drag it, else centred on the primary monitor (`None` without one: the OS
/// decides).
pub fn place(
    saved: Option<[i32; 2]>,
    size: (u32, u32),
    monitors: &[Area],
    primary: Option<Area>,
) -> Option<(i32, i32)> {
    if let Some([x, y]) = saved
        && monitors.iter().any(|m| title_bar_shows((x, y), size, m))
    {
        return Some((x, y));
    }
    let p = primary?;
    let centre = |start: i32, space: u32, len: u32| start + (space.saturating_sub(len) / 2) as i32;
    Some((centre(p.x, p.w, size.0), centre(p.y, p.h, size.1)))
}

fn title_bar_shows((x, y): (i32, i32), (w, h): (u32, u32), m: &Area) -> bool {
    let (x, y, mx, my) = (i64::from(x), i64::from(y), i64::from(m.x), i64::from(m.y));
    let bar = i64::from(h.min(14));
    let overlap = (x + i64::from(w)).min(mx + i64::from(m.w)) - x.max(mx);
    overlap >= MIN_VISIBLE && y >= my && y + bar <= my + i64::from(m.h)
}
