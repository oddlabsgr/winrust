# M4 — Playlist Design

**Parent spec:** [Winrust umbrella design](2026-10-10-winrust-design.md) (§7.5, §8.1, §8.4, §9.3, §9.4, §10, §12.3, §13).
**Previous milestone:** [M3 main window](2026-10-11-m3-main-window-design.md), whose rules for driving `winrust-ui` and whose app shell this milestone extends.
**Reference implementation:** Webamp at commit 999f567 (`packages/webamp`), as in M3. Where this spec says "as Webamp", the file named is the authority for exact values.

## 1. Goal

M4 makes Winrust daily-drivable: a playlist window to build, reorder and trim a list, titles and durations read from tags, M3U/PLS files, a real shuffle order, and a playlist that is still there the next day.

**Done when:**
- the umbrella milestone row holds: "Playlist crate + playlist window + M3U/PLS + tag worker + autosave";
- the manual checklist in §6.8 passes on Windows;
- CI is green on all three OSes.

## 2. Scope

**In:**
- the `winrust-playlist` crate: entries, selection, editing, sorting, play order, playlist files, title templates, tag reading, the tag worker;
- the playlist window, normal and shade, in `winrust-ui`;
- a second window in the app, swapping M3's queue for the playlist;
- autosave of the playlist;
- the playlist key table (§7), and Alt+W and Ctrl+R from M3's table.

**Out, with owner:**

| Item | Milestone |
|---|---|
| Equalizer window | M5 |
| Docking, snapping, window groups | M6 |
| Menus (ADD/REM/SEL/MISC/LIST popups, right-click), file picker, Add URL, File Info, Jump to file, saving or loading a playlist by name, the skin's named playlist font, user skins | M7 |

**Inert in M4:**
- the ADD, REM, SEL, MISC and LIST buttons show pressed but open nothing;
- the playlist's Eject mini button, like the main window's.

Every action those menus will hold that M4 needs has a key (§7).

## 3. Decomposition and workflow

M4 is built in three parts, each with its own plan, native inline execution, one fresh final reviewer, and a draft PR to `main`:

- **Part 1 — `crates/playlist`** (pure, plus tag reading): §4.
- **Part 2 — the playlist window in `crates/ui`** (pure, no windowing): §5.
- **Part 3 — `crates/app`**: the second window, the model swap, tags, autosave, keys: §6.

## 4. Part 1 — `crates/playlist`

### 4.1 Crate

`winrust-playlist` depends on:
- `winrust-audio`, for `Source`;
- `lofty`, for tags;
- `crossbeam-channel`, for the tag worker.

It has no windowing or OS API, as umbrella §5.1 requires. Versions are fixed in the plan, and `cargo deny` stays green.

### 4.2 Entries

An entry holds:
- a stable `EntryId`, never reused within a session;
- its `Source`;
- tags once read (artist, title, album, year, track number);
- a duration once known;
- a `dead` flag: set when the file is missing or the engine failed to open it.

URLs are never read for tags; their title is the URL until the stream sends one.

### 4.3 Playlist and selection

`Playlist` holds the entries in order, the current entry (by id, so edits never lose it) and the selection.

Selection follows Webamp's reducer (`reducers/playlist.ts`):
- a click selects one row and sets the anchor;
- Ctrl-click toggles a row;
- Shift-click selects the range from the anchor;
- select all, select none, and invert.

The anchor is also the keyboard cursor (§7).

### 4.4 Editing

All edits keep the current entry and the selection attached to the same entries.

- **Adding:**
  - insert sources at a position (a drop between rows);
  - append;
  - replace all.
- **Removing:**
  - remove selected;
  - crop (keep only the selected entries);
  - clear;
  - remove dead entries (Alt+Del; files are checked for existence at that moment).
- **Moving:** move the selected entries up or down by n rows. This serves dragging and Alt+↑/↓. The block stops at the list's ends.
- **Ordering:**
  - sort by title (as displayed), by file name, or by path;
  - reverse;
  - randomize, from the play-order generator (§4.5).

Sorting is case-insensitive, compares paths one component at a time, and is stable. This fixes M3's deferred minor where "Live 2001/" sorted before "Live/".

### 4.5 Play order

Pure and seeded, so tests repeat exactly.

