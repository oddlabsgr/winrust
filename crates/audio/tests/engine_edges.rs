mod common;

use common::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use winrust_audio::sink::{SharedRenderer, SinkEvent};
use winrust_audio::{
    AudioSink, CaptureSink, Command, Engine, EngineError, EngineEvent, PlaybackState, Source,
};

const RATE: u32 = 44_100;
const LONG: Duration = Duration::from_secs(10);

/// CaptureSink that reports a device rate change when `trigger` is set.
struct RateChangingSink {
    inner: CaptureSink,
    trigger: Arc<AtomicBool>,
}

impl AudioSink for RateChangingSink {
    fn start(&mut self, renderer: SharedRenderer) -> Result<u32, EngineError> {
        self.inner.start(renderer)
    }
    fn set_device(&mut self, name: Option<String>) -> Result<u32, EngineError> {
        self.inner.set_device(name)
    }
    fn poll(&mut self) -> Option<SinkEvent> {
        self.trigger
            .swap(false, Ordering::AcqRel)
            .then_some(SinkEvent::RateChanged(48_000))
    }
    fn stop(&mut self) {
        self.inner.stop();
    }
}

#[test]
fn device_rate_change_resumes_same_track_near_same_position() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.wav");
    write_wav(&path, RATE, 2, &sine_i16(RATE, 0, 4 * RATE as usize));
    let trigger = Arc::new(AtomicBool::new(false));
    let sink = RateChangingSink {
        inner: CaptureSink::realtime(RATE),
        trigger: trigger.clone(),
    };
    let engine = Engine::spawn(Box::new(sink)).unwrap();
    engine.handle().send(Command::Load(Source::File(path)));
    let before = wait_for(
        engine.events(),
        LONG,
        |e| matches!(e, EngineEvent::Position(p) if *p >= Duration::from_millis(500)),
    );
    let EngineEvent::Position(at) = before.last().cloned().unwrap() else {
        unreachable!()
    };

    trigger.store(true, Ordering::Release);
    let after = wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::Position(_))
    });
    let EngineEvent::Position(resumed) = after.last().cloned().unwrap() else {
        unreachable!()
    };
    assert!(
        resumed + Duration::from_millis(50) >= at && resumed < at + Duration::from_millis(500),
        "{at:?} -> {resumed:?}"
    );
    assert!(
        !after.iter().any(|e| matches!(e, EngineEvent::Error { .. })),
        "{after:#?}"
    );
}

#[test]
fn stop_is_prompt_while_stream_is_stalled() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        let mut req = [0u8; 1024];
        let _ = sock.read(&mut req);
        let mut out = b"HTTP/1.0 200 OK\r\ncontent-type: audio/wav\r\n\r\n".to_vec();
        // Header and a sliver of audio, then silence on the wire.
        out.extend(&wav_bytes(RATE, &sine_i16(RATE, 0, 10 * RATE as usize))[..4096]);
        sock.write_all(&out).unwrap();
        std::thread::sleep(Duration::from_secs(20));
    });
    let sink = CaptureSink::new(RATE);
    let engine = Engine::spawn(Box::new(sink)).unwrap();
    engine
        .handle()
        .send(Command::Load(Source::Url(format!("http://{addr}/"))));
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(PlaybackState::Buffering))
    });
    std::thread::sleep(Duration::from_millis(200));
    let asked = Instant::now();
    engine.handle().send(Command::Stop);
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(PlaybackState::Stopped))
    });
    assert!(
        asked.elapsed() < Duration::from_secs(1),
        "stop took {:?}",
        asked.elapsed()
    );
}

#[test]
fn rapid_loads_end_on_the_last_one() {
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (0..4)
        .map(|i| {
            let p = dir.path().join(format!("{i}.wav"));
            write_wav(&p, RATE, 2, &sine_i16(RATE, 0, RATE as usize / 2));
            p
        })
        .collect();
    let sink = CaptureSink::realtime(RATE);
    let engine = Engine::spawn(Box::new(sink)).unwrap();
    for p in &paths {
        engine.handle().send(Command::Load(Source::File(p.clone())));
    }
    let events = wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::TrackEnded)
    });
    let last_started = events.iter().rev().find_map(|e| match e {
        EngineEvent::TrackStarted { info, .. } => Some(info.source.clone()),
        _ => None,
    });
    assert_eq!(last_started, Some(Source::File(paths[3].clone())));
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, EngineEvent::TrackEnded))
            .count(),
        1
    );
}

