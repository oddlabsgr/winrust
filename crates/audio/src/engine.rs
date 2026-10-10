//! The decode thread and the public `Engine` / `EngineHandle`.

use crate::decoder::Decoder;
use crate::dsp::DspChain;
use crate::renderer::{AnalyzerReader, Renderer, Shared};
use crate::resample::StreamResampler;
use crate::sink::{AudioSink, SinkEvent};
use crate::stream::{self, Cancel, extension_for_mime};
use crate::timeline::{Timeline, TimelineEvent};
use crate::types::{Command, EngineError, EngineEvent, EqSettings, PlaybackState, Source};
use crate::volume::slider_to_gain;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, unbounded};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::ReadOnlySource;

/// Ring capacity: 8 s of stereo at 48 kHz.
const RING_FRAMES: usize = 8 * 48_000;
/// Files: keep this much decoded ahead (also the latency of EQ/balance changes).
const FILE_FILL: Duration = Duration::from_millis(200);
/// Files: start playing once this much is queued.
const FILE_START: Duration = Duration::from_millis(50);
/// Streams: prebuffer before starting and after an underrun.
const STREAM_START: Duration = Duration::from_secs(2);
/// Streams: keep this much decoded ahead.
const STREAM_FILL: Duration = Duration::from_secs(4);
const IDLE_TICK: Duration = Duration::from_millis(10);
const POSITION_EVERY: Duration = Duration::from_millis(100);
const FLUSH_TIMEOUT: Duration = Duration::from_millis(500);
/// Packets decoded per loop turn before checking commands again.
const PACKETS_PER_TURN: usize = 16;

/// Cheap, cloneable way to control the engine from any thread.
#[derive(Clone)]
pub struct EngineHandle {
    tx: Sender<Command>,
    shared: Arc<Shared>,
    epoch: Arc<AtomicU64>,
}

impl EngineHandle {
    pub fn send(&self, cmd: Command) {
        match &cmd {
            // Volume never waits behind the decode thread.
            Command::SetVolume(v) => {
                self.shared.set_volume(slider_to_gain(*v));
                return;
            }
            // Interrupt any blocking network work so the command is handled promptly.
            Command::Load(_) | Command::Stop | Command::Shutdown | Command::SetDevice(_) => {
                self.epoch.fetch_add(1, Ordering::AcqRel);
            }
            _ => {}
        }
        let _ = self.tx.send(cmd);
    }
}

pub struct Engine {
    handle: EngineHandle,
    events: Receiver<EngineEvent>,
    analyzer: Option<AnalyzerReader>,
    thread: Option<JoinHandle<()>>,
}

impl Engine {
    /// Starts the output `sink` and the decode thread.
    pub fn spawn(mut sink: Box<dyn AudioSink>) -> Result<Engine, EngineError> {
        let shared = Arc::new(Shared::new());
        let (producer, consumer) = rtrb::RingBuffer::new(RING_FRAMES * 2);
        let (renderer, analyzer) = Renderer::new(consumer, shared.clone());
        let device_rate = sink.start(Arc::new(Mutex::new(renderer)))?;
        let (tx, rx) = unbounded();
        let (events_tx, events) = unbounded();
        let epoch = Arc::new(AtomicU64::new(0));
        let worker = Worker::new(
            rx,
            events_tx,
            shared.clone(),
            epoch.clone(),
            producer,
            sink,
            device_rate,
        );
        let thread = thread::Builder::new()
            .name("winrust-decode".into())
            .spawn(move || worker.run())
            .map_err(|e| EngineError::Device(format!("cannot start decode thread: {e}")))?;
        Ok(Engine {
            handle: EngineHandle { tx, shared, epoch },
            events,
            analyzer: Some(analyzer),
            thread: Some(thread),
        })
    }

    pub fn handle(&self) -> EngineHandle {
        self.handle.clone()
    }

    pub fn events(&self) -> &Receiver<EngineEvent> {
        &self.events
    }

