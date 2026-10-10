//! The audio-callback side: pulls stereo frames from the ring, applies volume, feeds the analyzer.
//!
//! Real-time rules: no allocation, no blocking, no panics in `render`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// Samples per channel kept for visualization (Winamp's window size).
pub const SCOPE_FRAMES: usize = 576;

/// Latest audio for visualizations, oldest sample first.
#[derive(Clone, Debug, PartialEq)]
pub struct Scope {
    pub left: [f32; SCOPE_FRAMES],
    pub right: [f32; SCOPE_FRAMES],
}

impl Default for Scope {
    fn default() -> Self {
        Scope {
            left: [0.0; SCOPE_FRAMES],
            right: [0.0; SCOPE_FRAMES],
        }
    }
}

/// Reader half of the analyzer tap. Lock-free; call from the UI thread.
pub struct AnalyzerReader(triple_buffer::Output<Scope>);

impl AnalyzerReader {
    pub fn read(&mut self) -> &Scope {
        self.0.read()
    }
}

/// State shared between the decode thread, the engine handle and the callback.
#[derive(Debug)]
pub struct Shared {
    volume_bits: AtomicU32,
    pub paused: AtomicBool,
    pub flush_requested: AtomicU64,
    pub flush_done: AtomicU64,
    /// Ring frames consumed since start: played, or discarded by a flush. Silence is not
    /// counted. Comparable with the decode thread's count of frames pushed.
    pub frames_played: AtomicU64,
    /// True while the callback waits for enough audio before (re)starting.
    pub buffering: AtomicBool,
    /// Frames that must be queued before playback starts after a flush or underrun.
    pub start_threshold: AtomicU64,
    /// Streams re-enter buffering on underrun; files just play what they have.
    pub rebuffer_on_underrun: AtomicBool,
    /// The decoder finished; play out whatever is left even below the threshold.
    pub producer_done: AtomicBool,
}

impl Shared {
    pub fn new() -> Self {
        Shared {
            volume_bits: AtomicU32::new(1.0f32.to_bits()),
            paused: AtomicBool::new(false),
            flush_requested: AtomicU64::new(0),
            flush_done: AtomicU64::new(0),
            frames_played: AtomicU64::new(0),
            buffering: AtomicBool::new(false),
            start_threshold: AtomicU64::new(0),
            rebuffer_on_underrun: AtomicBool::new(false),
            producer_done: AtomicBool::new(true),
        }
    }

