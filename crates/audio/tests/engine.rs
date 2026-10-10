mod common;

use common::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;
use winrust_audio::{
    CaptureSink, Command, Engine, EngineError, EngineEvent, PlaybackState, Source,
};

const RATE: u32 = 44_100;
const LONG: Duration = Duration::from_secs(10);

fn engine(sink: CaptureSink) -> (Engine, std::sync::Arc<std::sync::Mutex<Vec<f32>>>) {
    let captured = sink.captured();
    (Engine::spawn(Box::new(sink)).unwrap(), captured)
}

fn is_ended(e: &EngineEvent) -> bool {
    matches!(e, EngineEvent::TrackEnded)
}

#[test]
fn plays_file_to_the_end() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.wav");
    let samples = sine_i16(RATE, 0, 22_050);
    write_wav(&path, RATE, 2, &samples);

    let (engine, captured) = engine(CaptureSink::new(RATE));
    engine
        .handle()
        .send(Command::Load(Source::File(path.clone())));
    let events = wait_for(engine.events(), LONG, is_ended);

    let started = events.iter().find_map(|e| match e {
        EngineEvent::TrackStarted { info, gapless } => Some((info.clone(), *gapless)),
        _ => None,
    });
    let (info, gapless) = started.expect("TrackStarted");
    assert_eq!(info.source, Source::File(path));
    assert_eq!(info.sample_rate, RATE);
    assert!(!gapless);
    assert!(events.contains(&EngineEvent::State(PlaybackState::Playing)));
    let audio = captured.lock().unwrap().clone();
    assert_eq!(
        audio.len(),
        2 * samples.len(),
        "every frame played exactly once"
    );
    let state = wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(_))
    });
    assert_eq!(
        state.last(),
        Some(&EngineEvent::State(PlaybackState::Stopped))
    );
}

#[test]
fn gapless_join_is_sample_exact() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.wav"), dir.path().join("b.wav"));
    // One continuous sine split across two files at an odd frame.
    write_wav(&a, RATE, 2, &sine_i16(RATE, 0, 30_001));
    write_wav(&b, RATE, 2, &sine_i16(RATE, 30_001, 50_000));

    let (engine, captured) = engine(CaptureSink::new(RATE));
    let h = engine.handle();
    h.send(Command::Load(Source::File(a)));
    h.send(Command::QueueNext(Source::File(b.clone())));
    let events = wait_for(engine.events(), LONG, is_ended);

    let gapless_start = events.iter().any(|e| {
        matches!(e, EngineEvent::TrackStarted { info, gapless: true } if info.source == Source::File(b.clone()))
    });
    assert!(gapless_start, "{events:#?}");
    let expected: Vec<f32> = sine_i16(RATE, 0, 50_000)
        .into_iter()
        .flat_map(|s| [to_f32(s), to_f32(s)])
        .collect();
    let audio = captured.lock().unwrap().clone();
    assert_eq!(audio.len(), expected.len());
    let worst = audio
        .iter()
        .zip(&expected)
        .map(|(x, y)| (x - y).abs())
        .fold(0f32, f32::max);
    assert!(worst < 1e-6, "max deviation {worst}");
}

#[test]
fn resamples_to_device_rate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("low.wav");
    write_wav(&path, 22_050, 1, &sine_i16(22_050, 0, 22_050));

    let (engine, captured) = engine(CaptureSink::new(48_000));
    engine.handle().send(Command::Load(Source::File(path)));
    let events = wait_for(engine.events(), LONG, is_ended);

    let info = events.iter().find_map(|e| match e {
        EngineEvent::TrackStarted { info, .. } => Some(info.clone()),
        _ => None,
    });
    assert_eq!(
        info.unwrap().sample_rate,
        22_050,
        "LCD shows the source rate"
    );
    let frames = captured.lock().unwrap().len() / 2;
    assert!((frames as i64 - 48_000).abs() < 300, "frames = {frames}");
}

#[test]
fn seek_jumps_and_reports_position() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.wav");
    write_wav(&path, RATE, 2, &sine_i16(RATE, 0, 3 * RATE as usize));

    let (engine, captured) = engine(CaptureSink::realtime(RATE));
    let h = engine.handle();
    h.send(Command::Load(Source::File(path)));
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::TrackStarted { .. })
    });
    h.send(Command::Seek(Duration::from_millis(2_500)));
    let events = wait_for(engine.events(), LONG, is_ended);

    let first_pos = events.iter().find_map(|e| match e {
        EngineEvent::Position(p) => Some(*p),
        _ => None,
    });
    let p = first_pos.expect("position after seek");
    assert!(
        p >= Duration::from_millis(2_490) && p < Duration::from_millis(2_700),
        "{p:?}"
    );
    let frames = captured.lock().unwrap().len() / 2;
    assert!(
        frames < 2 * RATE as usize,
        "skipped most of the file, played {frames}"
    );
}