    /// The visualization tap. Can be taken once.
    pub fn take_analyzer(&mut self) -> Option<AnalyzerReader> {
        self.analyzer.take()
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.handle.send(Command::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Active {
    decoder: Decoder,
    resampler: StreamResampler,
    source_rate: u32,
    is_stream: bool,
    titles: Option<Receiver<String>>,
}

struct Worker {
    rx: Receiver<Command>,
    events: Sender<EngineEvent>,
    shared: Arc<Shared>,
    epoch: Arc<AtomicU64>,
    producer: rtrb::Producer<f32>,
    sink: Box<dyn AudioSink>,
    device_rate: u32,
    dsp: DspChain,
    eq: EqSettings,
    balance: f32,
    active: Option<Active>,
    queued: Option<Source>,
    last_source: Option<Source>,
    timeline: Timeline,
    /// Device frame index the next pushed frame will have.
    pushed: u64,
    /// Processed stereo samples not yet in the ring.
    pending: Vec<f32>,
    decoded: Vec<f32>,
    resampled: Vec<f32>,
    fill_target: usize,
    state: PlaybackState,
    last_position: Instant,
    last_bitrate: Option<u32>,
}

impl Worker {
    fn new(
        rx: Receiver<Command>,
        events: Sender<EngineEvent>,
        shared: Arc<Shared>,
        epoch: Arc<AtomicU64>,
        producer: rtrb::Producer<f32>,
        sink: Box<dyn AudioSink>,
        device_rate: u32,
    ) -> Self {
        Worker {
            rx,
            events,
            shared,
            epoch,
            producer,
            sink,
            device_rate,
            dsp: DspChain::new(device_rate),
            eq: EqSettings::default(),
            balance: 0.0,
            active: None,
            queued: None,
            last_source: None,
            timeline: Timeline::new(device_rate),
            pushed: 0,
            pending: Vec::new(),
            decoded: Vec::new(),
            resampled: Vec::new(),
            fill_target: 0,
            state: PlaybackState::Stopped,
            last_position: Instant::now(),
            last_bitrate: None,
        }
    }

    fn run(mut self) {
        'main: loop {
            let wait = if self.wants_work() {
                Duration::ZERO
            } else {
                IDLE_TICK
            };
            let mut batch = Vec::new();
            match self.rx.recv_timeout(wait) {
                Ok(cmd) => batch.push(cmd),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            batch.extend(self.rx.try_iter());
            for cmd in coalesce(batch) {
                if !self.handle(cmd) {
                    break 'main;
                }
            }
            self.pump();
            self.poll_sink();
            self.report();
        }
        self.stop_playback();
        self.sink.stop();
    }

    fn emit(&self, event: EngineEvent) {
        let _ = self.events.send(event);
    }

    fn frames(&self, d: Duration) -> usize {
        let f = (d.as_secs_f64() * f64::from(self.device_rate)) as usize;
        f.min(RING_FRAMES * 3 / 4)
    }

    fn ring_frames(&self) -> usize {
        (self.producer.buffer().capacity() - self.producer.slots()) / 2
    }

    fn wants_work(&self) -> bool {
        (self.active.is_some() && self.ring_frames() < self.fill_target)
            || (!self.pending.is_empty() && self.producer.slots() >= 2)
    }

    /// Returns false on shutdown.
    fn handle(&mut self, cmd: Command) -> bool {
        match cmd {
            Command::Load(source) => self.load(source),
            Command::QueueNext(source) => self.queue_next(source),
            Command::Play => {
                let was_paused = self.shared.paused.swap(false, Ordering::AcqRel);
                let stopped = self.active.is_none() && self.timeline.current().is_none();
                if !was_paused
                    && stopped
                    && let Some(source) = self.last_source.clone()
                {
                    self.load(source);
                }
            }
            Command::Pause => {
                if self.active.is_some() || self.timeline.current().is_some() {
                    self.shared.paused.store(true, Ordering::Release);
                }
            }
            Command::Stop => {
                self.stop_playback();
                self.shared.paused.store(false, Ordering::Release);
            }
            Command::Seek(to) => self.seek(to),
            Command::SetVolume(v) => self.shared.set_volume(slider_to_gain(v)),
            Command::SetBalance(b) => {
                self.balance = b;
                self.dsp.set_balance(b);
            }
            Command::SetEq(eq) => {
                self.eq = eq;
                self.dsp.set_eq(&eq);
            }
            Command::SetDevice(name) => {
                let resume = self.resume_point();
                self.stop_playback();
                match self.sink.set_device(name) {
                    Ok(rate) => self.apply_rate(rate),
                    Err(e) => self.emit(EngineEvent::Error(e)),
                }
                self.resume(resume);
            }
            Command::Shutdown => return false,
        }
        true
    }

    fn load(&mut self, source: Source) {
        self.stop_playback();
        self.shared.paused.store(false, Ordering::Release);
        self.last_source = Some(source.clone());
        self.queued = None;
        if matches!(source, Source::Url(_)) {
            // Connecting and probing can take seconds; tell the UI right away.
            self.set_state(PlaybackState::Buffering);
        }
        let epoch = self.epoch.load(Ordering::Acquire);
        match self.open(&source) {
            Ok(active) => {
                self.set_stream_mode(active.is_stream);
                self.timeline
                    .push_track(self.pushed, active.decoder.info().clone(), false);
                self.shared.producer_done.store(false, Ordering::Release);
                self.shared.buffering.store(true, Ordering::Release);
                self.active = Some(active);
            }
            // A newer Load/Stop interrupted this one: not an error worth showing.
            Err(_) if self.epoch.load(Ordering::Acquire) != epoch => {}
            Err(e) => self.emit(EngineEvent::Error(e)),
        }
    }

    fn set_state(&mut self, state: PlaybackState) {
        if state != self.state {
            self.state = state;
            self.emit(EngineEvent::State(state));
        }
    }

    fn queue_next(&mut self, source: Source) {
        if self.active.is_none() && self.timeline.current().is_some() {
            // Current track already fully decoded and draining: append right away.
            match self.open(&source) {
                Ok(active) => {
                    let start = self.pushed + (self.pending.len() / 2) as u64;
                    self.timeline
                        .push_track(start, active.decoder.info().clone(), true);
                    self.set_stream_mode(active.is_stream);
                    self.shared.producer_done.store(false, Ordering::Release);
                    self.last_source = Some(source);
                    self.active = Some(active);
                }
                Err(e) => self.emit(EngineEvent::Error(e)),
            }
        } else {
            self.queued = Some(source);
        }
    }

    fn open(&mut self, source: &Source) -> Result<Active, EngineError> {
        // symphonia parses untrusted files and network data: a panic there must cost one
        // track, not the whole engine.
        let (epoch, device_rate) = (self.epoch.clone(), self.device_rate);
        catch_unwind(AssertUnwindSafe(|| open_source(source, epoch, device_rate))).unwrap_or_else(
            |_| {
                Err(EngineError::Decode(format!(
                    "decoder crashed opening {source}"
                )))
            },
        )
    }

    fn set_stream_mode(&mut self, is_stream: bool) {
        let (start, fill) = if is_stream {
            (STREAM_START, STREAM_FILL)
        } else {
            (FILE_START, FILE_FILL)
        };
        self.shared
            .start_threshold
            .store(self.frames(start) as u64, Ordering::Release);
        self.shared
            .rebuffer_on_underrun
            .store(is_stream, Ordering::Release);
        self.fill_target = self.frames(fill);
    }

    fn seek(&mut self, to: Duration) {
        let Some(active) = self.active.as_ref() else {
            return;
        };
        let audible = self.timeline.current().map(|i| &i.source);
        if !active.decoder.info().seekable || audible != Some(&active.decoder.info().source) {
            return;
        }
        self.flush();
        let active = self.active.as_mut().expect("checked above");
        match active.decoder.seek(to) {
            Ok(actual) => {
                active.resampler.reset();
                self.dsp.reset();
                self.timeline.reposition(self.pushed, actual);
                self.shared.producer_done.store(false, Ordering::Release);
                self.emit(EngineEvent::Position(actual));
            }
            Err(e) => self.emit(EngineEvent::Error(e)),
        }
    }

    fn stop_playback(&mut self) {
        self.active = None;
        self.queued = None;
        self.flush();
        self.timeline.clear();
        self.dsp.reset();
        self.shared.producer_done.store(true, Ordering::Release);
        self.shared.buffering.store(false, Ordering::Release);
    }

    /// Empties the ring: the callback drains it and acknowledges.
    fn flush(&mut self) {
        self.pending.clear();
        let request = self.shared.flush_requested.fetch_add(1, Ordering::AcqRel) + 1;
        let deadline = Instant::now() + FLUSH_TIMEOUT;
        while self.shared.flush_done.load(Ordering::Acquire) < request && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(1));
        }
        self.pushed = self.shared.frames_played.load(Ordering::Acquire);
    }

    /// Moves pending samples into the ring. Returns true when nothing is left pending.
    fn push_pending(&mut self) -> bool {
        let n = self.producer.slots().min(self.pending.len()) & !1;
        if n > 0 && self.producer.push_entire_slice(&self.pending[..n]).is_ok() {
            self.pending.drain(..n);
            self.pushed += (n / 2) as u64;
        }
        self.pending.is_empty()
    }

    fn pump(&mut self) {
        for _ in 0..PACKETS_PER_TURN {
            if !self.push_pending() {
                return;
            }
            if self.ring_frames() >= self.fill_target {
                return;
            }
            let Some(active) = self.active.as_mut() else {
                return;
            };
            self.decoded.clear();
            let decoded = &mut self.decoded;
            let step = catch_unwind(AssertUnwindSafe(|| active.decoder.decode_next(decoded)))
                .unwrap_or_else(|_| Err(EngineError::Decode("decoder crashed".into())));
            let step = match step {
                Ok(true) if active.decoder.sample_rate() != active.source_rate => {
                    // Chained stream changed rate mid-way.
                    active.resampler.flush(&mut self.resampled);
                    active.source_rate = active.decoder.sample_rate();
                    StreamResampler::new(active.source_rate, self.device_rate).map(|r| {
                        active.resampler = r;
                        true
                    })
                }
                other => other,
            };
            match step {
                Ok(true) => {
                    let active = self.active.as_mut().expect("checked above");
                    active.resampler.process(&self.decoded, &mut self.resampled);
                    self.commit_resampled();
                }
                Ok(false) => self.end_of_track(),
                Err(e) => {
                    self.emit(EngineEvent::Error(e));
                    self.end_of_track();
                }
            }
        }
    }

    fn commit_resampled(&mut self) {
        self.dsp.process(&mut self.resampled);
        self.pending.append(&mut self.resampled);
    }

    fn end_of_track(&mut self) {
        let Some(mut finished) = self.active.take() else {
            return;
        };
        if let Some(next) = self.queued.take() {
            match self.open(&next) {
                Ok(mut active) => {
                    if active.source_rate == finished.source_rate && !active.is_stream {
                        // Same rate: keep the resampler running so not a single sample is lost.
                        active.resampler = finished.resampler;
                    } else {
                        finished.resampler.flush(&mut self.resampled);
                        self.commit_resampled();
                    }
                    let start = self.pushed + (self.pending.len() / 2) as u64;
                    self.timeline
                        .push_track(start, active.decoder.info().clone(), true);
                    self.set_stream_mode(active.is_stream);
                    self.last_source = Some(next);
                    self.active = Some(active);
                    return;
                }
                Err(e) => self.emit(EngineEvent::Error(e)),
            }
        }
        finished.resampler.flush(&mut self.resampled);
        self.commit_resampled();
        self.timeline
            .set_end(self.pushed + (self.pending.len() / 2) as u64);
        self.shared.producer_done.store(true, Ordering::Release);
    }

    fn apply_rate(&mut self, rate: u32) {
        self.device_rate = rate;
        self.dsp = DspChain::new(rate);
        self.dsp.set_eq(&self.eq);
        self.dsp.set_balance(self.balance);
        self.timeline = Timeline::new(rate);
    }

    fn resume_point(&self) -> Option<(Source, Duration)> {
        let info = self.timeline.current()?;
        let played = self.shared.frames_played.load(Ordering::Acquire);
        Some((
            info.source.clone(),
            self.timeline.position(played).unwrap_or_default(),
        ))
    }

    fn resume(&mut self, point: Option<(Source, Duration)>) {
        if let Some((source, position)) = point {
            self.load(source);
            if position > Duration::ZERO {
                // The track must be audible before a seek is allowed; prime the timeline.
                self.timeline.poll(self.pushed);
                self.seek(position);
            }
        }
    }

    fn poll_sink(&mut self) {
        while let Some(event) = self.sink.poll() {
            match event {
                SinkEvent::RateChanged(rate) => {
                    let resume = self.resume_point();
                    self.stop_playback();
                    self.apply_rate(rate);
                    self.resume(resume);
                }
                SinkEvent::Error(msg) => self.emit(EngineEvent::Error(EngineError::Device(msg))),
            }
        }
    }

    fn report(&mut self) {
        let played = self.shared.frames_played.load(Ordering::Acquire);
        for event in self.timeline.poll(played) {
            match event {
                TimelineEvent::Started { info, gapless } => {
                    self.last_bitrate = None;
                    self.emit(EngineEvent::TrackStarted { info, gapless });
                }
                TimelineEvent::Ended => self.emit(EngineEvent::TrackEnded),
            }
        }
        if let Some(active) = self.active.as_mut() {
            let mut titles: Vec<String> = active
                .titles
                .as_ref()
                .map(|t| t.try_iter().collect())
                .unwrap_or_default();
            titles.extend(active.decoder.take_title().filter(|_| active.is_stream));
            let bitrate = active.decoder.take_bitrate();
            for title in titles {
                self.emit(EngineEvent::StreamTitle(title));
            }
            if let Some(kbps) = bitrate.filter(|k| Some(*k) != self.last_bitrate) {
                self.last_bitrate = Some(kbps);
                self.emit(EngineEvent::Bitrate(kbps));
            }
        }
        let state = if self.active.is_none() && self.timeline.current().is_none() {
            PlaybackState::Stopped
        } else if self.shared.paused.load(Ordering::Acquire) {
            PlaybackState::Paused
        } else if self.shared.buffering.load(Ordering::Acquire) {
            PlaybackState::Buffering
        } else {
            PlaybackState::Playing
        };
        self.set_state(state);
        if state == PlaybackState::Playing && self.last_position.elapsed() >= POSITION_EVERY {
            self.last_position = Instant::now();
            if let Some(pos) = self.timeline.position(played) {
                self.emit(EngineEvent::Position(pos));
            }
        }
    }
}

/// Opens a file or stream and prepares its resampler. Runs inside `catch_unwind`.
fn open_source(
    source: &Source,
    epoch: Arc<AtomicU64>,
    device_rate: u32,
) -> Result<Active, EngineError> {
    let (decoder, is_stream, titles) = match source {
        Source::File(path) => (Decoder::open_file(path)?, false, None),
        Source::Url(url) => {
            let s = stream::open(url, Cancel::new(epoch))?;
            let mut hint = Hint::new();
            if let Some(mime) = &s.content_type {
                hint.mime_type(mime);
                if let Some(ext) = extension_for_mime(mime) {
                    hint.with_extension(ext);
                }
            }
            let mut decoder = Decoder::open_media(
                source.clone(),
                Box::new(ReadOnlySource::new(s.reader)),
                &hint,
            )?;
            decoder.set_kbps(s.bitrate);
            (decoder, true, Some(s.titles))
        }
    };
    let source_rate = decoder.sample_rate();
    Ok(Active {
        resampler: StreamResampler::new(source_rate, device_rate)?,
        decoder,
        source_rate,
        is_stream,
        titles,
    })
}

/// Drops commands made pointless by a later one in the same batch: anything before the
/// last Load/Stop/Shutdown, except settings, which always apply.
fn coalesce(batch: Vec<Command>) -> Vec<Command> {
    let cut = batch
        .iter()
        .rposition(|c| matches!(c, Command::Load(_) | Command::Stop | Command::Shutdown))
        .unwrap_or(0);
    batch
        .into_iter()
        .enumerate()
        .filter(|(i, c)| {
            *i >= cut
                || matches!(
                    c,
                    Command::SetEq(_)
                        | Command::SetBalance(_)
                        | Command::SetVolume(_)
                        | Command::SetDevice(_)
                )
        })
        .map(|(_, c)| c)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesce_keeps_only_latest_load() {
        let a = Command::Load(Source::Url("a".into()));
        let b = Command::Load(Source::Url("b".into()));
        let out = coalesce(vec![
            a,
            Command::Seek(Duration::ZERO),
            Command::SetBalance(0.5),
            b.clone(),
            Command::Play,
        ]);
        assert_eq!(out, vec![Command::SetBalance(0.5), b, Command::Play]);
    }

    #[test]
    fn coalesce_stop_wins_over_earlier_load() {
        let out = coalesce(vec![Command::Load(Source::Url("a".into())), Command::Stop]);
        assert_eq!(out, vec![Command::Stop]);
    }

    #[test]
    fn coalesce_leaves_plain_batches_alone() {
        let batch = vec![Command::Pause, Command::Play];
        assert_eq!(coalesce(batch.clone()), batch);
    }
}