- **Shuffle:** a random permutation of the entries, regenerated when exhausted; the first track of a new permutation is never the one just played (umbrella §9.3). Edits keep the permutation valid: added entries are placed at random positions in the part not yet played, and removed ones drop out.
- **Repeat:** wraps the whole list (Winamp 2.x semantics), in list order or permutation order.
- **Without either:** next and previous stop at the ends.
- **Jumps:** the ±10 jumps (numpad 1/3) move through the list order, even with shuffle on, and clamp at the ends, as in M3.

### 4.6 Playback cursor

This is the engine bookkeeping from M3's `app::queue`:
- the current track, plus the track handed to the engine early with `QueueNext` so it starts without a gap;
- `Unqueue` when settings change;
- stop-after-current;
- skipping entries the engine cannot open.

It now walks the play order, with three changes:
- **Streams are never queued gaplessly.** A URL loads with `Load` when the previous track ends. This fixes M3's deferred minor: umbrella §6.3 limits gapless to local files.
- **The skip guard counts distinct entries that failed**, not attempts. Playback stops only when every entry has failed. This fixes M3's shuffle minor.
- **Failed entries are marked `dead`.**

### 4.7 Playlist files

**Read:**
- **M3U / M3U8:**
  - `#EXTM3U`, with `#EXTINF:<seconds>,<title>` giving a title and duration until tags are read;
  - other `#` lines are ignored.
- **PLS:**
  - `[playlist]` with `FileN`, `TitleN`, `LengthN`;
  - keys are case-insensitive;
  - entries are taken in N order, gaps skipped.
- **All formats:**
  - relative paths resolve against the playlist file's folder;
  - `file://` URLs become paths;
  - http(s) and icy URLs pass through.
- **Encoding:**
  - `.m3u8` and `.pls` are UTF-8;
  - a `.m3u` that is not valid UTF-8 is read as Windows-1252;
  - a UTF-8 byte-order mark is skipped.

**Write:**
- **M3U8:** `#EXTM3U`, then `#EXTINF:<seconds>,<title>` and the location for each entry, in UTF-8. Unknown durations are written as `-1`.
- **PLS:** version 2.
- **Paths:** the writer takes an optional base folder; paths under it are written relative, others absolute.

The autosave (§6.5) writes absolute paths. Unreadable lines are skipped; a playlist file never fails as a whole.

### 4.8 Title templates

The format is a template string, by default `[%artist% - ]%title%`.

- **Fields:** `%artist%`, `%title%`, `%album%`, `%year%`, `%track%`, `%filename%` (the file name without extension; for URLs the URL).
- **Optional sections:** `[...]` vanishes when any field inside it is empty. Sections may nest.
- **Literals:** `%%`, `[[` and `]]` produce a literal `%`, `[` and `]`.
- **Errors:** unknown fields expand to nothing. An unbalanced bracket or a lone `%` is taken literally, so no template is an error.
- **Fallback:** when the result is empty, or the entry has no title tag, the title is `%filename%`.

This closes umbrella §13's "playlist title format syntax".

### 4.9 Tags

`tags::read(path)` returns the tag fields and the duration, using `lofty`'s primary tag and audio properties. A file without tags still gets its duration. A file that cannot be read returns an error: it is not marked dead, because playing it may still work.

### 4.10 Tag worker

`TagWorker` owns one background thread.

- **Requests:**
  - a request is a batch of `(EntryId, path)` pairs, in priority order (visible rows first);
  - a new request replaces the pending ones; reads already in flight finish.
- **Results:** `(EntryId, Result<Tags>)` arrive on a channel; the caller applies them to the playlist by id, so edits made meanwhile are harmless.
- **Shutdown:** dropping the worker stops it.

### 4.11 Folders

Expanding a folder moves here from `app::sources`:
- recursive;
- filtered by the extension list from M3 (`aac flac m4a mp3 mp4 oga ogg wav`);
- sorted as in §4.4.

Playlist files (`.m3u`, `.m3u8`, `.pls`) given or dropped expand to their entries; inside folders they are ignored.

### 4.12 Tests

- **Unit tests:**
  - every edit, including that the current entry and the selection survive;
  - selection;
  - sorting;
  - shuffle permutations: never empty, regenerated, no immediate repeat;
  - the cursor's gapless, stream, stop-after and skip rules, ported from M3's queue tests;
  - templates.
