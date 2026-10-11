#![allow(dead_code)]

use std::path::PathBuf;
use std::time::{Duration, Instant};
use winrust_app::model::{Action, Control, Effect, Model};
use winrust_audio::{Command, EngineEvent, PlaybackState, Source, TrackInfo};

pub fn file(name: &str) -> Source {
    Source::File(PathBuf::from(format!("{name}.mp3")))
}

pub fn info(source: &Source, secs: u64) -> TrackInfo {
    TrackInfo {
        source: source.clone(),
        kbps: Some(128),
        sample_rate: 44_100,
        channels: 2,
        duration: Some(Duration::from_secs(secs)),
        seekable: true,
    }
}

pub fn commands(effects: &[Effect]) -> Vec<Command> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Engine(c) => Some(c.clone()),
            _ => None,
        })
        .collect()
}

/// A model plus the clock its actions see.
pub struct Harness {
    pub model: Model,
    pub now: Instant,
}

impl Harness {
    pub fn new() -> Self {
        Harness {
            model: Model::new(7),
            now: Instant::now(),
        }
    }

    /// `names` opened, the first one started and playing; tracks last `secs` seconds.
    pub fn playing(names: &[&str], secs: u64) -> Self {
        let mut h = Harness::new();
        h.apply(Action::Open(names.iter().map(|n| file(n)).collect()));
        h.started(names[0], secs, false);
        h.engine(EngineEvent::State(PlaybackState::Playing));
        h
    }

    pub fn apply(&mut self, action: Action) -> Vec<Effect> {
        self.model.apply(action, self.now)
    }

    pub fn control(&mut self, c: Control) -> Vec<Effect> {
        self.apply(Action::Control(c))
    }

    pub fn engine(&mut self, e: EngineEvent) -> Vec<Effect> {
        self.apply(Action::Engine(e))
    }

    pub fn started(&mut self, name: &str, secs: u64, gapless: bool) -> Vec<Effect> {
        self.engine(EngineEvent::TrackStarted {
            info: info(&file(name), secs),
            gapless,
        })
    }

    pub fn advance(&mut self, by: Duration) {
        self.now += by;
    }
}