#[test]
fn plays_from_non_ascii_path() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Μουσική ♫ 音楽");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("Χρυσοβέργης – τραγούδι.wav");
    write_wav(&path, RATE, 2, &sine_i16(RATE, 0, 4_410));
    let sink = CaptureSink::new(RATE);
    let engine = Engine::spawn(Box::new(sink)).unwrap();
    engine
        .handle()
        .send(Command::Load(Source::File(path.clone())));
    let events = wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::TrackEnded)
    });
    assert!(events.iter().any(|e| matches!(e, EngineEvent::TrackStarted { info, .. } if info.source == Source::File(path.clone()))));
}

#[test]
fn seek_past_the_end_just_ends_the_track() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("short.wav");
    write_wav(&path, RATE, 2, &sine_i16(RATE, 0, 2 * RATE as usize));
    let sink = CaptureSink::realtime(RATE);
    let engine = Engine::spawn(Box::new(sink)).unwrap();
    engine.handle().send(Command::Load(Source::File(path)));
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::TrackStarted { .. })
    });
    engine
        .handle()
        .send(Command::Seek(Duration::from_secs(600)));
    let events = wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::TrackEnded)
    });
    assert!(
        events
            .iter()
            .all(|e| !matches!(e, EngineEvent::Position(p) if *p > Duration::from_secs(3))),
        "{events:#?}"
    );
}

#[test]
fn zero_sample_rate_file_is_rejected_and_engine_survives() {
    // Review C1: symphonia accepts a WAV claiming 0 Hz; building a resampler for it used to
    // panic the decode thread, silently killing the engine for the rest of the session.
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("zero.wav");
    std::fs::write(&bad, raw_wav(0, 4096)).unwrap();
    let good = dir.path().join("good.wav");
    write_wav(&good, RATE, 2, &sine_i16(RATE, 0, 4_410));
    let engine = Engine::spawn(Box::new(CaptureSink::new(RATE))).unwrap();
    engine.handle().send(Command::Load(Source::File(bad)));
    wait_for(engine.events(), LONG, |e| {
        matches!(
            e,
            EngineEvent::Error {
                error: EngineError::Unsupported(_),
                ..
            }
        )
    });
    engine.handle().send(Command::Load(Source::File(good)));
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::TrackEnded)
    });
}

#[test]
fn late_queue_next_plays_the_next_track_fully() {
    // Review I4: QueueNext after the current track is fully decoded used to report
    // TrackEnded the instant the queued track started.
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.wav"), dir.path().join("b.wav"));
    write_wav(&a, RATE, 2, &sine_i16(RATE, 0, RATE as usize / 10));
    write_wav(&b, RATE, 2, &sine_i16(RATE, 0, RATE as usize / 2));
    let engine = Engine::spawn(Box::new(CaptureSink::realtime(RATE))).unwrap();
    engine.handle().send(Command::Load(Source::File(a)));
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::TrackStarted { .. })
    });
    engine
        .handle()
        .send(Command::QueueNext(Source::File(b.clone())));
    wait_for(
        engine.events(),
        LONG,
        |e| matches!(e, EngineEvent::TrackStarted { info, .. } if info.source == Source::File(b.clone())),
    );
    let rest = wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::TrackEnded)
    });
    let positions = rest
        .iter()
        .filter(|e| matches!(e, EngineEvent::Position(_)))
        .count();
    assert!(positions >= 2, "b ended without playing: {rest:#?}");
}

/// Output that stops pulling audio while `hold` is set (e.g. a USB DAC unplugged with no
/// fallback device). Records real frames only.
struct StallingSink {
    hold: Arc<AtomicBool>,
    captured: Arc<std::sync::Mutex<Vec<f32>>>,
    stop: Arc<AtomicBool>,
}

