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
