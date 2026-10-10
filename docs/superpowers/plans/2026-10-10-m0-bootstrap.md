# M0 Bootstrap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the empty repository into a Cargo workspace with a stub `winrust` binary, an `xtask` runner, dual licensing, a cargo-deny policy and a CI pipeline that is green on Windows, macOS and Linux.

**Architecture:** A workspace root whose members are `crates/*` and `xtask`. In M0 only `crates/app` (the `winrust` binary) exists; later milestones add `crates/audio`, `crates/cli`, `crates/skin`, `crates/ui` and `crates/playlist` as they are needed. Version, edition, license and `publish = false` live in `[workspace.package]` and every crate inherits them. CI has three jobs: rustfmt, a Windows/macOS/Linux matrix running clippy (`-D warnings`) and tests, and cargo-deny.

**Tech Stack:** Rust stable (edition 2024), Cargo workspaces (resolver 3), GitHub Actions, cargo-deny 0.20.

**Spec:** `docs/superpowers/specs/2026-10-10-winrust-design.md` — §4 (licensing), §5.1 (workspace layout), §11 (lint gates), §12.1 (CI), §12.3 (M0 row).

## Global Constraints

- Every crate is licensed `MIT OR Apache-2.0` (via `license.workspace = true`) and has `publish = false`.
- Product name is "Winrust". "Winamp" appears only descriptively (e.g. "Winamp-compatible skins"). No Winamp artwork, ever.
- Dependency licenses are limited to the allowlist in `deny.toml`: 0BSD, Apache-2.0, Apache-2.0 WITH LLVM-exception, BSD-2-Clause, BSD-3-Clause, CDLA-Permissive-2.0, ISC, MIT, MPL-2.0, Unicode-3.0, Unlicense, Zlib. No GPL or LGPL.
- Edition 2024, stable toolchain.
- Every commit passes `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`.
- Text files are stored with LF line endings (`.gitattributes`).
- Commit messages end with the trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Creating the GitHub repository and pushing to it are outward-facing: they require the user's explicit approval at the moment they happen (Task 3).
- Shell commands below are bash (Git Bash on Windows). Run them from the repository root.

## Review Focus

M0 has no user-facing behavior, so the list is short. These are the conditions most likely to break contributors:

- **Windows checkout with `core.autocrlf=true`** (this machine has it): files must stay LF in the index, otherwise `cargo fmt --check` and diffs churn across OSes. Task 1 Step 7 checks `git ls-files --eol`.
- **Typo in an xtask name:** `cargo xtask nope` must print the help and exit non-zero, not silently succeed. Task 1 Step 6 checks it.
- **Contributor without rustfmt/clippy installed:** `rust-toolchain.toml` makes rustup install both components automatically. Task 1 Step 8 checks it.

---

### Task 1: Workspace skeleton, `winrust` stub binary, `xtask`

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `.cargo/config.toml`
- Create: `.gitattributes`
- Create: `crates/app/Cargo.toml`
- Create: `crates/app/src/main.rs`
- Create: `xtask/Cargo.toml`
- Create: `xtask/src/main.rs`
- Existing, unchanged: `.gitignore` (already ignores `/target` and fuzz output)

**Interfaces:**
- Consumes: nothing.
- Produces:
  - Workspace members glob `crates/*` plus `xtask`; new crates only need a directory under `crates/` and `version.workspace = true`, `edition.workspace = true`, `license.workspace = true`, `publish.workspace = true` in their `[package]`.
  - Cargo alias `cargo xtask <task>` → `cargo run --quiet --package xtask -- <task>`.
  - `xtask` dispatches on its first argument with a `match task.as_deref()`; M1 adds a `gen-fixtures` arm.
  - Binary `winrust` (package `winrust`, `crates/app`) with private `fn banner() -> String` returning `"Winrust <version>"`.

- [ ] **Step 1: Create the workspace manifest and toolchain file**

`Cargo.toml`:

```toml
[workspace]
resolver = "3"
members = ["crates/*", "xtask"]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT OR Apache-2.0"
publish = false
```

`rust-toolchain.toml`:

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 2: Write the failing test for the stub binary**

`crates/app/Cargo.toml`:

```toml
[package]
name = "winrust"
description = "Winrust: a Winamp 2.x-style music player"
version.workspace = true
edition.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
```

`crates/app/src/main.rs` (test only, `banner` does not exist yet):

