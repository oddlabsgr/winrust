use std::f32::consts::PI;
use std::path::Path;
use std::time::Duration;
use winrust_audio::decoder::Decoder;

fn write_tone(path: &Path, rate: u32, channels: u16, secs: f32) {
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    let n = (rate as f32 * secs) as usize;
    for i in 0..n {
        let s = (0.5 * (2.0 * PI * 440.0 * i as f32 / rate as f32).sin() * i16::MAX as f32) as i16;
        for _ in 0..channels {
            w.write_sample(s).unwrap();
        }
    }
    w.finalize().unwrap();
}

fn decode_all(dec: &mut Decoder) -> Vec<f32> {
    let mut out = Vec::new();
    while dec.decode_next(&mut out).unwrap() {}
    out
}

#[test]
fn decodes_mono_wav_to_stereo() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mono.wav");
    write_tone(&path, 22_050, 1, 0.5);
    let mut dec = Decoder::open_file(&path).unwrap();
    assert_eq!(dec.info().sample_rate, 22_050);
    assert_eq!(dec.info().channels, 1);
    assert!(dec.info().seekable);
    let d = dec.info().duration.unwrap();
    assert!((d.as_secs_f64() - 0.5).abs() < 0.001, "{d:?}");
    let out = decode_all(&mut dec);
    assert_eq!(out.len(), 2 * 11_025);
    assert_eq!(out[200], out[201], "mono duplicated to both channels");
}

#[test]
fn seek_lands_on_requested_time() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("seek.wav");
    write_tone(&path, 44_100, 2, 2.0);
    let mut dec = Decoder::open_file(&path).unwrap();
    let pos = dec.seek(Duration::from_millis(1500)).unwrap();
    assert!((pos.as_secs_f64() - 1.5).abs() < 0.001, "{pos:?}");
    let out = decode_all(&mut dec);
    let frames = out.len() / 2;
    assert!(
        (frames as i64 - 22_050).abs() <= 1,
        "frames after seek = {frames}"
    );
}

#[test]
fn missing_file_is_open_error() {
    let err = Decoder::open_file(Path::new("definitely/not/here.mp3"))
        .err()
        .unwrap();
    assert!(
        matches!(err, winrust_audio::types::EngineError::Open { .. }),
        "{err:?}"
    );
}

#[test]
fn garbage_file_is_unsupported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("garbage.mp3");
    std::fs::write(&path, vec![0x42u8; 4096]).unwrap();
    let err = Decoder::open_file(&path).err().unwrap();
    assert!(
        matches!(err, winrust_audio::types::EngineError::Unsupported(_)),
        "{err:?}"
    );
}