impl AudioSink for StallingSink {
    fn start(&mut self, renderer: SharedRenderer) -> Result<u32, EngineError> {
        let (hold, captured, stop) = (self.hold.clone(), self.captured.clone(), self.stop.clone());
        std::thread::spawn(move || {
            let mut buf = [0.0f32; 1024];
            while !stop.load(Ordering::Relaxed) {
                if !hold.load(Ordering::Relaxed) {
                    let n = renderer.lock().unwrap().render(&mut buf, 2);
                    captured.lock().unwrap().extend_from_slice(&buf[..n * 2]);
                }
                std::thread::sleep(Duration::from_micros(500));
            }
        });
        Ok(RATE)
    }
    fn set_device(&mut self, _: Option<String>) -> Result<u32, EngineError> {
        Ok(RATE)
    }
    fn poll(&mut self) -> Option<SinkEvent> {
        None
    }
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[test]
fn load_during_output_stall_still_plays_whole_track() {
    // Review I6: a flush the output could not acknowledge in time used to discard the new
    // track's audio later while still counting it as queued, so the track never ended.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.wav");
    write_wav(&path, RATE, 2, &sine_i16(RATE, 0, RATE as usize));
    let hold = Arc::new(AtomicBool::new(true));
    let captured = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = StallingSink {
        hold: hold.clone(),
        captured: captured.clone(),
        stop: Arc::default(),
    };
    let engine = Engine::spawn(Box::new(sink)).unwrap();
    let asked = Instant::now();
    engine.handle().send(Command::Load(Source::File(path)));
    std::thread::sleep(Duration::from_millis(800));
    hold.store(false, Ordering::Relaxed);
    wait_for(engine.events(), Duration::from_secs(5), |e| {
        matches!(e, EngineEvent::TrackEnded)
    });
    assert!(asked.elapsed() < Duration::from_secs(5));
    assert_eq!(
        captured.lock().unwrap().len() / 2,
        RATE as usize,
        "every frame played"
    );
}

#[test]
fn stop_after_underrun_on_stalled_stream_does_not_end_track() {
    // Review I1: cancelling a stalled read made the pipe report EOF, which the engine took
    // as a natural end; after an underrun that fired TrackEnded (or started the queued
    // track) before Stop was handled, so players skipped ahead.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        let mut req = [0u8; 1024];
        let _ = sock.read(&mut req);
        let mut out = b"HTTP/1.0 200 OK
content-type: audio/wav

"
        .to_vec();
        let wav = wav_bytes(RATE, &sine_i16(RATE, 0, 10 * RATE as usize));
        // Header plus 96 whole 1152-frame packets (~2.5 s): passes the 2 s prebuffer and
        // stalls exactly on a packet boundary, so every byte sent gets played.
        out.extend(&wav[..44 + 96 * 1152 * 4]);
        sock.write_all(&out).unwrap();
        std::thread::sleep(Duration::from_secs(20));
    });
    let sink = CaptureSink::new(RATE);
    let captured = sink.captured();
    let engine = Engine::spawn(Box::new(sink)).unwrap();
    engine
        .handle()
        .send(Command::Load(Source::Url(format!("http://{addr}/"))));
    // Wait for audio rather than State(Playing), which a slow runner may report late.
    let deadline = Instant::now() + LONG;
    while captured.lock().unwrap().is_empty() {
        assert!(Instant::now() < deadline, "playback never started");
        std::thread::sleep(Duration::from_millis(10));
    }
    // The fast capture sink drains 2.5 s of audio in well under 500 ms: now underrun.
    std::thread::sleep(Duration::from_millis(500));
    engine.handle().send(Command::Stop);
    let events = wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(PlaybackState::Stopped))
    });
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, EngineEvent::TrackEnded | EngineEvent::Error { .. })),
        "{events:#?}"
    );
}

