//! The visualizer: a spectrum analyzer or an oscilloscope drawn from the audio tap's latest
//! 576 frames, in `viscolor.txt` colours. A port of Webamp `VisPainter.ts` and
//! `FFTNullsoft.ts` at 999f567, quirks included (the 2-row push-down, zero peaks kept in
//! shade mode). The app calls [`Vis::frame`] once per display frame while playing.

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;
use winrust_skin::Image;

/// Frames per channel the audio tap provides (Winamp's visualization window).
pub const FRAMES: usize = 576;
const FFT_SIZE: usize = 1024;
const BINS: usize = 512;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VisMode {
    #[default]
    Spectrum,
    Oscilloscope,
    Off,
}

impl VisMode {
    /// What a click on the visualizer switches to.
    pub fn next(self) -> Self {
        match self {
            VisMode::Spectrum => VisMode::Oscilloscope,
            VisMode::Oscilloscope => VisMode::Off,
            VisMode::Off => VisMode::Spectrum,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Bandwidth {
    /// Bars three columns wide with a gap.
    #[default]
    Wide,
    Thin,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Coloring {
    /// Each row keeps its colour.
    #[default]
    Normal,
    /// The colours start at each bar's top.
    Fire,
    /// Each bar in the colour of its top row (Winamp's line style; Webamp draws fire here).
    Line,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OscStyle {
    Dots,
    #[default]
    Lines,
    Solid,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Falloff {
    Slower,
    Slow,
    #[default]
    Moderate,
    Fast,
    Faster,
}

impl Falloff {
    /// Sixteenths of a row a bar falls per frame.
    fn bars(self) -> f32 {
        match self {
            Falloff::Slower => 3.0,
            Falloff::Slow => 6.0,
            Falloff::Moderate => 12.0,
            Falloff::Fast => 16.0,
            Falloff::Faster => 32.0,
        }
    }

    /// How much faster a peak falls each frame.
    fn peaks(self) -> f32 {
        match self {
            Falloff::Slower => 1.05,
            Falloff::Slow => 1.1,
            Falloff::Moderate => 1.2,
            Falloff::Fast => 1.4,
            Falloff::Faster => 1.6,
        }
    }
}

/// Webamp's defaults (`Vis.tsx`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisSettings {
    pub mode: VisMode,
    pub bandwidth: Bandwidth,
    pub coloring: Coloring,
    pub osc_style: OscStyle,
    pub peaks: bool,
    pub falloff: Falloff,
    pub peak_falloff: Falloff,
}

impl Default for VisSettings {
    fn default() -> Self {
        VisSettings {
            mode: VisMode::Spectrum,
            bandwidth: Bandwidth::Wide,
            coloring: Coloring::Normal,
            osc_style: OscStyle::Lines,
            peaks: true,
            falloff: Falloff::Moderate,
            peak_falloff: Falloff::Slow,
        }
    }
}

pub struct Vis {
    pub settings: VisSettings,
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    equalize: Vec<f32>,
    /// Per column: the falling bar, the peak (in 1/256 rows) and the peak's fall speed.
    bars: [f32; 76],
    peaks: [i32; 76],
    peak_speed: [f32; 76],
    /// The mode and size the state above belongs to.
    last: Option<(VisMode, bool)>,
}

impl Vis {
    pub fn new(settings: VisSettings) -> Self {
        // FFTNullsoft's envelope is a Hann window; here it spans the 576 frames.
        let window = (0..FRAMES)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / FRAMES as f32).cos())
            .collect();
        // FFTNullsoft's equalization: lifts the high bins on a log curve.
        let mut bias = 0.04f32;
        let equalize = (0..BINS)
            .map(|i| {
                let step = (9.0 - bias) / BINS as f32;
                let e = (1.0 + bias + (i + 1) as f32 * step).log10();
                bias /= 1.0025;
                e
            })
            .collect();
        Vis {
            settings,
            fft: FftPlanner::new().plan_fft_forward(FFT_SIZE),
            window,
            equalize,
            bars: [0.0; 76],
            peaks: [0; 76],
            peak_speed: [0.0; 76],
            last: None,
        }
    }

    /// Draws one frame from the latest [`FRAMES`] frames of each channel: 76×16, or 38×5
    /// when `small` (shade mode). `None` when the mode is off.
    pub fn frame(
        &mut self,
        left: &[f32],
        right: &[f32],
        colors: &[u32; 24],
        small: bool,
    ) -> Option<Image> {
        let mode = self.settings.mode;
        if mode == VisMode::Off {
            return None;
        }
        if self.last != Some((mode, small)) {
            self.bars = [0.0; 76];
            self.peaks = [0; 76];
            self.peak_speed = [0.0; 76];
            self.last = Some((mode, small));
        }
        let bytes = time_bytes(left, right);
        let mut img = background(colors, small);
        match mode {
            VisMode::Spectrum => self.spectrum(&bytes, colors, small, &mut img),
            VisMode::Oscilloscope => self.oscilloscope(&bytes, colors, small, &mut img),
            VisMode::Off => {}
        }
        Some(img)
    }

    fn spectrum(&mut self, bytes: &[u8; FRAMES], colors: &[u32; 24], small: bool, img: &mut Image) {
        let mut buf = vec![Complex::new(0.0f32, 0.0); FFT_SIZE];
        // Webamp feeds 1024 samples; the scale keeps a 576-frame window's magnitudes the same.
        let scale = FFT_SIZE as f32 / FRAMES as f32 / 24.0;
        for (i, b) in bytes.iter().enumerate() {
            buf[i].re = (f32::from(*b) - 128.0) * scale * self.window[i];
        }
        self.fft.process(&mut buf);
        let spectral: Vec<f32> = (0..BINS)
            .map(|i| buf[i].norm() * self.equalize[i])
            .collect();

        let (target, max_height, max_width) = if small { (40, 5.0, 37) } else { (75, 15.0, 75) };
        let push_down = if small { 0 } else { 2 };
        let mut sample = [0.0f32; 76];
        let log_max = (BINS as f32).log10();
        for (x, s) in sample.iter_mut().enumerate().take(target) {
            // Webamp blends a linear and a log scale (0.91 log) to place bins on columns.
            let t = x as f32 / (target - 1) as f32;
            let linear = t * (BINS - 1) as f32;
            let log = 10f32.powf(log_max * t);
            let index = 0.09 * linear + 0.91 * log;
            let i1 = (index.floor() as usize).min(BINS - 1);
            let i2 = (index.ceil() as usize).min(BINS - 1);
            *s = if i1 == i2 {
                spectral[i1]
            } else {
                let f = index - i1 as f32;
                (1.0 - f) * spectral[i1] + f * spectral[i2]
            };
        }

        let wide = self.settings.bandwidth == Bandwidth::Wide;
        let falloff = self.settings.falloff.bars() / 16.0;
        let peak_falloff = self.settings.peak_falloff.peaks();
        for x in 0..max_width {
            let chunk = if wide { x & !3 } else { 0 };
            let data = if wide {
                (sample[chunk..chunk + 4].iter().sum::<f32>() / 4.0).trunc()
            } else {
                sample[x].trunc()
            }
            .min(max_height);
            self.peaks[x] = self.peaks[x].min(max_height as i32 * 256);
            self.bars[x] -= falloff;
            if self.bars[x] <= data {
                self.bars[x] = data;
            }
            if self.peaks[x] <= js_round(self.bars[x] * 256.0) {
                self.peaks[x] = (self.bars[x] * 256.0) as i32;
                self.peak_speed[x] = 3.0;
            }
            let mut bar_peak = self.peaks[x] / 256;
            self.peaks[x] -= js_round(self.peak_speed[x]);
            self.peak_speed[x] *= peak_falloff;
            if self.peaks[x] <= 0 {
                self.peaks[x] = 0;
            }
            // Webamp keeps zero peaks visible in the small display only.
            if !small && bar_peak < 1 {
                bar_peak = -3;
            }
            if !(wide && x == chunk + 3) {
                let height = js_round(self.bars[x]) - push_down;
                self.paint_bar(
                    img,
                    colors,
                    small,
                    x as u32,
                    height,
                    bar_peak + 1 - push_down,
                );
            }
        }
    }

    fn paint_bar(
        &self,
        img: &mut Image,
        colors: &[u32; 24],
        small: bool,
        x: u32,
        height: i32,
        peak: i32,
    ) {
        let h = img.height as i32;
        // Bar colour by row: viscolor 0-15 (Webamp's push-down shifts them two rows), or five
        // of them in the small display.
        let row_color = |row: i32| -> u32 {
            if small {
                colors[[4, 8, 11, 14, 17][row.clamp(0, 4) as usize]]
            } else {
                colors[row.clamp(0, 15) as usize]
            }
        };
        let top = h - height;
        for y in top.max(0)..h {
            let c = match self.settings.coloring {
                Coloring::Normal => row_color(y),
                Coloring::Fire => row_color(y - top),
                Coloring::Line => row_color(top),
            };
            img.set(x, y as u32, c);
        }
        let peak_y = h - peak;
        if self.settings.peaks && (0..h).contains(&peak_y) {
            img.set(x, peak_y as u32, colors[23]);
        }
    }

    fn oscilloscope(
        &mut self,
        bytes: &[u8; FRAMES],
        colors: &[u32; 24],
        small: bool,
        img: &mut Image,
    ) {
        let width = if small { 38 } else { 75 };
        let push_down = if small { 0 } else { 2 };
        let slice = FRAMES / 75;
        let mut last_y = 0;
        for x in 0..width {
            let amplitude = i32::from(bytes[slice * x]);
            let mut y = js_round(amplitude as f32 / 8.0) - 9;
            if small {
                y -= 5;
                y = js_round((y + 11) as f32 / 16.0 * 5.0) - 2;
            }
            let y = y.clamp(0, img.height as i32 - 1);
            if x == 0 {
                last_y = y;
            }
            let (mut top, mut bottom) = (y, last_y);
            last_y = y;
            match self.settings.osc_style {
                OscStyle::Solid => {
                    let middle = if small { 2 } else { 8 };
                    (top, bottom) = if y >= middle {
                        (middle, y)
                    } else {
                        (y, if small { 2 } else { 7 })
                    };
                    if x == 0 && small {
                        (top, bottom) = (y, y);
                    }
                }
                OscStyle::Dots => bottom = y,
                OscStyle::Lines => {
                    if bottom < top {
                        (top, bottom) = (bottom, top);
                        if !small {
                            top += 1;
                        }
                    }
                }
            }
            let c = colors[18 + scope_color(y, small)];
            for row in top..=bottom {
                img.set(x as u32, (row + push_down) as u32, c);
            }
        }
    }
}

/// The tap as Web Audio's byte time-domain data: mono, 128 ± 128.
fn time_bytes(left: &[f32], right: &[f32]) -> [u8; FRAMES] {
    let mut out = [128u8; FRAMES];
    for (i, b) in out.iter_mut().enumerate() {
        let l = left.get(i).copied().unwrap_or(0.0);
        let r = right.get(i).copied().unwrap_or(0.0);
        *b = (128.0 * (1.0 + (l + r) / 2.0)).floor().clamp(0.0, 255.0) as u8;
    }
    out
}

/// Colour 0 dotted with colour 1 on every other pixel and row; plain in the small display.
fn background(colors: &[u32; 24], small: bool) -> Image {
    if small {
        return Image::new(38, 5, colors[0]);
    }
    let mut img = Image::new(76, 16, colors[0]);
    for y in (1..16).step_by(2) {
        for x in (0..76).step_by(2) {
            img.set(x, y, colors[1]);
        }
    }
    img
}

/// Webamp's oscilloscope colour (0..=4 into viscolor 18-22) for a row.
fn scope_color(y: i32, small: bool) -> usize {
    match y {
        _ if small => 0,
        14.. => 4,
        12.. => 3,
        10.. => 2,
        8.. => 1,
        6.. => 0,
        4.. => 1,
        2.. => 2,
        _ => 3,
    }
}

/// JavaScript's `Math.round`: halves round up.
fn js_round(v: f32) -> i32 {
    (v + 0.5).floor() as i32
}
