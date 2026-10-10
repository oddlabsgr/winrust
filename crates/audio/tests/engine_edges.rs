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
        !after.iter().any(|e| matches!(e, EngineEvent::Error(_))),
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
        matches!(e, EngineEvent::Error(EngineError::Unsupported(_)))
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
