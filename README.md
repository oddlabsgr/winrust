# Winrust

A cross-platform clone of the classic Winamp 2.x music player, written in Rust.

> **Status:** early development (milestone M3). The skinned main window plays files, folders
> and internet radio given on the command line or dropped on it. The playlist and equalizer
> windows, menus and user skins come in later milestones.

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

`cargo xtask gen-skin` redraws the default skin, Winrust Base, into `assets/skin/` from the
code in `xtask/src/gen_skin/`. Its output is committed too, and a test fails when the two
drift apart.

## Running the player

```sh
cargo run --release -p winrust -- song.mp3 some/folder https://example.com/stream
```

Files, folders (their audio files, in name order) and stream URLs play in the order given;
dropping files or folders on the window replaces them. The buttons, sliders and Winamp's
hotkeys work as in Winamp 2: Z X C V B for previous, play, pause, stop and next, arrows for
seeking and volume, Ctrl+D double size, Ctrl+W shade mode; the full table is in the
[M3 design](docs/superpowers/specs/2026-10-11-m3-main-window-design.md#6-hotkey-table).
Settings are kept in `config.toml` in the OS config folder (`%APPDATA%\winrust`,
`~/Library/Application Support/winrust` or `~/.config/winrust`).

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

## Checking skins

The skin engine can lay out every sprite of a skin on one image, or draw the main window
with it, for comparing against [Webamp](https://webamp.org):

```sh
cargo run -p winrust-ui --example skin-tool -- contact path/to/skin.wsz sheet.png
cargo run -p winrust-ui --example skin-tool -- render path/to/skin.wsz playing main.png
```

`render` takes a state (`stopped`, `playing`, `paused`, `buffering` or `shade`), and `base`
in place of a skin draws Winrust Base.

To load a whole folder of skins and write a contact sheet for each (no skins are committed;
they belong to their authors):

```sh
WINRUST_SKINS_DIR=path/to/skins cargo test -p winrust-skin --test real_skins -- --ignored --nocapture
```

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