#[test]
fn rate_change_while_paused_stays_paused() {
    // Review I2: plugging in headphones while paused used to start playback.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.wav");
    write_wav(&path, RATE, 2, &sine_i16(RATE, 0, 4 * RATE as usize));
    let trigger = Arc::new(AtomicBool::new(false));
    let inner = CaptureSink::realtime(RATE);
    let captured = inner.captured();
    let sink = RateChangingSink {
        inner,
        trigger: trigger.clone(),
    };
    let engine = Engine::spawn(Box::new(sink)).unwrap();
    engine.handle().send(Command::Load(Source::File(path)));
    wait_for(
        engine.events(),
        LONG,
        |e| matches!(e, EngineEvent::Position(p) if *p >= Duration::from_millis(300)),
    );
    engine.handle().send(Command::Pause);
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(PlaybackState::Paused))
    });
    std::thread::sleep(Duration::from_millis(100));
    let held = captured.lock().unwrap().len();
    trigger.store(true, Ordering::Release);
    std::thread::sleep(Duration::from_millis(400));
    let after: Vec<_> = engine.events().try_iter().collect();
    assert_eq!(
        captured.lock().unwrap().len(),
        held,
        "audio played while paused"
    );
    assert!(
        !after
            .iter()
            .any(|e| matches!(e, EngineEvent::State(PlaybackState::Playing))),
        "{after:#?}"
    );
}

#[test]
fn rate_change_keeps_the_queued_track() {
    // Review I2: the gapless queue was dropped on a device rate change.
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.wav"), dir.path().join("b.wav"));
    write_wav(&a, RATE, 2, &sine_i16(RATE, 0, RATE as usize));
    write_wav(&b, RATE, 2, &sine_i16(RATE, 0, RATE as usize / 2));
    let trigger = Arc::new(AtomicBool::new(false));
    let sink = RateChangingSink {
        inner: CaptureSink::realtime(RATE),
        trigger: trigger.clone(),
    };
    let engine = Engine::spawn(Box::new(sink)).unwrap();
    engine.handle().send(Command::Load(Source::File(a)));
    engine
        .handle()
        .send(Command::QueueNext(Source::File(b.clone())));
    wait_for(
        engine.events(),
        LONG,
        |e| matches!(e, EngineEvent::Position(p) if *p >= Duration::from_millis(300)),
    );
    trigger.store(true, Ordering::Release);
    let events = wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::TrackEnded)
    });
    assert!(
        events.iter().any(|e| matches!(
            e,
            EngineEvent::TrackStarted { info, gapless: true } if info.source == Source::File(b.clone())
        )),
        "{events:#?}"
    );
}

#[test]
fn position_and_state_keep_updating_while_stream_stalls() {
    // M1 carry-forward: a stalled stream read used to block the decode thread, so position
    // and state stopped updating (and Pause waited) until data resumed or the read timed out.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        let mut req = [0u8; 1024];
        let _ = sock.read(&mut req);
        let mut out = b"HTTP/1.0 200 OK\r\ncontent-type: audio/wav\r\n\r\n".to_vec();
        // 3 s of audio: past the 2 s prebuffer, then silence on the wire.
        let wav = wav_bytes(RATE, &sine_i16(RATE, 0, 10 * RATE as usize));
        out.extend(&wav[..44 + 3 * RATE as usize * 4]);
        sock.write_all(&out).unwrap();
        std::thread::sleep(Duration::from_secs(30));
    });
    let engine = Engine::spawn(Box::new(CaptureSink::realtime(RATE))).unwrap();
    engine
        .handle()
        .send(Command::Load(Source::Url(format!("http://{addr}/"))));
    // The buffered audio plays on while the network is silent, and the time keeps moving...
    wait_for(
        engine.events(),
        LONG,
        |e| matches!(e, EngineEvent::Position(p) if *p >= Duration::from_millis(2500)),
    );
    // ...until the buffer runs dry, which the engine reports.
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(PlaybackState::Buffering))
    });
    let asked = Instant::now();
    engine.handle().send(Command::Pause);
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(PlaybackState::Paused))
    });
    assert!(
        asked.elapsed() < Duration::from_secs(1),
        "pause took {:?}",
        asked.elapsed()
    );
}
