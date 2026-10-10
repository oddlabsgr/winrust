//! Decode-thread DSP: channel mapping, preamp, equalizer, balance.

use crate::eq::Equalizer;
use crate::types::{EQ_MAX_DB, EqSettings};
use crate::volume::balance_gains;

/// Frames over which a preamp change is ramped, to avoid clicks.
pub const RAMP_FRAMES: u32 = 256;

/// Appends `input` (interleaved, `channels` wide) to `out` as interleaved stereo.
///
/// Mono is duplicated. More than two channels: channels 0/1 are kept as left/right and the
/// rest are mixed equally into both, then the result is scaled so full-scale input stays
/// full-scale.
pub fn to_stereo(input: &[f32], channels: usize, out: &mut Vec<f32>) {
    match channels {
        0 => {}
        1 => out.extend(input.iter().flat_map(|&s| [s, s])),
        2 => out.extend_from_slice(input),
        n => {
            let extra = (n - 2) as f32;
            let norm = 1.0 / (1.0 + 0.5 * extra);
            for frame in input.chunks_exact(n) {
                let rest: f32 = frame[2..].iter().sum::<f32>() * 0.5;
                out.push((frame[0] + rest) * norm);
                out.push((frame[1] + rest) * norm);
            }
        }
    }
}

/// Linear gain ramp.
#[derive(Clone, Copy, Debug)]
struct Ramp {
    current: f32,
    target: f32,
    step: f32,
}

impl Ramp {
    fn new(value: f32) -> Self {
        Ramp {
            current: value,
            target: value,
            step: 0.0,
        }
    }

    fn set(&mut self, target: f32) {
        self.target = target;
        self.step = (target - self.current) / RAMP_FRAMES as f32;
    }

    #[inline]
    fn next(&mut self) -> f32 {
        if self.current != self.target {
            self.current += self.step;
            if (self.step > 0.0 && self.current >= self.target)
                || (self.step < 0.0 && self.current <= self.target)
            {
                self.current = self.target;
            }
        }
        self.current
    }
}

fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db.clamp(-EQ_MAX_DB, EQ_MAX_DB) / 20.0)
}

/// Preamp → equalizer → balance, on interleaved stereo at the device rate.
pub struct DspChain {
    preamp: Ramp,
    eq: Equalizer,
    eq_enabled: bool,
    balance: f32,
}

impl DspChain {
    pub fn new(sample_rate: u32) -> Self {
        DspChain {
            preamp: Ramp::new(1.0),
            eq: Equalizer::new(sample_rate),
            eq_enabled: false,
            balance: 0.0,
        }
    }

    /// Applies EQ settings. A disabled EQ also bypasses the preamp (Winamp behavior).
    pub fn set_eq(&mut self, settings: &EqSettings) {
        self.eq_enabled = settings.enabled;
        if settings.enabled {
            self.eq.set_bands(&settings.bands_db);
            self.preamp.set(db_to_gain(settings.preamp_db));
        } else {
            self.preamp.set(1.0);
        }
    }

    pub fn set_balance(&mut self, balance: f32) {
        self.balance = balance;
    }

    /// Processes interleaved stereo in place and clamps to [-1, 1].
    pub fn process(&mut self, stereo: &mut [f32]) {
        for frame in stereo.as_chunks_mut::<2>().0 {
            let g = self.preamp.next();
            frame[0] *= g;
            frame[1] *= g;
        }
        if self.eq_enabled {
            self.eq.process(stereo);
        }
        let (gl, gr) = balance_gains(self.balance);
        for frame in stereo.as_chunks_mut::<2>().0 {
            frame[0] = (frame[0] * gl).clamp(-1.0, 1.0);
            frame[1] = (frame[1] * gr).clamp(-1.0, 1.0);
        }
    }

    /// Clears filter memory (after seek/stop) without touching settings.
    pub fn reset(&mut self) {
        self.eq.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_is_duplicated() {
        let mut out = Vec::new();
        to_stereo(&[0.1, 0.2], 1, &mut out);
        assert_eq!(out, vec![0.1, 0.1, 0.2, 0.2]);
    }

    #[test]
    fn stereo_passes_through() {
        let mut out = Vec::new();
        to_stereo(&[0.1, 0.2, 0.3, 0.4], 2, &mut out);
        assert_eq!(out, vec![0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn surround_is_downmixed_without_clipping() {
        let mut out = Vec::new();
        to_stereo(&[1.0; 6], 6, &mut out);
        assert_eq!(out.len(), 2);
        assert!(
            (out[0] - 1.0).abs() < 1e-6 && (out[1] - 1.0).abs() < 1e-6,
            "{out:?}"
        );
    }

    #[test]
    fn default_chain_is_transparent() {
        let mut dsp = DspChain::new(44_100);
        let mut buf = vec![0.5, -0.25, 0.1, 0.9];
        dsp.process(&mut buf);
        assert_eq!(buf, vec![0.5, -0.25, 0.1, 0.9]);
    }

    #[test]
    fn preamp_ramps_instead_of_jumping() {
        let mut dsp = DspChain::new(44_100);
        dsp.set_eq(&EqSettings {
            enabled: true,
            preamp_db: -6.0,
            bands_db: [0.0; 10],
        });
        let mut buf = vec![1.0; 2 * RAMP_FRAMES as usize * 2];
        dsp.process(&mut buf);
        let left: Vec<f32> = buf.iter().step_by(2).copied().collect();
        assert!(left[0] > 0.99, "first frame barely changed: {}", left[0]);
        assert!(left.windows(2).all(|w| w[1] <= w[0]), "monotonic ramp");
        let target = db_to_gain(-6.0);
        assert!((left[left.len() - 1] - target).abs() < 1e-4);
    }

    #[test]
    fn disabled_eq_bypasses_preamp() {
        let mut dsp = DspChain::new(44_100);
        dsp.set_eq(&EqSettings {
            enabled: false,
            preamp_db: -12.0,
            bands_db: [12.0; 10],
        });
        let mut buf = vec![0.5; 2048];
        dsp.process(&mut buf);
        assert!(buf.iter().all(|s| *s == 0.5));
    }

    #[test]
    fn balance_and_clamp_are_applied() {
        let mut dsp = DspChain::new(44_100);
        dsp.set_balance(0.5);
        let mut buf = vec![0.8, 0.8, 3.0, -3.0];
        dsp.process(&mut buf);
        assert_eq!(buf, vec![0.4, 0.8, 1.0, -1.0]);
    }
}
