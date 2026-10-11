# M3 — Main Window Design

**Parent spec:** [Winrust umbrella design](2026-10-10-winrust-design.md) (§5, §7.5, §8, §12.3, §13).
**Reference implementation:** Webamp at commit 999f567 (`packages/webamp`), the same cross-reference the sprite table uses. Where this spec says "as Webamp", the file named is the authority for exact values.

## 1. Goal

M3 turns Winrust into a first usable player: one skinned main window, drawn pixel for pixel from any classic skin (Winrust Base by default), that plays files given on the command line or dropped onto it.

**Done when:** the umbrella milestone row holds — "Main window: compositor + winit shell, LCD, transport, sliders, viz, shade, double-size, hotkeys. First usable player (files via CLI or drop)" — and the manual checklist in §5.11 passes on Windows, with CI green on all three OSes.

## 2. Scope

**In:** everything in the main window (normal and shade mode), the visualizer, double-size, always-on-top, hotkeys (§6), file drop, command-line files and URLs, and a minimal `config.toml` (§5.9, pulled forward from §9.4 by decision).

**Out, with owner:**

| Item | Milestone |
|---|---|
| Playlist window, playlist crate, real shuffle order, M3U/PLS | M4 |
| Equalizer window | M5 |
| Docking/snapping, `region.txt` shapes, skin cursors | M6 |
| Menus (options button, right-click, viz menu), dialogs, file picker, skin browser, single instance, media keys, log file | M7 |

Inert in M3: the options button and right-click; clutter-bar O and I; Eject, L and numpad 0 (they open a file picker in M7); the EQ and PL buttons toggle their lit state only. The app always uses Winrust Base; user skins are exercised by golden tests and `skin-tool render` (§4.10).

## 3. Decomposition and workflow

M3 is built in two parts, each with its own plan, native inline execution, one fresh final reviewer, and a draft PR to `main`:

- **Part 1 — `crates/ui`** (pure, no windowing): layout, hit-testing, interaction, compositor, marquee, visualizer, golden tests, `skin-tool render`.
- **Part 2 — `crates/app`**: winit + softbuffer window, model, engine glue, input, hotkeys, config.

## 4. Part 1 — `crates/ui`

### 4.1 Crate

`winrust-ui`, depending on `winrust-skin` and `rustfft` only. No winit, egui, audio or OS API (umbrella §5.1). The canvas is `winrust_skin::Image` (32-bit `0xAARRGGBB`, 1× size); no second image type.

### 4.2 Layout

A const table of widgets: id, rectangle, kind (button, toggle, slider, text area, drag area). Coordinates at 1×, from Webamp `css/main-window.css`:

| Widget | Rect (x, y, w, h) |
|---|---|
| Title bar (drag area) | 0, 0, 275, 14 |
| Options / minimize / shade / close | 6 / 244 / 254 / 264, 3, 9, 9 |
| Clutter bar O, A, I, D, V | 10, 25/33/40/47/55, 8, 8/7/7/8/7 |
| Work indicator / play state | 24, 28, 3, 9 / 26, 28, 9, 9 |
| Time (click area); digits at x 48, 60, 78, 90; minus 38, 32, 5×1 | 39, 26, 59, 13 |
| Visualizer | 24, 43, 76, 16 |
| Marquee (text cells at y 27) | 111, 24, 154, 12 |
| kbps / kHz | 111, 43, 15, 6 / 156, 43, 10, 6 |
| Mono / stereo | 212, 41, 27, 12 / 239, 41, 29, 12 |
| Volume (thumb 14×11 at y 58) | 107, 57, 68, 13 |
| Balance (thumb 14×11 at y 58) | 177, 57, 38, 13 |
| EQ / PL toggles | 219 / 242, 58, 23, 12 |
| Seek bar (thumb 29×10) | 16, 72, 248, 10 |
| Previous / play / pause / stop | 16 / 39 / 62 / 85, 88, 23, 18 |
| Next | 108, 88, 22, 18 |
| Eject | 136, 89, 22, 16 |
| Shuffle / repeat | 164, 89, 47, 15 / 210, 89, 28, 15 |
| About (inert) | 253, 91, 13, 15 |

Shade mode (275×14): title-bar buttons as above; mini visualizer 79, 5, 38, 5; mini time cells at x 128, 134, 139, 147, 152, y 4; mini transport previous 169 (w 7), play 176 (10), pause 186 (9), stop 195 (9), next 204 (10), eject 215 (10), all y 2, h 10; mini seek 226, 4, 17, 7 with a 3×7 thumb.