#[test]
fn pause_holds_and_play_resumes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.wav");
    write_wav(&path, RATE, 2, &sine_i16(RATE, 0, 2 * RATE as usize));

    let (engine, captured) = engine(CaptureSink::realtime(RATE));
    let h = engine.handle();
    h.send(Command::Load(Source::File(path)));
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(PlaybackState::Playing))
    });
    h.send(Command::Pause);
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(PlaybackState::Paused))
    });
    let held = captured.lock().unwrap().len();
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(
        captured.lock().unwrap().len(),
        held,
        "no audio while paused"
    );
    h.send(Command::Play);
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(PlaybackState::Playing))
    });
}

#[test]
fn stop_then_play_restarts_track() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.wav");
    write_wav(&path, RATE, 2, &sine_i16(RATE, 0, 2 * RATE as usize));

    let (engine, _) = engine(CaptureSink::realtime(RATE));
    let h = engine.handle();
    h.send(Command::Load(Source::File(path.clone())));
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(PlaybackState::Playing))
    });
    h.send(Command::Stop);
    wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::State(PlaybackState::Stopped))
    });
    h.send(Command::Play);
    let events = wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::TrackStarted { .. })
    });
    assert!(
        matches!(events.last(), Some(EngineEvent::TrackStarted { info, .. }) if info.source == Source::File(path))
    );
}

#[test]
fn unreadable_file_reports_error_and_stays_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.mp3");
    std::fs::write(&path, b"not audio at all").unwrap();

    let (engine, _) = engine(CaptureSink::new(RATE));
    engine.handle().send(Command::Load(Source::File(path)));
    let events = wait_for(engine.events(), LONG, |e| {
        matches!(e, EngineEvent::Error(_))
    });
    assert!(matches!(
        events.last(),
        Some(EngineEvent::Error(EngineError::Unsupported(_)))
    ));
    std::thread::sleep(Duration::from_millis(50));
    let later: Vec<_> = engine.events().try_iter().collect();
    assert!(
        !later
            .iter()
            .any(|e| matches!(e, EngineEvent::TrackStarted { .. })),
        "{later:?}"
    );
}

#[test]
fn bad_queued_track_still_ends_current_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ok.wav");
    write_wav(&path, RATE, 2, &sine_i16(RATE, 0, 4_410));

    let (engine, _) = engine(CaptureSink::new(RATE));
    let h = engine.handle();
    h.send(Command::Load(Source::File(path)));
    h.send(Command::QueueNext(Source::File(
        dir.path().join("missing.flac"),
    )));
    let events = wait_for(engine.events(), LONG, is_ended);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::Error(EngineError::Open { .. }))),
        "{events:#?}"
    );
}

#[test]
fn plays_icy_stream_with_titles() {
    let body = wav_bytes(RATE, &sine_i16(RATE, 0, 3 * RATE as usize));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        let mut req = [0u8; 2048];
        let n = sock.read(&mut req).unwrap();
        assert!(String::from_utf8_lossy(&req[..n]).contains("Icy-MetaData: 1"));
        let metaint = 16_000;
        let mut out = format!("ICY 200 OK\r\ncontent-type: audio/wav\r\nicy-metaint: {metaint}\r\nicy-br: 1411\r\n\r\n").into_bytes();
        let meta = b"StreamTitle='Winrust Radio - Test';";
        let mut block = meta.to_vec();
        block.resize(meta.len().div_ceil(16) * 16, 0);
        for (i, chunk) in body.chunks(metaint).enumerate() {
            out.extend_from_slice(chunk);
            if chunk.len() == metaint {
                if i == 0 {
                    out.push((block.len() / 16) as u8);
                    out.extend_from_slice(&block);
                } else {
                    out.push(0);
                }
            }
        }
        let _ = sock.write_all(&out);
    });

    let (engine, captured) = engine(CaptureSink::new(RATE));
    engine
        .handle()
        .send(Command::Load(Source::Url(format!("http://{addr}/live"))));
    let events = wait_for(engine.events(), LONG, is_ended);

    assert!(
        events.contains(&EngineEvent::StreamTitle("Winrust Radio - Test".into())),
        "{events:#?}"
    );
    let info = events
        .iter()
        .find_map(|e| match e {
            EngineEvent::TrackStarted { info, .. } => Some(info.clone()),
            _ => None,
        })
        .unwrap();
    assert!(!info.seekable);
    assert_eq!(info.kbps, Some(1411));
    let frames = captured.lock().unwrap().len() / 2;
    assert!(
        (frames as i64 - 3 * RATE as i64).abs() < 64,
        "frames = {frames}"
    );
}

#[test]
fn analyzer_sees_playing_audio() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v.wav");
    write_wav(&path, RATE, 2, &sine_i16(RATE, 0, RATE as usize));

    let sink = CaptureSink::new(RATE);
    let mut engine = Engine::spawn(Box::new(sink)).unwrap();
    let mut analyzer = engine.take_analyzer().unwrap();
    assert!(engine.take_analyzer().is_none(), "only one reader");
    engine.handle().send(Command::Load(Source::File(path)));
    wait_for(engine.events(), LONG, is_ended);
    let peak = analyzer
        .read()
        .left
        .iter()
        .fold(0f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.4, "peak {peak}");
}