- **Round trips:**
  - M3U8 and PLS, written then read back;
  - the playlist-file reading cases: odd encodings, relative paths, missing numbers.
- **Tags and the worker:** run against temporary copies of the existing audio fixtures, with tags written into them by `lofty`, so no new fixture files are needed.

## 5. Part 2 — the playlist window in `crates/ui`

### 5.1 Size and layout

The size is set in steps (`w`, `h` ≥ 0):

| Mode | Size |
|---|---|
| Normal | `275 + 25·w` × `116 + 29·h` |
| Shade | `275 + 25·w` × 14 |

As Webamp `css/playlist-window.css`:

- **Top bar (20 px):**
  - left corner 25 px and right corner 25 px;
  - the title piece, 100 px, centred;
  - tiled fills between them;
  - a 12 px or 13 px spacer on each side when `w` is even;
  - the close button at right 2, the shade button at right 12, both 9×9 at top 3.
- **Middle:**
  - a 12 px left edge;
  - the track list, with 3 px padding above and below;
  - a 20 px right edge holding the scrollbar.
- **Bottom bar (38 px):**
  - **left block (125 px):** the ADD, REM, SEL and MISC buttons, 22×18 at x 14, 43, 72 and 101, bottom 12;
  - **right block (150 px):**
    - the running time at (7, 10);
    - six 10×10 mini transport buttons from (3, 22);
    - the mini time at (66, 23);
    - LIST, 22×18, at right 22, bottom 12;
    - scroll-up and scroll-down buttons, 8×5, at right 7 and top 2 and 8;
    - the 20×20 resize grip in the corner;
  - **between them:** tiled fill. When `w ≥ 3`, the 75 px visualizer slot sits left of the right block.

All sprites come from `pledit.bmp` through the skin crate's sprite table. The window is drawn with the focused or unfocused title bar.

### 5.2 Track list

- **Rows:** 13 px tall; visible rows = `floor(list height / 13)`.
- **Contents:** `"<n>. <title>"` on the left, with the numbers padded to the width of the largest one, and the duration (`m:ss`) right-aligned with 3 px padding.
- **Long titles** are cut where the duration column begins, with no ellipsis.
- **Colours** come from `pledit.txt`:
  - text in `Normal`, or `Current` for the current entry;
  - row background `NormalBG`, or `SelectedBG` for selected rows.
- **Durations:** an unknown one shows nothing; a dead entry shows `--:--`.

### 5.3 Font

- **The font:** Arimo Regular (Apache-2.0), committed as `assets/fonts/Arimo-Regular.ttf`, embedded in `winrust-ui` and credited in `THIRD_PARTY.md`.
- **Size and spacing:** 9 px with 0.5 px letter spacing, as Webamp's CSS.
- **Rendering:** rasterized with `fontdue`, coverage-blended between the row background and text colours.
- **Cache:** glyphs are cached per character.
- **Missing characters** draw Arimo's missing-glyph box.
- **Golden images:** rendering must be identical on all three OSes for the golden images; the plan checks that on CI before the golden images are trusted.

### 5.4 Bottom bar

- **Running time:**
  - `<selected>/<total>` in `text.bmp` glyphs, as Webamp `RunningTimeDisplay.tsx`;
  - each part is `m:ss` under 100 minutes and `h:mm:ss` from there;
  - the total gets a `+` while some durations are unknown;
  - the text is padded to 18 cells. Hours keep long playlists inside them, which settles M3's carry-forward.
- **Mini time:** the playing time, drawn as the main window's shade mode draws it (`MiniTime.tsx`).
- **Visualizer slot:** shows the visualizer frame only while the main window is hidden (Webamp `index.tsx`).

### 5.5 Scrolling

- **Offset:** the scroll offset is a row index, clamped so the last row can reach the bottom.
- **Scrollbar:**
  - an 8×18 handle in a track `window height − 58` px tall, as Webamp `PlaylistScrollBar.tsx`;
  - dragging the handle scrolls proportionally;
  - clicking the track jumps the handle there;
  - the scrollbar is disabled when every row fits.
