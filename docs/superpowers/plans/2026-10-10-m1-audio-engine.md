# M1 Audio Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `winrust-audio`, Winrust's playback engine, plus the headless `winrust-cli` that drives it. The engine decodes local files and internet radio, resamples to the device, applies preamp/EQ/balance and volume, plays gaplessly, seeks, reports stream titles and feeds a visualization tap.

**Architecture:** Implements spec §5.2 and §6. A **decode thread** (`Worker` in `engine.rs`) owns the decoder, resampler, DSP chain and a `Timeline`. It pushes processed stereo into a lock-free SPSC ring (rtrb). The **audio callback** runs `Renderer::render`: ring → volume → device, plus the analyzer tap, and touches shared state only through atomics and `try_lock`. For streams, a **net thread** reads HTTP, strips ICY metadata and feeds a byte pipe that symphonia reads. Pure modules (EQ, DSP, resampler, ICY, timeline, renderer) are unit-tested. The engine is tested end to end through `CaptureSink`, a test output that records audio without a sound card.

**Tech Stack:** symphonia 0.6.1, cpal 0.18.2, rubato 5.0.1, rtrb 0.4, triple_buffer 9, crossbeam-channel 0.5, rustls 0.23 (ring provider) + webpki-roots 1.0 + url 2.5, thiserror 2, clap 4.6. Tests: hound, tempfile. Fixture generation (developer machines only): ffmpeg.

**Spec:** `docs/superpowers/specs/2026-10-10-winrust-design.md` — §6 (audio engine), §10 (errors), §11 (testing), §12.3 (M1 row), §13 (decisions this plan fixes).

**Prerequisite:** M0 complete (workspace, CI, deny policy).

**Provenance:** every code block in this plan was compiled and its tests run on Windows 11 with Rust 1.99 before the plan was written: 86 tests passing, clippy `-D warnings` clean, `cargo deny check` clean. One exception: the two fixture-based tests in Tasks 6 and 13 (`decodes_every_supported_format`, `mp3_halves_play_back_gaplessly`) could not be run because ffmpeg was not installed. Their tolerances are reasoned, not measured. Treat a failure there as something to investigate, not as a reason to loosen the numbers.

## Decisions this plan fixes (spec §13, owned by M1)

| Decision | Value |
|---|---|
| Volume curve | slider 0..=100 → gain `(slider/100)²` (50 % ≈ −12 dB). Applied in the callback, ramped across one buffer. |
| EQ filters | Band 1 (60 Hz): low shelf, Q 0.707. Band 10 (16 kHz): high shelf, Q 0.707. Bands 2–9: peaking, Q 1.4. All ±12 dB, preamp ±12 dB. Bands ≥ 0.45 × sample rate are bypassed. EQ off also bypasses the preamp (Winamp behavior). |
| Ring buffer | Capacity 8 s of stereo at 48 kHz. Files: start after 50 ms queued, keep 200 ms ahead (so EQ/balance changes are heard within 200 ms). Streams: prebuffer 2 s, keep 4 s ahead, rebuffer after an underrun. |
| Preamp ramp | 256 frames. |
| Resampler | rubato sinc, 128 taps, Blackman², 256× oversampling, quadratic interpolation, 1024-frame chunks. Pass-through when rates match. |

## Interface refinements vs spec §6.1

Spec §6.1 is a sketch. These refinements make the API simpler for M3:

- `Command::SetVolume(u8)` takes the slider value (0..=100). The engine applies the curve, and `EngineHandle` writes it straight into the callback's atomic, with no thread hop.
- The spec's `TrackInfo` event is `EngineEvent::TrackStarted { info, gapless }`. It fires when the track's first sample is *audible*, not when decoding starts. Bitrate changes (VBR, streams) come as a separate `EngineEvent::Bitrate(u32)` at about 1 Hz.
- `TrackInfo.sample_rate` is in Hz (the UI divides by 1000 for the kHz display).
- `Engine::spawn(sink) -> Result<Engine, EngineError>`, with `handle()`, `events()` and `take_analyzer()`, instead of a tuple. `Command::Shutdown` exists. Dropping `Engine` shuts it down.
- No ureq: old SHOUTcast servers answer `ICY 200 OK`, which general HTTP clients reject. `http.rs` is a small HTTP/1.0 client (http/https, redirects, ICY headers).
- New crate `crates/cli` (`winrust-cli`), the headless player named in the spec's M1 row. Spec §5.1 lists only the five long-lived crates.

## Known limitations (accepted for M1)

- HE-AAC (AAC+) streams play their AAC-LC core only (spec §6.6).
- When a stream stalls mid-play without closing, the decode thread blocks inside the read, so state/position events pause until data resumes or the 15 s read timeout fires. Stop and Load stay prompt, because the cancel epoch interrupts the read within 50 ms. M3 can show "buffering" from its own UI timer if needed.

## Deferred out of M1

- **Fuzzing (spec §11, §12.1).** The ICY parser (`icy::parse_stream_title`, `IcyReader`) and HTTP head parser (`http::read_head`) are fuzz candidates. The cargo-fuzz harness and the nightly CI job are set up in M2, together with the BMP and `.wsz` targets, so the infrastructure is built once. M2's plan must include `icy` and `http_head` targets.
- **Logging (spec §10).** `tracing` setup belongs to the app (M3). The engine reports problems through `EngineEvent::Error`.

## Global Constraints

- All M0 constraints still apply: license `MIT OR Apache-2.0`, `publish = false`, deny allowlist, gates (`fmt --check`, `clippy --workspace --all-targets -- -D warnings`, `test --workspace`) on every commit, LF files, the commit trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`, bash commands from the repo root.
- Audio callback code (`Renderer::render` and the cpal data closure in `sink.rs`) must not allocate (except growing the scratch buffer when a device asks for a bigger buffer than ever before), must not block (`try_lock` only, never `lock`), and must not panic.
- Library code never panics on bad media, bad network data or missing files. It returns `EngineError` or skips the bad packet.
- symphonia features exactly: `mp3, aac, alac, flac, vorbis, pcm, ogg, isomp4, wav, id3v1, id3v2` (`default-features = false`).
- Tests never open a real audio device: CI runners have no sound card. Use `CaptureSink`.
- `crates/audio` must not depend on winit, egui or any UI crate (spec §5.1 rule).

## Review Focus

Spec-implied conditions a user is likely to hit that the basic tests don't cover, most likely first. Each one is pinned by a test in the task that owns the code:

1. **Output device changes rate or is swapped mid-track** (headphones plugged in, Windows default device switched): the same track resumes near the same position at the new rate. Pinned by `device_rate_change_resumes_same_track_near_same_position` (Task 13).
2. **Internet radio stalls without closing and the user presses Stop:** the engine stops within 1 s and shows no spurious error. Pinned by `stop_is_prompt_while_stream_is_stalled` (Task 13). That test found a real bug during planning: no `Buffering` state was sent while a stream was connecting.
3. **User clicks rapidly through tracks:** only the last one plays, and it ends exactly once. Pinned by `rapid_loads_end_on_the_last_one` (Task 13).
4. **Music library under non-ASCII folders** (Greek, CJK, symbols): plays normally. Pinned by `plays_from_non_ascii_path` (Task 13).
5. **Seek past the end, and high-rate files (96 kHz):** the track ends cleanly, and downsampled pitch and length stay correct. Pinned by `seek_past_the_end_just_ends_the_track` (Task 13) and `downsampling_keeps_length_and_pitch` (Task 4).

---

### Task 1: `winrust-audio` crate, public types, volume and balance curves

**Files:**
- Create: `crates/audio/Cargo.toml`
- Create: `crates/audio/src/lib.rs`
- Create: `crates/audio/src/types.rs`
- Create: `crates/audio/src/volume.rs`
- Modify: `.github/workflows/ci.yml` (Linux ALSA headers, needed by cpal)

**Interfaces:**
- Consumes: workspace from M0.
- Produces (used by every later task):
  - `enum Source { File(PathBuf), Url(String) }` with `Display`
  - `enum PlaybackState { Stopped, Playing, Paused, Buffering }`
  - `struct TrackInfo { source: Source, kbps: Option<u32>, sample_rate: u32, channels: u16, duration: Option<Duration>, seekable: bool }`
  - `const EQ_BANDS_HZ: [f32; 10]`, `const EQ_MAX_DB: f32 = 12.0`
  - `struct EqSettings { enabled: bool, preamp_db: f32, bands_db: [f32; 10] }` (`Default`: disabled, flat)
  - `enum Command { Load(Source), QueueNext(Source), Play, Pause, Stop, Seek(Duration), SetVolume(u8), SetBalance(f32), SetEq(EqSettings), SetDevice(Option<String>), Shutdown }`
  - `enum EngineEvent { State(PlaybackState), Position(Duration), TrackStarted { info: TrackInfo, gapless: bool }, Bitrate(u32), StreamTitle(String), TrackEnded, Error(EngineError) }`
  - `enum EngineError { Open { path: String, reason: String }, Unsupported(String), Decode(String), Stream(String), Device(String) }` (thiserror, `Clone + PartialEq`)
  - `volume::slider_to_gain(u8) -> f32`, `volume::balance_gains(f32) -> (f32, f32)`

- [ ] **Step 1: Create the crate manifest with all M1 dependencies**

`crates/audio/Cargo.toml`:

```toml
[package]
name = "winrust-audio"
description = "Winrust audio engine: decoding, internet radio, DSP and output"
version.workspace = true
edition.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
cpal = "0.18.2"
crossbeam-channel = "0.5.17"
rtrb = "0.4.0"
rubato = { version = "5.0.1", default-features = false }
rustls = { version = "0.23.45", default-features = false, features = ["ring", "std", "tls12"] }
symphonia = { version = "0.6.1", default-features = false, features = ["mp3", "aac", "alac", "flac", "vorbis", "pcm", "ogg", "isomp4", "wav", "id3v1", "id3v2"] }
thiserror = "2.0.21"
triple_buffer = "9.0.0"
url = "2.5.8"
webpki-roots = "1.0.9"

[dev-dependencies]
hound = "3.5.1"
tempfile = "3"
```

- [ ] **Step 2: Write the failing tests for the curves**

`crates/audio/src/lib.rs` (first version; later tasks add one `pub mod` line each, and Task 13 gives the final file):

```rust
//! Winrust audio engine: decoding, streaming, DSP and output.

pub mod types;
pub mod volume;

pub use types::*;
```

`crates/audio/src/types.rs`:

```rust
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
    Error(EngineError),
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
```

`crates/audio/src/volume.rs` — tests only for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_endpoints() {
        assert_eq!(slider_to_gain(0), 0.0);
        assert_eq!(slider_to_gain(100), 1.0);
        assert_eq!(slider_to_gain(255), 1.0);
    }

    #[test]
    fn volume_half_is_about_minus_12_db() {
        let db = 20.0 * slider_to_gain(50).log10();
        assert!((db - -12.04).abs() < 0.1, "{db}");
    }

    #[test]
    fn balance_center_is_unity() {
        assert_eq!(balance_gains(0.0), (1.0, 1.0));
    }

    #[test]
    fn balance_right_attenuates_left() {
        assert_eq!(balance_gains(0.25), (0.75, 1.0));
        assert_eq!(balance_gains(1.0), (0.0, 1.0));
    }

    #[test]
    fn balance_left_attenuates_right() {
        assert_eq!(balance_gains(-0.5), (1.0, 0.5));
        assert_eq!(balance_gains(-7.0), (1.0, 0.0));
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p winrust-audio volume::`
Expected: compile errors `cannot find function `slider_to_gain` in this scope` and `cannot find function `balance_gains` in this scope`.

- [ ] **Step 4: Implement the curves above the tests module**

Put this at the top of `crates/audio/src/volume.rs`, above `#[cfg(test)]`:

```rust
//! Volume and balance curves.

/// Maps the 0..=100 volume slider to a linear gain.
///
/// Squared curve: 50% is about -12 dB, 25% about -24 dB. 0 is silence, 100 is unity.
pub fn slider_to_gain(slider: u8) -> f32 {
    let x = f32::from(slider.min(100)) / 100.0;
    x * x
}

/// Per-channel gains for a balance value in -1.0 (left) ..= 1.0 (right).
///
/// Winamp-style: the far channel is attenuated linearly, the near channel stays at unity.
pub fn balance_gains(balance: f32) -> (f32, f32) {
    let b = balance.clamp(-1.0, 1.0);
    if b >= 0.0 {
        (1.0 - b, 1.0)
    } else {
        (1.0, 1.0 + b)
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p winrust-audio volume::`
Expected: `test result: ok. 5 passed`.

- [ ] **Step 6: Add ALSA headers to Linux CI**

In `.github/workflows/ci.yml`, insert this step into the `test` job right after `Swatinem/rust-cache@v2` (cpal links against ALSA on Linux):

```yaml
      - name: Install ALSA headers
        if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y libasound2-dev pkg-config
```

The complete file after this step:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

env:
  CARGO_TERM_COLOR: always

