//! Decoding of every supported format, using fixtures from `cargo xtask gen-fixtures`.

use std::path::{Path, PathBuf};
use std::time::Duration;
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

#[test]
fn aac_priming_and_padding_are_trimmed() {
    // symphonia does not apply MP4 edit lists; Decoder must, or every M4A starts with
    // ~23 ms of encoder priming and ends with padding (and AAC can never be gapless).
    let mut dec = Decoder::open_file(&fixture("tone.m4a")).unwrap();
    let dur = dec.info().duration.unwrap();
    assert!((dur.as_secs_f64() - 1.0).abs() < 0.001, "{dur:?}");
    let mut out = Vec::new();
    while dec.decode_next(&mut out).unwrap() {}
    assert_eq!(out.len() / 2, 44_100);
    // The sine starts at zero and rises, exactly like ffmpeg's reference decode.
    assert!(
        out[0].abs() < 0.005 && out[2] > out[0],
        "starts with {:?}",
        &out[..6]
    );
    let crossings = rising_zero_crossings(&left(&out));
    assert!((439..=441).contains(&crossings), "{crossings} crossings");
}

#[test]
fn aac_seek_lands_on_audio_time() {
    let mut dec = Decoder::open_file(&fixture("tone.m4a")).unwrap();
    let pos = dec.seek(Duration::from_millis(500)).unwrap();
    assert!((pos.as_secs_f64() - 0.5).abs() < 0.001, "{pos:?}");
    let mut out = Vec::new();
    while dec.decode_next(&mut out).unwrap() {}
    let frames = (out.len() / 2) as i64;
    assert!((frames - 22_050).abs() <= 2, "{frames} frames after seek");
}
