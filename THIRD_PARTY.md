# Third-party material

## Rust dependencies

All Rust dependencies use permissive licenses. `cargo deny check` enforces the allowlist in
[`deny.toml`](deny.toml); `cargo deny list` prints the license of every crate.

Notable: the symphonia decoder crates are licensed MPL-2.0, a file-level copyleft license.
Winrust uses them unmodified from crates.io, where their source is available.

## Assets and references

### Webamp skin sprite map

`crates/skin/src/sprites.rs` and `crates/skin/src/text.rs` transcribe the sprite coordinate
table and the `text.bmp` character map from [Webamp](https://github.com/captbaritone/webamp)
(`packages/webamp/js/skinSprites.ts`, commit `999f567`). The skin loader also follows the
parsing rules of Webamp's skin, `pledit.txt`, `viscolor.txt` and `region.txt` parsers. Webamp is
licensed under the MIT License:

```text
The MIT License (MIT)

Copyright (c) [2015] [Jordan Eldredge]

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
