//! Developer tasks: `cargo xtask <task>`. The generators live in the library half (`lib.rs`)
//! so they can be tested.

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
