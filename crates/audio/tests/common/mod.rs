#![allow(dead_code)]

use crossbeam_channel::Receiver;
use std::f32::consts::PI;
use std::path::Path;
use std::time::{Duration, Instant};
use winrust_audio::EngineEvent;

/// 16-bit samples of a 440 Hz sine at half scale, frames `from..to`, every channel equal.
pub fn sine_i16(rate: u32, from: usize, to: usize) -> Vec<i16> {
    (from..to)
        .map(|i| {
            (0.5 * (2.0 * PI * 440.0 * i as f32 / rate as f32).sin() * f32::from(i16::MAX)) as i16
        })
        .collect()
}

pub fn write_wav(path: &Path, rate: u32, channels: u16, frames: &[i16]) {
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for &s in frames {
        for _ in 0..channels {
            w.write_sample(s).unwrap();
        }
    }
    w.finalize().unwrap();
}

/// WAV file bytes in memory.
pub fn wav_bytes(rate: u32, frames: &[i16]) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = std::io::Cursor::new(Vec::new());
    let mut w = hound::WavWriter::new(&mut cursor, spec).unwrap();
    for &s in frames {
        w.write_sample(s).unwrap();
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();
    cursor.into_inner()
}

/// Same conversion symphonia uses for 16-bit PCM.
pub fn to_f32(s: i16) -> f32 {
    f32::from(s) / 32_768.0
}

/// Collects events until `done` matches one (inclusive) or `timeout` passes.
pub fn wait_for(
    events: &Receiver<EngineEvent>,
    timeout: Duration,
    done: impl Fn(&EngineEvent) -> bool,
) -> Vec<EngineEvent> {
    let deadline = Instant::now() + timeout;
    let mut seen = Vec::new();
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match events.recv_timeout(left) {
            Ok(e) => {
                let stop = done(&e);
                seen.push(e);
                if stop {
                    return seen;
                }
            }
            Err(_) => break,
        }
    }
    panic!("timed out; events so far: {seen:#?}");
}
