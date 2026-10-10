# Winrust — Umbrella Design

- **Date:** 2026-10-10
- **Status:** Approved in brainstorming; awaiting written-spec review
- **Scope:** Whole-product architecture and milestone roadmap. Each milestone (M0–M8) gets its own detailed spec → implementation plan → build cycle. Details explicitly deferred to a milestone spec are listed in [§13](#13-decisions-deferred-to-milestone-specs).

---

## 1. Summary

Winrust is an open-source, cross-platform (Windows, macOS, Linux) clone of the classic **Winamp 2.x** music player, written in Rust. It reproduces Winamp 2.x's three-window interface (main window, equalizer, playlist editor), its behavior, and its classic skin format (`.wsz`) so that any classic skin renders pixel-identically to Winamp 2.9x / Webamp.

## 2. Goals

1. Pixel-identical rendering of classic `.wsz` skins at 1× and 2× (double-size), using Webamp as the reference implementation for skin-parsing quirks.
2. Behavioral parity with Winamp 2.x for everything in scope: transport, LCD display, sliders, shade modes, double-size, window snapping/docking, hotkeys, equalizer, playlist operations, visualizations.
3. Playback of MP3, FLAC, Ogg Vorbis, WAV, AAC/M4A (AAC-LC), ALAC, and SHOUTcast/Icecast HTTP(S) streams with ICY metadata.
4. Gapless playback between local tracks.
5. Runs on Windows, macOS, and Linux from one codebase; CI builds and release binaries for all three.
6. Daily-drivable: persistent settings, autosaved playlist, single-instance behavior, OS media keys.

### Success criteria

- Randomly chosen skins from skins.webamp.org render the same as Webamp (side-by-side manual check, golden tests for a licensed subset).
- A real music library can be played daily without crashes or audio glitches.
- CI is green on all three OSes; tagged releases publish binaries for all three.

## 3. Non-goals (v1)

- Winamp plugin DLL compatibility (`in_*`, `out_*`, `dsp_*`, `vis_*`, `gen_*`).
- Media Library window, Winamp 3/5 "modern" skins, minibrowser, video.
- MOD/XM/S3M/IT, MIDI, Audio CD playback.
- Crossfade.
- Milkdrop / AVS visualizations.
- OS file-type association registration (may be added by the installer later).
- Native Wayland support (Linux runs via X11/XWayland; see §6.6).
- Code signing / notarization (v1 binaries are unsigned).

## 4. Legal and licensing

- **License:** `MIT OR Apache-2.0`.
- **Name/branding:** the product is "Winrust". The word "Winamp" appears only descriptively (e.g. "plays Winamp-compatible classic skins"). No Winamp logos.
- **No Nullsoft assets in the repository.** Winrust ships an original default skin, "Winrust Base" (§7.6). Users may load `base-2.91.wsz` or any other skin they own.
- **Sprite coordinate table:** coordinates are factual layout data; Webamp's MIT-licensed sprite table is used as a cross-reference and credited in the About dialog and `THIRD_PARTY.md`.
- **Bundled font:** Arimo (Apache-2.0), an Arial-metric-compatible font, used when the skin's named playlist font is not installed.
- **Dependency licenses** enforced by `cargo-deny`. Allowlist: MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, Unicode-3.0, MPL-2.0 (symphonia is MPL-2.0; file-level copyleft, compatible with our distribution). GPL/LGPL dependencies are not allowed.

## 5. Architecture

### 5.1 Workspace layout

```
winrust/
├─ Cargo.toml              workspace
├─ crates/
│  ├─ audio/               engine: decode, streams, DSP, output, analyzer (no UI deps)
│  ├─ skin/                .wsz/zip/dir loading, BMP decode, sprite sheets, txt configs, cursors (pure)
│  ├─ ui/                  layout rects, hit-testing, widget state, compositor, docking math (pure, no winit)
│  ├─ playlist/            playlist model, M3U/M3U8/PLS, shuffle/repeat order, sorting, selection (pure)
│  └─ app/                 binary `winrust`: winit event loop, windows, menus, egui dialogs, config, glue
├─ xtask/                  dev tasks: default-skin generator, fixture generation, release helpers
├─ assets/
│  ├─ skin/                generated "Winrust Base" skin (BMP + txt files)
│  └─ fonts/               Arimo
├─ fuzz/                   cargo-fuzz targets
└─ docs/
```

Rule: `audio`, `skin`, `ui`, and `playlist` have no dependency on winit, egui, or any OS windowing API. Everything that can be tested headless lives in them.

### 5.2 Threads

```
main thread ── winit event loop, all windows, compositor, menus, dialogs
   │ Command (crossbeam channel)        ▲ EngineEvent via EventLoopProxy (wakes loop)
   ▼                                    │
decode thread ── symphonia → channel map → resample → preamp → EQ → balance → SPSC ring (rtrb)
   ▲ bytes                                                         │
net thread (HTTP stream, ICY strip)                                ▼
                                             cpal callback ── volume → device
                                                      └─ copy → analyzer triple-buffer → viz reads @60 Hz
tag worker ── lofty reads for playlist titles/durations (visible rows first)
skin loader ── parses a skin off the main thread, hands back Arc<Skin>
```

The winit event loop runs on the main thread (required on macOS). All rendering happens on the main thread; the compositor is cheap at Winamp's pixel counts.

### 5.3 State model

Elm-style, single owner:

- `App` owns one `Model`: playback status, playlist, EQ state, config, current `Arc<Skin>`, window layout (positions, visibility, shade, double-size), transient widget state (pressed button, active slider drag).
- Every input (mouse, key, menu, engine event, IPC message) becomes an `Action`.
- `Model::apply(Action) -> Effects` mutates state and returns effects (engine commands, redraw of specific windows, config save, window moves). The app executes effects.
- Windows are views over the `Model`; they hold no state of their own beyond OS handles.

`Model::apply` is pure and unit-tested without any window.

Sketch (final shapes defined in milestone specs):

```rust
pub enum Action {
    Transport(Transport),            // Prev, Play, Pause, Stop, StopFade, Next, Eject
    SetVolume(u8),                   // 0..=100
    SetBalance(i8),                  // -100..=100
    Seek(SeekTarget),
    ToggleShuffle, ToggleRepeat, ToggleTimeMode,
    ToggleWindow(WindowId), SetShade(WindowId, bool), ToggleDoubleSize, ToggleAlwaysOnTop,
    Playlist(PlaylistAction),
    Eq(EqAction),
    LoadSkin(SkinSource),
    Engine(EngineEvent),
    Ipc(IpcMessage),
    // ...
}

pub enum WindowId { Main, Equalizer, Playlist }
```

### 5.4 Redraw policy

- Idle (stopped/paused, no animation): fully event-driven, zero CPU.
- Playing: a 60 Hz timer (`ControlFlow::WaitUntil`) redraws only windows showing animated content (viz, scrolling title, time). Static windows redraw only when their view data changes.

## 6. Audio engine (`crates/audio`)

### 6.1 Public API

```rust
pub enum Source { File(PathBuf), Url(String) }

pub enum Command {
    Load(Source),          // replace current, start decoding (paused or playing per flag)
    QueueNext(Source),     // gapless: decode thread switches to it at end of current
    Play, Pause, Stop,
    Seek(Duration),
    SetVolume(f32),        // linear gain after volume curve, applied in callback
    SetBalance(f32),       // -1.0..=1.0
    SetEq(EqSettings),     // enabled, preamp dB, 10 band dB values
    SetDevice(Option<String>),
}

pub enum EngineEvent {
    State(PlaybackState),                         // Playing, Paused, Stopped, Buffering
    Position(Duration),                           // ~10 Hz, from samples actually played
    TrackInfo { kbps: Option<u32>, khz: u32, channels: u16, duration: Option<Duration>, seekable: bool },
    StreamTitle(String),
    TrackEnded,
    Error(EngineError),
}

pub fn spawn(sink: Box<dyn AudioSink>) -> (EngineHandle, Receiver<EngineEvent>);
```

`AudioSink` abstracts the output device. Production uses a cpal sink; tests use a null/capture sink that pulls samples without a device.

### 6.2 Pipeline

```
decoder (f32) → channel map → resample → preamp → 10-band EQ → balance → SPSC ring → [callback: volume → device]
                                                                                       └→ analyzer tap
```

- **Decoding:** symphonia probe. Formats: MP3, FLAC, Ogg Vorbis, WAV/PCM, AAC-LC (ADTS and MP4), ALAC.
- **Channel map:** mono duplicated to stereo; more than two channels downmixed to stereo. The LCD shows the *source* channel count.
- **Resampling:** the output stream is opened once at the device's default rate and kept open across tracks (required for gapless). Tracks at other rates are resampled with rubato. The LCD shows the *source* kHz.
- **Preamp + EQ:** Winamp's 10 bands — 60, 170, 310, 600 Hz, 1, 3, 6, 12, 14, 16 kHz — range ±12 dB, RBJ peaking biquads, plus preamp gain. New coefficients take effect with a short gain ramp so slider moves don't click. Disabled EQ bypasses the filters.
- **Balance:** applied in the decode thread.
- **Volume:** applied in the cpal callback so slider changes are heard immediately.
- **Clipping:** samples clamped to [-1, 1] before output.

### 6.3 Gapless

The app sends `QueueNext` as soon as the next track is known. When the current decoder hits end of stream, the decode thread continues with the queued decoder into the same ring buffer with no inserted silence. symphonia's gapless mode trims encoder delay and padding (LAME/iTunes info). Gapless applies to local files only.

### 6.4 Seeking and position

- Seeking uses symphonia's seek and flushes the ring buffer.
- Streams are non-seekable; `TrackInfo.seekable = false` disables the seek bar (Winamp behavior).
- Position is derived from an atomic count of samples actually written to the device by the callback, so the time display is correct regardless of buffering.

### 6.5 Analyzer tap

The callback copies the most recent 576 samples per channel (Winamp's visualization window) into a lock-free triple buffer. The UI thread reads it at frame time and computes FFT (rustfft) for the spectrum or uses raw samples for the oscilloscope. No allocation and no locks in the callback.

### 6.6 Streams

- HTTP client: ureq with rustls (HTTPS supported).
- Request header `Icy-MetaData: 1`; read `icy-metaint`; strip metadata blocks from the byte stream; parse `StreamTitle='...'` → `EngineEvent::StreamTitle`.
- Format hint from `Content-Type` (`audio/mpeg`, `audio/aac`, `audio/aacp`, `audio/ogg`).
- Prebuffer ~2 seconds before starting playback (`State(Buffering)` meanwhile).
- `.pls`/`.m3u` URLs are resolved by the playlist crate before reaching the engine.
- Disconnect → `Error` event. No auto-reconnect in v1.
- **Known limitation:** symphonia decodes AAC-LC only. HE-AAC (AAC+) streams, common on SHOUTcast, play their LC core at reduced quality. The only full HE-AAC decoders (FDK, FAAD2) are license-incompatible; revisit post-v1.

### 6.7 Error handling

- Recoverable decode error on a packet → skip the packet, continue.
- Unsupported or unreadable file → `Error` event; the app advances to the next playlist entry (Winamp behavior).
- Output device lost (e.g. headphones unplugged) → reopen the default device and resume.
- The callback never panics; ring underrun outputs silence.

### 6.8 Output device

Default device unless Preferences selects one (stored by name; falls back to default if the named device is missing).

## 7. Skin engine (`crates/skin`) and compositor (`crates/ui`)

### 7.1 Loading

- Sources: `.wsz`, `.zip`, or a directory.
- Parsing runs on a worker thread; result is an `Arc<Skin>` swapped into the `Model`, followed by a full redraw (hot-swap, no restart).

### 7.2 Files

Used: `main.bmp`, `cbuttons.bmp`, `titlebar.bmp`, `numbers.bmp` / `nums_ex.bmp`, `text.bmp`, `posbar.bmp`, `volume.bmp`, `balance.bmp`, `monoster.bmp`, `playpaus.bmp`, `shufrep.bmp`, `eqmain.bmp`, `eq_ex.bmp`, `pledit.bmp`, `pledit.txt`, `viscolor.txt`, `region.txt`, `*.cur`.

Ignored in v1: `gen.bmp`, `genex.bmp`, `video.bmp`, `mb.bmp`.

### 7.3 Quirk handling

- File names matched case-insensitively; files may sit in a subfolder inside the archive.
- BMP variants: 1/4/8/24/32-bit, RLE4/RLE8, malformed headers. Own tolerant decoder, falling back to the `image` crate.
- Missing sheet or sprite → use the corresponding Winrust Base sprite (Winamp falls back to its base skin the same way).
- `nums_ex.bmp` preferred over `numbers.bmp` (it has the minus sign). Missing `balance.bmp` → derived from `volume.bmp`.
- `pledit.txt`: lenient INI parsing (odd whitespace, missing `#`, extra sections).
- `viscolor.txt`: 24 RGB lines with trailing comments; missing lines use defaults.
- `region.txt`: sections `Normal`, `WindowShade`, `Equalizer`, `EqualizerWS` with `NumPoints` / `PointList`.
- EQ graph line colors are read from the dedicated pixel row in `eqmain.bmp`.

### 7.4 Sprite map

A const table of named rectangles per sheet (e.g. `MAIN_PLAY_BUTTON_PRESSED`). Coordinates follow Winamp 2.x; Webamp's table is the cross-reference.

### 7.5 Compositor

- Pure functions: `draw_main(&Skin, &MainView, &mut Canvas)`, `draw_eq(...)`, `draw_playlist(...)`, plus shade variants.
- `Canvas` is an RGBA `u32` buffer at native 1× size.
- Sprites are blitted opaque.
- LCD scrolling text and kbps/kHz use `text.bmp` glyphs; time uses `nums_ex`/`numbers`.
- Playlist text uses a real font: the skin's named font (usually Arial) via fontdb if installed, else bundled Arimo; rasterized with fontdue. Colors from `pledit.txt`. Golden tests always use Arimo.
- **Scaling:** compose at 1×, nearest-neighbor upscale at present time. Scale factor = `round(OS DPI scale)` (min 1) × 2 if double-size. Never fractional, so never blurry.
- Main and EQ windows are fixed size; the playlist resizes in Winamp's 25×29 px steps.

### 7.6 Default skin: "Winrust Base"

- Original pixel art in the Winamp 2.x style; no Nullsoft art copied.
- Generated by `cargo xtask gen-skin` from drawing code (bevels, gradients, own glyph bitmaps) into BMP and txt files committed under `assets/skin/`. The generator keeps the skin reproducible and editable.
- Also serves as the fallback source for sprites missing from user skins.
- Quality expectation: clean and functional classic look; community contributions to the art are welcome later.

### 7.7 Shaped windows (`region.txt`)

- **Windows:** `SetWindowRgn` on the window handle.
- **macOS:** transparent window with alpha mask; clicks pass through fully transparent pixels natively.
- **Linux/X11:** XShape bounding and input shapes via x11rb.

## 8. Windows, docking, input

### 8.1 Windows

Three borderless winit windows. Winrust draws all chrome.

| Window | Size (1×) | Contents |
|---|---|---|
| Main | 275×116; shade 275×14 | Titlebar (options menu, minimize, shade, close); clutterbar O-A-I-D-V; LCD: time (click toggles elapsed/remaining), play/pause/stop/buffering indicator, viz (click cycles modes), scrolling title (draggable), kbps, kHz, mono/stereo; volume, balance; seek bar; prev/play/pause/stop/next/eject; shuffle, repeat; EQ and PL toggles |
| Equalizer | 275×116; shade 275×14 | ON, AUTO, presets menu (load/save/delete, `.eqf`/`.q1` import); preamp + 10 band sliders; response graph (spline through band values) |
| Playlist | min 275×116, grows in 25×29 steps; shade 275×14 (width follows) | Numbered list with durations; selection (click, Ctrl, Shift, Ctrl+A); current-track color; double-click plays; drag reorder; scrollbar; ADD/REM/SEL/MISC/LIST menus; mini transport; running total time |

While dragging sliders the LCD title shows Winamp's text: `Volume: 55%`, `Balance: 30% Left`, `Seek to: 01:23/04:56 (28%)`.

### 8.2 Docking (`ui::dock`, pure)

- Windows snap to each other and to monitor work-area edges within a configurable distance (default 10 px).
- Dragging the main window moves its whole connected group (windows linked by touching edges, transitively).
- Dragging the EQ or playlist window alone detaches it from the group.
- Toggling shade or double-size re-lays-out the group so docked windows stay attached.
- Drag is implemented by Winrust (record pointer offset in screen coordinates, compute new rects via `ui::dock`, `set_outer_position` on each affected window). OS-native window drag is not used because it bypasses snapping.
- Positions are persisted. A window found off-screen at startup is reset onto the primary monitor.

### 8.3 Window behavior

- Only the main window appears in the taskbar/dock (Windows: EQ/playlist are owned windows; X11: skip-taskbar hint; macOS: single app icon).
- Always-on-top applies to all three windows.
- Active/inactive titlebar sprites follow focus.
- **Linux:** the app forces winit's X11 backend (runs under XWayland on Wayland desktops), because Wayland does not allow applications to position their own windows and docking would be impossible.

### 8.4 Input

- Classic Winamp hotkeys whenever any Winrust window has focus. Some keys depend on the focused window, as in Winamp (e.g. Ctrl+A toggles always-on-top in the main window but selects all in the playlist). Keys include: Z (prev), X (play), C (pause), V (stop), Shift+V (stop with fadeout), B (next), ←/→ (seek 5 s), ↑/↓ (volume), Ctrl+D (double-size), Ctrl+W (main shade), Alt+G (toggle EQ), Alt+E (toggle playlist), J (jump to file), Ctrl+J (jump to time), L (open file), Shift+L (open directory), Ctrl+L (open URL), Ctrl+P (preferences), Alt+S (skin browser), Alt+3 (file info), R (repeat), S (shuffle), Ctrl+T (time mode), Ctrl+A (always on top). The complete table is fixed in the M3 spec from Winamp 2.x documentation.
- Mouse wheel: over main = volume; over playlist = scroll; over an EQ slider = adjust that slider.
- File drop: on main → replace playlist and play; on playlist → insert at drop position.
- OS media keys and now-playing info via souvlaki (Windows SMTC, Linux MPRIS, macOS MPNowPlayingInfo).

### 8.5 Visualization

- Area: 76×16 px in the main window LCD.
- Modes: spectrum analyzer (thick bars or thin bands), oscilloscope (dots, lines, solid), off.
- Spectrum: peaks with configurable falloff; analyzer falloff speed configurable.
- Colors from `viscolor.txt`.
- Options via the viz right-click menu and Preferences.

## 9. Menus, dialogs, playlist, config

### 9.1 Menus

Structure mirrors Winamp 2.x: main menu (About, Play File, Play Location, window toggles, Skins, Options, Playback, Exit) plus widget menus (viz, EQ presets, playlist ADD/REM/SEL/MISC/LIST, playlist row context menu).

- **Windows and macOS:** native menus via muda.
- **Linux:** muda's context menus require a GTK window, and winit windows are not GTK. Linux uses an own-drawn popup menu window (same items and shortcuts, Win9x look, rendered by our compositor).

### 9.2 Dialogs (egui)

Rendered with egui-winit + egui-wgpu in separate windows on the same event loop. wgpu initializes lazily when the first dialog opens. Custom egui style approximating Win9x (gray, bevels).

- **Preferences** (Ctrl+P): General (snap distance, title scrolling, title format such as `%artist% - %title%`); Playlist (font size, title reading); Display (viz modes, falloff, frame rate, DPI scaling); Output (device, buffer size); Skins (skins folder).
- **File Info** (Alt+3): read/write title, artist, album, year, genre, track, comment via lofty; technical info. For streams: URL and current stream title.
- **Jump to file** (J): incremental filter over the playlist; Enter plays.
- **Jump to time** (Ctrl+J).
- **Open URL** (Ctrl+L) with history.
- **Skin browser** (Alt+S): lists skins in the skins folder plus Winrust Base; click applies; link to skins.webamp.org.
- **About:** credits and third-party licenses.
- File pickers and message boxes: rfd (native).

### 9.3 Playlist (`crates/playlist`)

- Entry: source, display title (from tags via format string; fallback to file name), duration. Title and duration are filled lazily by the tag worker, visible rows first.
- Formats: M3U, M3U8 (`#EXTINF`), PLS — read and write. Relative paths resolve against the playlist file's directory.
- Operations: add files, add directory (recursive, filtered by supported extension), add URL; remove selected, crop, remove all, remove dead files; select all/none/invert; sort by title, file name, path; reverse; randomize.
- Play order (pure, unit-tested): shuffle = random permutation, regenerated when exhausted, never repeating the just-played track immediately; repeat = repeat the whole playlist (Winamp 2.x semantics).

### 9.4 Config

- Location: OS config directory via `directories` (e.g. `%APPDATA%\winrust`, `~/Library/Application Support/winrust`, `~/.config/winrust`).
- Files: `config.toml`, autosaved `playlist.m3u8`, `eq_presets.toml`, `skins/` folder.
- Persisted: window positions, visibility, shade, double-size, always-on-top; volume, balance; EQ state; viz settings; time mode; shuffle, repeat; current skin; current playlist index; preferences.
- Writes are atomic (temp file + rename), debounced after changes, and flushed on exit.
- `config.toml` carries a version number; unknown fields are ignored. A corrupt file is renamed to `.bak`, defaults are used, and a warning is shown.

### 9.5 Single instance

A second launch (e.g. `winrust song.mp3`, `winrust --enqueue a.mp3 b.mp3`) sends its arguments to the running instance over a local socket (interprocess crate) and exits. Without `--enqueue`, files replace the playlist and play (Winamp's Explorer behavior).

## 10. Error handling and logging

- Library crates return typed errors (`thiserror`); the app uses `anyhow` at the top level.
- No panics on bad user data: skins, playlists, tags, streams, config.
- User-facing errors appear Winamp-style: a message in the LCD scrolling title for a few seconds; unreadable playlist entries are marked. A skin that fails to load keeps the current skin and shows an rfd message box.
- Logging via `tracing` to a rolling log file in the config directory; `--verbose` raises the level.
- A panic hook writes a crash log next to the regular log.

## 11. Testing

- **Unit tests** in the pure crates: playlist parsers, operations, play order; docking math; hit-testing; widget state; `Model::apply`; ICY parser; skin txt parsers; EQ coefficients (gain measured at band centers).
- **Golden image tests:** compositor output compared to committed PNG snapshots per window and state. `WINRUST_BLESS=1` regenerates snapshots. Bundled Arimo font guarantees identical output on all OSes.
- **Debug skin:** a synthetic skin in which every sprite is a unique labeled color, so any coordinate mistake is visible in snapshots.
- **Real-skin checks:** a small set of permissively licensed skins compared against Webamp reference screenshots.
- **Audio offline tests** using the capture sink: decode every supported format from small self-made fixtures; seek accuracy; sample-exact gapless join between two fixtures; EQ boost measured with FFT; streaming through a local test HTTP server that injects ICY metadata.
- **Fuzzing** (cargo-fuzz): BMP decoder, `.wsz` loader, M3U/PLS parsers, ICY parser.
- **Manual checklist** per milestone, including side-by-side comparison with Webamp for a skin set.
- **Lint gates:** `cargo clippy -- -D warnings`, `cargo fmt --check`, `cargo deny check`.

## 12. CI, release, milestones

### 12.1 CI

GitHub Actions matrix: `windows-latest`, `macos-latest` (arm64), `ubuntu-latest`. Jobs: build, test, clippy, fmt, deny. A nightly job runs each fuzz target briefly on Linux.

### 12.2 Release

cargo-dist produces:

- Windows: zip + MSI
- macOS: `.app` bundle in a `.dmg`
- Linux: tar.gz + AppImage

v1 binaries are unsigned. The README documents the SmartScreen and Gatekeeper warnings. Signing and notarization are post-v1.

### 12.3 Milestones

Each milestone gets its own spec → plan → build cycle.

| # | Milestone | Done when |
|---|---|---|
| M0 | Bootstrap: workspace, CI, licenses, README, `THIRD_PARTY.md`, xtask skeleton | CI green on all 3 OSes |
| M1 | Audio engine + headless `winrust-cli` | Plays files and streams, gapless, EQ, seek from the terminal; offline audio tests pass |
| M2 | Skin engine + debug skin + Winrust Base generator | Golden tests pass; real skins parse without errors |
| M3 | Main window: compositor + winit shell, LCD, transport, sliders, viz, shade, double-size, hotkeys | First usable player (files via CLI or drop) |
| M4 | Playlist crate + playlist window + M3U/PLS + tag worker + autosave | Daily-drivable |
| M5 | Equalizer window + presets + `.eqf`/`.q1` import | EQ fully usable from UI |
| M6 | Docking/snapping, window groups, `region.txt` shapes, skin cursors | Multi-window behavior matches Winamp |
| M7 | Menus, dialogs, preferences, single instance, media keys | Feature-complete |
| M8 | Packaging + v1.0 release | Binaries published on GitHub for all 3 OSes |

## 13. Decisions deferred to milestone specs

These are intentionally not fixed here; each is owned by the named milestone spec.

| Decision | Owner |
|---|---|
| Exact crate versions | M0 |
| Volume slider → gain curve (matched to Winamp by measurement) | M1 |
| EQ filter Q per band and slider → dB mapping (matched to Winamp by measurement) | M1 |
| Ring buffer size and stream prebuffer tuning | M1 |
| Full sprite coordinate table and BMP decoder fallback rules | M2 |
| Complete hotkey table | M3 |
| Spectrum bar math (FFT size, band grouping, falloff constants) | M3 |
| Playlist title format syntax | M4 |
| Linux own-drawn menu visual spec | M7 |
| Preferences page details | M7 |

## 14. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Wayland forbids window positioning | Docking impossible natively on Wayland | Force X11 backend (XWayland); native Wayland post-v1 |
| muda context menus need GTK on Linux | No native menus on Linux | Own-drawn popup menu on Linux (§9.1) |
| HE-AAC streams decode at LC quality | Some SHOUTcast stations sound muffled | Document; revisit with a license-compatible decoder post-v1 |
| Shaped windows behave differently per OS | Rare `region.txt` skins may look off on some OSes | Per-OS implementation (§7.7); manual test skins with regions |
| Default skin art quality | First impression | Generator-based art is easy to iterate; invite contributions |
| Unsigned binaries | Install friction | Document workarounds; plan signing post-v1 |