- **Wheel:** scrolls 3 rows per notch.
- **Buttons:** the scroll buttons move 4 rows.

### 5.6 Interaction

`PlaylistInteraction` is a pointer state machine like `Interaction` in M3: `pointer(Pointer, &PlaylistView) -> Option<PlaylistEvent>`.

- **Rows:**
  - click, Ctrl-click and Shift-click follow §4.3;
  - a click on an already selected row keeps the selection, so it can be dragged;
  - a double-click plays the row;
  - dragging a selected row moves the selection by whole rows (Webamp `TrackList.tsx`).
- **Empty list area:** a click selects none.
- **Buttons:**
  - press and release like the main window's;
  - close, shade and the mini transport act;
  - scroll-up and scroll-down act;
  - ADD, REM, SEL, MISC and LIST only show pressed.
- **Window:**
  - dragging the title bar or any blank frame area moves the window;
  - a double-click on the title bar shades it;
  - dragging the resize grip resizes in whole steps (`ResizeTarget`); in shade mode it changes the width only.
- **Drops:** `drop_index(y)` gives the insertion row for a drop, as Webamp `handleDrop`.

### 5.7 View and compositor

- **`PlaylistView`** is the snapshot the app builds:
  - size steps and shade;
  - focus;
  - the visible rows: number, title, duration text, current, selected;
  - the total row count and the scroll offset;
  - the running-time text and the mini-time state;
  - the visualizer frame;
  - the pressed widget.
- **Drawing:** `draw_playlist` and `draw_playlist_shade` draw it at 1×. The shade bar shows `"<n>. <title>"`, cut with `…` to fit, and the track's time, as Webamp `PlaylistShade.tsx`.
- **`skin-tool render`** gains a `playlist` state, drawn with fixed sample rows.

### 5.8 Tests

- **Unit tests:**
  - the layout for several sizes, odd and even widths included;
  - hit-testing;
  - every interaction;
  - running-time formatting;
  - scrolling;
  - drop indices.
- **Golden PNGs** (`WINRUST_BLESS=1`) for the debug skin and Winrust Base:
  - smallest size, unfocused;
  - `w=2, h=3` focused, with current, selected, dead and unknown-duration rows and a scrolled list;
  - shade mode;
  - the visualizer slot.

## 6. Part 3 — `crates/app`

### 6.1 Windows

- **Opening:** the playlist is a second borderless window. On first run it opens directly under the main window; until docking (M6) it moves independently.
- **Taskbar:** only the main window appears there:
  - on Windows the playlist is owned by it (so it minimizes with it) and skips the taskbar;
  - on X11 it is a utility window.
- **Showing and hiding:**
  - the PL button, Alt+E and the playlist's close button show and hide it;
  - the PL button is lit while it is visible;
  - Alt+W shows and hides the main window, but never hides the last visible window.
- **Always-on-top** applies to both windows.
- **Double-size** applies to the main window only, as in Winamp 2.
- **Scaling** is the OS scale, as in M3.
- **Focus** decides which title bar is active and which key table applies.

### 6.2 Model

- **Playlist:** `Model` holds the `Playlist` and its cursor in place of M3's `Queue`.
- **Window naming:** actions and effects name their window (`Main` or `Playlist`). Pointer, focus, move, resize and visibility become per-window.
- **Playlist window state** joins the model: visibility, shade, size steps, scroll offset, interaction and position.
- **The main window's marquee** shows `"<n>. <title> (<m:ss>)"` with the template title (M3 used the file name).

### 6.3 Drops and arguments

- **Dropped on the main window:** files, folders and playlist files replace the playlist and play.
- **Dropped on the playlist:** they insert at the drop row without starting playback (umbrella §8.4, Webamp `LOAD_STYLE.NONE`).
- **Command-line arguments** replace and play, as in M3. Single-instance handling is M7.

### 6.4 Tags in the app

- **Worker:** the app runs the `TagWorker`, and its results reach the event loop through the same `EventLoopProxy` as engine events.
- **Requests:** a request is sent when entries without tags appear, and again when the visible rows change; visible rows go first.
- **Results** update the playlist by id; the marquee follows if the current entry changed.

### 6.5 Autosave and config