`hit_test(mode, point) -> Option<Widget>` returns the topmost widget at a 1× point.

### 4.3 Interaction

`Interaction` is a pointer state machine: `pointer(&mut self, Pointer, &MainView) -> Option<UiEvent>`.

- **Buttons and toggles:** press shows the pressed sprite; the event fires on release inside the widget; leaving the widget while held un-presses it, re-entering re-presses it.
- **Sliders:** pressing the thumb keeps the grab offset; pressing the track jumps the thumb's centre to the pointer, then drags. Volume and balance emit continuously while dragging; the seek bar emits `Seek` on release only, as in Winamp. A non-seekable track disables the seek bar.
- **Marquee:** dragging scrubs the text horizontally; stepping resumes 1 s after release.
- **Title bar:** dragging emits `MoveWindow(dx, dy)` in 1× units; double-click toggles shade.
- **Title buttons** keep a "clicked" latch: until a title button is first clicked, the compositor shows the title bar's own pixels for it (Webamp `ClickedDiv.tsx`).
- **Time:** click toggles elapsed/remaining. **Visualizer:** click cycles spectrum → oscilloscope → off.

### 4.4 MainView

A pure snapshot the app builds each frame: time (elapsed, duration), time mode, title text, kbps, kHz, channels, playback state (playing, paused, stopped, buffering), volume 0..=100, balance -100..=100, position fraction and seekable flag, shuffle, repeat, EQ and PL lit, focused, shade, double-size, always-on-top, easter-egg title, and the current visualizer frame.

### 4.5 Compositor

`draw_main(&Skin, &MainView, &Interaction, &mut Image)` and `draw_main_shade(...)`. Sprites are blitted opaque.

- **Title bar:** selected sprite when focused; easter-egg sprites when that flag is set. Shade mode uses the shade background sprites.
- **Background shows through** (Webamp): title buttons until first clicked; the time colon; the mini-time colon.
- **Time:** `nums_ex` if present, else `numbers`. Remaining mode draws the minus sign (`numbers` minus = row 6 of the "2", as in M2). When stopped the time, kbps and kHz are hidden and the seek thumb is hidden. When paused the time blinks: 1 s visible, 1 s hidden. Minutes above 99 display as hours:minutes. A track with unknown duration shows elapsed time in remaining mode, without the minus.
- **Indicators:** play, paused, stopped sprites; `MAIN_NOT_WORKING_INDICATOR` while playing, `MAIN_WORKING_INDICATOR` while buffering (play triangle hidden then).
- **Text:** marquee, kbps and kHz use `text.bmp` glyphs via `Skin::glyph`. Mono/stereo light per source channel count; both are unlit when stopped.
- **Sliders:** volume frame `clamp(round(v·28/100) − 1, 0, 27)`; balance frame `floor(|b|·27/100)` (Webamp `MainVolume.tsx`, `MainBalance.tsx`); thumb travel `(track width − thumb width) · value`.
- **Clutter bar:** A lit when always-on-top, D lit when double-size; the pressed button shows its selected sprite.
- **While dragging** a slider the marquee shows Winamp's text: `Volume: 55%`, `Balance: 30% Left` / `Center` / `30% Right`, `Seek to: 01:23/04:56 (28%)`.
- **Scaling** is not the compositor's job; it always draws at 1×.

### 4.6 Marquee

- Text: `"<n>. <title> (<m:ss>)"` followed by the separator `"  ***  "`, looping; titles that fit are not scrolled. Streams show their stream title.
- Steps one character (5 px) every 220 ms (Webamp `Marquee.tsx`). Pure: `Marquee::step()`, `offset()`; time is driven by the app.
- Error messages (§5.10) replace the text for 3 s.

### 4.7 Visualizer

A port of Webamp `components/VisPainter.ts`. Input: the audio tap's latest 576 frames per channel as `&[f32]` slices (the `ui` crate does not depend on `winrust-audio`).