```rust
fn main() {}

#[cfg(test)]
mod tests {
    #[test]
    fn banner_names_product_and_version() {
        assert_eq!(super::banner(), format!("Winrust {}", env!("CARGO_PKG_VERSION")));
    }
}
```

Note: the workspace also lists `xtask`, which does not exist yet. Create an empty placeholder so Cargo can load the workspace — Step 5 replaces it:

```bash
mkdir -p xtask/src
printf '[package]\nname = "xtask"\nversion.workspace = true\nedition.workspace = true\nlicense.workspace = true\npublish.workspace = true\n' > xtask/Cargo.toml
printf 'fn main() {}\n' > xtask/src/main.rs
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p winrust`
Expected: compile error ``error[E0425]: cannot find function `banner` in module `super` ``.

- [ ] **Step 4: Implement the stub binary**

`crates/app/src/main.rs`:

```rust
//! Winrust desktop app. The skinned UI arrives in milestone M3; until then this only reports
//! its version so the workspace, CI and release plumbing have a binary to build.

fn main() {
    println!("{}", banner());
}

fn banner() -> String {
    format!("Winrust {}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn banner_names_product_and_version() {
        assert_eq!(
            super::banner(),
            format!("Winrust {}", env!("CARGO_PKG_VERSION"))
        );
    }
}
```

Run: `cargo test -p winrust`
Expected: `test tests::banner_names_product_and_version ... ok`.

Run: `cargo run -q -p winrust`
Expected output: `Winrust 0.1.0`

- [ ] **Step 5: Add the xtask runner**

`xtask/Cargo.toml`:

```toml
[package]
name = "xtask"
version.workspace = true
edition.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
```

`xtask/src/main.rs`:

```rust
//! Developer tasks: `cargo xtask <task>`. Dependency-free on purpose so it builds instantly.

use std::process::ExitCode;

const HELP: &str = "\
Usage: cargo xtask <task>

Tasks:
  help    Show this message
";

fn main() -> ExitCode {
    let task = std::env::args().nth(1);
    match task.as_deref() {
        None | Some("help") | Some("--help") | Some("-h") => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unknown task: {other}\n\n{HELP}");
            ExitCode::FAILURE
        }
    }
}
```

`.cargo/config.toml`:

```toml
[alias]
xtask = "run --quiet --package xtask --"
```

- [ ] **Step 6: Check the xtask behavior, including a typo**

Run: `cargo xtask`
Expected:
```text
Usage: cargo xtask <task>

Tasks:
  help    Show this message
```

Run: `cargo xtask nope; echo "exit=$?"`
Expected: `unknown task: nope`, the help text, then `exit=1`.

- [ ] **Step 7: Force LF line endings and verify the index**

`.gitattributes`:

```text
* text=auto eol=lf
*.bmp binary
*.wsz binary
*.png binary
*.mp3 binary
*.flac binary
*.ogg binary
*.m4a binary
*.wav binary
```

Run:
```bash
git add --renormalize .
git add -A
git ls-files --eol | grep -v "i/lf" || echo "all text files are LF"
```
Expected: `all text files are LF` (the only exceptions allowed are entries marked `-text`, i.e. binaries; there are none yet).

- [ ] **Step 8: Verify the toolchain components and all gates**

Run: `rustup show active-toolchain && rustup component list --installed | grep -E "rustfmt|clippy"`
Expected: an active `stable-...` toolchain that names `rust-toolchain.toml` as the reason, and both `clippy-...` and `rustfmt-...` listed.

Run:
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Expected: no fmt output, no clippy warnings, `test result: ok. 1 passed`.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml Cargo.lock rust-toolchain.toml .cargo/config.toml .gitattributes crates/app xtask
git commit -m "build: set up Cargo workspace with winrust stub and xtask" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Licenses, README, third-party notice, cargo-deny policy

**Files:**
- Create: `LICENSE-APACHE` (downloaded, checksum-verified)
- Create: `LICENSE-MIT`
- Create: `README.md`
- Create: `THIRD_PARTY.md`
- Create: `deny.toml`

**Interfaces:**
- Consumes: workspace from Task 1.
- Produces: `deny.toml` license allowlist that later milestones' dependencies must satisfy; `THIRD_PARTY.md` that later milestones append to (fonts, sprite table reference).

- [ ] **Step 1: Install cargo-deny if missing**

Run: `cargo deny --version || cargo install cargo-deny --locked`
Expected: `cargo-deny 0.20.x` (installation takes a few minutes the first time).

- [ ] **Step 2: Run the check without a policy to see it fail**