jobs:
  fmt:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all --check

  test:
    name: test (${{ matrix.os }})
    strategy:
      fail-fast: false
      matrix:
        os: [windows-latest, macos-latest, ubuntu-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - name: Install ALSA headers
        if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y libasound2-dev pkg-config
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace

  deny:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v2
```

- [ ] **Step 7: Gates and commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo deny check
git add crates/audio .github/workflows/ci.yml Cargo.lock
git commit -m "feat(audio): add winrust-audio crate with engine types and volume curves" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: all gates pass. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok` (it may print `warning[duplicate]` for `windows-sys`, which is allowed).

---

### Task 2: Ten-band equalizer

**Files:**
- Create: `crates/audio/src/eq.rs`
- Modify: `crates/audio/src/lib.rs` (add `pub mod eq;`)

**Interfaces:**
- Consumes: `EQ_BANDS_HZ`, `EQ_MAX_DB` (Task 1).
- Produces:
  - `struct Biquad` with `const IDENTITY`, `fn peaking(fs, f0, q, gain_db)`, `fn low_shelf(...)`, `fn high_shelf(...)`, `fn response_db(&self, fs: f32, f: f32) -> f32`
  - `fn band_filter(fs: f32, index: usize, gain_db: f32) -> Biquad`
  - `struct Equalizer` with `fn new(sample_rate: u32)`, `fn set_bands(&mut self, &[f32; 10])`, `fn process(&mut self, stereo: &mut [f32])` (interleaved stereo, in place), `fn reset(&mut self)`

- [ ] **Step 1: Write the failing tests**

Add `pub mod eq;` to `crates/audio/src/lib.rs` (keep the `pub mod` lines alphabetical). Create `crates/audio/src/eq.rs` with only the tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 44_100.0;

    #[test]
    fn zero_gain_is_identity() {
        for i in 0..10 {
            assert_eq!(band_filter(FS, i, 0.0), Biquad::IDENTITY);
        }
    }

    #[test]
    fn peaking_band_hits_gain_at_center() {
        for (i, &f0) in EQ_BANDS_HZ.iter().enumerate().take(9).skip(1) {
            let boost = band_filter(FS, i, 12.0).response_db(FS, f0);
            assert!((boost - 12.0).abs() < 0.05, "band {i}: {boost}");
            let cut = band_filter(FS, i, -12.0).response_db(FS, f0);
            assert!((cut + 12.0).abs() < 0.05, "band {i} cut: {cut}");
        }
    }

    #[test]
    fn low_shelf_boosts_deep_bass() {
        let f = band_filter(FS, 0, 12.0);
        assert!((f.response_db(FS, 20.0) - 12.0).abs() < 1.0);
        assert!(f.response_db(FS, 1_000.0).abs() < 0.5);
    }

    #[test]
    fn high_shelf_boosts_top_end() {
        let f = band_filter(FS, 9, 12.0);
        assert!((f.response_db(FS, 20_000.0) - 12.0).abs() < 1.5);
        assert!(f.response_db(FS, 1_000.0).abs() < 0.5);
    }

    #[test]
    fn gain_is_clamped_to_12_db() {
        assert_eq!(band_filter(FS, 4, 30.0), band_filter(FS, 4, 12.0));
    }

    #[test]
    fn band_above_nyquist_limit_is_identity() {
        assert_eq!(band_filter(22_050.0, 9, 12.0), Biquad::IDENTITY);
    }

    #[test]
    fn flat_equalizer_leaves_samples_untouched() {
        let mut eq = Equalizer::new(44_100);
        eq.set_bands(&[0.0; 10]);
        let mut buf: Vec<f32> = (0..64).map(|i| (i as f32 * 0.1).sin()).collect();
        let before = buf.clone();
        eq.process(&mut buf);
        assert_eq!(buf, before);
    }

    #[test]
    fn boosted_equalizer_amplifies_its_band() {
        let mut eq = Equalizer::new(44_100);
        let mut bands = [0.0; 10];
        bands[4] = 12.0; // 1 kHz
        eq.set_bands(&bands);
        let mut buf: Vec<f32> = (0..44_100)
            .flat_map(|i| {
                let s = 0.1 * (2.0 * PI * 1_000.0 * i as f32 / FS).sin();
                [s, s]
            })
            .collect();
        eq.process(&mut buf);
        // Skip 0.1 s of filter settling, then measure the peak.
        let peak = buf[2 * 4_410..].iter().fold(0f32, |m, s| m.max(s.abs()));
        let db = 20.0 * (peak / 0.1).log10();
        assert!((db - 12.0).abs() < 0.3, "{db}");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p winrust-audio eq::`
Expected: compile errors (`band_filter`, `Biquad`, `Equalizer`, `PI` not found).

- [ ] **Step 3: Implement above the tests module**

```rust
//! Ten-band Winamp-style equalizer built from RBJ cookbook biquads.
//!
//! Band 1 (60 Hz) is a low shelf, band 10 (16 kHz) a high shelf, bands 2..=9 are
//! peaking filters with Q = 1.4. Bands at 0 dB are exact identity filters.

use crate::types::{EQ_BANDS_HZ, EQ_MAX_DB};
use std::f32::consts::PI;

const PEAK_Q: f32 = 1.4;
const SHELF_Q: f32 = std::f32::consts::FRAC_1_SQRT_2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Biquad {
    pub const IDENTITY: Biquad = Biquad {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    fn from_raw(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        Biquad {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    pub fn peaking(fs: f32, f0: f32, q: f32, gain_db: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0 / fs;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        Self::from_raw(
            1.0 + alpha * a,
            -2.0 * cos,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cos,
            1.0 - alpha / a,
        )
    }

    pub fn low_shelf(fs: f32, f0: f32, q: f32, gain_db: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0 / fs;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let sq = 2.0 * a.sqrt() * alpha;
        Self::from_raw(
            a * ((a + 1.0) - (a - 1.0) * cos + sq),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
            a * ((a + 1.0) - (a - 1.0) * cos - sq),
            (a + 1.0) + (a - 1.0) * cos + sq,
            -2.0 * ((a - 1.0) + (a + 1.0) * cos),
            (a + 1.0) + (a - 1.0) * cos - sq,
        )
    }

    pub fn high_shelf(fs: f32, f0: f32, q: f32, gain_db: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0 / fs;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let sq = 2.0 * a.sqrt() * alpha;
        Self::from_raw(
            a * ((a + 1.0) + (a - 1.0) * cos + sq),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
            a * ((a + 1.0) + (a - 1.0) * cos - sq),
            (a + 1.0) - (a - 1.0) * cos + sq,
            2.0 * ((a - 1.0) - (a + 1.0) * cos),
            (a + 1.0) - (a - 1.0) * cos - sq,
        )
    }

    /// Magnitude response in dB at `f` Hz.
    pub fn response_db(&self, fs: f32, f: f32) -> f32 {
        let w = 2.0 * PI * f / fs;
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let (nr, ni) = (
            self.b0 + self.b1 * c1 + self.b2 * c2,
            -(self.b1 * s1 + self.b2 * s2),
        );
        let (dr, di) = (
            1.0 + self.a1 * c1 + self.a2 * c2,
            -(self.a1 * s1 + self.a2 * s2),
        );
        10.0 * ((nr * nr + ni * ni) / (dr * dr + di * di)).log10()
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct State {
    z1: f32,
    z2: f32,
}

impl State {
    #[inline]
    fn run(&mut self, c: &Biquad, x: f32) -> f32 {
        // Transposed direct form II.
        let y = c.b0 * x + self.z1;
        self.z1 = c.b1 * x - c.a1 * y + self.z2;
        self.z2 = c.b2 * x - c.a2 * y;
        y
    }
}

/// Filter for band `index` (0..10) at `gain_db`. Bands too close to Nyquist become identity.
pub fn band_filter(fs: f32, index: usize, gain_db: f32) -> Biquad {
    let f0 = EQ_BANDS_HZ[index];
    let gain_db = gain_db.clamp(-EQ_MAX_DB, EQ_MAX_DB);
    if gain_db == 0.0 || f0 >= 0.45 * fs {
        return Biquad::IDENTITY;
    }
    match index {
        0 => Biquad::low_shelf(fs, f0, SHELF_Q, gain_db),
        9 => Biquad::high_shelf(fs, f0, SHELF_Q, gain_db),
        _ => Biquad::peaking(fs, f0, PEAK_Q, gain_db),
    }
}

/// Stereo ten-band equalizer.
pub struct Equalizer {
    fs: f32,
    coeffs: [Biquad; 10],
    state: [[State; 10]; 2],
    active: bool,
}

impl Equalizer {
    pub fn new(sample_rate: u32) -> Self {
        Equalizer {
            fs: sample_rate as f32,
            coeffs: [Biquad::IDENTITY; 10],
            state: [[State::default(); 10]; 2],
            active: false,
        }
    }

    /// Sets band gains in dB. Filter state is kept so changes do not click.
    pub fn set_bands(&mut self, bands_db: &[f32; 10]) {
        for (i, gain) in bands_db.iter().enumerate() {
            self.coeffs[i] = band_filter(self.fs, i, *gain);
        }
        self.active = self.coeffs.iter().any(|c| *c != Biquad::IDENTITY);
    }

    /// Processes interleaved stereo samples in place.
    pub fn process(&mut self, stereo: &mut [f32]) {
        if !self.active {
            return;
        }
        for frame in stereo.as_chunks_mut::<2>().0 {
            for (ch, sample) in frame.iter_mut().enumerate() {
                let mut x = *sample;
                for (st, c) in self.state[ch].iter_mut().zip(&self.coeffs) {
                    x = st.run(c, x);
                }
                *sample = x;
            }
        }
    }

    pub fn reset(&mut self) {
        self.state = [[State::default(); 10]; 2];
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p winrust-audio eq::`
Expected: `test result: ok. 8 passed`.

- [ ] **Step 5: Gates and commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/audio/src/eq.rs crates/audio/src/lib.rs
git commit -m "feat(audio): add 10-band biquad equalizer" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: DSP chain (channel mapping, preamp ramp, EQ, balance, clamp)

**Files:**
- Create: `crates/audio/src/dsp.rs`
- Modify: `crates/audio/src/lib.rs` (add `pub mod dsp;`)

**Interfaces:**
- Consumes: `Equalizer` (Task 2), `EqSettings`, `EQ_MAX_DB` (Task 1), `balance_gains` (Task 1).
- Produces:
  - `const RAMP_FRAMES: u32 = 256`
  - `fn to_stereo(input: &[f32], channels: usize, out: &mut Vec<f32>)` — appends interleaved stereo
  - `struct DspChain` with `fn new(sample_rate: u32)`, `fn set_eq(&mut self, &EqSettings)`, `fn set_balance(&mut self, f32)`, `fn process(&mut self, stereo: &mut [f32])`, `fn reset(&mut self)`

- [ ] **Step 1: Write the failing tests**

Add `pub mod dsp;` to `lib.rs`. Create `crates/audio/src/dsp.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_is_duplicated() {
        let mut out = Vec::new();
        to_stereo(&[0.1, 0.2], 1, &mut out);
        assert_eq!(out, vec![0.1, 0.1, 0.2, 0.2]);
    }

    #[test]
    fn stereo_passes_through() {
        let mut out = Vec::new();
        to_stereo(&[0.1, 0.2, 0.3, 0.4], 2, &mut out);
        assert_eq!(out, vec![0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn surround_is_downmixed_without_clipping() {
        let mut out = Vec::new();
        to_stereo(&[1.0; 6], 6, &mut out);
        assert_eq!(out.len(), 2);
        assert!(
            (out[0] - 1.0).abs() < 1e-6 && (out[1] - 1.0).abs() < 1e-6,
            "{out:?}"
        );
    }

    #[test]
    fn default_chain_is_transparent() {
        let mut dsp = DspChain::new(44_100);
        let mut buf = vec![0.5, -0.25, 0.1, 0.9];
        dsp.process(&mut buf);
        assert_eq!(buf, vec![0.5, -0.25, 0.1, 0.9]);
    }

    #[test]
    fn preamp_ramps_instead_of_jumping() {
        let mut dsp = DspChain::new(44_100);
        dsp.set_eq(&EqSettings {
            enabled: true,
            preamp_db: -6.0,
            bands_db: [0.0; 10],
        });
        let mut buf = vec![1.0; 2 * RAMP_FRAMES as usize * 2];
        dsp.process(&mut buf);
        let left: Vec<f32> = buf.iter().step_by(2).copied().collect();
        assert!(left[0] > 0.99, "first frame barely changed: {}", left[0]);
        assert!(left.windows(2).all(|w| w[1] <= w[0]), "monotonic ramp");
        let target = db_to_gain(-6.0);
        assert!((left[left.len() - 1] - target).abs() < 1e-4);
    }

    #[test]
    fn disabled_eq_bypasses_preamp() {
        let mut dsp = DspChain::new(44_100);
        dsp.set_eq(&EqSettings {
            enabled: false,
            preamp_db: -12.0,
            bands_db: [12.0; 10],
        });
        let mut buf = vec![0.5; 2048];
        dsp.process(&mut buf);
        assert!(buf.iter().all(|s| *s == 0.5));
    }

    #[test]
    fn balance_and_clamp_are_applied() {
        let mut dsp = DspChain::new(44_100);
        dsp.set_balance(0.5);
        let mut buf = vec![0.8, 0.8, 3.0, -3.0];
        dsp.process(&mut buf);
        assert_eq!(buf, vec![0.4, 0.8, 1.0, -1.0]);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p winrust-audio dsp::`
Expected: compile errors (`to_stereo`, `DspChain`, `db_to_gain`, `RAMP_FRAMES` not found).

- [ ] **Step 3: Implement above the tests module**

```rust
//! Decode-thread DSP: channel mapping, preamp, equalizer, balance.

use crate::eq::Equalizer;
use crate::types::{EQ_MAX_DB, EqSettings};
use crate::volume::balance_gains;

/// Frames over which a preamp change is ramped, to avoid clicks.
pub const RAMP_FRAMES: u32 = 256;

/// Appends `input` (interleaved, `channels` wide) to `out` as interleaved stereo.
///
/// Mono is duplicated. More than two channels: channels 0/1 are kept as left/right and the
/// rest are mixed equally into both, then the result is scaled so full-scale input stays
/// full-scale.
pub fn to_stereo(input: &[f32], channels: usize, out: &mut Vec<f32>) {
    match channels {
        0 => {}
        1 => out.extend(input.iter().flat_map(|&s| [s, s])),
        2 => out.extend_from_slice(input),
        n => {
            let extra = (n - 2) as f32;
            let norm = 1.0 / (1.0 + 0.5 * extra);
            for frame in input.chunks_exact(n) {
                let rest: f32 = frame[2..].iter().sum::<f32>() * 0.5;
                out.push((frame[0] + rest) * norm);
                out.push((frame[1] + rest) * norm);
            }
        }
    }
}

/// Linear gain ramp.
#[derive(Clone, Copy, Debug)]
struct Ramp {
    current: f32,
    target: f32,
    step: f32,
}

impl Ramp {
    fn new(value: f32) -> Self {
        Ramp {
            current: value,
            target: value,
            step: 0.0,
        }
    }

    fn set(&mut self, target: f32) {
        self.target = target;
        self.step = (target - self.current) / RAMP_FRAMES as f32;
    }

    #[inline]
    fn next(&mut self) -> f32 {
        if self.current != self.target {
            self.current += self.step;
            if (self.step > 0.0 && self.current >= self.target)
                || (self.step < 0.0 && self.current <= self.target)
            {
                self.current = self.target;
            }
        }
        self.current
    }
}

fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db.clamp(-EQ_MAX_DB, EQ_MAX_DB) / 20.0)
}

/// Preamp → equalizer → balance, on interleaved stereo at the device rate.
pub struct DspChain {
    preamp: Ramp,
    eq: Equalizer,
    eq_enabled: bool,
    balance: f32,
}

impl DspChain {
    pub fn new(sample_rate: u32) -> Self {
        DspChain {
            preamp: Ramp::new(1.0),
            eq: Equalizer::new(sample_rate),
            eq_enabled: false,
            balance: 0.0,
        }
    }

    /// Applies EQ settings. A disabled EQ also bypasses the preamp (Winamp behavior).
    pub fn set_eq(&mut self, settings: &EqSettings) {
        self.eq_enabled = settings.enabled;
        if settings.enabled {
            self.eq.set_bands(&settings.bands_db);
            self.preamp.set(db_to_gain(settings.preamp_db));
        } else {
            self.preamp.set(1.0);
        }
    }

    pub fn set_balance(&mut self, balance: f32) {
        self.balance = balance;
    }

    /// Processes interleaved stereo in place and clamps to [-1, 1].
    pub fn process(&mut self, stereo: &mut [f32]) {
        for frame in stereo.as_chunks_mut::<2>().0 {
            let g = self.preamp.next();
            frame[0] *= g;
            frame[1] *= g;
        }
        if self.eq_enabled {
            self.eq.process(stereo);
        }
        let (gl, gr) = balance_gains(self.balance);
        for frame in stereo.as_chunks_mut::<2>().0 {
            frame[0] = (frame[0] * gl).clamp(-1.0, 1.0);
            frame[1] = (frame[1] * gr).clamp(-1.0, 1.0);
        }
    }

    /// Clears filter memory (after seek/stop) without touching settings.
    pub fn reset(&mut self) {
        self.eq.reset();
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p winrust-audio dsp::`
Expected: `test result: ok. 7 passed`.

- [ ] **Step 5: Gates and commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/audio/src/dsp.rs crates/audio/src/lib.rs
git commit -m "feat(audio): add DSP chain with preamp ramp, EQ and balance" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Streaming resampler

**Files:**
- Create: `crates/audio/src/resample.rs`
- Modify: `crates/audio/src/lib.rs` (add `pub mod resample;`)

**Interfaces:**
- Consumes: rubato 5 (`Async`, `Indexing`, `InterleavedSlice` via `rubato::audioadapter_buffers`).
- Produces: `struct StreamResampler` with `fn new(from: u32, to: u32)`, `fn is_passthrough(&self) -> bool`, `fn process(&mut self, input: &[f32], out: &mut Vec<f32>)` (interleaved stereo, appends), `fn flush(&mut self, out: &mut Vec<f32>)` (end of stream; then resets), `fn reset(&mut self)` (drop pending audio; used on seek/stop). The resampler's own delay is trimmed from the output, so positions stay accurate.

- [ ] **Step 1: Write the failing tests**

Add `pub mod resample;` to `lib.rs`. Create `crates/audio/src/resample.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn sine(rate: u32, freq: f32, secs: f32) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        (0..n)
            .flat_map(|i| {
                let s = 0.5 * (2.0 * PI * freq * i as f32 / rate as f32).sin();
                [s, s]
            })
            .collect()
    }

    fn zero_crossings(stereo: &[f32]) -> usize {
        let left: Vec<f32> = stereo.iter().step_by(2).copied().collect();
        left.windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count()
    }

    #[test]
    fn equal_rates_pass_through() {
        let mut r = StreamResampler::new(48_000, 48_000);
        assert!(r.is_passthrough());
        let mut out = Vec::new();
        r.process(&[0.1, 0.2], &mut out);
        assert_eq!(out, vec![0.1, 0.2]);
    }

    #[test]
    fn upsampling_keeps_length_and_pitch() {
        let input = sine(44_100, 440.0, 1.0);
        let mut r = StreamResampler::new(44_100, 48_000);
        let mut out = Vec::new();
        for chunk in input.chunks(2 * 700) {
            r.process(chunk, &mut out);
        }
        r.flush(&mut out);
        let frames = out.len() / 2;
        assert!((frames as i64 - 48_000).abs() < 200, "frames = {frames}");
        let crossings = zero_crossings(&out);
        assert!(
            (crossings as i64 - 440).abs() <= 2,
            "crossings = {crossings}"
        );
    }

    #[test]
    fn downsampling_keeps_length_and_pitch() {
        let input = sine(96_000, 440.0, 1.0);
        let mut r = StreamResampler::new(96_000, 48_000);
        let mut out = Vec::new();
        for chunk in input.chunks(2 * 999) {
            r.process(chunk, &mut out);
        }
        r.flush(&mut out);
        let frames = out.len() / 2;
        assert!((frames as i64 - 48_000).abs() < 200, "frames = {frames}");
        let crossings = zero_crossings(&out);
        assert!(
            (crossings as i64 - 440).abs() <= 2,
            "crossings = {crossings}"
        );
    }

    #[test]
    fn reset_discards_pending_audio() {
        let mut r = StreamResampler::new(44_100, 48_000);
        let mut out = Vec::new();
        r.process(&sine(44_100, 440.0, 0.01), &mut out);
        r.reset();
        r.flush(&mut out);
        assert!(out.is_empty());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p winrust-audio resample::`
Expected: compile error `cannot find type `StreamResampler``.

- [ ] **Step 3: Implement above the tests module**

```rust
//! Streaming stereo sample-rate conversion (rubato sinc), or pass-through when rates match.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Async, FixedAsync, Indexing, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};

const CHUNK_FRAMES: usize = 1024;

pub struct StreamResampler {
    inner: Option<Async<f32>>,
    /// Interleaved stereo input waiting for a full chunk.
    pending: Vec<f32>,
    scratch: Vec<f32>,
    /// Output frames still to drop to compensate the resampler's delay.
    skip_frames: usize,
}

impl StreamResampler {
    /// Converts stereo audio from `from` Hz to `to` Hz.
    pub fn new(from: u32, to: u32) -> Self {
        let inner = (from != to).then(|| {
            let params = SincInterpolationParameters::new(128, WindowFunction::Blackman2)
                .oversampling_factor(256)
                .interpolation(SincInterpolationType::Quadratic);
            Async::<f32>::new_sinc(
                f64::from(to) / f64::from(from),
                1.0,
                &params,
                CHUNK_FRAMES,
                2,
                FixedAsync::Input,
            )
            .expect("valid resampler parameters")
        });
        let mut r = StreamResampler {
            inner,
            pending: Vec::new(),
            scratch: Vec::new(),
            skip_frames: 0,
        };
        r.reset();
        r
    }

    pub fn is_passthrough(&self) -> bool {
        self.inner.is_none()
    }

    /// Feeds interleaved stereo `input`; appends converted interleaved stereo to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let Some(inner) = self.inner.as_mut() else {
            out.extend_from_slice(input);
            return;
        };
        self.pending.extend_from_slice(input);
        loop {
            let need = inner.input_frames_next();
            if self.pending.len() / 2 < need {
                break;
            }
            let produced = run_chunk(inner, &self.pending[..need * 2], None, &mut self.scratch);
            self.pending.drain(..need * 2);
            emit(&self.scratch[..produced * 2], &mut self.skip_frames, out);
        }
    }

    /// Pushes out everything still buffered (end of stream). Then resets.
    pub fn flush(&mut self, out: &mut Vec<f32>) {
        if let Some(inner) = self.inner.as_mut() {
            let frames = self.pending.len() / 2;
            if frames > 0 {
                let need = inner.input_frames_next();
                let mut chunk = std::mem::take(&mut self.pending);
                chunk.resize(need * 2, 0.0);
                let produced = run_chunk(inner, &chunk, Some(frames), &mut self.scratch);
                let ratio = inner.resample_ratio();
                let keep = ((frames as f64 * ratio).round() as usize).min(produced);
                emit(&self.scratch[..keep * 2], &mut self.skip_frames, out);
            }
        }
        self.reset();
    }

    /// Drops buffered audio and filter memory (after seek or stop).
    pub fn reset(&mut self) {
        self.pending.clear();
        if let Some(inner) = self.inner.as_mut() {
            inner.reset();
            self.skip_frames = inner.output_delay();
        }
    }
}

fn run_chunk(
    inner: &mut Async<f32>,
    input: &[f32],
    partial: Option<usize>,
    scratch: &mut Vec<f32>,
) -> usize {
    let in_frames = input.len() / 2;
    let out_cap = inner.output_frames_max();
    scratch.resize(out_cap * 2, 0.0);
    let in_adapter = InterleavedSlice::new(input, 2, in_frames).expect("input adapter");
    let mut out_adapter =
        InterleavedSlice::new_mut(&mut scratch[..], 2, out_cap).expect("output adapter");
    let mut indexing = Indexing::new();
    indexing.partial_len = partial;
    let (_, produced) = inner
        .process_into_buffer(&in_adapter, &mut out_adapter, Some(&indexing))
        .expect("resample chunk");
    produced
}

fn emit(frames: &[f32], skip_frames: &mut usize, out: &mut Vec<f32>) {
    let n = frames.len() / 2;
    let skip = (*skip_frames).min(n);
    *skip_frames -= skip;
    out.extend_from_slice(&frames[skip * 2..]);
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p winrust-audio resample::`
Expected: `test result: ok. 4 passed`. This covers Review Focus item 5 (96 kHz → 48 kHz).

- [ ] **Step 5: Gates and commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/audio/src/resample.rs crates/audio/src/lib.rs
git commit -m "feat(audio): add streaming sinc resampler" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Decoder (symphonia wrapper)

**Files:**
- Create: `crates/audio/src/decoder.rs`
- Modify: `crates/audio/src/lib.rs` (add `pub mod decoder;`)
- Test: `crates/audio/tests/decoder_wav.rs`

**Interfaces:**
- Consumes: `to_stereo` (Task 3), `Source`, `TrackInfo`, `EngineError` (Task 1).
- Produces: `struct Decoder` with
  - `fn open_file(path: &Path) -> Result<Decoder, EngineError>` — `Open` if the file can't be read, `Unsupported` if no decodable audio track; fills `TrackInfo.kbps` with the average bitrate (file size ÷ duration)
  - `fn open_media(source: Source, media: Box<dyn MediaSource>, hint: &Hint) -> Result<Decoder, EngineError>` — any source (used for streams)
  - `fn info(&self) -> &TrackInfo`, `fn set_kbps(&mut self, Option<u32>)`, `fn sample_rate(&self) -> u32`
  - `fn decode_next(&mut self, out: &mut Vec<f32>) -> Result<bool, EngineError>` — appends interleaved **stereo** at the source rate; `Ok(false)` at end; corrupt packets are skipped; chained Ogg streams re-select their track
  - `fn seek(&mut self, to: Duration) -> Result<Duration, EngineError>` — accurate seek; returns the position reached
  - `fn take_bitrate(&mut self) -> Option<u32>` — kbit/s over the audio decoded since the last call, once ≥ 1 s of audio
  - `fn take_title(&mut self) -> Option<String>` — in-band title changes ("Artist - Title")
  - `fn title_from_tags(tags: &[Tag]) -> Option<String>`

- [ ] **Step 1: Write the failing tests**

Add `pub mod decoder;` to `lib.rs`. Create `crates/audio/tests/decoder_wav.rs`:

```rust
use std::f32::consts::PI;
use std::path::Path;
use std::time::Duration;
use winrust_audio::decoder::Decoder;

fn write_tone(path: &Path, rate: u32, channels: u16, secs: f32) {
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    let n = (rate as f32 * secs) as usize;
    for i in 0..n {
        let s = (0.5 * (2.0 * PI * 440.0 * i as f32 / rate as f32).sin() * i16::MAX as f32) as i16;
        for _ in 0..channels {
            w.write_sample(s).unwrap();
        }
    }
    w.finalize().unwrap();
}

fn decode_all(dec: &mut Decoder) -> Vec<f32> {
    let mut out = Vec::new();
    while dec.decode_next(&mut out).unwrap() {}
    out
}

#[test]
fn decodes_mono_wav_to_stereo() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mono.wav");
    write_tone(&path, 22_050, 1, 0.5);
    let mut dec = Decoder::open_file(&path).unwrap();
    assert_eq!(dec.info().sample_rate, 22_050);
    assert_eq!(dec.info().channels, 1);
    assert!(dec.info().seekable);
    let d = dec.info().duration.unwrap();
    assert!((d.as_secs_f64() - 0.5).abs() < 0.001, "{d:?}");
    let out = decode_all(&mut dec);
    assert_eq!(out.len(), 2 * 11_025);
    assert_eq!(out[200], out[201], "mono duplicated to both channels");
}

#[test]
fn seek_lands_on_requested_time() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("seek.wav");
    write_tone(&path, 44_100, 2, 2.0);
    let mut dec = Decoder::open_file(&path).unwrap();
    let pos = dec.seek(Duration::from_millis(1500)).unwrap();
    assert!((pos.as_secs_f64() - 1.5).abs() < 0.001, "{pos:?}");
    let out = decode_all(&mut dec);
    let frames = out.len() / 2;
    assert!(
        (frames as i64 - 22_050).abs() <= 1,
        "frames after seek = {frames}"
    );
}

#[test]
fn missing_file_is_open_error() {
    let err = Decoder::open_file(Path::new("definitely/not/here.mp3"))
        .err()
        .unwrap();
    assert!(
        matches!(err, winrust_audio::types::EngineError::Open { .. }),
        "{err:?}"
    );
}

#[test]
fn garbage_file_is_unsupported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("garbage.mp3");
    std::fs::write(&path, vec![0x42u8; 4096]).unwrap();
    let err = Decoder::open_file(&path).err().unwrap();
    assert!(
        matches!(err, winrust_audio::types::EngineError::Unsupported(_)),
        "{err:?}"
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p winrust-audio --test decoder_wav`
Expected: compile error `could not find `decoder` in `winrust_audio`` (the module file does not exist yet, so `pub mod decoder;` also errors with `file not found for module`).

- [ ] **Step 3: Implement the decoder**

`crates/audio/src/decoder.rs`:

```rust
//! symphonia wrapper: opens a source and yields interleaved stereo f32 at the source rate.

use crate::dsp::to_stereo;
use crate::types::{EngineError, Source, TrackInfo};
use std::fs::File;
use std::path::Path;
use std::time::Duration;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::{MetadataOptions, StandardTag, Tag};
use symphonia::core::units::{Time, TimeBase};

pub struct Decoder {
    source: Source,
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    sample_rate: u32,
    channels: usize,
    info: TrackInfo,
    scratch: Vec<f32>,
    /// Source frames still to drop after an accurate seek.
    skip_frames: u64,
    bitrate_bytes: u64,
    bitrate_frames: u64,
    pending_title: Option<String>,
}

impl Decoder {
    pub fn open_file(path: &Path) -> Result<Self, EngineError> {
        let source = Source::File(path.to_path_buf());
        let file = File::open(path).map_err(|e| EngineError::Open {
            path: source.to_string(),
            reason: e.to_string(),
        })?;
        let byte_len = file.metadata().map(|m| m.len()).ok();
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let mut dec = Self::open_media(source, Box::new(file), &hint)?;
        if let (Some(len), Some(d)) = (byte_len, dec.info.duration)
            && d.as_secs_f64() > 0.0
        {
            dec.info.kbps = Some((len as f64 * 8.0 / d.as_secs_f64() / 1000.0).round() as u32);
        }
        Ok(dec)
    }

    /// Opens any media source. `hint` carries a file extension or MIME type when known.
    pub fn open_media(
        source: Source,
        media: Box<dyn MediaSource>,
        hint: &Hint,
    ) -> Result<Self, EngineError> {
        let seekable = media.is_seekable();
        let mss = MediaSourceStream::new(media, Default::default());
        let format = symphonia::default::get_probe()
            .probe(
                hint,
                mss,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| EngineError::Unsupported(format!("{source}: {e}")))?;
        let sel = select_track(format.as_ref(), &source)?;
        let info = TrackInfo {
            source: source.clone(),
            kbps: None,
            sample_rate: sel.sample_rate,
            channels: sel.channels as u16,
            duration: sel.duration,
            seekable,
        };
        let mut dec = Decoder {
            source,
            format,
            decoder: sel.decoder,
            track_id: sel.track_id,
            time_base: sel.time_base,
            sample_rate: sel.sample_rate,
            channels: sel.channels,
            info,
            scratch: Vec::new(),
            skip_frames: 0,
            bitrate_bytes: 0,
            bitrate_frames: 0,
            pending_title: None,
        };
        dec.read_title();
        Ok(dec)
    }

    /// Re-selects the default track after a chained-stream reset.
    fn reselect(&mut self) -> Result<(), EngineError> {
        let sel = select_track(self.format.as_ref(), &self.source)?;
        self.decoder = sel.decoder;
        self.track_id = sel.track_id;
        self.time_base = sel.time_base;
        self.sample_rate = sel.sample_rate;
        self.channels = sel.channels;
        self.info.sample_rate = sel.sample_rate;
        self.info.channels = sel.channels as u16;
        self.info.duration = sel.duration;
        Ok(())
    }

    pub fn info(&self) -> &TrackInfo {
        &self.info
    }

    /// Bitrate announced out-of-band (e.g. the `icy-br` stream header).
    pub fn set_kbps(&mut self, kbps: Option<u32>) {
        self.info.kbps = kbps;
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Decodes the next packet and appends interleaved stereo samples to `out`.
    /// Returns `Ok(false)` at end of stream. Corrupt packets are skipped.
    pub fn decode_next(&mut self, out: &mut Vec<f32>) -> Result<bool, EngineError> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => return Ok(false),
                Err(SymError::ResetRequired) => {
                    // Chained Ogg streams (e.g. Icecast song change): new track, new decoder.
                    self.reselect()?;
                    self.read_title();
                    continue;
                }
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Ok(false);
                }
                Err(SymError::IoError(e)) => return Err(EngineError::Decode(e.to_string())),
                Err(e) => return Err(EngineError::Decode(e.to_string())),
            };
            self.read_title();
            if packet.track_id != self.track_id {
                continue;
            }
            self.bitrate_bytes += packet.data.len() as u64;
            match self.decoder.decode(&packet) {
                Ok(buf) => {
                    let frames = buf.frames() as u64;
                    self.bitrate_frames += frames;
                    let channels = buf.spec().channels().count().max(1);
                    buf.copy_to_vec_interleaved(&mut self.scratch);
                    let skip = self.skip_frames.min(frames);
                    self.skip_frames -= skip;
                    to_stereo(&self.scratch[skip as usize * channels..], channels, out);
                    return Ok(true);
                }
                Err(SymError::DecodeError(_)) | Err(SymError::IoError(_)) => continue,
                Err(e) => return Err(EngineError::Decode(e.to_string())),
            }
        }
    }

    /// Seeks to `to`; returns the position actually reached.
    pub fn seek(&mut self, to: Duration) -> Result<Duration, EngineError> {
        let time = Time::try_from_secs_f64(to.as_secs_f64()).unwrap_or(Time::ZERO);
        let seeked = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time,
                    track_id: Some(self.track_id),
                },
            )
            .map_err(|e| EngineError::Decode(format!("seek failed: {e}")))?;
        self.decoder.reset();
        self.skip_frames = 0;
        if let Some(tb) = self.time_base {
            let required = tb
                .calc_time(seeked.required_ts)
                .map(time_to_duration)
                .unwrap_or(to);
            let actual = tb
                .calc_time(seeked.actual_ts)
                .map(time_to_duration)
                .unwrap_or(required);
            if required > actual {
                self.skip_frames =
                    ((required - actual).as_secs_f64() * f64::from(self.sample_rate)) as u64;
            }
            return Ok(required);
        }
        Ok(to)
    }

    /// Bitrate over the audio decoded since the last call, once at least one second has passed.
    pub fn take_bitrate(&mut self) -> Option<u32> {
        if self.sample_rate == 0 || self.bitrate_frames < u64::from(self.sample_rate) {
            return None;
        }
        let secs = self.bitrate_frames as f64 / f64::from(self.sample_rate);
        let kbps = (self.bitrate_bytes as f64 * 8.0 / secs / 1000.0).round() as u32;
        self.bitrate_bytes = 0;
        self.bitrate_frames = 0;
        Some(kbps)
    }

    /// In-band title change (chained Ogg streams), if any since the last call.
    pub fn take_title(&mut self) -> Option<String> {
        self.pending_title.take()
    }

    fn read_title(&mut self) {
        let mut metadata = self.format.metadata();
        if metadata.is_latest() && self.pending_title.is_some() {
            return;
        }
        if let Some(rev) = metadata.skip_to_latest()
            && let Some(title) = title_from_tags(&rev.media.tags)
        {
            self.pending_title = Some(title);
        }
    }
}

/// "Artist - Title", or whichever of the two exists.
pub fn title_from_tags(tags: &[Tag]) -> Option<String> {
    let mut artist = None;
    let mut title = None;
    for tag in tags {
        match &tag.std {
            Some(StandardTag::Artist(a)) => artist = Some(a.to_string()),
            Some(StandardTag::TrackTitle(t)) => title = Some(t.to_string()),
            _ => {}
        }
    }
    match (artist, title) {
        (Some(a), Some(t)) => Some(format!("{a} - {t}")),
        (None, Some(t)) => Some(t),
        (Some(a), None) => Some(a),
        (None, None) => None,
    }
}

fn time_to_duration(t: Time) -> Duration {
    Duration::from_secs_f64(t.as_secs_f64().max(0.0))
}

struct SelectedTrack {
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    sample_rate: u32,
    channels: usize,
    duration: Option<Duration>,
}

fn select_track(format: &dyn FormatReader, source: &Source) -> Result<SelectedTrack, EngineError> {
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| EngineError::Unsupported(format!("{source}: no audio track")))?;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| EngineError::Unsupported(format!("{source}: unknown codec")))?;
    let decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| EngineError::Unsupported(format!("{source}: {e}")))?;
    let sample_rate = params.sample_rate.unwrap_or(44_100);
    let channels = params.channels.as_ref().map(|c| c.count()).unwrap_or(2);
    let duration = match (track.num_frames, track.duration, track.time_base) {
        (Some(n), _, _) if sample_rate > 0 => {
            Some(Duration::from_secs_f64(n as f64 / f64::from(sample_rate)))
        }
        (_, Some(d), Some(tb)) => tb.calc_duration(d).map(time_to_duration),
        _ => None,
    };
    Ok(SelectedTrack {
        decoder,
        track_id: track.id,
        time_base: track.time_base,
        sample_rate,
        channels,
        duration,
    })
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p winrust-audio --test decoder_wav`
Expected: `test result: ok. 4 passed`.

- [ ] **Step 5: Gates and commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/audio/src/decoder.rs crates/audio/src/lib.rs crates/audio/tests/decoder_wav.rs
git commit -m "feat(audio): add symphonia decoder with accurate seek and stream titles" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Format fixtures and decoding of every supported format

**Files:**
- Modify: `xtask/src/main.rs` (add `gen-fixtures`)
- Create: `crates/audio/tests/fixtures/` (`tone.mp3`, `tone.flac`, `tone.ogg`, `tone.m4a`, `tone-alac.m4a`, `half1.mp3`, `half2.mp3`), generated, committed
- Test: `crates/audio/tests/formats.rs`

**Interfaces:**
- Consumes: `Decoder` (Task 5).
- Produces: committed fixtures. Each one is a 440 Hz stereo sine at 44.1 kHz, 1 s long. `half1.mp3` and `half2.mp3` are the two halves of one continuous 2 s sine (Task 13 uses them for the MP3 gapless test). Command: `cargo xtask gen-fixtures`.

- [ ] **Step 1: Make sure ffmpeg is available — ask before installing**

Run: `ffmpeg -version | head -1`

If the command is not found, STOP and ask the user for approval to install it, because it changes the system. Suggested commands: Windows `winget install --id Gyan.FFmpeg -e`, macOS `brew install ffmpeg`, Debian/Ubuntu `sudo apt install ffmpeg`. After a winget install, open a new shell so `PATH` picks it up. ffmpeg is needed only to *generate* fixtures. CI and other contributors use the committed files.

- [ ] **Step 2: Write the failing test**

`crates/audio/tests/formats.rs`:

```rust
//! Decoding of every supported format, using fixtures from `cargo xtask gen-fixtures`.

use std::path::{Path, PathBuf};
use winrust_audio::decoder::Decoder;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn left(stereo: &[f32]) -> Vec<f32> {
    stereo.iter().step_by(2).copied().collect()
}

fn rising_zero_crossings(signal: &[f32]) -> usize {
    signal
        .windows(2)
        .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
        .count()
}

#[test]
fn decodes_every_supported_format() {
    // (file, lossless)
    let cases = [
        ("tone.mp3", false),
        ("tone.flac", true),
        ("tone.ogg", false),
        ("tone.m4a", false),
        ("tone-alac.m4a", true),
    ];
    for (name, lossless) in cases {
        let mut dec = Decoder::open_file(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let info = dec.info().clone();
        assert_eq!(info.sample_rate, 44_100, "{name}");
        assert_eq!(info.channels, 2, "{name}");
        let dur = info
            .duration
            .unwrap_or_else(|| panic!("{name}: no duration"));
        assert!(
            (dur.as_secs_f64() - 1.0).abs() < 0.05,
            "{name}: duration {dur:?}"
        );
        let mut out = Vec::new();
        while dec
            .decode_next(&mut out)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
        {}
        let frames = out.len() / 2;
        let slack = if lossless { 0 } else { 2_048 };
        assert!(
            (frames as i64 - 44_100).abs() <= slack,
            "{name}: {frames} frames"
        );
        let crossings = rising_zero_crossings(&left(&out));
        assert!(
            (crossings as i64 - 440).abs() <= 3,
            "{name}: {crossings} crossings"
        );
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p winrust-audio --test formats`
Expected: FAIL with `tone.mp3: cannot open ...tests/fixtures/tone.mp3: ...` (fixtures don't exist yet).

- [ ] **Step 4: Add the fixture generator**

Replace `xtask/src/main.rs` with:

```rust
//! Developer tasks: `cargo xtask <task>`. Dependency-free on purpose so it builds instantly.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const HELP: &str = "\
Usage: cargo xtask <task>

Tasks:
  help           Show this message
  gen-fixtures   Regenerate audio test fixtures (needs ffmpeg on PATH)
";

fn main() -> ExitCode {
    let task = std::env::args().nth(1);
    let result = match task.as_deref() {
        None | Some("help") | Some("--help") | Some("-h") => {
            print!("{HELP}");
            Ok(())
        }
        Some("gen-fixtures") => gen_fixtures(),
        Some(other) => Err(format!("unknown task: {other}\n\n{HELP}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives one level below the workspace root")
        .to_path_buf()
}

/// One-second 440 Hz stereo tones in every supported format, plus a two-second tone split
/// into two MP3 halves for the gapless test. lavfi's sine is phase-continuous, so the halves
/// join seamlessly when the decoder trims encoder delay and padding correctly.
fn gen_fixtures() -> Result<(), String> {
    let out = workspace_root().join("crates/audio/tests/fixtures");
    std::fs::create_dir_all(&out).map_err(|e| format!("create {}: {e}", out.display()))?;
    let tone = "sine=frequency=440:sample_rate=44100:duration=1";
    let formats: [(&str, &[&str]); 5] = [
        ("tone.mp3", &["-c:a", "libmp3lame", "-b:a", "128k"]),
        ("tone.flac", &["-c:a", "flac"]),
        ("tone.ogg", &["-c:a", "libvorbis", "-q:a", "4"]),
        ("tone.m4a", &["-c:a", "aac", "-b:a", "128k"]),
        ("tone-alac.m4a", &["-c:a", "alac"]),
    ];
    for (name, codec) in formats {
        ffmpeg(
            &["-f", "lavfi", "-i", tone, "-ac", "2"],
            codec,
            &out.join(name),
        )?;
    }
    let long = "sine=frequency=440:sample_rate=44100:duration=2";
    let halves = [
        ("half1.mp3", "atrim=0:1"),
        ("half2.mp3", "atrim=1:2,asetpts=PTS-STARTPTS"),
    ];
    for (name, trim) in halves {
        ffmpeg(
            &["-f", "lavfi", "-i", long, "-ac", "2", "-af", trim],
            &["-c:a", "libmp3lame", "-b:a", "128k"],
            &out.join(name),
        )?;
    }
    println!("fixtures written to {}", out.display());
    Ok(())
}

fn ffmpeg(input: &[&str], codec: &[&str], dest: &Path) -> Result<(), String> {
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(input)
        .args(codec)
        .args(["-map_metadata", "-1"])
        .arg(dest)
        .status()
        .map_err(|e| format!("cannot run ffmpeg (is it on PATH?): {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("ffmpeg failed for {}", dest.display()))
    }
}
```

Run:
```bash
cargo xtask gen-fixtures
ls -la crates/audio/tests/fixtures
```
Expected: `fixtures written to .../crates/audio/tests/fixtures` and seven files, each well under 400 KB (the ALAC and FLAC ones are the largest).

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p winrust-audio --test formats`
Expected: `test result: ok. 1 passed`.

If one format fails, the message names the file and the assertion. Use superpowers:systematic-debugging. Check `ffprobe crates/audio/tests/fixtures/<file>` first: sample rate, channels, duration. The tolerances (duration ±50 ms; lossy frame count ±2048, which covers one AAC/MP3 frame of priming; 440 ± 3 zero crossings) were derived from codec frame sizes, not measured; see Provenance. Do not loosen a tolerance without understanding why the decoder output differs.

- [ ] **Step 6: Gates and commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add xtask/src/main.rs crates/audio/tests/formats.rs crates/audio/tests/fixtures
git ls-files --eol crates/audio/tests/fixtures | head -3
git commit -m "test(audio): add format fixtures and decode tests for every supported codec" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected `ls-files --eol`: the fixtures show `-text` (binary, from `.gitattributes`).

---

### Task 7: ICY metadata reader

**Files:**
- Create: `crates/audio/src/icy.rs`
- Modify: `crates/audio/src/lib.rs` (add `pub mod icy;`)

**Interfaces:**
- Consumes: crossbeam-channel.
- Produces: `struct IcyReader<R>` with `fn new(inner: R, metaint: usize, titles: Sender<String>)`, implementing `Read` (yields audio bytes only, sends each non-empty title); `fn parse_stream_title(block: &[u8]) -> Option<String>` (UTF-8, falling back to Latin-1; keeps apostrophes inside titles).

- [ ] **Step 1: Write the failing tests**

Add `pub mod icy;` to `lib.rs`. Create `crates/audio/src/icy.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;

    fn block(meta: &str) -> Vec<u8> {
        let mut m = meta.as_bytes().to_vec();
        m.resize(m.len().div_ceil(16) * 16, 0);
        let mut out = vec![(m.len() / 16) as u8];
        out.extend(m);
        out
    }

    #[test]
    fn parses_plain_title() {
        assert_eq!(
            parse_stream_title(b"StreamTitle='Daft Punk - One More Time';StreamUrl='';\0\0"),
            Some("Daft Punk - One More Time".into())
        );
    }

    #[test]
    fn keeps_apostrophes_inside_title() {
        assert_eq!(
            parse_stream_title(b"StreamTitle='Guns N' Roses - Don't Cry';"),
            Some("Guns N' Roses - Don't Cry".into())
        );
    }

    #[test]
    fn empty_title_is_none() {
        assert_eq!(parse_stream_title(b"StreamTitle='';"), None);
        assert_eq!(parse_stream_title(b"garbage"), None);
    }

    #[test]
    fn latin1_fallback() {
        assert_eq!(
            parse_stream_title(b"StreamTitle='Caf\xe9';"),
            Some("Café".into())
        );
    }

    #[test]
    fn strips_metadata_and_reports_titles() {
        let mut data = b"abcd".to_vec();
        data.extend(block("StreamTitle='One';"));
        data.extend(b"efgh");
        data.push(0); // empty metadata block
        data.extend(b"ij");
        let (tx, rx) = unbounded();
        let mut r = IcyReader::new(&data[..], 4, tx);
        let mut audio = Vec::new();
        r.read_to_end(&mut audio).unwrap();
        assert_eq!(audio, b"abcdefghij");
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), vec!["One".to_string()]);
    }

    #[test]
    fn zero_metaint_passes_through() {
        let (tx, _rx) = unbounded();
        let mut r = IcyReader::new(&b"raw bytes"[..], 0, tx);
        let mut out = Vec::new();
        r.read_to_end(&mut out).unwrap();
        assert_eq!(out, b"raw bytes");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p winrust-audio icy::`
Expected: compile errors (`parse_stream_title`, `IcyReader` not found).

- [ ] **Step 3: Implement above the tests module**

```rust
//! SHOUTcast/Icecast in-band metadata ("ICY") handling.

use crossbeam_channel::Sender;
use std::io::{self, Read};

/// Wraps a stream body and removes the metadata block inserted every `metaint` bytes.
/// Each non-empty `StreamTitle` is sent to `titles`.
pub struct IcyReader<R> {
    inner: R,
    metaint: usize,
    until_meta: usize,
    titles: Sender<String>,
}

impl<R: Read> IcyReader<R> {
    pub fn new(inner: R, metaint: usize, titles: Sender<String>) -> Self {
        IcyReader {
            inner,
            metaint,
            until_meta: metaint,
            titles,
        }
    }

    fn read_metadata(&mut self) -> io::Result<()> {
        let mut len = [0u8; 1];
        self.inner.read_exact(&mut len)?;
        let size = usize::from(len[0]) * 16;
        if size > 0 {
            let mut block = vec![0u8; size];
            self.inner.read_exact(&mut block)?;
            if let Some(title) = parse_stream_title(&block) {
                let _ = self.titles.send(title);
            }
        }
        self.until_meta = self.metaint;
        Ok(())
    }
}

impl<R: Read> Read for IcyReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.metaint == 0 || buf.is_empty() {
            return self.inner.read(buf);
        }
        if self.until_meta == 0 {
            self.read_metadata()?;
        }
        let want = buf.len().min(self.until_meta);
        let n = self.inner.read(&mut buf[..want])?;
        self.until_meta -= n;
        Ok(n)
    }
}

/// Extracts the title from a metadata block like `StreamTitle='Artist - Song';StreamUrl='';`.
///
/// The title ends at the first `';` so apostrophes inside the title survive.
/// Bytes are decoded as UTF-8 when valid, otherwise as Latin-1 (common on old servers).
pub fn parse_stream_title(block: &[u8]) -> Option<String> {
    let text = decode_text(block);
    let start = text.find("StreamTitle='")? + "StreamTitle='".len();
    let rest = &text[start..];
    let end = rest
        .find("';")
        .or_else(|| rest.rfind('\''))
        .unwrap_or(rest.len());
    let title = rest[..end].trim();
    (!title.is_empty()).then(|| title.to_string())
}

fn decode_text(bytes: &[u8]) -> String {
    let bytes = match bytes.iter().position(|&b| b == 0) {
        Some(nul) => &bytes[..nul],
        None => bytes,
    };
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p winrust-audio icy::`
Expected: `test result: ok. 6 passed`.

- [ ] **Step 5: Gates and commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/audio/src/icy.rs crates/audio/src/lib.rs
git commit -m "feat(audio): add ICY metadata reader for stream titles" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: HTTP/1.0 stream client

**Files:**
- Create: `crates/audio/src/http.rs`
- Modify: `crates/audio/src/lib.rs` (add `pub mod http;`)

**Interfaces:**
- Consumes: `EngineError` (Task 1); rustls (ring provider), webpki-roots, url.
- Produces: `struct StreamResponse { body: Box<dyn Read + Send>, content_type: Option<String>, metaint: usize, bitrate: Option<u32>, name: Option<String> }` and `fn open(url: &str) -> Result<StreamResponse, EngineError>`. Accepts `HTTP/1.x` and `ICY` status lines, follows up to 5 redirects. Timeouts: 10 s connect, 15 s read. Sends `Icy-MetaData: 1`. The content type is lower-cased with any parameters stripped.

- [ ] **Step 1: Write the failing tests**

Add `pub mod http;` to `lib.rs`. Create `crates/audio/src/http.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_lines() {
        assert_eq!(parse_status("HTTP/1.1 200 OK"), Some(200));
        assert_eq!(parse_status("HTTP/1.0 302 Found"), Some(302));
        assert_eq!(parse_status("ICY 200 OK"), Some(200));
        assert_eq!(parse_status("SSH-2.0-OpenSSH"), None);
        assert_eq!(parse_status(""), None);
    }

    #[test]
    fn reads_head_and_leaves_body() {
        let raw = b"ICY 200 OK\r\nicy-metaint: 8192\r\nContent-Type: audio/mpeg\r\nicy-br:128\r\n\r\nBODY";
        let mut reader = BufReader::new(&raw[..]);
        let head = read_head(&mut reader).unwrap();
        assert_eq!(head.status, 200);
        assert_eq!(head.header("ICY-METAINT"), Some("8192"));
        assert_eq!(head.header("content-type"), Some("audio/mpeg"));
        let mut body = String::new();
        reader.read_to_string(&mut body).unwrap();
        assert_eq!(body, "BODY");
    }

    #[test]
    fn truncated_head_is_error() {
        let mut reader = BufReader::new(&b"HTTP/1.0 200 OK\r\nfoo: bar\r\n"[..]);
        assert!(read_head(&mut reader).is_err());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p winrust-audio http::`
Expected: compile errors (`parse_status`, `read_head`, `BufReader` not found).

- [ ] **Step 3: Implement above the tests module**

```rust
//! Minimal HTTP/1.0 client for audio streams.
//!
//! Hand-rolled because old SHOUTcast servers answer `ICY 200 OK`, which general-purpose
//! HTTP clients reject. Supports http/https, redirects, and the ICY response headers.

use crate::types::EngineError;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;
use url::Url;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_REDIRECTS: usize = 5;
const MAX_HEAD_BYTES: usize = 64 * 1024;

pub struct StreamResponse {
    pub body: Box<dyn Read + Send>,
    pub content_type: Option<String>,
    /// Bytes of audio between ICY metadata blocks; 0 when the server sends none.
    pub metaint: usize,
    pub bitrate: Option<u32>,
    pub name: Option<String>,
}

trait Conn: Read + Write + Send {}
impl<T: Read + Write + Send> Conn for T {}

/// Connects and returns the response body positioned at the first audio byte.
pub fn open(url: &str) -> Result<StreamResponse, EngineError> {
    let mut url = Url::parse(url).map_err(|e| err(format!("bad URL {url}: {e}")))?;
    for _ in 0..=MAX_REDIRECTS {
        let mut reader = BufReader::new(connect(&url)?);
        let head = read_head(&mut reader)?;
        match head.status {
            200 => {
                return Ok(StreamResponse {
                    body: Box::new(reader),
                    content_type: head.header("content-type").map(|v| {
                        v.split(';')
                            .next()
                            .unwrap_or("")
                            .trim()
                            .to_ascii_lowercase()
                    }),
                    metaint: head
                        .header("icy-metaint")
                        .and_then(|v| v.trim().parse().ok())
                        .unwrap_or(0),
                    bitrate: head
                        .header("icy-br")
                        .and_then(|v| v.split(',').next()?.trim().parse().ok()),
                    name: head.header("icy-name").map(|v| v.trim().to_string()),
                });
            }
            301 | 302 | 303 | 307 | 308 => {
                let location = head
                    .header("location")
                    .ok_or_else(|| err("redirect without Location".into()))?;
                url = url
                    .join(location.trim())
                    .map_err(|e| err(format!("bad redirect: {e}")))?;
            }
            code => return Err(err(format!("server returned {code}"))),
        }
    }
    Err(err("too many redirects".into()))
}

fn err(msg: String) -> EngineError {
    EngineError::Stream(msg)
}

fn connect(url: &Url) -> Result<Box<dyn Conn>, EngineError> {
    let host = url
        .host_str()
        .ok_or_else(|| err(format!("no host in {url}")))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| err(format!("no port for {url}")))?;
    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|e| err(format!("cannot resolve {host}: {e}")))?;
    let mut last = err(format!("no address for {host}"));
    let mut tcp = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
            Ok(s) => {
                tcp = Some(s);
                break;
            }
            Err(e) => last = err(format!("cannot connect to {host}: {e}")),
        }
    }
    let tcp = tcp.ok_or(last)?;
    tcp.set_read_timeout(Some(READ_TIMEOUT))
        .map_err(|e| err(e.to_string()))?;
    let _ = tcp.set_nodelay(true);

    let mut conn: Box<dyn Conn> = match url.scheme() {
        "http" | "icy" => Box::new(tcp),
        "https" => Box::new(tls(host, tcp)?),
        other => return Err(err(format!("unsupported scheme {other}"))),
    };
    let path = &url[url::Position::BeforePath..url::Position::AfterQuery];
    let host_header = match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host.to_string(),
    };
    let request = format!(
        "GET {} HTTP/1.0\r\nHost: {host_header}\r\nUser-Agent: Winrust/{}\r\nAccept: */*\r\nIcy-MetaData: 1\r\nConnection: close\r\n\r\n",
        if path.is_empty() { "/" } else { path },
        env!("CARGO_PKG_VERSION"),
    );
    conn.write_all(request.as_bytes())
        .map_err(|e| err(format!("send failed: {e}")))?;
    conn.flush().map_err(|e| err(format!("send failed: {e}")))?;
    Ok(conn)
}

fn tls(
    host: &str,
    tcp: TcpStream,
) -> Result<rustls::StreamOwned<rustls::ClientConnection, TcpStream>, EngineError> {
    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| err(format!("TLS setup: {e}")))?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|e| err(format!("bad TLS name {host}: {e}")))?;
    let conn = rustls::ClientConnection::new(Arc::new(config), name)
        .map_err(|e| err(format!("TLS: {e}")))?;
    Ok(rustls::StreamOwned::new(conn, tcp))
}

struct Head {
    status: u16,
    headers: Vec<(String, String)>,
}

impl Head {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn read_head(reader: &mut impl BufRead) -> Result<Head, EngineError> {
    let mut total = 0;
    let mut line = String::new();
    let mut next_line = |line: &mut String| -> Result<(), EngineError> {
        line.clear();
        let mut raw = Vec::new();
        let n = reader
            .read_until(b'\n', &mut raw)
            .map_err(|e| err(format!("read failed: {e}")))?;
        total += n;
        if n == 0 || total > MAX_HEAD_BYTES {
            return Err(err("malformed response header".into()));
        }
        line.push_str(String::from_utf8_lossy(&raw).trim_end_matches(['\r', '\n']));
        Ok(())
    };
    next_line(&mut line)?;
    let status = parse_status(&line).ok_or_else(|| err(format!("bad status line: {line}")))?;
    let mut headers = Vec::new();
    loop {
        next_line(&mut line)?;
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    Ok(Head { status, headers })
}

/// Accepts `HTTP/1.x 200 OK` and SHOUTcast's `ICY 200 OK`.
fn parse_status(line: &str) -> Option<u16> {
    let mut parts = line.split_whitespace();
    let proto = parts.next()?;
    if !(proto.starts_with("HTTP/") || proto == "ICY") {
        return None;
    }
    parts.next()?.parse().ok()
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p winrust-audio http::`
Expected: `test result: ok. 3 passed`.

- [ ] **Step 5: Gates and commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo deny check
git add crates/audio/src/http.rs crates/audio/src/lib.rs
git commit -m "feat(audio): add HTTP/1.0 stream client that accepts ICY responses" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Stream pipe and network thread

**Files:**
- Create: `crates/audio/src/stream.rs`
- Modify: `crates/audio/src/lib.rs` (add `pub mod stream;`)

**Interfaces:**
- Consumes: `http::open` (Task 8), `IcyReader` (Task 7).
- Produces:
  - `struct Cancel` (`Clone + Default`) with `fn new(epoch: Arc<AtomicU64>) -> Cancel` and `fn is_cancelled(&self) -> bool`. It snapshots the epoch at creation and reports cancelled once anyone bumps the epoch. `Cancel::default()` is never cancelled.
  - `struct OpenStream { reader: PipeReader, content_type: Option<String>, bitrate: Option<u32>, titles: Receiver<String> }`
  - `fn open(url: &str, cancel: Cancel) -> Result<OpenStream, EngineError>` — connects on thread `winrust-net` and waits for the headers (returns early if cancelled)
  - `struct PipeReader` implementing `Read` (returns EOF when the stream closes or is cancelled; polls every 50 ms)
  - `fn extension_for_mime(mime: &str) -> Option<&'static str>`

- [ ] **Step 1: Write the failing tests**

Add `pub mod stream;` to `lib.rs`. Create `crates/audio/src/stream.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    /// Serves one response then closes. Returns the URL.
    fn serve_once(response: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut req = [0u8; 1024];
            let _ = sock.read(&mut req);
            sock.write_all(&response).unwrap();
        });
        format!("http://{addr}/stream")
    }

    #[test]
    fn reads_icy_stream_and_titles() {
        let mut resp =
            b"ICY 200 OK\r\ncontent-type: audio/mpeg\r\nicy-metaint: 4\r\nicy-br: 128\r\n\r\n"
                .to_vec();
        resp.extend(b"AAAA");
        let meta = b"StreamTitle='Hello';\0\0\0\0\0\0\0\0\0\0\0\0";
        resp.push((meta.len() / 16) as u8);
        resp.extend(meta);
        resp.extend(b"BB");
        let url = serve_once(resp);
        let mut s = open(&url, Cancel::default()).unwrap();
        assert_eq!(s.content_type.as_deref(), Some("audio/mpeg"));
        assert_eq!(s.bitrate, Some(128));
        let mut body = Vec::new();
        s.reader.read_to_end(&mut body).unwrap();
        assert_eq!(body, b"AAAABB");
        assert_eq!(s.titles.try_recv().unwrap(), "Hello");
    }

    #[test]
    fn follows_redirect() {
        let target = serve_once(b"HTTP/1.0 200 OK\r\ncontent-type: audio/ogg\r\n\r\nOGG".to_vec());
        let first =
            serve_once(format!("HTTP/1.1 302 Found\r\nLocation: {target}\r\n\r\n").into_bytes());
        let mut s = open(&first, Cancel::default()).unwrap();
        let mut body = Vec::new();
        s.reader.read_to_end(&mut body).unwrap();
        assert_eq!(body, b"OGG");
    }

    #[test]
    fn http_error_is_reported() {
        let url = serve_once(b"HTTP/1.0 404 Not Found\r\n\r\n".to_vec());
        let e = open(&url, Cancel::default()).err().unwrap();
        assert_eq!(e, EngineError::Stream("server returned 404".into()));
    }

    #[test]
    fn connection_refused_is_reported() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let e = open(&format!("http://127.0.0.1:{port}/"), Cancel::default())
            .err()
            .unwrap();
        assert!(matches!(e, EngineError::Stream(_)), "{e:?}");
    }

    #[test]
    fn cancel_ends_reader() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut req = [0u8; 1024];
            let _ = sock.read(&mut req);
            sock.write_all(b"HTTP/1.0 200 OK\r\n\r\nX").unwrap();
            thread::sleep(Duration::from_secs(5));
        });
        let epoch = Arc::new(AtomicU64::new(0));
        let mut s = open(&format!("http://{addr}/"), Cancel::new(epoch.clone())).unwrap();
        let mut one = [0u8; 1];
        assert_eq!(s.reader.read(&mut one).unwrap(), 1);
        epoch.fetch_add(1, Ordering::AcqRel);
        let started = std::time::Instant::now();
        assert_eq!(s.reader.read(&mut one).unwrap(), 0);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn mime_mapping() {
        assert_eq!(extension_for_mime("audio/aacp"), Some("aac"));
        assert_eq!(extension_for_mime("text/html"), None);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p winrust-audio stream::`
Expected: compile errors (`open`, `Cancel`, `extension_for_mime`, `EngineError`, `Arc`, `AtomicU64`, `thread`, `Duration` not found).

- [ ] **Step 3: Implement above the tests module**

```rust
//! Network thread for internet radio: HTTP body → ICY strip → byte pipe read by the decoder.

use crate::http;
use crate::icy::IcyReader;
use crate::types::EngineError;
use crossbeam_channel::{Receiver, RecvTimeoutError, bounded, unbounded};
use std::io::{self, Read};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

const CHUNK_BYTES: usize = 16 * 1024;
/// 64 chunks of 16 KiB: about a minute of 128 kbit/s audio buffered ahead at most.
const PIPE_CHUNKS: usize = 64;
const POLL: Duration = Duration::from_millis(50);

/// Cancellation token. The engine bumps a shared epoch counter whenever the user loads,
/// stops or quits; every token taken before the bump reports cancelled.
#[derive(Clone, Debug, Default)]
pub struct Cancel {
    epoch: Arc<AtomicU64>,
    seen: u64,
}

impl Cancel {
    pub fn new(epoch: Arc<AtomicU64>) -> Self {
        let seen = epoch.load(Ordering::Acquire);
        Cancel { epoch, seen }
    }

    pub fn is_cancelled(&self) -> bool {
        self.epoch.load(Ordering::Acquire) != self.seen
    }
}

type Chunk = Result<Vec<u8>, String>;

pub struct OpenStream {
    pub reader: PipeReader,
    pub content_type: Option<String>,
    pub bitrate: Option<u32>,
    pub titles: Receiver<String>,
}

/// Connects on a background thread and waits for the response headers.
/// Returns early with an error if `cancel` fires first.
pub fn open(url: &str, cancel: Cancel) -> Result<OpenStream, EngineError> {
    let (meta_tx, meta_rx) = bounded::<Result<(Option<String>, Option<u32>), EngineError>>(1);
    let (chunk_tx, chunk_rx) = bounded::<Chunk>(PIPE_CHUNKS);
    let (title_tx, title_rx) = unbounded();
    let url_owned = url.to_string();
    let net_cancel = cancel.clone();
    thread::Builder::new()
        .name("winrust-net".into())
        .spawn(move || {
            let resp = match http::open(&url_owned) {
                Ok(r) => r,
                Err(e) => {
                    let _ = meta_tx.send(Err(e));
                    return;
                }
            };
            let _ = meta_tx.send(Ok((resp.content_type.clone(), resp.bitrate)));
            let mut body: Box<dyn Read> = if resp.metaint > 0 {
                Box::new(IcyReader::new(resp.body, resp.metaint, title_tx))
            } else {
                resp.body
            };
            loop {
                if net_cancel.is_cancelled() {
                    return;
                }
                let mut buf = vec![0u8; CHUNK_BYTES];
                match body.read(&mut buf) {
                    Ok(0) => return,
                    Ok(n) => {
                        buf.truncate(n);
                        if chunk_tx.send(Ok(buf)).is_err() {
                            return;
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        let _ = chunk_tx.send(Err(e.to_string()));
                        return;
                    }
                }
            }
        })
        .map_err(|e| EngineError::Stream(format!("cannot start network thread: {e}")))?;

    let (content_type, bitrate) = loop {
        if cancel.is_cancelled() {
            return Err(EngineError::Stream("cancelled".into()));
        }
        match meta_rx.recv_timeout(POLL) {
            Ok(result) => break result?,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                return Err(EngineError::Stream("connection closed".into()));
            }
        }
    };
    Ok(OpenStream {
        reader: PipeReader {
            rx: chunk_rx,
            buf: Vec::new(),
            pos: 0,
            cancel,
        },
        content_type,
        bitrate,
        titles: title_rx,
    })
}

/// Blocking reader over the chunk pipe. Ends (EOF) when the stream closes or is cancelled.
pub struct PipeReader {
    rx: Receiver<Chunk>,
    buf: Vec<u8>,
    pos: usize,
    cancel: Cancel,
}

impl Read for PipeReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.pos == self.buf.len() {
            if self.cancel.is_cancelled() {
                return Ok(0);
            }
            match self.rx.recv_timeout(POLL) {
                Ok(Ok(chunk)) => {
                    self.buf = chunk;
                    self.pos = 0;
                }
                Ok(Err(e)) => return Err(io::Error::other(e)),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
        let n = out.len().min(self.buf.len() - self.pos);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// File extension symphonia should try first for a stream's Content-Type.
pub fn extension_for_mime(mime: &str) -> Option<&'static str> {
    match mime {
        "audio/mpeg" | "audio/mp3" | "audio/x-mpeg" => Some("mp3"),
        "audio/aac" | "audio/aacp" | "audio/x-aac" | "audio/x-aacp" => Some("aac"),
        "audio/ogg" | "application/ogg" | "audio/x-ogg" | "audio/vorbis" => Some("ogg"),
        "audio/flac" | "audio/x-flac" => Some("flac"),
        "audio/wav" | "audio/x-wav" | "audio/wave" => Some("wav"),
        _ => None,
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p winrust-audio stream::`
Expected: `test result: ok. 6 passed`. The tests run local TCP servers on 127.0.0.1. If a firewall prompt appears on Windows, allow it for private networks.

- [ ] **Step 5: Gates and commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/audio/src/stream.rs crates/audio/src/lib.rs
git commit -m "feat(audio): add network thread and cancellable byte pipe for streams" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Renderer (audio-callback side) and analyzer tap

**Files:**
- Create: `crates/audio/src/renderer.rs`
- Modify: `crates/audio/src/lib.rs` (add `pub mod renderer;`)

**Interfaces:**
- Consumes: rtrb `Consumer<f32>`, triple_buffer.
- Produces:
  - `const SCOPE_FRAMES: usize = 576`; `struct Scope { left: [f32; 576], right: [f32; 576] }` (oldest sample first, pre-volume)
  - `struct AnalyzerReader` with `fn read(&mut self) -> &Scope` (lock-free)
  - `struct Shared` with `fn new()`, `fn set_volume(&self, gain: f32)`, `fn volume(&self) -> f32`, and pub atomics `paused`, `flush_requested`, `flush_done`, `frames_played`, `buffering`, `start_threshold`, `rebuffer_on_underrun`, `producer_done`
  - `struct Renderer` with `fn new(consumer: rtrb::Consumer<f32>, shared: Arc<Shared>) -> (Renderer, AnalyzerReader)` and `fn render(&mut self, out: &mut [f32], channels: usize) -> usize` (fills `out`, returns real audio frames; the rest is silence)
- Flush protocol, which Task 13 relies on: the worker increments `flush_requested`; the next `render` empties the ring, sets `flush_done` to the same value and re-arms the start threshold.

- [ ] **Step 1: Write the failing tests**

Add `pub mod renderer;` to `lib.rs`. Create `crates/audio/src/renderer.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn setup(capacity: usize) -> (rtrb::Producer<f32>, Renderer, AnalyzerReader, Arc<Shared>) {
        let (p, c) = rtrb::RingBuffer::new(capacity);
        let shared = Arc::new(Shared::new());
        shared.producer_done.store(false, Ordering::Relaxed);
        let (r, a) = Renderer::new(c, shared.clone());
        (p, r, a, shared)
    }

    fn push(p: &mut rtrb::Producer<f32>, frames: &[(f32, f32)]) {
        for &(l, r) in frames {
            p.push(l).unwrap();
            p.push(r).unwrap();
        }
    }

    #[test]
    fn plays_queued_frames_then_silence() {
        let (mut p, mut r, _, shared) = setup(64);
        push(&mut p, &[(0.5, -0.5), (0.25, -0.25)]);
        let mut out = [9.0f32; 8];
        assert_eq!(r.render(&mut out, 2), 2);
        assert_eq!(out, [0.5, -0.5, 0.25, -0.25, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(shared.frames_played.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn waits_for_threshold_before_starting() {
        let (mut p, mut r, _, shared) = setup(64);
        shared.start_threshold.store(4, Ordering::Relaxed);
        push(&mut p, &[(0.1, 0.1); 3]);
        let mut out = [9.0f32; 4];
        r.render(&mut out, 2);
        assert_eq!(out, [0.0; 4]);
        assert!(shared.buffering.load(Ordering::Relaxed));
        push(&mut p, &[(0.1, 0.1)]);
        r.render(&mut out, 2);
        assert_eq!(out, [0.1; 4]);
        assert!(!shared.buffering.load(Ordering::Relaxed));
    }

    #[test]
    fn producer_done_plays_out_below_threshold() {
        let (mut p, mut r, _, shared) = setup(64);
        shared.start_threshold.store(100, Ordering::Relaxed);
        push(&mut p, &[(0.3, 0.3)]);
        shared.producer_done.store(true, Ordering::Relaxed);
        let mut out = [0.0f32; 2];
        r.render(&mut out, 2);
        assert_eq!(out, [0.3, 0.3]);
    }

    #[test]
    fn pause_outputs_silence_and_keeps_audio() {
        let (mut p, mut r, _, shared) = setup(64);
        push(&mut p, &[(0.7, 0.7)]);
        shared.paused.store(true, Ordering::Relaxed);
        let mut out = [9.0f32; 2];
        r.render(&mut out, 2);
        assert_eq!(out, [0.0, 0.0]);
        shared.paused.store(false, Ordering::Relaxed);
        r.render(&mut out, 2);
        assert_eq!(out, [0.7, 0.7]);
    }

    #[test]
    fn flush_discards_and_acknowledges() {
        let (mut p, mut r, _, shared) = setup(64);
        push(&mut p, &[(0.7, 0.7); 4]);
        shared.flush_requested.store(1, Ordering::Release);
        let mut out = [9.0f32; 2];
        r.render(&mut out, 2);
        assert_eq!(out, [0.0, 0.0]);
        assert_eq!(shared.flush_done.load(Ordering::Acquire), 1);
        assert_eq!(p.slots(), 64);
    }

    #[test]
    fn volume_ramps_within_one_buffer() {
        let (mut p, mut r, _, shared) = setup(64);
        push(&mut p, &[(1.0, 1.0); 4]);
        shared.set_volume(0.0);
        let mut out = [0.0f32; 8];
        r.render(&mut out, 2);
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        assert_eq!(left, vec![0.75, 0.5, 0.25, 0.0]);
    }

    #[test]
    fn maps_stereo_to_mono_and_multichannel_devices() {
        let (mut p, mut r, _, _) = setup(64);
        push(&mut p, &[(0.2, 0.4), (0.2, 0.4)]);
        let mut mono = [0.0f32; 1];
        r.render(&mut mono, 1);
        assert!((mono[0] - 0.3).abs() < 1e-6);
        let mut quad = [9.0f32; 4];
        r.render(&mut quad, 4);
        assert_eq!(quad, [0.2, 0.4, 0.0, 0.0]);
    }

    #[test]
    fn stream_underrun_rebuffers() {
        let (mut p, mut r, _, shared) = setup(64);
        shared.rebuffer_on_underrun.store(true, Ordering::Relaxed);
        shared.start_threshold.store(2, Ordering::Relaxed);
        push(&mut p, &[(0.1, 0.1); 2]);
        let mut out = [0.0f32; 6];
        r.render(&mut out, 2);
        assert!(shared.buffering.load(Ordering::Relaxed));
        push(&mut p, &[(0.1, 0.1)]);
        r.render(&mut out, 2);
        assert_eq!(out, [0.0; 6], "still below threshold");
    }

    #[test]
    fn analyzer_sees_latest_audio() {
        let (mut p, mut r, mut a, _) = setup(4096);
        let frames: Vec<(f32, f32)> = (0..600).map(|i| (i as f32, -(i as f32))).collect();
        push(&mut p, &frames);
        let mut out = vec![0.0f32; 1200];
        r.render(&mut out, 2);
        let scope = a.read();
        assert_eq!(scope.left[SCOPE_FRAMES - 1], 599.0);
        assert_eq!(scope.left[0], (600 - SCOPE_FRAMES) as f32);
        assert_eq!(scope.right[SCOPE_FRAMES - 1], -599.0);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p winrust-audio renderer::`
Expected: compile errors (`Renderer`, `Shared`, `AnalyzerReader`, `Arc`, `Ordering`, `SCOPE_FRAMES` not found).

- [ ] **Step 3: Implement above the tests module**

```rust
//! The audio-callback side: pulls stereo frames from the ring, applies volume, feeds the analyzer.
//!
//! Real-time rules: no allocation, no blocking, no panics in `render`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// Samples per channel kept for visualization (Winamp's window size).
pub const SCOPE_FRAMES: usize = 576;

/// Latest audio for visualizations, oldest sample first.
#[derive(Clone, Debug, PartialEq)]
pub struct Scope {
    pub left: [f32; SCOPE_FRAMES],
    pub right: [f32; SCOPE_FRAMES],
}

impl Default for Scope {
    fn default() -> Self {
        Scope {
            left: [0.0; SCOPE_FRAMES],
            right: [0.0; SCOPE_FRAMES],
        }
    }
}

/// Reader half of the analyzer tap. Lock-free; call from the UI thread.
pub struct AnalyzerReader(triple_buffer::Output<Scope>);

impl AnalyzerReader {
    pub fn read(&mut self) -> &Scope {
        self.0.read()
    }
}

/// State shared between the decode thread, the engine handle and the callback.
#[derive(Debug)]
pub struct Shared {
    volume_bits: AtomicU32,
    pub paused: AtomicBool,
    pub flush_requested: AtomicU64,
    pub flush_done: AtomicU64,
    /// Real (non-silence) frames written to the device since start.
    pub frames_played: AtomicU64,
    /// True while the callback waits for enough audio before (re)starting.
    pub buffering: AtomicBool,
    /// Frames that must be queued before playback starts after a flush or underrun.
    pub start_threshold: AtomicU64,
    /// Streams re-enter buffering on underrun; files just play what they have.
    pub rebuffer_on_underrun: AtomicBool,
    /// The decoder finished; play out whatever is left even below the threshold.
    pub producer_done: AtomicBool,
}

impl Shared {
    pub fn new() -> Self {
        Shared {
            volume_bits: AtomicU32::new(1.0f32.to_bits()),
            paused: AtomicBool::new(false),
            flush_requested: AtomicU64::new(0),
            flush_done: AtomicU64::new(0),
            frames_played: AtomicU64::new(0),
            buffering: AtomicBool::new(false),
            start_threshold: AtomicU64::new(0),
            rebuffer_on_underrun: AtomicBool::new(false),
            producer_done: AtomicBool::new(true),
        }
    }

    pub fn set_volume(&self, gain: f32) {
        self.volume_bits.store(gain.to_bits(), Ordering::Relaxed);
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Renderer {
    consumer: rtrb::Consumer<f32>,
    shared: Arc<Shared>,
    gain: f32,
    flush_seen: u64,
    gated: bool,
    scope_in: triple_buffer::Input<Scope>,
    window: Scope,
    window_pos: usize,
}

impl Renderer {
    /// Creates the renderer plus the reader for its analyzer tap.
    pub fn new(consumer: rtrb::Consumer<f32>, shared: Arc<Shared>) -> (Self, AnalyzerReader) {
        let (scope_in, scope_out) = triple_buffer::triple_buffer(&Scope::default());
        let gain = shared.volume();
        let r = Renderer {
            consumer,
            shared,
            gain,
            flush_seen: 0,
            gated: true,
            scope_in,
            window: Scope::default(),
            window_pos: 0,
        };
        (r, AnalyzerReader(scope_out))
    }

    /// Fills `out` (interleaved, `channels` wide) with the next audio.
    /// Returns how many frames were real audio; the rest is silence.
    pub fn render(&mut self, out: &mut [f32], channels: usize) -> usize {
        let channels = channels.max(1);
        let frames = out.len() / channels;
        let shared = Arc::clone(&self.shared);

        let requested = shared.flush_requested.load(Ordering::Acquire);
        if requested != self.flush_seen {
            let n = self.consumer.slots();
            if let Ok(chunk) = self.consumer.read_chunk(n) {
                chunk.commit_all();
            }
            self.flush_seen = requested;
            self.gated = true;
            shared.flush_done.store(requested, Ordering::Release);
        }

        if shared.paused.load(Ordering::Relaxed) {
            out.fill(0.0);
            return 0;
        }

        let available = self.consumer.slots() / 2;
        let done = shared.producer_done.load(Ordering::Acquire);
        if self.gated {
            let threshold = shared.start_threshold.load(Ordering::Relaxed) as usize;
            if available >= threshold.max(1) || (done && available > 0) {
                self.gated = false;
                shared.buffering.store(false, Ordering::Relaxed);
            } else {
                shared.buffering.store(!done, Ordering::Relaxed);
                out.fill(0.0);
                return 0;
            }
        }

        let n = frames.min(available);
        let target = shared.volume();
        let step = if n > 0 {
            (target - self.gain) / n as f32
        } else {
            0.0
        };
        if let Ok(chunk) = self.consumer.read_chunk(n * 2) {
            let (a, b) = chunk.as_slices();
            let mut samples = a.iter().chain(b.iter());
            for frame in out.chunks_exact_mut(channels).take(n) {
                let l = *samples.next().unwrap_or(&0.0);
                let r = *samples.next().unwrap_or(&0.0);
                // Analyzer window (pre-volume, like Winamp's vis).
                self.window.left[self.window_pos] = l;
                self.window.right[self.window_pos] = r;
                self.window_pos = (self.window_pos + 1) % SCOPE_FRAMES;
                self.gain += step;
                let (l, r) = (l * self.gain, r * self.gain);
                if channels == 1 {
                    frame[0] = 0.5 * (l + r);
                } else {
                    frame[0] = l;
                    frame[1] = r;
                    frame[2..].fill(0.0);
                }
            }
            chunk.commit_all();
        }
        self.gain = target;
        out[n * channels..].fill(0.0);
        if n > 0 {
            shared.frames_played.fetch_add(n as u64, Ordering::Release);
            self.publish_scope();
        }
        if n < frames && !done && shared.rebuffer_on_underrun.load(Ordering::Relaxed) {
            self.gated = true;
            shared.buffering.store(true, Ordering::Relaxed);
        }
        n
    }

    fn publish_scope(&mut self) {
        let dst = self.scope_in.input_buffer_mut();
        let split = self.window_pos;
        let tail = SCOPE_FRAMES - split;
        dst.left[..tail].copy_from_slice(&self.window.left[split..]);
        dst.left[tail..].copy_from_slice(&self.window.left[..split]);
        dst.right[..tail].copy_from_slice(&self.window.right[split..]);
        dst.right[tail..].copy_from_slice(&self.window.right[..split]);
        self.scope_in.publish();
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p winrust-audio renderer::`
Expected: `test result: ok. 9 passed`.

- [ ] **Step 5: Gates and commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/audio/src/renderer.rs crates/audio/src/lib.rs
git commit -m "feat(audio): add real-time renderer with volume ramp, flush protocol and analyzer tap" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Timeline (which track is audible, and where)

**Files:**
- Create: `crates/audio/src/timeline.rs`
- Modify: `crates/audio/src/lib.rs` (add `pub mod timeline;`)

**Interfaces:**
- Consumes: `TrackInfo` (Task 1).
- Produces: `enum TimelineEvent { Started { info: TrackInfo, gapless: bool }, Ended }` and `struct Timeline` with `fn new(device_rate: u32)`, `fn push_track(&mut self, start_frame: u64, info: TrackInfo, gapless: bool)`, `fn set_end(&mut self, end_frame: u64)`, `fn reposition(&mut self, start_frame: u64, base: Duration)` (after a seek), `fn clear(&mut self)`, `fn current(&self) -> Option<&TrackInfo>`, `fn poll(&mut self, played: u64) -> Vec<TimelineEvent>`, `fn position(&self, played: u64) -> Option<Duration>`. Frame numbers count device frames actually played (`Shared::frames_played`).

- [ ] **Step 1: Write the failing tests**

Add `pub mod timeline;` to `lib.rs`. Create `crates/audio/src/timeline.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Source;

    fn info(name: &str) -> TrackInfo {
        TrackInfo {
            source: Source::File(name.into()),
            kbps: None,
            sample_rate: 44_100,
            channels: 2,
            duration: None,
            seekable: true,
        }
    }

    #[test]
    fn track_starts_when_its_first_frame_plays() {
        let mut t = Timeline::new(1_000);
        t.push_track(10, info("a"), false);
        assert!(t.poll(9).is_empty());
        assert_eq!(
            t.poll(10),
            vec![TimelineEvent::Started {
                info: info("a"),
                gapless: false
            }]
        );
        assert_eq!(t.position(510), Some(Duration::from_millis(500)));
    }

    #[test]
    fn gapless_handover_then_end() {
        let mut t = Timeline::new(1_000);
        t.push_track(0, info("a"), false);
        t.push_track(1_000, info("b"), true);
        t.set_end(1_500);
        t.poll(0);
        assert!(t.poll(999).is_empty());
        assert_eq!(
            t.poll(1_000),
            vec![TimelineEvent::Started {
                info: info("b"),
                gapless: true
            }]
        );
        assert_eq!(t.position(1_250), Some(Duration::from_millis(250)));
        assert_eq!(t.poll(1_500), vec![TimelineEvent::Ended]);
        assert_eq!(t.position(1_600), None);
        assert!(t.poll(2_000).is_empty(), "end reported once");
    }

    #[test]
    fn end_waits_for_pending_tracks() {
        let mut t = Timeline::new(1_000);
        t.push_track(0, info("a"), false);
        t.set_end(100);
        // Callback jumped past both the start and the end in one poll.
        assert_eq!(
            t.poll(200),
            vec![
                TimelineEvent::Started {
                    info: info("a"),
                    gapless: false
                },
                TimelineEvent::Ended
            ]
        );
    }

    #[test]
    fn reposition_after_seek() {
        let mut t = Timeline::new(1_000);
        t.push_track(0, info("a"), false);
        t.poll(0);
        t.reposition(400, Duration::from_secs(60));
        assert_eq!(t.position(400), Some(Duration::from_secs(60)));
        assert_eq!(t.position(900), Some(Duration::from_millis(60_500)));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p winrust-audio timeline::`
Expected: compile errors (`Timeline`, `TimelineEvent`, `TrackInfo`, `Duration` not found).

- [ ] **Step 3: Implement above the tests module**

```rust
//! Maps "frames played by the device" to tracks and positions.
//!
//! The decode thread records where each track starts in the device frame stream; the
//! callback counts frames actually played. Comparing the two tells us which track is
//! audible, where in it we are, and when it ended.

use crate::types::TrackInfo;
use std::collections::VecDeque;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub enum TimelineEvent {
    Started { info: TrackInfo, gapless: bool },
    Ended,
}

#[derive(Clone, Debug)]
struct Segment {
    start_frame: u64,
    base: Duration,
    info: TrackInfo,
    gapless: bool,
}

#[derive(Debug)]
pub struct Timeline {
    device_rate: u32,
    current: Option<Segment>,
    upcoming: VecDeque<Segment>,
    end_frame: Option<u64>,
}

impl Timeline {
    pub fn new(device_rate: u32) -> Self {
        Timeline {
            device_rate,
            current: None,
            upcoming: VecDeque::new(),
            end_frame: None,
        }
    }

    /// A track whose first frame will be device frame `start_frame`.
    pub fn push_track(&mut self, start_frame: u64, info: TrackInfo, gapless: bool) {
        self.upcoming.push_back(Segment {
            start_frame,
            base: Duration::ZERO,
            info,
            gapless,
        });
    }

    /// The last pushed track ends at device frame `end_frame` (exclusive).
    pub fn set_end(&mut self, end_frame: u64) {
        self.end_frame = Some(end_frame);
    }

    /// After a seek the current track restarts at `start_frame` from position `base`.
    pub fn reposition(&mut self, start_frame: u64, base: Duration) {
        self.upcoming.clear();
        self.end_frame = None;
        if let Some(seg) = self.current.as_mut() {
            seg.start_frame = start_frame;
            seg.base = base;
        }
    }

    pub fn clear(&mut self) {
        self.current = None;
        self.upcoming.clear();
        self.end_frame = None;
    }

    pub fn current(&self) -> Option<&TrackInfo> {
        self.current.as_ref().map(|s| &s.info)
    }

    /// Advances to `played` device frames and reports what changed.
    pub fn poll(&mut self, played: u64) -> Vec<TimelineEvent> {
        let mut events = Vec::new();
        while self
            .upcoming
            .front()
            .is_some_and(|s| s.start_frame <= played)
        {
            let seg = self.upcoming.pop_front().expect("checked front");
            events.push(TimelineEvent::Started {
                info: seg.info.clone(),
                gapless: seg.gapless,
            });
            self.current = Some(seg);
        }
        if self.upcoming.is_empty() && self.end_frame.is_some_and(|end| played >= end) {
            self.end_frame = None;
            if self.current.take().is_some() {
                events.push(TimelineEvent::Ended);
            }
        }
        events
    }

    /// Position inside the audible track.
    pub fn position(&self, played: u64) -> Option<Duration> {
        let seg = self.current.as_ref()?;
        let frames = played.saturating_sub(seg.start_frame);
        Some(seg.base + Duration::from_secs_f64(frames as f64 / f64::from(self.device_rate)))
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p winrust-audio timeline::`
Expected: `test result: ok. 4 passed`.

- [ ] **Step 5: Gates and commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/audio/src/timeline.rs crates/audio/src/lib.rs
git commit -m "feat(audio): add playback timeline for track boundaries and position" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 12: Output sinks (`CpalSink` for devices, `CaptureSink` for tests)

**Files:**
- Create: `crates/audio/src/sink.rs`
- Modify: `crates/audio/src/lib.rs` (add `pub mod sink;`)
- Test: `crates/audio/tests/sink.rs`

**Interfaces:**
- Consumes: `Renderer` (Task 10), `EngineError` (Task 1), cpal 0.18.
- Produces:
  - `type SharedRenderer = Arc<Mutex<Renderer>>` (the callback only ever calls `try_lock`)
  - `enum SinkEvent { RateChanged(u32), Error(String) }`
  - `trait AudioSink: Send { fn start(&mut self, renderer: SharedRenderer) -> Result<u32, EngineError>; fn set_device(&mut self, name: Option<String>) -> Result<u32, EngineError>; fn poll(&mut self) -> Option<SinkEvent>; fn stop(&mut self); }` — `start` and `set_device` return the device sample rate
  - `fn output_devices() -> Vec<String>`
  - `struct CaptureSink` with `fn new(rate: u32)` (as fast as possible), `fn realtime(rate: u32)` (device pace), `fn captured(&self) -> Arc<Mutex<Vec<f32>>>` (interleaved stereo, real frames only)
  - `struct CpalSink` with `fn new(device: Option<String>)`. A dedicated `winrust-output` thread owns the stream (cpal streams are not `Send` everywhere). It rebuilds the stream when the device is lost, retrying every 1 s, and reports `RateChanged` if the new stream runs at another rate.

- [ ] **Step 1: Write the failing tests**

Add `pub mod sink;` to `lib.rs`. Create `crates/audio/tests/sink.rs`:

```rust
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winrust_audio::renderer::{Renderer, Shared};
use winrust_audio::sink::{AudioSink, CaptureSink, output_devices};

#[test]
fn capture_sink_records_rendered_audio() {
    let (mut producer, consumer) = rtrb::RingBuffer::new(64);
    let shared = Arc::new(Shared::new());
    shared.producer_done.store(true, Ordering::Release);
    let (renderer, _analyzer) = Renderer::new(consumer, shared);
    let mut sink = CaptureSink::new(44_100);
    let captured = sink.captured();
    assert_eq!(sink.start(Arc::new(Mutex::new(renderer))).unwrap(), 44_100);

    for s in [0.1f32, 0.2, 0.3, 0.4] {
        producer.push(s).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while captured.lock().unwrap().len() < 4 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    sink.stop();
    assert_eq!(*captured.lock().unwrap(), vec![0.1, 0.2, 0.3, 0.4]);
}

#[test]
fn listing_devices_never_panics() {
    // CI machines have no sound card; this must still return (possibly empty).
    let _ = output_devices();
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p winrust-audio --test sink`
Expected: compile error (`file not found for module sink` / unresolved import `winrust_audio::sink`).

- [ ] **Step 3: Implement the sinks**

`crates/audio/src/sink.rs`:

```rust
//! Audio outputs. `CpalSink` drives a real device; `CaptureSink` records for tests.

use crate::renderer::Renderer;
use crate::types::EngineError;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, ErrorKind, FromSample, SampleFormat, SizedSample, StreamConfig};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, bounded, unbounded};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// The renderer is locked only with `try_lock` from the callback, so it never blocks there.
pub type SharedRenderer = Arc<Mutex<Renderer>>;

#[derive(Clone, Debug, PartialEq)]
pub enum SinkEvent {
    /// The device was reopened at a different rate; the engine must reconfigure.
    RateChanged(u32),
    Error(String),
}

pub trait AudioSink: Send {
    /// Starts pulling audio from `renderer`. Returns the device sample rate.
    fn start(&mut self, renderer: SharedRenderer) -> Result<u32, EngineError>;
    /// Switches to the named device (`None` = system default). Returns the new rate.
    fn set_device(&mut self, name: Option<String>) -> Result<u32, EngineError>;
    /// Problems noticed since the last call. Never blocks.
    fn poll(&mut self) -> Option<SinkEvent>;
    fn stop(&mut self);
}

/// Names of all output devices on the default host.
pub fn output_devices() -> Vec<String> {
    let host = cpal::default_host();
    let Ok(devices) = host.output_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|d| d.description().ok().map(|desc| desc.name().to_string()))
        .collect()
}

// ---------------------------------------------------------------------------------------------

/// Test output: records only real (non-silence) frames, as interleaved stereo.
pub struct CaptureSink {
    rate: u32,
    realtime: bool,
    captured: Arc<Mutex<Vec<f32>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl CaptureSink {
    /// Pulls audio as fast as the engine can produce it.
    pub fn new(rate: u32) -> Self {
        CaptureSink {
            rate,
            realtime: false,
            captured: Arc::default(),
            stop: Arc::default(),
            thread: None,
        }
    }

    /// Pulls audio at the speed a real device would.
    pub fn realtime(rate: u32) -> Self {
        let mut sink = Self::new(rate);
        sink.realtime = true;
        sink
    }

    /// Interleaved stereo captured so far.
    pub fn captured(&self) -> Arc<Mutex<Vec<f32>>> {
        self.captured.clone()
    }
}

impl AudioSink for CaptureSink {
    fn start(&mut self, renderer: SharedRenderer) -> Result<u32, EngineError> {
        let captured = self.captured.clone();
        let stop = self.stop.clone();
        let period = if self.realtime {
            Duration::from_secs_f64(512.0 / f64::from(self.rate))
        } else {
            Duration::from_micros(500)
        };
        self.thread = Some(thread::spawn(move || {
            let mut buf = [0.0f32; 1024];
            while !stop.load(Ordering::Relaxed) {
                let n = renderer
                    .lock()
                    .map(|mut r| r.render(&mut buf, 2))
                    .unwrap_or(0);
                if n > 0 {
                    captured.lock().unwrap().extend_from_slice(&buf[..n * 2]);
                }
                thread::sleep(period);
            }
        }));
        Ok(self.rate)
    }

    fn set_device(&mut self, _name: Option<String>) -> Result<u32, EngineError> {
        Ok(self.rate)
    }

    fn poll(&mut self) -> Option<SinkEvent> {
        None
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for CaptureSink {
    fn drop(&mut self) {
        self.stop();
    }
}

// ---------------------------------------------------------------------------------------------

/// Real output device. A dedicated thread owns the cpal stream (streams are not `Send` on
/// every platform) and rebuilds it when the device disappears.
pub struct CpalSink {
    device: Option<String>,
    rate: u32,
    renderer: Option<SharedRenderer>,
    events_tx: Sender<SinkEvent>,
    events_rx: Receiver<SinkEvent>,
    worker: Option<(Sender<()>, JoinHandle<()>)>,
}

impl CpalSink {
    /// `device`: output device name, `None` for the system default.
    pub fn new(device: Option<String>) -> Self {
        let (events_tx, events_rx) = unbounded();
        CpalSink {
            device,
            rate: 0,
            renderer: None,
            events_tx,
            events_rx,
            worker: None,
        }
    }

    fn launch(&mut self, preferred_rate: Option<u32>) -> Result<u32, EngineError> {
        self.stop();
        let renderer = self
            .renderer
            .clone()
            .ok_or_else(|| EngineError::Device("sink not started".into()))?;
        let (ready_tx, ready_rx) = bounded(1);
        let (stop_tx, stop_rx) = bounded(1);
        let name = self.device.clone();
        let events = self.events_tx.clone();
        let handle = thread::Builder::new()
            .name("winrust-output".into())
            .spawn(move || device_thread(name, renderer, preferred_rate, ready_tx, stop_rx, events))
            .map_err(|e| EngineError::Device(e.to_string()))?;
        self.worker = Some((stop_tx, handle));
        let rate = ready_rx
            .recv()
            .map_err(|_| EngineError::Device("output thread died".into()))??;
        self.rate = rate;
        Ok(rate)
    }
}

impl AudioSink for CpalSink {
    fn start(&mut self, renderer: SharedRenderer) -> Result<u32, EngineError> {
        self.renderer = Some(renderer);
        self.launch(None)
    }

    fn set_device(&mut self, name: Option<String>) -> Result<u32, EngineError> {
        self.device = name;
        let rate = self.rate;
        self.launch(Some(rate))
    }

    fn poll(&mut self) -> Option<SinkEvent> {
        match self.events_rx.try_recv().ok()? {
            SinkEvent::RateChanged(r) => {
                self.rate = r;
                Some(SinkEvent::RateChanged(r))
            }
            other => Some(other),
        }
    }

    fn stop(&mut self) {
        if let Some((stop, handle)) = self.worker.take() {
            let _ = stop.send(());
            let _ = handle.join();
        }
    }
}

impl Drop for CpalSink {
    fn drop(&mut self) {
        self.stop();
    }
}

fn device_thread(
    name: Option<String>,
    renderer: SharedRenderer,
    mut preferred_rate: Option<u32>,
    ready: Sender<Result<u32, EngineError>>,
    stop: Receiver<()>,
    events: Sender<SinkEvent>,
) {
    let mut first = true;
    loop {
        let lost = Arc::new(AtomicBool::new(false));
        match open_stream(name.as_deref(), &renderer, preferred_rate, lost.clone()) {
            Ok((stream, rate)) => {
                if first {
                    let _ = ready.send(Ok(rate));
                    first = false;
                } else if Some(rate) != preferred_rate {
                    let _ = events.send(SinkEvent::RateChanged(rate));
                }
                preferred_rate = Some(rate);
                loop {
                    match stop.recv_timeout(Duration::from_millis(100)) {
                        Ok(()) | Err(RecvTimeoutError::Disconnected) => return,
                        Err(RecvTimeoutError::Timeout) if lost.load(Ordering::Relaxed) => break,
                        Err(RecvTimeoutError::Timeout) => {}
                    }
                }
                drop(stream);
                let _ = events.send(SinkEvent::Error("audio device lost, reopening".into()));
            }
            Err(e) => {
                if first {
                    let _ = ready.send(Err(e));
                    return;
                }
                // Device gone (e.g. unplugged): retry once a second until stopped.
                match stop.recv_timeout(Duration::from_secs(1)) {
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => return,
                    Err(RecvTimeoutError::Timeout) => {}
                }
            }
        }
    }
}

fn open_stream(
    name: Option<&str>,
    renderer: &SharedRenderer,
    preferred_rate: Option<u32>,
    lost: Arc<AtomicBool>,
) -> Result<(cpal::Stream, u32), EngineError> {
    let host = cpal::default_host();
    let named = name.and_then(|n| {
        host.output_devices().ok()?.find(|d| {
            d.description()
                .map(|desc| desc.name() == n)
                .unwrap_or(false)
        })
    });
    let device = named
        .or_else(|| host.default_output_device())
        .ok_or_else(|| EngineError::Device("no output device".into()))?;
    let default = device
        .default_output_config()
        .map_err(|e| EngineError::Device(e.to_string()))?;
    let channels = default.channels();
    let format = default.sample_format();
    let mut rate = default.sample_rate();
    if let Some(want) = preferred_rate {
        let supported = device.supported_output_configs().map(|mut configs| {
            configs.any(|c| {
                c.channels() == channels
                    && c.sample_format() == format
                    && c.min_sample_rate() <= want
                    && want <= c.max_sample_rate()
            })
        });
        if supported.unwrap_or(false) {
            rate = want;
        }
    }
    let config = StreamConfig {
        channels,
        sample_rate: rate,
        buffer_size: BufferSize::Default,
    };
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, config, renderer, lost),
        SampleFormat::I16 => build::<i16>(&device, config, renderer, lost),
        SampleFormat::U16 => build::<u16>(&device, config, renderer, lost),
        SampleFormat::I32 => build::<i32>(&device, config, renderer, lost),
        SampleFormat::F64 => build::<f64>(&device, config, renderer, lost),
        SampleFormat::I8 => build::<i8>(&device, config, renderer, lost),
        SampleFormat::U8 => build::<u8>(&device, config, renderer, lost),
        other => Err(EngineError::Device(format!(
            "unsupported sample format {other}"
        ))),
    }?;
    stream
        .play()
        .map_err(|e| EngineError::Device(e.to_string()))?;
    Ok((stream, rate))
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    renderer: &SharedRenderer,
    lost: Arc<AtomicBool>,
) -> Result<cpal::Stream, EngineError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels);
    let renderer = renderer.clone();
    let mut scratch = vec![0.0f32; 16_384];
    device
        .build_output_stream::<T, _, _>(
            config,
            move |data: &mut [T], _| {
                if scratch.len() < data.len() {
                    scratch.resize(data.len(), 0.0);
                }
                let buf = &mut scratch[..data.len()];
                match renderer.try_lock() {
                    Ok(mut r) => {
                        r.render(buf, channels);
                    }
                    Err(_) => buf.fill(0.0),
                }
                for (d, s) in data.iter_mut().zip(buf.iter()) {
                    *d = T::from_sample(*s);
                }
            },
            move |err: cpal::Error| match err.kind() {
                ErrorKind::Xrun | ErrorKind::RealtimeDenied => {}
                _ => lost.store(true, Ordering::Relaxed),
            },
            None,
        )
        .map_err(|e| EngineError::Device(e.to_string()))
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p winrust-audio --test sink`
Expected: `test result: ok. 2 passed`.

- [ ] **Step 5: Gates and commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/audio/src/sink.rs crates/audio/src/lib.rs crates/audio/tests/sink.rs
git commit -m "feat(audio): add cpal output sink with device recovery and capture sink for tests" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 13: Engine (decode thread, handle, gapless, seek, streams)

**Files:**
- Create: `crates/audio/src/engine.rs`
- Modify: `crates/audio/src/lib.rs` (final version below)
- Create: `crates/audio/tests/common/mod.rs`
- Create: `crates/audio/tests/engine.rs`
- Create: `crates/audio/tests/engine_edges.rs` (Review Focus 1–5)
- Modify: `crates/audio/tests/formats.rs` (add the MP3 gapless test)

**Interfaces:**
- Consumes: everything from Tasks 1–12.
- Produces (the API M3 builds on):
  - `struct EngineHandle` (`Clone`) with `fn send(&self, cmd: Command)`. `SetVolume` is applied immediately via the shared atomic. `Load`, `Stop`, `Shutdown` and `SetDevice` bump the cancel epoch first, so blocked network work aborts.
  - `struct Engine` with `fn spawn(sink: Box<dyn AudioSink>) -> Result<Engine, EngineError>`, `fn handle(&self) -> EngineHandle`, `fn events(&self) -> &Receiver<EngineEvent>`, `fn take_analyzer(&mut self) -> Option<AnalyzerReader>` (once). Dropping it sends `Shutdown` and joins the thread.
  - Event contract:
    - `TrackStarted` fires when the first sample is audible; `gapless: true` when it followed via `QueueNext`.
    - `TrackEnded` fires once, after the last queued track's final sample plays.
    - `State` fires on every change (`Buffering` is sent immediately when a URL starts loading).
    - `Position` arrives at ~10 Hz while playing, and once right after each seek.
    - A cancelled load sends no `Error`.
    - `Play` after `Stop` (or after the end) restarts the last source.
    - Seeking is ignored for streams, and while the audible track's decoder is already finished.
  - Gapless rule: when the queued track has the same sample rate, the resampler keeps running across the boundary, so not a single sample is added or lost.

- [ ] **Step 1: Write the failing tests**

`crates/audio/tests/common/mod.rs`:

```rust
#![allow(dead_code)]

use crossbeam_channel::Receiver;
use std::f32::consts::PI;
use std::path::Path;
use std::time::{Duration, Instant};
use winrust_audio::EngineEvent;

/// 16-bit samples of a 440 Hz sine at half scale, frames `from..to`, every channel equal.
pub fn sine_i16(rate: u32, from: usize, to: usize) -> Vec<i16> {
    (from..to)
        .map(|i| {
            (0.5 * (2.0 * PI * 440.0 * i as f32 / rate as f32).sin() * f32::from(i16::MAX)) as i16
        })
        .collect()
}

pub fn write_wav(path: &Path, rate: u32, channels: u16, frames: &[i16]) {
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for &s in frames {
        for _ in 0..channels {
            w.write_sample(s).unwrap();
        }
    }
    w.finalize().unwrap();
}

/// WAV file bytes in memory.
pub fn wav_bytes(rate: u32, frames: &[i16]) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = std::io::Cursor::new(Vec::new());
    let mut w = hound::WavWriter::new(&mut cursor, spec).unwrap();
    for &s in frames {
        w.write_sample(s).unwrap();
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();
    cursor.into_inner()
}

/// Same conversion symphonia uses for 16-bit PCM.
pub fn to_f32(s: i16) -> f32 {
    f32::from(s) / 32_768.0
}

/// Collects events until `done` matches one (inclusive) or `timeout` passes.
pub fn wait_for(
    events: &Receiver<EngineEvent>,
    timeout: Duration,
    done: impl Fn(&EngineEvent) -> bool,
) -> Vec<EngineEvent> {
    let deadline = Instant::now() + timeout;
    let mut seen = Vec::new();
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match events.recv_timeout(left) {
            Ok(e) => {
                let stop = done(&e);
                seen.push(e);
                if stop {
                    return seen;
                }
            }
            Err(_) => break,
        }
    }
    panic!("timed out; events so far: {seen:#?}");
}
```

`crates/audio/tests/engine.rs`:

```rust
mod common;

use common::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;
use winrust_audio::{
    CaptureSink, Command, Engine, EngineError, EngineEvent, PlaybackState, Source,
};

const RATE: u32 = 44_100;
const LONG: Duration = Duration::from_secs(10);

fn engine(sink: CaptureSink) -> (Engine, std::sync::Arc<std::sync::Mutex<Vec<f32>>>) {
    let captured = sink.captured();
    (Engine::spawn(Box::new(sink)).unwrap(), captured)
}

fn is_ended(e: &EngineEvent) -> bool {
    matches!(e, EngineEvent::TrackEnded)
}

#[test]
fn plays_file_to_the_end() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.wav");
    let samples = sine_i16(RATE, 0, 22_050);
    write_wav(&path, RATE, 2, &samples);

    let (engine, captured) = engine(CaptureSink::new(RATE));
    engine
        .handle()
        .send(Command::Load(Source::File(path.clone())));
    let events = wait_for(engine.events(), LONG, is_ended);

    let started = events.iter().find_map(|e| match e {
        EngineEvent::TrackStarted { info, gapless } => Some((info.clone(), *gapless)),
        _ => None,
    });
    let (info, gapless) = started.expect("TrackStarted");
    assert_eq!(info.source, Source::File(path));
    assert_eq!(info.sample_rate, RATE);
    assert!(!gapless);
    assert!(events.contains(&EngineEvent::State(PlaybackState::Playing)));
    let audio = captured.lock().unwrap().clone();
    assert_eq!(
        audio.len(),
        2 * samples.len(),
        "every frame played exactly once"
    );
    let state = wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(_))
    });
    assert_eq!(
        state.last(),
        Some(&EngineEvent::State(PlaybackState::Stopped))
    );
}

#[test]
fn gapless_join_is_sample_exact() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.wav"), dir.path().join("b.wav"));
    // One continuous sine split across two files at an odd frame.
    write_wav(&a, RATE, 2, &sine_i16(RATE, 0, 30_001));
    write_wav(&b, RATE, 2, &sine_i16(RATE, 30_001, 50_000));

    let (engine, captured) = engine(CaptureSink::new(RATE));
    let h = engine.handle();
    h.send(Command::Load(Source::File(a)));
    h.send(Command::QueueNext(Source::File(b.clone())));
    let events = wait_for(engine.events(), LONG, is_ended);

    let gapless_start = events.iter().any(|e| {
        matches!(e, EngineEvent::TrackStarted { info, gapless: true } if info.source == Source::File(b.clone()))
    });
    assert!(gapless_start, "{events:#?}");
    let expected: Vec<f32> = sine_i16(RATE, 0, 50_000)
        .into_iter()
        .flat_map(|s| [to_f32(s), to_f32(s)])
        .collect();
    let audio = captured.lock().unwrap().clone();
    assert_eq!(audio.len(), expected.len());
    let worst = audio
        .iter()
        .zip(&expected)
        .map(|(x, y)| (x - y).abs())
        .fold(0f32, f32::max);
    assert!(worst < 1e-6, "max deviation {worst}");
}

#[test]
fn resamples_to_device_rate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("low.wav");
    write_wav(&path, 22_050, 1, &sine_i16(22_050, 0, 22_050));

    let (engine, captured) = engine(CaptureSink::new(48_000));
    engine.handle().send(Command::Load(Source::File(path)));
    let events = wait_for(engine.events(), LONG, is_ended);

    let info = events.iter().find_map(|e| match e {
        EngineEvent::TrackStarted { info, .. } => Some(info.clone()),
        _ => None,
    });
    assert_eq!(
        info.unwrap().sample_rate,
        22_050,
        "LCD shows the source rate"
    );
    let frames = captured.lock().unwrap().len() / 2;
    assert!((frames as i64 - 48_000).abs() < 300, "frames = {frames}");
}

#[test]
fn seek_jumps_and_reports_position() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.wav");
    write_wav(&path, RATE, 2, &sine_i16(RATE, 0, 3 * RATE as usize));

    let (engine, captured) = engine(CaptureSink::realtime(RATE));
    let h = engine.handle();
    h.send(Command::Load(Source::File(path)));
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::TrackStarted { .. })
    });
    h.send(Command::Seek(Duration::from_millis(2_500)));
    let events = wait_for(engine.events(), LONG, is_ended);

    let first_pos = events.iter().find_map(|e| match e {
        EngineEvent::Position(p) => Some(*p),
        _ => None,
    });
    let p = first_pos.expect("position after seek");
    assert!(
        p >= Duration::from_millis(2_490) && p < Duration::from_millis(2_700),
        "{p:?}"
    );
    let frames = captured.lock().unwrap().len() / 2;
    assert!(
        frames < 2 * RATE as usize,
        "skipped most of the file, played {frames}"
    );
}

#[test]
fn pause_holds_and_play_resumes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.wav");
    write_wav(&path, RATE, 2, &sine_i16(RATE, 0, 2 * RATE as usize));

    let (engine, captured) = engine(CaptureSink::realtime(RATE));
    let h = engine.handle();
    h.send(Command::Load(Source::File(path)));
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(PlaybackState::Playing))
    });
    h.send(Command::Pause);
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(PlaybackState::Paused))
    });
    let held = captured.lock().unwrap().len();
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(
        captured.lock().unwrap().len(),
        held,
        "no audio while paused"
    );
    h.send(Command::Play);
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(PlaybackState::Playing))
    });
}

#[test]
fn stop_then_play_restarts_track() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.wav");
    write_wav(&path, RATE, 2, &sine_i16(RATE, 0, 2 * RATE as usize));

    let (engine, _) = engine(CaptureSink::realtime(RATE));
    let h = engine.handle();
    h.send(Command::Load(Source::File(path.clone())));
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(PlaybackState::Playing))
    });
    h.send(Command::Stop);
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(PlaybackState::Stopped))
    });
    h.send(Command::Play);
    let events = wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::TrackStarted { .. })
    });
    assert!(
        matches!(events.last(), Some(EngineEvent::TrackStarted { info, .. }) if info.source == Source::File(path))
    );
}

#[test]
fn unreadable_file_reports_error_and_stays_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.mp3");
    std::fs::write(&path, b"not audio at all").unwrap();

    let (engine, _) = engine(CaptureSink::new(RATE));
    engine.handle().send(Command::Load(Source::File(path)));
    let events = wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::Error(_))
    });
    assert!(matches!(
        events.last(),
        Some(EngineEvent::Error(EngineError::Unsupported(_)))
    ));
    std::thread::sleep(Duration::from_millis(50));
    let later: Vec<_> = engine.events().try_iter().collect();
    assert!(
        !later
            .iter()
            .any(|e| matches!(e, EngineEvent::TrackStarted { .. })),
        "{later:?}"
    );
}

#[test]
fn bad_queued_track_still_ends_current_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ok.wav");
    write_wav(&path, RATE, 2, &sine_i16(RATE, 0, 4_410));

    let (engine, _) = engine(CaptureSink::new(RATE));
    let h = engine.handle();
    h.send(Command::Load(Source::File(path)));
    h.send(Command::QueueNext(Source::File(
        dir.path().join("missing.flac"),
    )));
    let events = wait_for(engine.events(), LONG, is_ended);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::Error(EngineError::Open { .. }))),
        "{events:#?}"
    );
}

#[test]
fn plays_icy_stream_with_titles() {
    let body = wav_bytes(RATE, &sine_i16(RATE, 0, 3 * RATE as usize));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        let mut req = [0u8; 2048];
        let n = sock.read(&mut req).unwrap();
        assert!(String::from_utf8_lossy(&req[..n]).contains("Icy-MetaData: 1"));
        let metaint = 16_000;
        let mut out = format!("ICY 200 OK\r\ncontent-type: audio/wav\r\nicy-metaint: {metaint}\r\nicy-br: 1411\r\n\r\n").into_bytes();
        let meta = b"StreamTitle='Winrust Radio - Test';";
        let mut block = meta.to_vec();
        block.resize(meta.len().div_ceil(16) * 16, 0);
        for (i, chunk) in body.chunks(metaint).enumerate() {
            out.extend_from_slice(chunk);
            if chunk.len() == metaint {
                if i == 0 {
                    out.push((block.len() / 16) as u8);
                    out.extend_from_slice(&block);
                } else {
                    out.push(0);
                }
            }
        }
        let _ = sock.write_all(&out);
    });

    let (engine, captured) = engine(CaptureSink::new(RATE));
    engine
        .handle()
        .send(Command::Load(Source::Url(format!("http://{addr}/live"))));
    let events = wait_for(engine.events(), LONG, is_ended);

    assert!(
        events.contains(&EngineEvent::StreamTitle("Winrust Radio - Test".into())),
        "{events:#?}"
    );
    let info = events
        .iter()
        .find_map(|e| match e {
            EngineEvent::TrackStarted { info, .. } => Some(info.clone()),
            _ => None,
        })
        .unwrap();
    assert!(!info.seekable);
    assert_eq!(info.kbps, Some(1411));
    let frames = captured.lock().unwrap().len() / 2;
    assert!(
        (frames as i64 - 3 * RATE as i64).abs() < 64,
        "frames = {frames}"
    );
}

#[test]
fn analyzer_sees_playing_audio() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v.wav");
    write_wav(&path, RATE, 2, &sine_i16(RATE, 0, RATE as usize));

    let sink = CaptureSink::new(RATE);
    let mut engine = Engine::spawn(Box::new(sink)).unwrap();
    let mut analyzer = engine.take_analyzer().unwrap();
    assert!(engine.take_analyzer().is_none(), "only one reader");
    engine.handle().send(Command::Load(Source::File(path)));
    wait_for(engine.events(), LONG, is_ended);
    let peak = analyzer
        .read()
        .left
        .iter()
        .fold(0f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.4, "peak {peak}");
}
```

`crates/audio/tests/engine_edges.rs`:

```rust
mod common;

use common::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use winrust_audio::sink::{SharedRenderer, SinkEvent};
use winrust_audio::{
    AudioSink, CaptureSink, Command, Engine, EngineError, EngineEvent, PlaybackState, Source,
};

const RATE: u32 = 44_100;
const LONG: Duration = Duration::from_secs(10);

/// CaptureSink that reports a device rate change when `trigger` is set.
struct RateChangingSink {
    inner: CaptureSink,
    trigger: Arc<AtomicBool>,
}

impl AudioSink for RateChangingSink {
    fn start(&mut self, renderer: SharedRenderer) -> Result<u32, EngineError> {
        self.inner.start(renderer)
    }
    fn set_device(&mut self, name: Option<String>) -> Result<u32, EngineError> {
        self.inner.set_device(name)
    }
    fn poll(&mut self) -> Option<SinkEvent> {
        self.trigger
            .swap(false, Ordering::AcqRel)
            .then_some(SinkEvent::RateChanged(48_000))
    }
    fn stop(&mut self) {
        self.inner.stop();
    }
}

#[test]
fn device_rate_change_resumes_same_track_near_same_position() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.wav");
    write_wav(&path, RATE, 2, &sine_i16(RATE, 0, 4 * RATE as usize));
    let trigger = Arc::new(AtomicBool::new(false));
    let sink = RateChangingSink {
        inner: CaptureSink::realtime(RATE),
        trigger: trigger.clone(),
    };
    let engine = Engine::spawn(Box::new(sink)).unwrap();
    engine.handle().send(Command::Load(Source::File(path)));
    let before = wait_for(
        engine.events(),
        LONG,
        |e| matches!(e, EngineEvent::Position(p) if *p >= Duration::from_millis(500)),
    );
    let EngineEvent::Position(at) = before.last().cloned().unwrap() else {
        unreachable!()
    };

    trigger.store(true, Ordering::Release);
    let after = wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::Position(_))
    });
    let EngineEvent::Position(resumed) = after.last().cloned().unwrap() else {
        unreachable!()
    };
    assert!(
        resumed + Duration::from_millis(50) >= at && resumed < at + Duration::from_millis(500),
        "{at:?} -> {resumed:?}"
    );
    assert!(
        !after.iter().any(|e| matches!(e, EngineEvent::Error(_))),
        "{after:#?}"
    );
}

#[test]
fn stop_is_prompt_while_stream_is_stalled() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        let mut req = [0u8; 1024];
        let _ = sock.read(&mut req);
        let mut out = b"HTTP/1.0 200 OK\r\ncontent-type: audio/wav\r\n\r\n".to_vec();
        // Header and a sliver of audio, then silence on the wire.
        out.extend(&wav_bytes(RATE, &sine_i16(RATE, 0, 10 * RATE as usize))[..4096]);
        sock.write_all(&out).unwrap();
        std::thread::sleep(Duration::from_secs(20));
    });
    let sink = CaptureSink::new(RATE);
    let engine = Engine::spawn(Box::new(sink)).unwrap();
    engine
        .handle()
        .send(Command::Load(Source::Url(format!("http://{addr}/"))));
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(PlaybackState::Buffering))
    });
    std::thread::sleep(Duration::from_millis(200));
    let asked = Instant::now();
    engine.handle().send(Command::Stop);
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(PlaybackState::Stopped))
    });
    assert!(
        asked.elapsed() < Duration::from_secs(1),
        "stop took {:?}",
        asked.elapsed()
    );
}

#[test]
fn rapid_loads_end_on_the_last_one() {
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (0..4)
        .map(|i| {
            let p = dir.path().join(format!("{i}.wav"));
            write_wav(&p, RATE, 2, &sine_i16(RATE, 0, RATE as usize / 2));
            p
        })
        .collect();
    let sink = CaptureSink::realtime(RATE);
    let engine = Engine::spawn(Box::new(sink)).unwrap();
    for p in &paths {
        engine.handle().send(Command::Load(Source::File(p.clone())));
    }
    let events = wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::TrackEnded)
    });
    let last_started = events.iter().rev().find_map(|e| match e {
        EngineEvent::TrackStarted { info, .. } => Some(info.source.clone()),
        _ => None,
    });
    assert_eq!(last_started, Some(Source::File(paths[3].clone())));
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, EngineEvent::TrackEnded))
            .count(),
        1
    );
}

#[test]
fn plays_from_non_ascii_path() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Μουσική ♫ 音楽");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("Χρυσοβέργης – τραγούδι.wav");
    write_wav(&path, RATE, 2, &sine_i16(RATE, 0, 4_410));
    let sink = CaptureSink::new(RATE);
    let engine = Engine::spawn(Box::new(sink)).unwrap();
    engine
        .handle()
        .send(Command::Load(Source::File(path.clone())));
    let events = wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::TrackEnded)
    });
    assert!(events.iter().any(|e| matches!(e, EngineEvent::TrackStarted { info, .. } if info.source == Source::File(path.clone()))));
}

#[test]
fn seek_past_the_end_just_ends_the_track() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("short.wav");
    write_wav(&path, RATE, 2, &sine_i16(RATE, 0, 2 * RATE as usize));
    let sink = CaptureSink::realtime(RATE);
    let engine = Engine::spawn(Box::new(sink)).unwrap();
    engine.handle().send(Command::Load(Source::File(path)));
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::TrackStarted { .. })
    });
    engine
        .handle()
        .send(Command::Seek(Duration::from_secs(600)));
    let events = wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::TrackEnded)
    });
    assert!(
        events
            .iter()
            .all(|e| !matches!(e, EngineEvent::Position(p) if *p > Duration::from_secs(3))),
        "{events:#?}"
    );
}
```

Replace `crates/audio/tests/formats.rs` with the version that adds `mp3_halves_play_back_gaplessly`:

```rust
//! Decoding of every supported format, using fixtures from `cargo xtask gen-fixtures`.

mod common;

use common::*;
use std::path::{Path, PathBuf};
use std::time::Duration;
use winrust_audio::decoder::Decoder;
use winrust_audio::{CaptureSink, Command, Engine, EngineEvent, Source};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn left(stereo: &[f32]) -> Vec<f32> {
    stereo.iter().step_by(2).copied().collect()
}

fn rising_zero_crossings(signal: &[f32]) -> usize {
    signal
        .windows(2)
        .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
        .count()
}

#[test]
fn decodes_every_supported_format() {
    // (file, lossless)
    let cases = [
        ("tone.mp3", false),
        ("tone.flac", true),
        ("tone.ogg", false),
        ("tone.m4a", false),
        ("tone-alac.m4a", true),
    ];
    for (name, lossless) in cases {
        let mut dec = Decoder::open_file(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let info = dec.info().clone();
        assert_eq!(info.sample_rate, 44_100, "{name}");
        assert_eq!(info.channels, 2, "{name}");
        let dur = info
            .duration
            .unwrap_or_else(|| panic!("{name}: no duration"));
        assert!(
            (dur.as_secs_f64() - 1.0).abs() < 0.05,
            "{name}: duration {dur:?}"
        );
        let mut out = Vec::new();
        while dec
            .decode_next(&mut out)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
        {}
        let frames = out.len() / 2;
        let slack = if lossless { 0 } else { 2_048 };
        assert!(
            (frames as i64 - 44_100).abs() <= slack,
            "{name}: {frames} frames"
        );
        let crossings = rising_zero_crossings(&left(&out));
        assert!(
            (crossings as i64 - 440).abs() <= 3,
            "{name}: {crossings} crossings"
        );
    }
}

#[test]
fn mp3_halves_play_back_gaplessly() {
    let sink = CaptureSink::new(44_100);
    let captured = sink.captured();
    let engine = Engine::spawn(Box::new(sink)).unwrap();
    let h = engine.handle();
    h.send(Command::Load(Source::File(fixture("half1.mp3"))));
    h.send(Command::QueueNext(Source::File(fixture("half2.mp3"))));
    wait_for(engine.events(), Duration::from_secs(10), |e| {
        matches!(e, EngineEvent::TrackEnded)
    });

    let signal = left(&captured.lock().unwrap());
    // Encoder delay and padding trimmed: two seconds, give or take a few frames.
    assert!(
        (signal.len() as i64 - 88_200).abs() < 100,
        "{} frames",
        signal.len()
    );
    // No click at the join: the biggest sample-to-sample step near the boundary is no
    // bigger than anywhere else in the signal.
    let steps: Vec<f32> = signal.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
    let elsewhere = steps[1_000..40_000].iter().fold(0f32, |m, s| m.max(*s));
    let at_join = steps[43_600..44_600].iter().fold(0f32, |m, s| m.max(*s));
    assert!(
        at_join <= elsewhere * 1.5 + 0.002,
        "join step {at_join} vs normal {elsewhere}"
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p winrust-audio --tests`
Expected: compile errors (`Engine`, `CaptureSink`, `Command` etc. not found at the crate root — `engine` module and re-exports don't exist yet).

- [ ] **Step 3: Implement the engine**

`crates/audio/src/engine.rs`:

```rust
//! The decode thread and the public `Engine` / `EngineHandle`.

use crate::decoder::Decoder;
use crate::dsp::DspChain;
use crate::renderer::{AnalyzerReader, Renderer, Shared};
use crate::resample::StreamResampler;
use crate::sink::{AudioSink, SinkEvent};
use crate::stream::{self, Cancel, extension_for_mime};
use crate::timeline::{Timeline, TimelineEvent};
use crate::types::{Command, EngineError, EngineEvent, EqSettings, PlaybackState, Source};
use crate::volume::slider_to_gain;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, unbounded};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::ReadOnlySource;

/// Ring capacity: 8 s of stereo at 48 kHz.
const RING_FRAMES: usize = 8 * 48_000;
/// Files: keep this much decoded ahead (also the latency of EQ/balance changes).
const FILE_FILL: Duration = Duration::from_millis(200);
/// Files: start playing once this much is queued.
const FILE_START: Duration = Duration::from_millis(50);
/// Streams: prebuffer before starting and after an underrun.
const STREAM_START: Duration = Duration::from_secs(2);
/// Streams: keep this much decoded ahead.
const STREAM_FILL: Duration = Duration::from_secs(4);
const IDLE_TICK: Duration = Duration::from_millis(10);
const POSITION_EVERY: Duration = Duration::from_millis(100);
const FLUSH_TIMEOUT: Duration = Duration::from_millis(500);
/// Packets decoded per loop turn before checking commands again.
const PACKETS_PER_TURN: usize = 16;

/// Cheap, cloneable way to control the engine from any thread.
#[derive(Clone)]
pub struct EngineHandle {
    tx: Sender<Command>,
    shared: Arc<Shared>,
    epoch: Arc<AtomicU64>,
}

impl EngineHandle {
    pub fn send(&self, cmd: Command) {
        match &cmd {
            // Volume never waits behind the decode thread.
            Command::SetVolume(v) => {
                self.shared.set_volume(slider_to_gain(*v));
                return;
            }
            // Interrupt any blocking network work so the command is handled promptly.
            Command::Load(_) | Command::Stop | Command::Shutdown | Command::SetDevice(_) => {
                self.epoch.fetch_add(1, Ordering::AcqRel);
            }
            _ => {}
        }
        let _ = self.tx.send(cmd);
    }
}

pub struct Engine {
    handle: EngineHandle,
    events: Receiver<EngineEvent>,
    analyzer: Option<AnalyzerReader>,
    thread: Option<JoinHandle<()>>,
}

impl Engine {
    /// Starts the output `sink` and the decode thread.
    pub fn spawn(mut sink: Box<dyn AudioSink>) -> Result<Engine, EngineError> {
        let shared = Arc::new(Shared::new());
        let (producer, consumer) = rtrb::RingBuffer::new(RING_FRAMES * 2);
        let (renderer, analyzer) = Renderer::new(consumer, shared.clone());
        let device_rate = sink.start(Arc::new(Mutex::new(renderer)))?;
        let (tx, rx) = unbounded();
        let (events_tx, events) = unbounded();
        let epoch = Arc::new(AtomicU64::new(0));
        let worker = Worker::new(
            rx,
            events_tx,
            shared.clone(),
            epoch.clone(),
            producer,
            sink,
            device_rate,
        );
        let thread = thread::Builder::new()
            .name("winrust-decode".into())
            .spawn(move || worker.run())
            .map_err(|e| EngineError::Device(format!("cannot start decode thread: {e}")))?;
        Ok(Engine {
            handle: EngineHandle { tx, shared, epoch },
            events,
            analyzer: Some(analyzer),
            thread: Some(thread),
        })
    }

    pub fn handle(&self) -> EngineHandle {
        self.handle.clone()
    }

    pub fn events(&self) -> &Receiver<EngineEvent> {
        &self.events
    }

    /// The visualization tap. Can be taken once.
    pub fn take_analyzer(&mut self) -> Option<AnalyzerReader> {
        self.analyzer.take()
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.handle.send(Command::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Active {
    decoder: Decoder,
    resampler: StreamResampler,
    source_rate: u32,
    is_stream: bool,
    titles: Option<Receiver<String>>,
}

struct Worker {
    rx: Receiver<Command>,
    events: Sender<EngineEvent>,
    shared: Arc<Shared>,
    epoch: Arc<AtomicU64>,
    producer: rtrb::Producer<f32>,
    sink: Box<dyn AudioSink>,
    device_rate: u32,
    dsp: DspChain,
    eq: EqSettings,
    balance: f32,
    active: Option<Active>,
    queued: Option<Source>,
    last_source: Option<Source>,
    timeline: Timeline,
    /// Device frame index the next pushed frame will have.
    pushed: u64,
    /// Processed stereo samples not yet in the ring.
    pending: Vec<f32>,
    decoded: Vec<f32>,
    resampled: Vec<f32>,
    fill_target: usize,
    state: PlaybackState,
    last_position: Instant,
    last_bitrate: Option<u32>,
}

impl Worker {
    fn new(
        rx: Receiver<Command>,
        events: Sender<EngineEvent>,
        shared: Arc<Shared>,
        epoch: Arc<AtomicU64>,
        producer: rtrb::Producer<f32>,
        sink: Box<dyn AudioSink>,
        device_rate: u32,
    ) -> Self {
        Worker {
            rx,
            events,
            shared,
            epoch,
            producer,
            sink,
            device_rate,
            dsp: DspChain::new(device_rate),
            eq: EqSettings::default(),
            balance: 0.0,
            active: None,
            queued: None,
            last_source: None,
            timeline: Timeline::new(device_rate),
            pushed: 0,
            pending: Vec::new(),
            decoded: Vec::new(),
            resampled: Vec::new(),
            fill_target: 0,
            state: PlaybackState::Stopped,
            last_position: Instant::now(),
            last_bitrate: None,
        }
    }

    fn run(mut self) {
        'main: loop {
            let wait = if self.wants_work() {
                Duration::ZERO
            } else {
                IDLE_TICK
            };
            let mut batch = Vec::new();
            match self.rx.recv_timeout(wait) {
                Ok(cmd) => batch.push(cmd),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            batch.extend(self.rx.try_iter());
            for cmd in coalesce(batch) {
                if !self.handle(cmd) {
                    break 'main;
                }
            }
            self.pump();
            self.poll_sink();
            self.report();
        }
        self.stop_playback();
        self.sink.stop();
    }

    fn emit(&self, event: EngineEvent) {
        let _ = self.events.send(event);
    }

    fn frames(&self, d: Duration) -> usize {
        let f = (d.as_secs_f64() * f64::from(self.device_rate)) as usize;
        f.min(RING_FRAMES * 3 / 4)
    }

    fn ring_frames(&self) -> usize {
        (self.producer.buffer().capacity() - self.producer.slots()) / 2
    }

    fn wants_work(&self) -> bool {
        (self.active.is_some() && self.ring_frames() < self.fill_target)
            || (!self.pending.is_empty() && self.producer.slots() >= 2)
    }

    /// Returns false on shutdown.
    fn handle(&mut self, cmd: Command) -> bool {
        match cmd {
            Command::Load(source) => self.load(source),
            Command::QueueNext(source) => self.queue_next(source),
            Command::Play => {
                let was_paused = self.shared.paused.swap(false, Ordering::AcqRel);
                let stopped = self.active.is_none() && self.timeline.current().is_none();
                if !was_paused
                    && stopped
                    && let Some(source) = self.last_source.clone()
                {
                    self.load(source);
                }
            }
            Command::Pause => {
                if self.active.is_some() || self.timeline.current().is_some() {
                    self.shared.paused.store(true, Ordering::Release);
                }
            }
            Command::Stop => {
                self.stop_playback();
                self.shared.paused.store(false, Ordering::Release);
            }
            Command::Seek(to) => self.seek(to),
            Command::SetVolume(v) => self.shared.set_volume(slider_to_gain(v)),
            Command::SetBalance(b) => {
                self.balance = b;
                self.dsp.set_balance(b);
            }
            Command::SetEq(eq) => {
                self.eq = eq;
                self.dsp.set_eq(&eq);
            }
            Command::SetDevice(name) => {
                let resume = self.resume_point();
                self.stop_playback();
                match self.sink.set_device(name) {
                    Ok(rate) => self.apply_rate(rate),
                    Err(e) => self.emit(EngineEvent::Error(e)),
                }
                self.resume(resume);
            }
            Command::Shutdown => return false,
        }
        true
    }

    fn load(&mut self, source: Source) {
        self.stop_playback();
        self.shared.paused.store(false, Ordering::Release);
        self.last_source = Some(source.clone());
        self.queued = None;
        if matches!(source, Source::Url(_)) {
            // Connecting and probing can take seconds; tell the UI right away.
            self.set_state(PlaybackState::Buffering);
        }
        let epoch = self.epoch.load(Ordering::Acquire);
        match self.open(&source) {
            Ok(active) => {
                self.set_stream_mode(active.is_stream);
                self.timeline
                    .push_track(self.pushed, active.decoder.info().clone(), false);
                self.shared.producer_done.store(false, Ordering::Release);
                self.shared.buffering.store(true, Ordering::Release);
                self.active = Some(active);
            }
            // A newer Load/Stop interrupted this one: not an error worth showing.
            Err(_) if self.epoch.load(Ordering::Acquire) != epoch => {}
            Err(e) => self.emit(EngineEvent::Error(e)),
        }
    }

    fn set_state(&mut self, state: PlaybackState) {
        if state != self.state {
            self.state = state;
            self.emit(EngineEvent::State(state));
        }
    }

    fn queue_next(&mut self, source: Source) {
        if self.active.is_none() && self.timeline.current().is_some() {
            // Current track already fully decoded and draining: append right away.
            match self.open(&source) {
                Ok(active) => {
                    let start = self.pushed + (self.pending.len() / 2) as u64;
                    self.timeline
                        .push_track(start, active.decoder.info().clone(), true);
                    self.set_stream_mode(active.is_stream);
                    self.shared.producer_done.store(false, Ordering::Release);
                    self.last_source = Some(source);
                    self.active = Some(active);
                }
                Err(e) => self.emit(EngineEvent::Error(e)),
            }
        } else {
            self.queued = Some(source);
        }
    }

    fn open(&mut self, source: &Source) -> Result<Active, EngineError> {
        let (decoder, is_stream, titles) = match source {
            Source::File(path) => (Decoder::open_file(path)?, false, None),
            Source::Url(url) => {
                let s = stream::open(url, Cancel::new(self.epoch.clone()))?;
                let mut hint = Hint::new();
                if let Some(mime) = &s.content_type {
                    hint.mime_type(mime);
                    if let Some(ext) = extension_for_mime(mime) {
                        hint.with_extension(ext);
                    }
                }
                let mut decoder = Decoder::open_media(
                    source.clone(),
                    Box::new(ReadOnlySource::new(s.reader)),
                    &hint,
                )?;
                decoder.set_kbps(s.bitrate);
                (decoder, true, Some(s.titles))
            }
        };
        let source_rate = decoder.sample_rate();
        Ok(Active {
            resampler: StreamResampler::new(source_rate, self.device_rate),
            decoder,
            source_rate,
            is_stream,
            titles,
        })
    }

    fn set_stream_mode(&mut self, is_stream: bool) {
        let (start, fill) = if is_stream {
            (STREAM_START, STREAM_FILL)
        } else {
            (FILE_START, FILE_FILL)
        };
        self.shared
            .start_threshold
            .store(self.frames(start) as u64, Ordering::Release);
        self.shared
            .rebuffer_on_underrun
            .store(is_stream, Ordering::Release);
        self.fill_target = self.frames(fill);
    }

    fn seek(&mut self, to: Duration) {
        let Some(active) = self.active.as_ref() else {
            return;
        };
        let audible = self.timeline.current().map(|i| &i.source);
        if !active.decoder.info().seekable || audible != Some(&active.decoder.info().source) {
            return;
        }
        self.flush();
        let active = self.active.as_mut().expect("checked above");
        match active.decoder.seek(to) {
            Ok(actual) => {
                active.resampler.reset();
                self.dsp.reset();
                self.timeline.reposition(self.pushed, actual);
                self.shared.producer_done.store(false, Ordering::Release);
                self.emit(EngineEvent::Position(actual));
            }
            Err(e) => self.emit(EngineEvent::Error(e)),
        }
    }

    fn stop_playback(&mut self) {
        self.active = None;
        self.queued = None;
        self.flush();
        self.timeline.clear();
        self.dsp.reset();
        self.shared.producer_done.store(true, Ordering::Release);
        self.shared.buffering.store(false, Ordering::Release);
    }

    /// Empties the ring: the callback drains it and acknowledges.
    fn flush(&mut self) {
        self.pending.clear();
        let request = self.shared.flush_requested.fetch_add(1, Ordering::AcqRel) + 1;
        let deadline = Instant::now() + FLUSH_TIMEOUT;
        while self.shared.flush_done.load(Ordering::Acquire) < request && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(1));
        }
        self.pushed = self.shared.frames_played.load(Ordering::Acquire);
    }

    /// Moves pending samples into the ring. Returns true when nothing is left pending.
    fn push_pending(&mut self) -> bool {
        let n = self.producer.slots().min(self.pending.len()) & !1;
        if n > 0 && self.producer.push_entire_slice(&self.pending[..n]).is_ok() {
            self.pending.drain(..n);
            self.pushed += (n / 2) as u64;
        }
        self.pending.is_empty()
    }

    fn pump(&mut self) {
        for _ in 0..PACKETS_PER_TURN {
            if !self.push_pending() {
                return;
            }
            if self.ring_frames() >= self.fill_target {
                return;
            }
            let Some(active) = self.active.as_mut() else {
                return;
            };
            self.decoded.clear();
            match active.decoder.decode_next(&mut self.decoded) {
                Ok(true) => {
                    if active.decoder.sample_rate() != active.source_rate {
                        // Chained stream changed rate mid-way.
                        active.resampler.flush(&mut self.resampled);
                        active.source_rate = active.decoder.sample_rate();
                        active.resampler =
                            StreamResampler::new(active.source_rate, self.device_rate);
                    }
                    active.resampler.process(&self.decoded, &mut self.resampled);
                    self.commit_resampled();
                }
                Ok(false) => self.end_of_track(),
                Err(e) => {
                    self.emit(EngineEvent::Error(e));
                    self.end_of_track();
                }
            }
        }
    }

    fn commit_resampled(&mut self) {
        self.dsp.process(&mut self.resampled);
        self.pending.append(&mut self.resampled);
    }

    fn end_of_track(&mut self) {
        let Some(mut finished) = self.active.take() else {
            return;
        };
        if let Some(next) = self.queued.take() {
            match self.open(&next) {
                Ok(mut active) => {
                    if active.source_rate == finished.source_rate && !active.is_stream {
                        // Same rate: keep the resampler running so not a single sample is lost.
                        active.resampler = finished.resampler;
                    } else {
                        finished.resampler.flush(&mut self.resampled);
                        self.commit_resampled();
                    }
                    let start = self.pushed + (self.pending.len() / 2) as u64;
                    self.timeline
                        .push_track(start, active.decoder.info().clone(), true);
                    self.set_stream_mode(active.is_stream);
                    self.last_source = Some(next);
                    self.active = Some(active);
                    return;
                }
                Err(e) => self.emit(EngineEvent::Error(e)),
            }
        }
        finished.resampler.flush(&mut self.resampled);
        self.commit_resampled();
        self.timeline
            .set_end(self.pushed + (self.pending.len() / 2) as u64);
        self.shared.producer_done.store(true, Ordering::Release);
    }

    fn apply_rate(&mut self, rate: u32) {
        self.device_rate = rate;
        self.dsp = DspChain::new(rate);
        self.dsp.set_eq(&self.eq);
        self.dsp.set_balance(self.balance);
        self.timeline = Timeline::new(rate);
    }

    fn resume_point(&self) -> Option<(Source, Duration)> {
        let info = self.timeline.current()?;
        let played = self.shared.frames_played.load(Ordering::Acquire);
        Some((
            info.source.clone(),
            self.timeline.position(played).unwrap_or_default(),
        ))
    }

    fn resume(&mut self, point: Option<(Source, Duration)>) {
        if let Some((source, position)) = point {
            self.load(source);
            if position > Duration::ZERO {
                // The track must be audible before a seek is allowed; prime the timeline.
                self.timeline.poll(self.pushed);
                self.seek(position);
            }
        }
    }

    fn poll_sink(&mut self) {
        while let Some(event) = self.sink.poll() {
            match event {
                SinkEvent::RateChanged(rate) => {
                    let resume = self.resume_point();
                    self.stop_playback();
                    self.apply_rate(rate);
                    self.resume(resume);
                }
                SinkEvent::Error(msg) => self.emit(EngineEvent::Error(EngineError::Device(msg))),
            }
        }
    }

    fn report(&mut self) {
        let played = self.shared.frames_played.load(Ordering::Acquire);
        for event in self.timeline.poll(played) {
            match event {
                TimelineEvent::Started { info, gapless } => {
                    self.last_bitrate = None;
                    self.emit(EngineEvent::TrackStarted { info, gapless });
                }
                TimelineEvent::Ended => self.emit(EngineEvent::TrackEnded),
            }
        }
        if let Some(active) = self.active.as_mut() {
            let mut titles: Vec<String> = active
                .titles
                .as_ref()
                .map(|t| t.try_iter().collect())
                .unwrap_or_default();
            titles.extend(active.decoder.take_title().filter(|_| active.is_stream));
            let bitrate = active.decoder.take_bitrate();
            for title in titles {
                self.emit(EngineEvent::StreamTitle(title));
            }
            if let Some(kbps) = bitrate.filter(|k| Some(*k) != self.last_bitrate) {
                self.last_bitrate = Some(kbps);
                self.emit(EngineEvent::Bitrate(kbps));
            }
        }
        let state = if self.active.is_none() && self.timeline.current().is_none() {
            PlaybackState::Stopped
        } else if self.shared.paused.load(Ordering::Acquire) {
            PlaybackState::Paused
        } else if self.shared.buffering.load(Ordering::Acquire) {
            PlaybackState::Buffering
        } else {
            PlaybackState::Playing
        };
        self.set_state(state);
        if state == PlaybackState::Playing && self.last_position.elapsed() >= POSITION_EVERY {
            self.last_position = Instant::now();
            if let Some(pos) = self.timeline.position(played) {
                self.emit(EngineEvent::Position(pos));
            }
        }
    }
}

/// Drops commands made pointless by a later one in the same batch: anything before the
/// last Load/Stop/Shutdown, except settings, which always apply.
fn coalesce(batch: Vec<Command>) -> Vec<Command> {
    let cut = batch
        .iter()
        .rposition(|c| matches!(c, Command::Load(_) | Command::Stop | Command::Shutdown))
        .unwrap_or(0);
    batch
        .into_iter()
        .enumerate()
        .filter(|(i, c)| {
            *i >= cut
                || matches!(
                    c,
                    Command::SetEq(_)
                        | Command::SetBalance(_)
                        | Command::SetVolume(_)
                        | Command::SetDevice(_)
                )
        })
        .map(|(_, c)| c)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesce_keeps_only_latest_load() {
        let a = Command::Load(Source::Url("a".into()));
        let b = Command::Load(Source::Url("b".into()));
        let out = coalesce(vec![
            a,
            Command::Seek(Duration::ZERO),
            Command::SetBalance(0.5),
            b.clone(),
            Command::Play,
        ]);
        assert_eq!(out, vec![Command::SetBalance(0.5), b, Command::Play]);
    }

    #[test]
    fn coalesce_stop_wins_over_earlier_load() {
        let out = coalesce(vec![Command::Load(Source::Url("a".into())), Command::Stop]);
        assert_eq!(out, vec![Command::Stop]);
    }

    #[test]
    fn coalesce_leaves_plain_batches_alone() {
        let batch = vec![Command::Pause, Command::Play];
        assert_eq!(coalesce(batch.clone()), batch);
    }
}
```

`crates/audio/src/lib.rs` (final):

```rust
//! Winrust audio engine: decoding, streaming, DSP and output.

pub mod decoder;
pub mod dsp;
pub mod engine;
pub mod eq;
pub mod http;
pub mod icy;
pub mod renderer;
pub mod resample;
pub mod sink;
pub mod stream;
pub mod timeline;
pub mod types;
pub mod volume;

pub use engine::{Engine, EngineHandle};
pub use renderer::{AnalyzerReader, SCOPE_FRAMES, Scope};
pub use sink::{AudioSink, CaptureSink, CpalSink, output_devices};
pub use types::*;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p winrust-audio`
Expected: every test target passes — unit tests `test result: ok. 55 passed` (including the three new `engine::tests::coalesce_*`), `decoder_wav` 4, `engine` 10, `engine_edges` 5, `formats` 2, `sink` 2.

Run the timing-sensitive targets several times to catch flakiness:
```bash
for i in 1 2 3 4 5; do cargo test -q -p winrust-audio --test engine --test engine_edges 2>&1 | grep "test result"; done
```
Expected: every line `ok`. During planning, 6 consecutive runs passed.

If `mp3_halves_play_back_gaplessly` fails, see Provenance: it has not run yet. The frame-count assertion tells whether the encoder delay and padding were trimmed (≈ 88 200 frames). The step assertion tells whether the join clicks. Debug with superpowers:systematic-debugging, starting with `Track::delay`/`padding` from symphonia for `half1.mp3`.

- [ ] **Step 5: Gates and commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/audio
git commit -m "feat(audio): add playback engine with gapless queue, seek and internet radio" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 14: `winrust-cli` headless player

**Files:**
- Create: `crates/cli/Cargo.toml`
- Create: `crates/cli/src/queue.rs`
- Create: `crates/cli/src/main.rs`

**Interfaces:**
- Consumes: `Engine`, `EngineHandle`, `CpalSink`, `Command`, `EngineEvent`, `EqSettings`, `Source`, `output_devices` (Task 13).
- Produces: binary `winrust-cli` with subcommands `devices` and `play <sources>... [--volume 0..=100] [--balance F] [--eq b1,...,b10] [--preamp DB] [--start SECS] [--device NAME]`, and stdin line commands `p s n f b + - q`. Internal `queue::Queue` with `Next { Load(usize), Enqueue(usize), Quit, Nothing }`. The skip-on-error and gapless queueing logic matches what M4's playlist will need.

- [ ] **Step 1: Write the failing tests**

`crates/cli/Cargo.toml`:

```toml
[package]
name = "winrust-cli"
description = "Headless Winrust player for exercising the audio engine"
version.workspace = true
edition.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
clap = { version = "4.6.7", features = ["derive"] }
crossbeam-channel = "0.5.17"
winrust-audio = { path = "../audio" }
```

`crates/cli/src/queue.rs` — tests only for now:

```rust
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
        assert_eq!(q.on_error(), Next::Load(1));
        assert_eq!(q.on_started(false), Next::Nothing);
    }

    #[test]
    fn unreadable_queued_track_is_skipped_at_end() {
        let mut q = Queue::new(3);
        q.start();
        assert_eq!(q.on_started(false), Next::Enqueue(1));
        assert_eq!(
            q.on_error(),
            Next::Nothing,
            "error belongs to the queued track"
        );
        assert_eq!(q.on_ended(), Next::Load(2));
    }

    #[test]
    fn skip_moves_on_and_quits_past_the_end() {
        let mut q = Queue::new(2);
        q.start();
        assert_eq!(q.skip(), Next::Load(1));
        assert_eq!(q.skip(), Next::Quit);
    }
}
```

`crates/cli/src/main.rs` — temporary, just enough to compile the queue tests:

```rust
mod queue;

fn main() {}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p winrust-cli`
Expected: compile errors (`Queue`, `Next` not found).

- [ ] **Step 3: Implement the queue above its tests module**

```rust
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
    awaiting_start: bool,
}

impl Queue {
    pub fn new(len: usize) -> Self {
        Queue {
            len,
            current: None,
            queued: None,
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

    /// The engine reported an error.
    pub fn on_error(&mut self) -> Next {
        match (self.awaiting_start, self.current) {
            (true, Some(i)) => self.load(i + 1),
            _ => Next::Nothing,
        }
    }

    /// Playback stopped at the end of the last queued track.
    pub fn on_ended(&mut self) -> Next {
        match self.queued.take() {
            // The queued track never started, so it failed to open: skip past it.
            Some(q) => self.load(q + 1),
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
```

Run: `cargo test -p winrust-cli queue::`
Expected: `test result: ok. 5 passed`.

- [ ] **Step 4: Write the real `main.rs` (its tests are at the bottom)**

`crates/cli/src/main.rs`:

```rust
//! `winrust-cli`: headless player for exercising the audio engine from a terminal.

mod queue;

use clap::{Args, Parser, Subcommand};
use crossbeam_channel::{select, unbounded};
use queue::{Next, Queue};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;
use winrust_audio::{Command, CpalSink, Engine, EngineEvent, EqSettings, Source, output_devices};

#[derive(Parser)]
#[command(
    name = "winrust-cli",
    version,
    about = "Headless Winrust player (audio engine test tool)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List audio output devices.
    Devices,
    /// Play files and stream URLs in order, gaplessly.
    Play(PlayArgs),
}

#[derive(Args)]
struct PlayArgs {
    /// Files or http(s):// stream URLs.
    #[arg(required = true)]
    sources: Vec<String>,
    /// Volume slider, 0..=100.
    #[arg(long, default_value_t = 100, value_parser = clap::value_parser!(u8).range(0..=100))]
    volume: u8,
    /// Balance, -1.0 (left) ..= 1.0 (right).
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    balance: f32,
    /// Ten comma-separated band gains in dB (60 Hz .. 16 kHz). Enables the equalizer.
    #[arg(long, value_delimiter = ',', allow_hyphen_values = true)]
    eq: Option<Vec<f32>>,
    /// Equalizer preamp in dB.
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    preamp: f32,
    /// Start the first track at this many seconds.
    #[arg(long)]
    start: Option<f64>,
    /// Output device name (see `devices`).
    #[arg(long)]
    device: Option<String>,
}

fn main() -> ExitCode {
    match Cli::parse().cmd {
        Cmd::Devices => {
            for name in output_devices() {
                println!("{name}");
            }
            ExitCode::SUCCESS
        }
        Cmd::Play(args) => match play(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(msg) => {
                eprintln!("error: {msg}");
                ExitCode::FAILURE
            }
        },
    }
}

fn parse_source(s: &str) -> Source {
    let lower = s.to_ascii_lowercase();
    if ["http://", "https://", "icy://"]
        .iter()
        .any(|p| lower.starts_with(p))
    {
        Source::Url(s.to_string())
    } else {
        Source::File(PathBuf::from(s))
    }
}

fn eq_settings(args: &PlayArgs) -> Result<EqSettings, String> {
    let Some(bands) = &args.eq else {
        return Ok(EqSettings::default());
    };
    let bands_db: [f32; 10] = bands
        .as_slice()
        .try_into()
        .map_err(|_| format!("--eq needs 10 values, got {}", bands.len()))?;
    Ok(EqSettings {
        enabled: true,
        preamp_db: args.preamp,
        bands_db,
    })
}

fn fmt_time(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}

fn play(args: PlayArgs) -> Result<(), String> {
    let sources: Vec<Source> = args.sources.iter().map(|s| parse_source(s)).collect();
    let eq = eq_settings(&args)?;
    let engine =
        Engine::spawn(Box::new(CpalSink::new(args.device.clone()))).map_err(|e| e.to_string())?;
    let h = engine.handle();
    let mut volume = args.volume;
    h.send(Command::SetVolume(volume));
    h.send(Command::SetBalance(args.balance));
    h.send(Command::SetEq(eq));

    let (lines_tx, lines) = unbounded();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if lines_tx.send(line).is_err() {
                break;
            }
        }
    });
    println!("keys + Enter: p=pause/resume  s=stop  n=next  f/b=seek ±5s  +/-=volume  q=quit");

    let mut queue = Queue::new(sources.len());
    let mut position = Duration::ZERO;
    let mut last_print = Duration::MAX;
    let mut paused = false;
    let mut pending_start = args.start.map(Duration::from_secs_f64);
    let act = |next: Next| -> bool {
        match next {
            Next::Load(i) => h.send(Command::Load(sources[i].clone())),
            Next::Enqueue(i) => h.send(Command::QueueNext(sources[i].clone())),
            Next::Quit => return false,
            Next::Nothing => {}
        }
        true
    };
    if !act(queue.start()) {
        return Ok(());
    }

    loop {
        select! {
            recv(engine.events()) -> ev => {
                let Ok(ev) = ev else { return Ok(()) };
                let next = match ev {
                    EngineEvent::TrackStarted { info, gapless } => {
                        let khz = f64::from(info.sample_rate) / 1000.0;
                        let kbps = info.kbps.map(|k| format!("{k} kbps, ")).unwrap_or_default();
                        let len = info.duration.map(fmt_time).unwrap_or_else(|| "live".into());
                        println!("\n▶ {} [{kbps}{khz} kHz, {} ch, {len}]", info.source, info.channels);
                        if let Some(at) = pending_start.take() {
                            h.send(Command::Seek(at));
                        }
                        queue.on_started(gapless)
                    }
                    EngineEvent::Position(p) => {
                        position = p;
                        if p.as_secs() != last_print.as_secs() {
                            last_print = p;
                            print!("\r  {}   ", fmt_time(p));
                            let _ = std::io::stdout().flush();
                        }
                        Next::Nothing
                    }
                    EngineEvent::StreamTitle(t) => {
                        println!("\n  ♪ {t}");
                        Next::Nothing
                    }
                    EngineEvent::Bitrate(_) => Next::Nothing,
                    EngineEvent::State(s) => {
                        println!("\n  [{s:?}]");
                        Next::Nothing
                    }
                    EngineEvent::Error(e) => {
                        println!("\n  ✖ {e}");
                        queue.on_error()
                    }
                    EngineEvent::TrackEnded => queue.on_ended(),
                };
                if !act(next) {
                    return Ok(());
                }
            }
            recv(lines) -> line => {
                let Ok(line) = line else { return Ok(()) };
                match line.trim() {
                    "p" => {
                        paused = !paused;
                        h.send(if paused { Command::Pause } else { Command::Play });
                    }
                    "s" => h.send(Command::Stop),
                    "n" => {
                        if !act(queue.skip()) {
                            return Ok(());
                        }
                    }
                    "f" => h.send(Command::Seek(position + Duration::from_secs(5))),
                    "b" => h.send(Command::Seek(position.saturating_sub(Duration::from_secs(5)))),
                    "+" => {
                        volume = (volume + 5).min(100);
                        h.send(Command::SetVolume(volume));
                    }
                    "-" => {
                        volume = volume.saturating_sub(5);
                        h.send(Command::SetVolume(volume));
                    }
                    "q" => return Ok(()),
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_and_files_are_told_apart() {
        assert_eq!(
            parse_source("HTTPS://radio.example/live"),
            Source::Url("HTTPS://radio.example/live".into())
        );
        assert_eq!(
            parse_source("C:\\music\\a.mp3"),
            Source::File(PathBuf::from("C:\\music\\a.mp3"))
        );
    }

    #[test]
    fn eq_needs_ten_bands() {
        let cli = Cli::parse_from(["winrust-cli", "play", "a.mp3", "--eq", "1,2,3"]);
        let Cmd::Play(args) = cli.cmd else { panic!() };
        assert!(eq_settings(&args).unwrap_err().contains("10 values"));
    }

    #[test]
    fn negative_values_parse() {
        let cli = Cli::parse_from([
            "winrust-cli",
            "play",
            "a.mp3",
            "--balance",
            "-0.5",
            "--preamp",
            "-3",
            "--eq",
            "-1,0,0,0,0,0,0,0,0,12",
        ]);
        let Cmd::Play(args) = cli.cmd else { panic!() };
        assert_eq!(args.balance, -0.5);
        let eq = eq_settings(&args).unwrap();
        assert!(eq.enabled);
        assert_eq!(eq.preamp_db, -3.0);
        assert_eq!(eq.bands_db[0], -1.0);
        assert_eq!(eq.bands_db[9], 12.0);
    }

    #[test]
    fn volume_is_range_checked() {
        assert!(Cli::try_parse_from(["winrust-cli", "play", "a.mp3", "--volume", "101"]).is_err());
    }
}
```

- [ ] **Step 5: Run all CLI tests and smoke-test the binary**

Run: `cargo test -p winrust-cli`
Expected: `test result: ok. 9 passed`.

Run: `cargo run -q -p winrust-cli -- --help`
Expected: usage listing the `devices` and `play` commands.

Run: `cargo run -q -p winrust-cli -- devices`
Expected: one line per output device. On the planning machine that was `Digital Audio (HDMI) (High Definition Audio Device)`, `Speakers (High Definition Audio Device)` and `Digital Audio (S/PDIF) (High Definition Audio Device)`.

- [ ] **Step 6: Gates and commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo deny check
git add crates/cli Cargo.lock
git commit -m "feat(cli): add winrust-cli headless player for the audio engine" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 15: README for M1

**Files:**
- Modify: `README.md`

**Interfaces:**
- Consumes: CLI from Task 14.
- Produces: user-facing docs for building on Linux (ALSA), regenerating fixtures, and trying the engine.

- [ ] **Step 1: Replace `README.md`**

````markdown
# Winrust

A cross-platform clone of the classic Winamp 2.x music player, written in Rust.

> **Status:** early development (milestone M1). The audio engine works and can be driven from
> the terminal with `winrust-cli`. The skinned player window is not built yet.

Winrust aims to reproduce Winamp 2.x's main window, equalizer and playlist editor, and to
render classic `.wsz` skins pixel for pixel on Windows, macOS and Linux. Scope and roadmap:
[design document](docs/superpowers/specs/2026-10-10-winrust-design.md).

## Building

Needs stable Rust via [rustup](https://rustup.rs). On Linux, also the ALSA headers
(Debian/Ubuntu: `sudo apt install libasound2-dev pkg-config`).

```sh
cargo build --workspace
cargo test --workspace
```

Developer tasks: `cargo xtask help`. Regenerating the audio test fixtures
(`cargo xtask gen-fixtures`) needs `ffmpeg` on `PATH`; the generated files are committed, so
normal builds and tests do not.

## Trying the audio engine

```sh
cargo run -p winrust-cli -- devices
cargo run -p winrust-cli -- play song.mp3 other.flac
cargo run -p winrust-cli -- play https://example.com/stream --volume 60
cargo run -p winrust-cli -- play song.mp3 --eq 6,4,0,0,0,0,0,2,4,6 --preamp -3
```

While playing, type a key and press Enter: `p` pause/resume, `s` stop, `n` next,
`f`/`b` seek ±5 s, `+`/`-` volume, `q` quit.

Supported: MP3, FLAC, Ogg Vorbis, WAV, AAC (ADTS and MP4/M4A, AAC-LC), ALAC, and
SHOUTcast/Icecast streams over HTTP or HTTPS with stream titles. HE-AAC (AAC+) streams play
their AAC-LC core only, at reduced quality.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion
in Winrust by you, as defined in the Apache-2.0 license, shall be dual licensed as above,
without any additional terms or conditions.

## Trademarks

Winrust is an independent project. It is not affiliated with or endorsed by Winamp, Nullsoft
or Llama Group. "Winamp" is a trademark of its owner and is used here only to describe
compatibility with classic Winamp skins. Winrust ships no Winamp artwork.
````

- [ ] **Step 2: Check the commands in the README actually work**

Run: `cargo run -q -p winrust-cli -- play --help`
Expected: help showing `--volume`, `--balance`, `--eq`, `--preamp`, `--start`, `--device`.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: document winrust-cli and M1 build requirements" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 16: Listening check on real hardware, CI, done

Automated tests cannot hear. This task needs the user, at the machine, with speakers or headphones.

**Files:** none (fix anything found, with a regression test where feasible).

- [ ] **Step 1: Push and confirm CI is green on all three OSes**

```bash
git push
gh run watch --exit-status $(gh run list --limit 1 --json databaseId --jq '.[0].databaseId')
```
Expected: `fmt`, `test (windows-latest)`, `test (macos-latest)`, `test (ubuntu-latest)`, `deny` all green.

- [ ] **Step 2: Ask the user to run the listening checklist**

Give the user this checklist and wait for their results. Build first with `cargo build --release -p winrust-cli`, so the binary is `target/release/winrust-cli`.

1. **Local files:** `winrust-cli play <an mp3> <a flac>`. Expected: clean audio, correct kHz and kbps printed, gapless transition, `TrackEnded` → exits.
2. **Internet radio:** `winrust-cli play https://ice1.somafm.com/groovesalad-128-mp3` (or any station the user likes). Expected: `[Buffering]` → `[Playing]` within a few seconds, `♪ <title>` lines when songs change.
3. **EQ audible:** `winrust-cli play <file> --eq 12,12,0,0,0,0,0,0,0,0 --preamp -6`. Expected: clearly more bass, no distortion pops.
4. **Volume and balance:** type `-` a few times (quieter, smooth, no zipper noise); restart with `--balance -1` (left channel only).
5. **Seek and pause:** `f`, `b`, `p`, `p`. Expected: instant response, no stale audio after a seek.
6. **Device change:** while playing, unplug headphones or switch the Windows default output. Expected: an `✖ audio device error: audio device lost, reopening` line, then audio continues on the new device within about 1 s, at the right pitch.

- [ ] **Step 3: Fix what the user reports**

For each problem, use superpowers:systematic-debugging, add a regression test where the behavior is testable without a sound card, fix, commit and push. M1 is done when the checklist passes and CI is green.