- **Spectrum:** mix to mono, Hann window, zero-pad to 1024, `rustfft` forward, 512 magnitude bins. Columns: 75, each reading bin index `(1 − 0.91)·linear + 0.91·log` (Webamp's blend), interpolated. Bar height 0..=16 rows, calibrated so silence draws 0 rows and a full-scale sine fills its column.
- **Bandwidth:** wide (default) averages groups of 4 columns and leaves the 4th blank; thin draws every column.
- **Falloff:** bars fall `falloff / 16` rows per frame with falloff slower 3, slow 6, moderate 12 (default), fast 16, faster 32. Peaks fall with acceleration factor slower 1.05, slow 1.1, moderate 1.2, fast 1.4, faster 1.6 (default as Webamp `Vis.tsx`); peaks on by default.
- **Colouring:** normal (default), fire, line, from `viscolor.txt` rows as Webamp.
- **Oscilloscope:** 75 columns from the 576 frames; styles dots, lines (default), solid; colours from `viscolor.txt`.
- **Off** draws the visualizer background only.
- Shade mode draws the mini visualizer (38×5) as Webamp's `smallVis` mode in `VisPainter.ts`.

### 4.8 Skin crate carry-forwards (from M2 part 1)

- `Skin::glyph(char)` for `text.bmp` lookup.
- An own-sprite query (does the user's skin have this sprite, without fallback), for the EQ maximize alias.
- Debug skin: slider frames striped so frame index mistakes show in golden images.

### 4.9 Winrust Base art minors (from M2 part 2)

Folded in, each with a test: next button drawn 22 px wide; the two main shade bars no longer share row 42's look; groove stubs at y 67–68 removed; playlist visualizer panel `rect(vis.x+1, vis.y+11, 74, 18)`; playlist shade-right stripes kept visible; "STEREO" fits its 29 px cell; EQ maximize/minimize pressed icons match Webamp's use.

### 4.10 `skin-tool render`

The `skin-tool` example moves from `winrust-skin` to `winrust-ui` (rendering needs the compositor, and `winrust-skin` cannot depend on `winrust-ui`). It keeps `contact` and `debug-wsz` and gains `render`: `cargo run -p winrust-ui --example skin-tool -- render <skin|base> <state> out.png` composes the main window in a named state (stopped, playing, paused, buffering, shade) for manual comparison with Webamp. The README commands follow the move.

### 4.11 Tests

- Unit: hit-testing for every widget and gaps; interaction (press/leave/re-enter/release, slider grab offset and track jump, seek on release, title latch, double-click); marquee stepping and wrap; slider frame formulas; time formatting (blink, minus, hours, unknown duration); visualizer calibration (silence → 0, full-scale sine → 16 in its column), falloff and peak sequences.
- Golden PNGs (`WINRUST_BLESS=1`), debug skin and Winrust Base: stopped, playing elapsed, playing remaining, paused (both blink phases), buffering, shade, pressed play, volume drag text, focused and unfocused.

## 5. Part 2 — `crates/app` (binary `winrust`)

### 5.1 Dependencies

`winit`, `softbuffer`, `directories`, `serde` + `toml`; workspace crates `winrust-audio`, `winrust-skin`, `winrust-ui`. Versions fixed in the plan; `cargo deny` stays green.

### 5.2 Window and scaling

One borderless winit window: 275×116, or 275×14 in shade. Present scale = `max(1, round(OS scale factor)) × (2 if double-size)`, nearest-neighbour upscale into the softbuffer surface. The window resizes when scale, shade or double-size changes. On Linux the X11 backend is forced (umbrella §8.3).

### 5.3 Model

`app::model`: `Model`, `Action`, `Effect`, `Model::apply(Action) -> Vec<Effect>`, pure and unit-tested without a window. Effects: engine `Command`, `Redraw`, `SaveConfig`, `MoveWindow`, `Resize`, `SetAlwaysOnTop`, `Minimize`, `Exit`. Windows hold no state beyond OS handles.

### 5.4 Input

- Pointer: winit events → 1× coordinates → `Interaction` → `UiEvent` → `Action`.
- Keys: the hotkey table (§6), one pure module.
- Mouse wheel over the window: volume, same step as ↑/↓.
- File drop: replaces the queue and plays. Command line: `winrust [file|URL]...` does the same at start-up.

### 5.5 Queue

`app::queue` starts from the CLI's pure `Queue` and extends it; the CLI keeps its own unchanged. Previous/next, gapless `QueueNext`, unreadable entries skipped (umbrella §6.7). Repeat wraps the queue; shuffle picks a random entry other than the current one; numpad 1/3 jump 10 entries; Ctrl+V stops after the current track. The playlist crate replaces this in M4.

### 5.6 Engine glue

The engine spawns with the cpal sink; `EngineEvent`s wake the loop through an `EventLoopProxy`; the analyzer reader is read at frame time. **M1 carry-forward:** position and state must keep updating while a stream read stalls; fixed with a test in this part.

### 5.7 Redraw policy

Idle (stopped/paused, nothing animating): event-driven, zero CPU. Playing: `ControlFlow::WaitUntil` at 60 Hz for the time, marquee and visualizer. A paused time blink wakes at 1 s steps.

### 5.8 Window behaviour

Self-implemented drag (`MoveWindow` → `set_outer_position`; snapping waits for M6). Focus drives the active/inactive title bar. Minimize; close exits. Always-on-top (Ctrl+A, clutter A); double-size (Ctrl+D, clutter D); shade (Ctrl+W, title button, title double-click). Shift+V fades the volume to zero over 1 s, stops, and restores the volume setting.

### 5.9 Config (minimal)

`config.toml` in the OS config directory (`directories`). Fields: `version`, window position, double-size, always-on-top, volume, balance, time mode, visualizer mode, shuffle, repeat. Atomic writes (temp file + rename), debounced 1 s after a change, flushed on exit. Unknown fields ignored; a corrupt file is renamed to `config.toml.bak`, defaults are used, and the marquee shows a warning. A window position off every monitor resets onto the primary monitor.

### 5.10 Errors

Shown Winamp-style as marquee text for 3 s (umbrella §10). An unreadable file advances to the next entry.

### 5.11 Tests and manual checklist

- Unit: `Model::apply` (transport, volume and balance, seek, toggles, hotkeys, drop, queue order, Ctrl+V, fadeout), config round trip, corrupt and missing config, off-screen reset.
- CI builds and tests on all three OSes.
- Manual checklist (Windows): launch empty; drop files; command-line files and a stream URL; every button, slider and hotkey; shade and double-size; always-on-top; focus change; unplug headphones during play; restart restores config.

## 6. Hotkey table

Active whenever the main window has focus. Numpad aliases in brackets.

| Key | Action | M3 |
|---|---|---|
| Z (Num 4) | Previous | yes |
| X (Num 5) | Play; restart if playing; resume if paused | yes |
| C | Pause / resume | yes |
| V | Stop | yes |
| Shift+V | Stop with 1 s fadeout | yes |
| Ctrl+V | Stop after current track | yes |
| B (Num 6) | Next | yes |
| ← / → (Num 7 / Num 9) | Seek −5 s / +5 s | yes |
| ↑ / ↓ (Num 8 / Num 2) | Volume +1 / −1 | yes |
| Num 1 / Num 3 | Jump back / forward 10 entries | yes |
| R | Toggle repeat | yes |
| S | Toggle shuffle | yes |
| Ctrl+T | Toggle time mode | yes |
| Ctrl+D | Toggle double-size | yes |
| Ctrl+A | Toggle always-on-top | yes |
| Ctrl+W | Toggle main shade | yes |
| Alt+G / Alt+E | Toggle EQ / playlist (lit state only) | yes |
| Typing N U L L S O F T | Toggle easter-egg title bar | yes |
| Alt+W | Toggle main window | M4 |
| Ctrl+R | Reverse playlist | M4 |
| L (Num 0) / Shift+L / Ctrl+L | Open file / directory / URL | M7 |
| J / Ctrl+J | Jump to file / time | M7 |
| Ctrl+P / Alt+S / Alt+3 / Alt+F | Preferences / skins / file info / main menu | M7 |

## 7. Decisions closed here

- **Complete hotkey table** (umbrella §13): §6.
- **Spectrum maths** (umbrella §13): §4.7.
- **Config subset pulled into M3:** §5.9; the rest of umbrella §9.4 stays with its milestones.
- **Title buttons follow Webamp's `.clicked` rule** (§4.3), matching the Winrust Base art.
- **576-frame tap, zero-padded FFT:** the audio tap keeps Winamp's 576-frame window (umbrella §6.5); the spectrum zero-pads to 1024.
- **Long times:** minutes above 99 show as hours:minutes.

## 8. Risks

| Risk | Mitigation |
|---|---|
| Visualizer looks different from Winamp | Calibration tests; side-by-side check against Webamp in the manual checklist |
| DPI scale factors like 1.25 round to 1 and look small | Integer scaling is the spec's rule (no blur); double-size covers it |
| winit borderless-window quirks per OS (focus, drag, always-on-top) | Manual checklist on Windows; CI builds on all three; macOS/Linux manual passes before M8 |