Run: `cargo deny check licenses`
Expected: FAIL — without `deny.toml` no license is allowed, so the workspace crates' `MIT OR Apache-2.0` is rejected (`error[rejected]`).

- [ ] **Step 3: Write the policy**

`deny.toml`:

```toml
# cargo-deny configuration. Run: cargo deny check

[graph]
all-features = false

[advisories]
yanked = "deny"

[licenses]
# Permissive licenses only. MPL-2.0 (symphonia, triple_buffer) is file-level copyleft and
# compatible with distributing Winrust under MIT OR Apache-2.0.
# CDLA-Permissive-2.0 covers the webpki-roots certificate data.
allow = [
    "0BSD",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "CDLA-Permissive-2.0",
    "ISC",
    "MIT",
    "MPL-2.0",
    "Unicode-3.0",
    "Unlicense",
    "Zlib",
]
confidence-threshold = 0.9

[licenses.private]
ignore = true

[bans]
multiple-versions = "warn"
wildcards = "deny"
allow-wildcard-paths = true

[sources]
unknown-registry = "deny"
unknown-git = "deny"
```

Run: `cargo deny check`
Expected: `advisories ok, bans ok, licenses ok, sources ok`.

- [ ] **Step 4: Add the license texts**

```bash
curl -fsSL https://www.apache.org/licenses/LICENSE-2.0.txt -o LICENSE-APACHE
sha256sum LICENSE-APACHE
```
Expected checksum: `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30`. If it differs, stop and inspect the file before continuing (the canonical text has 202 lines and starts with "Apache License / Version 2.0, January 2004").

`LICENSE-MIT`:

```text
MIT License

Copyright (c) 2026 The Winrust Contributors

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

- [ ] **Step 5: Write the README and the third-party notice**

`README.md`:

````markdown
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
````

`THIRD_PARTY.md`:

```markdown
# Third-party material

## Rust dependencies

All Rust dependencies use permissive licenses. `cargo deny check` enforces the allowlist in
[`deny.toml`](deny.toml); `cargo deny list` prints the license of every crate.

Notable: the symphonia decoder crates are licensed MPL-2.0, a file-level copyleft license.
Winrust uses them unmodified from crates.io, where their source is available.

## Assets and references

None yet. Fonts, skin layout references (such as Webamp's MIT-licensed sprite coordinate
table) and other material will be listed here as milestones add them.
```

- [ ] **Step 6: Verify and commit**

```bash
git add LICENSE-APACHE LICENSE-MIT README.md THIRD_PARTY.md deny.toml
cargo deny check
git ls-files --eol | grep -v "i/lf" || echo "all text files are LF"
```
Expected: `advisories ok, bans ok, licenses ok, sources ok`, then `all text files are LF`.

```bash
git commit -m "docs: add dual license, README, third-party notice and cargo-deny policy" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: CI workflow and first green run on GitHub

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: gates from Tasks 1–2.
- Produces: CI jobs `fmt`, `test (windows-latest|macos-latest|ubuntu-latest)`, `deny`. M1 adds a Linux-only ALSA install step to the `test` job.

- [ ] **Step 1: Write the workflow**

`.github/workflows/ci.yml`:

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
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace

  deny:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v2
```

- [ ] **Step 2: Commit locally**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add fmt, 3-OS clippy/test matrix and cargo-deny" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 3: STOP — get approval to publish**

Creating a public GitHub repository and pushing is outward-facing. Ask the user, and wait for an explicit yes:

> "Ready to create the public GitHub repo and push `main` so CI runs. `gh` is logged in as `oddlabsgr`. Create `oddlabsgr/winrust` (public), or a different owner/name?"

Do not continue to Step 4 without that answer.

- [ ] **Step 4: Create the repository and push (only after approval)**

Using the owner/name the user approved (example shown with `oddlabsgr/winrust`):

```bash
gh repo create oddlabsgr/winrust --public --source . --remote origin \
  --description "Cross-platform clone of the classic Winamp 2.x player, in Rust"
git push -u origin main
```

- [ ] **Step 5: Watch CI until it is green**

Run: `gh run watch --exit-status $(gh run list --limit 1 --json databaseId --jq '.[0].databaseId')`
Expected: jobs `fmt`, `test (windows-latest)`, `test (macos-latest)`, `test (ubuntu-latest)` and `deny` all succeed, exit code 0.

If a job fails: read its log with `gh run view --log-failed`, fix the cause locally (use superpowers:systematic-debugging), commit, push, and watch again. M0 is done only when the run is fully green.
