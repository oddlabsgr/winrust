//! Public types shared between the engine and its users.

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

/// Where audio comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    File(PathBuf),
    Url(String),
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::File(p) => write!(f, "{}", p.display()),
            Source::Url(u) => f.write_str(u),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
    Buffering,
}

/// Facts about the track that just became audible.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackInfo {
    pub source: Source,
    /// Average bitrate in kbit/s, when known.
    pub kbps: Option<u32>,
    /// Sample rate of the *source* in Hz (not the device rate).
    pub sample_rate: u32,
    /// Channel count of the *source*.
    pub channels: u16,
    pub duration: Option<Duration>,
    pub seekable: bool,
}

/// Winamp's ten equalizer band centers in Hz.
pub const EQ_BANDS_HZ: [f32; 10] = [
    60.0, 170.0, 310.0, 600.0, 1_000.0, 3_000.0, 6_000.0, 12_000.0, 14_000.0, 16_000.0,
];

/// Maximum boost/cut of a band or the preamp, in dB.
pub const EQ_MAX_DB: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EqSettings {
    pub enabled: bool,
    pub preamp_db: f32,
    pub bands_db: [f32; 10],
}

impl Default for EqSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            preamp_db: 0.0,
            bands_db: [0.0; 10],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Replace whatever is playing and start playing `Source`.
    Load(Source),
    /// Play `Source` right after the current track ends, with no gap.
    QueueNext(Source),
    Play,
    Pause,
    Stop,
    Seek(Duration),
    /// Slider value 0..=100; mapped through [`crate::volume::slider_to_gain`].
    SetVolume(u8),
    /// -1.0 (left) ..= 1.0 (right).
    SetBalance(f32),
    SetEq(EqSettings),
    /// `None` = system default device.
    SetDevice(Option<String>),
    Shutdown,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EngineEvent {
    State(PlaybackState),
    /// Position inside the current track, from samples actually played. ~10 Hz while playing.
    Position(Duration),
    /// A track became audible. `gapless` is true when it followed the previous one via `QueueNext`.
    TrackStarted {
        info: TrackInfo,
        gapless: bool,
    },
    /// Current bitrate in kbit/s (changes for VBR files and streams). ~1 Hz.
    Bitrate(u32),
    StreamTitle(String),
    /// The last queued track finished; the engine is now stopped.
    TrackEnded,
    /// Something failed. `source` names the track it concerns (unreadable file, queued
    /// track that failed to open, decode error) so a player can skip exactly that entry;
    /// `None` for device problems.
    Error {
        source: Option<Source>,
        error: EngineError,
    },
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum EngineError {
    #[error("cannot open {path}: {reason}")]
    Open { path: String, reason: String },
    #[error("unsupported format: {0}")]
    Unsupported(String),
    #[error("decode failed: {0}")]
    Decode(String),
    #[error("stream error: {0}")]
    Stream(String),
    #[error("audio device error: {0}")]
    Device(String),
}