    pub fn set_volume(&self, gain: f32) {
        self.volume_bits.store(gain.to_bits(), Ordering::Relaxed);
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Renderer {
    consumer: rtrb::Consumer<f32>,
    shared: Arc<Shared>,
    gain: f32,
    flush_seen: u64,
    gated: bool,
    scope_in: triple_buffer::Input<Scope>,
    window: Scope,
    window_pos: usize,
}

impl Renderer {
    /// Creates the renderer plus the reader for its analyzer tap.
    pub fn new(consumer: rtrb::Consumer<f32>, shared: Arc<Shared>) -> (Self, AnalyzerReader) {
        let (scope_in, scope_out) = triple_buffer::triple_buffer(&Scope::default());
        let gain = shared.volume();
        let r = Renderer {
            consumer,
            shared,
            gain,
            flush_seen: 0,
            gated: true,
            scope_in,
            window: Scope::default(),
            window_pos: 0,
        };
        (r, AnalyzerReader(scope_out))
    }

    /// Fills `out` (interleaved, `channels` wide) with the next audio.
    /// Returns how many frames were real audio; the rest is silence.
    pub fn render(&mut self, out: &mut [f32], channels: usize) -> usize {
        let channels = channels.max(1);
        let frames = out.len() / channels;
        let shared = Arc::clone(&self.shared);

        let requested = shared.flush_requested.load(Ordering::Acquire);
        if requested != self.flush_seen {
            let n = self.consumer.slots();
            if let Ok(chunk) = self.consumer.read_chunk(n) {
                chunk.commit_all();
            }
            shared
                .frames_played
                .fetch_add((n / 2) as u64, Ordering::Release);
            self.flush_seen = requested;
            self.gated = true;
            shared.flush_done.store(requested, Ordering::Release);
        }

        if shared.paused.load(Ordering::Relaxed) {
            out.fill(0.0);
            return 0;
        }

        let available = self.consumer.slots() / 2;
        let done = shared.producer_done.load(Ordering::Acquire);
        if self.gated {
            let threshold = shared.start_threshold.load(Ordering::Relaxed) as usize;
            if available >= threshold.max(1) || (done && available > 0) {
                self.gated = false;
                shared.buffering.store(false, Ordering::Relaxed);
            } else {
                shared.buffering.store(!done, Ordering::Relaxed);
                out.fill(0.0);
                return 0;
            }
        }

        let n = frames.min(available);
        let target = shared.volume();
        let step = if n > 0 {
            (target - self.gain) / n as f32
        } else {
            0.0
        };
        if let Ok(chunk) = self.consumer.read_chunk(n * 2) {
            let (a, b) = chunk.as_slices();
            let mut samples = a.iter().chain(b.iter());
            for frame in out.chunks_exact_mut(channels).take(n) {
                let l = *samples.next().unwrap_or(&0.0);
                let r = *samples.next().unwrap_or(&0.0);
                // Analyzer window (pre-volume, like Winamp's vis).
                self.window.left[self.window_pos] = l;
                self.window.right[self.window_pos] = r;
                self.window_pos = (self.window_pos + 1) % SCOPE_FRAMES;
                self.gain += step;
                let (l, r) = (l * self.gain, r * self.gain);
                if channels == 1 {
                    frame[0] = 0.5 * (l + r);
                } else {
                    frame[0] = l;
                    frame[1] = r;
                    frame[2..].fill(0.0);
                }
            }
            chunk.commit_all();
        }
        self.gain = target;
        out[n * channels..].fill(0.0);
        if n > 0 {
            shared.frames_played.fetch_add(n as u64, Ordering::Release);
            self.publish_scope();
        }
        if n < frames && !done && shared.rebuffer_on_underrun.load(Ordering::Relaxed) {
            self.gated = true;
            shared.buffering.store(true, Ordering::Relaxed);
        }
        n
    }

    fn publish_scope(&mut self) {
        let dst = self.scope_in.input_buffer_mut();
        let split = self.window_pos;
        let tail = SCOPE_FRAMES - split;
        dst.left[..tail].copy_from_slice(&self.window.left[split..]);
        dst.left[tail..].copy_from_slice(&self.window.left[..split]);
        dst.right[..tail].copy_from_slice(&self.window.right[split..]);
        dst.right[tail..].copy_from_slice(&self.window.right[..split]);
        self.scope_in.publish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(capacity: usize) -> (rtrb::Producer<f32>, Renderer, AnalyzerReader, Arc<Shared>) {
        let (p, c) = rtrb::RingBuffer::new(capacity);
        let shared = Arc::new(Shared::new());
        shared.producer_done.store(false, Ordering::Relaxed);
        let (r, a) = Renderer::new(c, shared.clone());
        (p, r, a, shared)
    }

    fn push(p: &mut rtrb::Producer<f32>, frames: &[(f32, f32)]) {
        for &(l, r) in frames {
            p.push(l).unwrap();
            p.push(r).unwrap();
        }
    }

    #[test]
    fn plays_queued_frames_then_silence() {
        let (mut p, mut r, _, shared) = setup(64);
        push(&mut p, &[(0.5, -0.5), (0.25, -0.25)]);
        let mut out = [9.0f32; 8];
        assert_eq!(r.render(&mut out, 2), 2);
        assert_eq!(out, [0.5, -0.5, 0.25, -0.25, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(shared.frames_played.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn waits_for_threshold_before_starting() {
        let (mut p, mut r, _, shared) = setup(64);
        shared.start_threshold.store(4, Ordering::Relaxed);
        push(&mut p, &[(0.1, 0.1); 3]);
        let mut out = [9.0f32; 4];
        r.render(&mut out, 2);
        assert_eq!(out, [0.0; 4]);
        assert!(shared.buffering.load(Ordering::Relaxed));
        push(&mut p, &[(0.1, 0.1)]);
        r.render(&mut out, 2);
        assert_eq!(out, [0.1; 4]);
        assert!(!shared.buffering.load(Ordering::Relaxed));
    }

    #[test]
    fn producer_done_plays_out_below_threshold() {
        let (mut p, mut r, _, shared) = setup(64);
        shared.start_threshold.store(100, Ordering::Relaxed);
        push(&mut p, &[(0.3, 0.3)]);
        shared.producer_done.store(true, Ordering::Relaxed);
        let mut out = [0.0f32; 2];
        r.render(&mut out, 2);
        assert_eq!(out, [0.3, 0.3]);
    }

    #[test]
    fn pause_outputs_silence_and_keeps_audio() {
        let (mut p, mut r, _, shared) = setup(64);
        push(&mut p, &[(0.7, 0.7)]);
        shared.paused.store(true, Ordering::Relaxed);
        let mut out = [9.0f32; 2];
        r.render(&mut out, 2);
        assert_eq!(out, [0.0, 0.0]);
        shared.paused.store(false, Ordering::Relaxed);
        r.render(&mut out, 2);
        assert_eq!(out, [0.7, 0.7]);
    }

    #[test]
    fn flush_discards_and_acknowledges() {
        let (mut p, mut r, _, shared) = setup(64);
        push(&mut p, &[(0.7, 0.7); 4]);
        shared.flush_requested.store(1, Ordering::Release);
        let mut out = [9.0f32; 2];
        r.render(&mut out, 2);
        assert_eq!(out, [0.0, 0.0]);
        assert_eq!(shared.flush_done.load(Ordering::Acquire), 1);
        assert_eq!(p.slots(), 64);
        // Discarded frames count as consumed, so ring positions stay in step with the
        // decode thread's push count (review I6).
        assert_eq!(shared.frames_played.load(Ordering::Acquire), 4);
    }

    #[test]
    fn volume_ramps_within_one_buffer() {
        let (mut p, mut r, _, shared) = setup(64);
        push(&mut p, &[(1.0, 1.0); 4]);
        shared.set_volume(0.0);
        let mut out = [0.0f32; 8];
        r.render(&mut out, 2);
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        assert_eq!(left, vec![0.75, 0.5, 0.25, 0.0]);
    }

    #[test]
    fn maps_stereo_to_mono_and_multichannel_devices() {
        let (mut p, mut r, _, _) = setup(64);
        push(&mut p, &[(0.2, 0.4), (0.2, 0.4)]);
        let mut mono = [0.0f32; 1];
        r.render(&mut mono, 1);
        assert!((mono[0] - 0.3).abs() < 1e-6);
        let mut quad = [9.0f32; 4];
        r.render(&mut quad, 4);
        assert_eq!(quad, [0.2, 0.4, 0.0, 0.0]);
    }

    #[test]
    fn stream_underrun_rebuffers() {
        let (mut p, mut r, _, shared) = setup(64);
        shared.rebuffer_on_underrun.store(true, Ordering::Relaxed);
        shared.start_threshold.store(2, Ordering::Relaxed);
        push(&mut p, &[(0.1, 0.1); 2]);
        let mut out = [0.0f32; 6];
        r.render(&mut out, 2);
        assert!(shared.buffering.load(Ordering::Relaxed));
        push(&mut p, &[(0.1, 0.1)]);
        r.render(&mut out, 2);
        assert_eq!(out, [0.0; 6], "still below threshold");
    }

    #[test]
    fn analyzer_sees_latest_audio() {
        let (mut p, mut r, mut a, _) = setup(4096);
        let frames: Vec<(f32, f32)> = (0..600).map(|i| (i as f32, -(i as f32))).collect();
        push(&mut p, &frames);
        let mut out = vec![0.0f32; 1200];
        r.render(&mut out, 2);
        let scope = a.read();
        assert_eq!(scope.left[SCOPE_FRAMES - 1], 599.0);
        assert_eq!(scope.left[0], (600 - SCOPE_FRAMES) as f32);
        assert_eq!(scope.right[SCOPE_FRAMES - 1], -599.0);
    }
}
