//! Streaming stereo sample-rate conversion (rubato sinc), or pass-through when rates match.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Async, FixedAsync, Indexing, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};

const CHUNK_FRAMES: usize = 1024;

pub struct StreamResampler {
    inner: Option<Async<f32>>,
    /// Interleaved stereo input waiting for a full chunk.
    pending: Vec<f32>,
    scratch: Vec<f32>,
    /// Output frames still to drop to compensate the resampler's delay.
    skip_frames: usize,
}

impl StreamResampler {
    /// Converts stereo audio from `from` Hz to `to` Hz.
    pub fn new(from: u32, to: u32) -> Self {
        let inner = (from != to).then(|| {
            let params = SincInterpolationParameters::new(128, WindowFunction::Blackman2)
                .oversampling_factor(256)
                .interpolation(SincInterpolationType::Quadratic);
            Async::<f32>::new_sinc(
                f64::from(to) / f64::from(from),
                1.0,
                &params,
                CHUNK_FRAMES,
                2,
                FixedAsync::Input,
            )
            .expect("valid resampler parameters")
        });
        let mut r = StreamResampler {
            inner,
            pending: Vec::new(),
            scratch: Vec::new(),
            skip_frames: 0,
        };
        r.reset();
        r
    }

    pub fn is_passthrough(&self) -> bool {
        self.inner.is_none()
    }

    /// Feeds interleaved stereo `input`; appends converted interleaved stereo to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let Some(inner) = self.inner.as_mut() else {
            out.extend_from_slice(input);
            return;
        };
        self.pending.extend_from_slice(input);
        loop {
            let need = inner.input_frames_next();
            if self.pending.len() / 2 < need {
                break;
            }
            let produced = run_chunk(inner, &self.pending[..need * 2], None, &mut self.scratch);
            self.pending.drain(..need * 2);
            emit(&self.scratch[..produced * 2], &mut self.skip_frames, out);
        }
    }

    /// Pushes out everything still buffered (end of stream). Then resets.
    pub fn flush(&mut self, out: &mut Vec<f32>) {
        if let Some(inner) = self.inner.as_mut() {
            let frames = self.pending.len() / 2;
            if frames > 0 {
                let need = inner.input_frames_next();
                let mut chunk = std::mem::take(&mut self.pending);
                chunk.resize(need * 2, 0.0);
                let produced = run_chunk(inner, &chunk, Some(frames), &mut self.scratch);
                let ratio = inner.resample_ratio();
                let keep = ((frames as f64 * ratio).round() as usize).min(produced);
                emit(&self.scratch[..keep * 2], &mut self.skip_frames, out);
            }
        }
        self.reset();
    }

    /// Drops buffered audio and filter memory (after seek or stop).
    pub fn reset(&mut self) {
        self.pending.clear();
        if let Some(inner) = self.inner.as_mut() {
            inner.reset();
            self.skip_frames = inner.output_delay();
        }
    }
}

fn run_chunk(
    inner: &mut Async<f32>,
    input: &[f32],
    partial: Option<usize>,
    scratch: &mut Vec<f32>,
) -> usize {
    let in_frames = input.len() / 2;
    let out_cap = inner.output_frames_max();
    scratch.resize(out_cap * 2, 0.0);
    let in_adapter = InterleavedSlice::new(input, 2, in_frames).expect("input adapter");
    let mut out_adapter =
        InterleavedSlice::new_mut(&mut scratch[..], 2, out_cap).expect("output adapter");
    let mut indexing = Indexing::new();
    indexing.partial_len = partial;
    let (_, produced) = inner
        .process_into_buffer(&in_adapter, &mut out_adapter, Some(&indexing))
        .expect("resample chunk");
    produced
}

fn emit(frames: &[f32], skip_frames: &mut usize, out: &mut Vec<f32>) {
    let n = frames.len() / 2;
    let skip = (*skip_frames).min(n);
    *skip_frames -= skip;
    out.extend_from_slice(&frames[skip * 2..]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn sine(rate: u32, freq: f32, secs: f32) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        (0..n)
            .flat_map(|i| {
                let s = 0.5 * (2.0 * PI * freq * i as f32 / rate as f32).sin();
                [s, s]
            })
            .collect()
    }

    fn zero_crossings(stereo: &[f32]) -> usize {
        let left: Vec<f32> = stereo.iter().step_by(2).copied().collect();
        left.windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count()
    }

    #[test]
    fn equal_rates_pass_through() {
        let mut r = StreamResampler::new(48_000, 48_000);
        assert!(r.is_passthrough());
        let mut out = Vec::new();
        r.process(&[0.1, 0.2], &mut out);
        assert_eq!(out, vec![0.1, 0.2]);
    }

    #[test]
    fn upsampling_keeps_length_and_pitch() {
        let input = sine(44_100, 440.0, 1.0);
        let mut r = StreamResampler::new(44_100, 48_000);
        let mut out = Vec::new();
        for chunk in input.chunks(2 * 700) {
            r.process(chunk, &mut out);
        }
        r.flush(&mut out);
        let frames = out.len() / 2;
        assert!((frames as i64 - 48_000).abs() < 200, "frames = {frames}");
        let crossings = zero_crossings(&out);
        assert!(
            (crossings as i64 - 440).abs() <= 2,
            "crossings = {crossings}"
        );
    }

    #[test]
    fn downsampling_keeps_length_and_pitch() {
        let input = sine(96_000, 440.0, 1.0);
        let mut r = StreamResampler::new(96_000, 48_000);
        let mut out = Vec::new();
        for chunk in input.chunks(2 * 999) {
            r.process(chunk, &mut out);
        }
        r.flush(&mut out);
        let frames = out.len() / 2;
        assert!((frames as i64 - 48_000).abs() < 200, "frames = {frames}");
        let crossings = zero_crossings(&out);
        assert!(
            (crossings as i64 - 440).abs() <= 2,
            "crossings = {crossings}"
        );
    }

    #[test]
    fn reset_discards_pending_audio() {
        let mut r = StreamResampler::new(44_100, 48_000);
        let mut out = Vec::new();
        r.process(&sine(44_100, 440.0, 0.01), &mut out);
        r.reset();
        r.flush(&mut out);
        assert!(out.is_empty());
    }
}
