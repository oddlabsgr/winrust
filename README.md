# Winrust

A cross-platform clone of the classic Winamp 2.x music player, written in Rust.

> **Status:** early development (milestone M0, project bootstrap). It cannot play music yet.

Winrust aims to reproduce Winamp 2.x's main window, equalizer and playlist editor, and to
render classic `.wsz` skins pixel for pixel on Windows, macOS and Linux. Scope and roadmap:
[design document](docs/superpowers/specs/2026-10-10-winrust-design.md).

## Building

Needs stable Rust via [rustup](https://rustup.rs).

```sh
cargo build --workspace
cargo test --workspace
```

Developer tasks: `cargo xtask help`.

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