- **Playlist file:**
  - `playlist.m3u8` in the config folder, written with absolute paths;
  - written atomically, debounced 1 s after any playlist change, and flushed on exit, like `config.toml`;
  - at start it loads, unless files are given on the command line;
  - an unreadable file is renamed `playlist.m3u8.bak` with a marquee warning, like the config.
- **`config.toml` gains:**
  - `main_visible`;
  - the playlist window's `position`, `size` (steps), `visible` and `shade`;
  - `current` (index) and `scroll`;
  - `title_format` (default `[%artist% - ]%title%`).
- **Old configs:** M3's files load unchanged, with the new fields at their defaults.

### 6.6 Keys

- Keys act on the focused window.
- The global table from M3 §6 works in both windows, except where §7 gives the playlist window its own meaning.
- Ctrl+A is always-on-top in the main window and select-all in the playlist (umbrella §8.4).

### 6.7 Mouse wheel

- **Over the playlist:** scrolls the list (§5.5).
- **Over the main window:** sets the volume, as in M3.

### 6.8 Tests and manual checklist

**Tests:**
- model tests for both windows;
- drops on each window;
- focus-dependent keys;
- tag results applied by id;
- autosave round trip, and corrupt and missing playlist files;
- config migration from an M3 `config.toml`.

**Manual checklist (Windows):**
- [ ] Drop a folder on the playlist: titles and durations fill in, visible rows first.
- [ ] Drop more files between rows: they insert there.
- [ ] Reorder by dragging and with Alt+↑/↓.
- [ ] Sort, reverse and randomize.
- [ ] Remove, crop and clear.
- [ ] Double-click plays; the current row is highlighted.
- [ ] Shuffle and repeat play every track before repeating.
- [ ] Resize and shade the playlist.
- [ ] Hide the main window with Alt+W; the visualizer moves into the playlist.
- [ ] Close and restart: the playlist, the current track and both windows come back.

## 7. Playlist key table

Active when the playlist window has focus; M3's global keys (§6 of the M3 spec) also work there unless overridden here.

| Key | Action |
|---|---|
| ↑ / ↓ | Select the row above / below the cursor |
| PgUp / PgDn | Move the cursor a page |
| Home / End | Move the cursor to the first / last row |
| Shift + any of those | Extend the selection from the anchor |
| Enter | Play the cursor row |
| Del | Remove selected |
| Ctrl+Del | Crop |
| Ctrl+Shift+Del | Clear the playlist |
| Alt+Del | Remove dead entries |
| Ctrl+A | Select all |
| Ctrl+I | Invert selection |
| Alt+↑ / Alt+↓ | Move the selection up / down one row |
| Ctrl+Shift+1 / 2 / 3 | Sort by title / file name / path |
| Ctrl+R | Reverse the list |
| Ctrl+Shift+R | Randomize the list |
| Ctrl+W | Toggle the playlist's shade mode |

These keys work in both windows: ← / → seek ±5 s, Alt+W toggles the main window, Alt+E toggles the playlist.

## 8. Decisions closed here

- **Playlist title format syntax** (umbrella §13): §4.8.
- **Fonts:** the bundled Arimo only in M4. The umbrella's lookup of the skin's named font via fontdb moves to M7, with user skins; until then the app only draws Winrust Base.
- **Menu buttons inert until M7;** keys and drops cover M4's needs (§2, §7).
- **Gapless is for local files only;** streams load after the previous track ends (§4.6).
- **The playlist is not doubled** by double-size (Winamp 2).
- **Dead entries** show `--:--` as their duration (umbrella §10, "unreadable playlist entries are marked").

## 9. Risks

| Risk | Mitigation |
|---|---|
| `fontdue` output differs between OSes, breaking golden images | Golden images are checked on CI on all three OSes; if they differ, the plan pins rasterization to integer coverage or drops anti-aliasing for golden tests |
| Huge playlists (10 000+ entries) are slow to draw, sort or save | Only visible rows are drawn; sorting and saving are linear or n·log n; tag reading is lazy |
| Tag reading of a slow or network file blocks the worker | One worker thread, visible rows first, new requests replace old ones; the UI never waits |
| Hiding the main window removes the Windows taskbar button (the playlist is an owned window) | Alt+W never hides the last visible window; window groups and taskbar ownership are revisited in M6 |
