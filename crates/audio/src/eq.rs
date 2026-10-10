//! Ten-band Winamp-style equalizer built from RBJ cookbook biquads.
//!
//! Band 1 (60 Hz) is a low shelf, band 10 (16 kHz) a high shelf, bands 2..=9 are
//! peaking filters with Q = 1.4. Bands at 0 dB are exact identity filters.

use crate::types::{EQ_BANDS_HZ, EQ_MAX_DB};
use std::f32::consts::PI;

const PEAK_Q: f32 = 1.4;
const SHELF_Q: f32 = std::f32::consts::FRAC_1_SQRT_2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Biquad {
    pub const IDENTITY: Biquad = Biquad {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    fn from_raw(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        Biquad {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    pub fn peaking(fs: f32, f0: f32, q: f32, gain_db: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0 / fs;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        Self::from_raw(
            1.0 + alpha * a,
            -2.0 * cos,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cos,
            1.0 - alpha / a,
        )
    }

    pub fn low_shelf(fs: f32, f0: f32, q: f32, gain_db: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0 / fs;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let sq = 2.0 * a.sqrt() * alpha;
        Self::from_raw(
            a * ((a + 1.0) - (a - 1.0) * cos + sq),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
            a * ((a + 1.0) - (a - 1.0) * cos - sq),
            (a + 1.0) + (a - 1.0) * cos + sq,
            -2.0 * ((a - 1.0) + (a + 1.0) * cos),
            (a + 1.0) + (a - 1.0) * cos - sq,
        )
    }

    pub fn high_shelf(fs: f32, f0: f32, q: f32, gain_db: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0 / fs;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let sq = 2.0 * a.sqrt() * alpha;
        Self::from_raw(
            a * ((a + 1.0) + (a - 1.0) * cos + sq),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
            a * ((a + 1.0) + (a - 1.0) * cos - sq),
            (a + 1.0) - (a - 1.0) * cos + sq,
            2.0 * ((a - 1.0) - (a + 1.0) * cos),
            (a + 1.0) - (a - 1.0) * cos - sq,
        )
    }

    /// Magnitude response in dB at `f` Hz.
    pub fn response_db(&self, fs: f32, f: f32) -> f32 {
        let w = 2.0 * PI * f / fs;
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let (nr, ni) = (
            self.b0 + self.b1 * c1 + self.b2 * c2,
            -(self.b1 * s1 + self.b2 * s2),
        );
        let (dr, di) = (
            1.0 + self.a1 * c1 + self.a2 * c2,
            -(self.a1 * s1 + self.a2 * s2),
        );
        10.0 * ((nr * nr + ni * ni) / (dr * dr + di * di)).log10()
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct State {
    z1: f32,
    z2: f32,
}

impl State {
    #[inline]
    fn run(&mut self, c: &Biquad, x: f32) -> f32 {
        // Transposed direct form II.
        let y = c.b0 * x + self.z1;
        self.z1 = c.b1 * x - c.a1 * y + self.z2;
        self.z2 = c.b2 * x - c.a2 * y;
        y
    }
}

/// Filter for band `index` (0..10) at `gain_db`. Bands too close to Nyquist become identity.
pub fn band_filter(fs: f32, index: usize, gain_db: f32) -> Biquad {
    let f0 = EQ_BANDS_HZ[index];
    let gain_db = gain_db.clamp(-EQ_MAX_DB, EQ_MAX_DB);
    if gain_db == 0.0 || f0 >= 0.45 * fs {
        return Biquad::IDENTITY;
    }
    match index {
        0 => Biquad::low_shelf(fs, f0, SHELF_Q, gain_db),
        9 => Biquad::high_shelf(fs, f0, SHELF_Q, gain_db),
        _ => Biquad::peaking(fs, f0, PEAK_Q, gain_db),
    }
}

/// Stereo ten-band equalizer.
pub struct Equalizer {
    fs: f32,
    coeffs: [Biquad; 10],
    state: [[State; 10]; 2],
    active: bool,
}

impl Equalizer {
    pub fn new(sample_rate: u32) -> Self {
        Equalizer {
            fs: sample_rate as f32,
            coeffs: [Biquad::IDENTITY; 10],
            state: [[State::default(); 10]; 2],
            active: false,
        }
    }

    /// Sets band gains in dB. Filter state is kept so changes do not click.
    pub fn set_bands(&mut self, bands_db: &[f32; 10]) {
        for (i, gain) in bands_db.iter().enumerate() {
            self.coeffs[i] = band_filter(self.fs, i, *gain);
        }
        self.active = self.coeffs.iter().any(|c| *c != Biquad::IDENTITY);
    }

    /// Processes interleaved stereo samples in place.
    pub fn process(&mut self, stereo: &mut [f32]) {
        if !self.active {
            return;
        }
        for frame in stereo.as_chunks_mut::<2>().0 {
            for (ch, sample) in frame.iter_mut().enumerate() {
                let mut x = *sample;
                for (st, c) in self.state[ch].iter_mut().zip(&self.coeffs) {
                    x = st.run(c, x);
                }
                *sample = x;
            }
        }
    }

    pub fn reset(&mut self) {
        self.state = [[State::default(); 10]; 2];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 44_100.0;

    #[test]
    fn zero_gain_is_identity() {
        for i in 0..10 {
            assert_eq!(band_filter(FS, i, 0.0), Biquad::IDENTITY);
        }
    }

    #[test]
    fn peaking_band_hits_gain_at_center() {
        for (i, &f0) in EQ_BANDS_HZ.iter().enumerate().take(9).skip(1) {
            let boost = band_filter(FS, i, 12.0).response_db(FS, f0);
            assert!((boost - 12.0).abs() < 0.05, "band {i}: {boost}");
            let cut = band_filter(FS, i, -12.0).response_db(FS, f0);
            assert!((cut + 12.0).abs() < 0.05, "band {i} cut: {cut}");
        }
    }

    #[test]
    fn low_shelf_boosts_deep_bass() {
        let f = band_filter(FS, 0, 12.0);
        assert!((f.response_db(FS, 20.0) - 12.0).abs() < 1.0);
        assert!(f.response_db(FS, 1_000.0).abs() < 0.5);
    }

    #[test]
    fn high_shelf_boosts_top_end() {
        let f = band_filter(FS, 9, 12.0);
        assert!((f.response_db(FS, 20_000.0) - 12.0).abs() < 1.5);
        assert!(f.response_db(FS, 1_000.0).abs() < 0.5);
    }

    #[test]
    fn gain_is_clamped_to_12_db() {
        assert_eq!(band_filter(FS, 4, 30.0), band_filter(FS, 4, 12.0));
    }

    #[test]
    fn band_above_nyquist_limit_is_identity() {
        assert_eq!(band_filter(22_050.0, 9, 12.0), Biquad::IDENTITY);
    }

    #[test]
    fn flat_equalizer_leaves_samples_untouched() {
        let mut eq = Equalizer::new(44_100);
        eq.set_bands(&[0.0; 10]);
        let mut buf: Vec<f32> = (0..64).map(|i| (i as f32 * 0.1).sin()).collect();
        let before = buf.clone();
        eq.process(&mut buf);
        assert_eq!(buf, before);
    }

    #[test]
    fn boosted_equalizer_amplifies_its_band() {
        let mut eq = Equalizer::new(44_100);
        let mut bands = [0.0; 10];
        bands[4] = 12.0; // 1 kHz
        eq.set_bands(&bands);
        let mut buf: Vec<f32> = (0..44_100)
            .flat_map(|i| {
                let s = 0.1 * (2.0 * PI * 1_000.0 * i as f32 / FS).sin();
                [s, s]
            })
            .collect();
        eq.process(&mut buf);
        // Skip 0.1 s of filter settling, then measure the peak.
        let peak = buf[2 * 4_410..].iter().fold(0f32, |m, s| m.max(s.abs()));
        let db = 20.0 * (peak / 0.1).log10();
        assert!((db - 12.0).abs() < 0.3, "{db}");
    }
}
