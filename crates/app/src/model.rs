//! The app's state and rules (umbrella spec §5.3): every input becomes an [`Action`];
//! [`Model::apply`] updates the state and returns the [`Effect`]s for the app to carry out.
//! Pure, so it is tested without a window or an audio device.

use crate::queue::{Queue, Requeue};
use std::time::{Duration, Instant};
use winrust_audio::{Command, EngineEvent, PlaybackState, Source, TrackInfo};
use winrust_ui::interaction::{Interaction, Pointer};
use winrust_ui::marquee::{self, Marquee};
use winrust_ui::view::TimeMode;
use winrust_ui::vis::VisMode;

/// A request from a button, a hotkey or the mouse wheel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Previous,
    /// Play; restart if playing; resume if paused.
    Play,
    /// Pause, or resume if paused.
    Pause,
    Stop,
    /// Stop with a 1 s fadeout (Shift+V).
    FadeStop,
    /// Stop when the current track ends (Ctrl+V); pressing again cancels it.
    StopAfterCurrent,
    Next,
    /// Jump this many entries (numpad 1 and 3: ±10).
    Jump(i32),
    /// Seek this many seconds from the current position.
    SeekBy(i32),
    /// Change the volume by this many percent.
    VolumeBy(i32),
    ToggleRepeat,
    ToggleShuffle,
    ToggleTimeMode,
    ToggleDoubleSize,
    ToggleAlwaysOnTop,
    ToggleShade,
    /// The EQ and playlist buttons only light up until their windows exist (M4, M5).
    ToggleEq,
    TogglePlaylist,
    ToggleEasterEgg,
    Minimize,
    Close,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Files or URLs from the command line or a drop: replace the queue and play.
    Open(Vec<Source>),
    Control(Control),
    /// The pointer, in 1× window coordinates.
    Pointer(Pointer),
    Engine(EngineEvent),
    Focus(bool),
    /// The window's new outer position, in physical pixels.
    Moved {
        x: i32,
        y: i32,
    },
    /// Show a message in the marquee for a few seconds.
    Message(String),
    /// Time passed: animations advance.
    Tick,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Engine(Command),
    Redraw,
    /// A saved setting changed: write the config soon.
    SaveConfig,
    /// Move the window by this many 1× pixels.
    MoveWindow {
        dx: i32,
        dy: i32,
    },
    /// Shade or double-size changed the window's size.
    Resize,
    SetAlwaysOnTop(bool),
    Minimize,
    Exit,
}

/// The audible track, as the main window shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub title: String,
    pub kbps: Option<u32>,
    /// Sample rate in whole kHz.
    pub khz: u32,
    pub channels: u16,
    pub duration: Option<Duration>,
    pub seekable: bool,
}

pub struct Model {
    pub queue: Queue,
    /// As the engine last reported it.
    pub state: PlaybackState,
    pub elapsed: Duration,
    pub track: Option<Track>,
    /// 0..=100.
    pub volume: u8,
    /// -100..=100.
    pub balance: i8,
    pub time_mode: TimeMode,
    pub vis_mode: VisMode,
    pub eq: bool,
    pub playlist: bool,
    pub shade: bool,
    pub double_size: bool,
    pub always_on_top: bool,
    pub easter_egg: bool,
    pub focused: bool,
    /// The window's outer position, in physical pixels.
    pub position: Option<(i32, i32)>,
    pub interaction: Interaction,
    pub marquee: Marquee,
}

impl Model {
    /// `seed` drives the shuffle order.
    pub fn new(seed: u64) -> Self {
        let mut marquee = Marquee::new();
        marquee.set_text(&format!("Winrust {}", env!("CARGO_PKG_VERSION")));
        Model {
            queue: Queue::new(seed),
            state: PlaybackState::Stopped,
            elapsed: Duration::ZERO,
            track: None,
            volume: 75,
            balance: 0,
            time_mode: TimeMode::Elapsed,
            vis_mode: VisMode::Spectrum,
            eq: false,
            playlist: false,
            shade: false,
            double_size: false,
            always_on_top: false,
            easter_egg: false,
            focused: true,
            position: None,
            interaction: Interaction::new(),
            marquee,
        }
    }

    /// Commands that give a fresh engine the saved volume and balance.
    pub fn engine_settings(&self) -> Vec<Command> {
        vec![
            Command::SetVolume(self.volume),
            Command::SetBalance(f32::from(self.balance) / 100.0),
        ]
    }

    pub fn apply(&mut self, action: Action, now: Instant) -> Vec<Effect> {
        let _ = now;
        let mut fx = Vec::new();
        // Whether the window shows something new, beyond what `fx` already implies.
        let changed = match action {
            Action::Open(sources) => {
                if let Some(i) = self.queue.replace(sources) {
                    self.load(i, &mut fx);
                }
                false
            }
            Action::Control(c) => {
                self.control(c, &mut fx);
                false
            }
            Action::Engine(e) => {
                self.engine(e, &mut fx);
                true
            }
            Action::Message(text) => {
                self.marquee.show_message(&text);
                true
            }
            Action::Pointer(_) | Action::Focus(_) | Action::Moved { .. } | Action::Tick => false,
        };
        if (changed || !fx.is_empty()) && !fx.contains(&Effect::Redraw) {
            fx.push(Effect::Redraw);
        }
        fx
    }

    fn control(&mut self, c: Control, fx: &mut Vec<Effect>) {
        use PlaybackState::*;
        match c {
            Control::Play => match self.state {
                Paused => fx.push(Effect::Engine(Command::Play)),
                _ => {
                    if let Some(i) = self.queue.restart() {
                        self.load(i, fx);
                    }
                }
            },
            Control::Pause => match self.state {
                Playing | Buffering => fx.push(Effect::Engine(Command::Pause)),
                Paused => fx.push(Effect::Engine(Command::Play)),
                Stopped => {}
            },
            Control::Stop | Control::FadeStop => self.stop(fx),
            Control::StopAfterCurrent => {
                self.queue.stop_after = !self.queue.stop_after;
                self.requeue(fx);
            }
            Control::Previous => self.step(-1, fx),
            Control::Next => self.step(1, fx),
            Control::Jump(n) => self.step(n, fx),
            Control::SeekBy(secs) => {
                let Some(duration) = self
                    .track
                    .as_ref()
                    .filter(|t| t.seekable)
                    .and_then(|t| t.duration)
                else {
                    return;
                };
                if self.state == Stopped {
                    return;
                }
                let to = (self.elapsed.as_secs_f64() + f64::from(secs))
                    .clamp(0.0, duration.as_secs_f64());
                self.seek(Duration::from_secs_f64(to), fx);
            }
            Control::VolumeBy(d) => {
                self.set_volume((i32::from(self.volume) + d).clamp(0, 100) as u8, fx);
            }
            Control::ToggleRepeat => {
                self.queue.repeat = !self.queue.repeat;
                self.requeue(fx);
                fx.push(Effect::SaveConfig);
            }
            Control::ToggleShuffle => {
                self.queue.shuffle = !self.queue.shuffle;
                self.requeue(fx);
                fx.push(Effect::SaveConfig);
            }
            Control::ToggleTimeMode
            | Control::ToggleDoubleSize
            | Control::ToggleAlwaysOnTop
            | Control::ToggleShade
            | Control::ToggleEq
            | Control::TogglePlaylist
            | Control::ToggleEasterEgg
            | Control::Minimize
            | Control::Close => {}
        }
    }

    fn engine(&mut self, event: EngineEvent, fx: &mut Vec<Effect>) {
        match event {
            EngineEvent::State(state) => self.state = state,
            EngineEvent::Position(p) => self.elapsed = p,
            EngineEvent::TrackStarted { info, gapless } => {
                if gapless && self.queue.stop_after {
                    // Stop-after-current came too late to unqueue this track.
                    self.queue.stop();
                    fx.push(Effect::Engine(Command::Stop));
                    return;
                }
                self.elapsed = Duration::ZERO;
                self.track = Some(track(&info));
                if let Some(next) = self.queue.on_started(gapless) {
                    self.queue_next(next, fx);
                }
                self.update_title();
            }
            EngineEvent::Bitrate(kbps) => {
                if let Some(t) = &mut self.track {
                    t.kbps = Some(kbps);
                }
            }
            EngineEvent::StreamTitle(title) => {
                if let Some(t) = &mut self.track {
                    t.title = title;
                }
                self.update_title();
            }
            EngineEvent::TrackEnded => {
                if let Some(i) = self.queue.on_ended() {
                    self.load(i, fx);
                }
            }
            EngineEvent::Error { source, error } => {
                self.marquee.show_message(&error.to_string());
                if let Some(source) = source
                    && let Some(i) = self.queue.on_error(|s| *s == source)
                {
                    self.load(i, fx);
                }
            }
        }
    }

    fn load(&mut self, index: usize, fx: &mut Vec<Effect>) {
        let Some(source) = self.queue.entry(index).cloned() else {
            return;
        };
        self.marquee
            .set_text(&marquee::track_title(index + 1, &name(&source), None));
        fx.push(Effect::Engine(Command::Load(source)));
    }

    fn queue_next(&mut self, index: usize, fx: &mut Vec<Effect>) {
        if let Some(source) = self.queue.entry(index).cloned() {
            fx.push(Effect::Engine(Command::QueueNext(source)));
        }
    }

    fn requeue(&mut self, fx: &mut Vec<Effect>) {
        match self.queue.requeue() {
            Requeue::Queue(i) => self.queue_next(i, fx),
            Requeue::Unqueue => fx.push(Effect::Engine(Command::Unqueue)),
            Requeue::Keep => {}
        }
    }

    fn step(&mut self, n: i32, fx: &mut Vec<Effect>) {
        match self.queue.step(n) {
            Some(i) => self.load(i, fx),
            // Past either end without repeat: Webamp stops.
            None if self.state != PlaybackState::Stopped => self.stop(fx),
            None => {}
        }
    }

    fn stop(&mut self, fx: &mut Vec<Effect>) {
        self.queue.stop();
        fx.push(Effect::Engine(Command::Stop));
    }

    fn seek(&mut self, to: Duration, fx: &mut Vec<Effect>) {
        self.elapsed = to;
        fx.push(Effect::Engine(Command::Seek(to)));
    }

    fn set_volume(&mut self, volume: u8, fx: &mut Vec<Effect>) {
        if volume != self.volume {
            self.volume = volume;
            fx.push(Effect::Engine(Command::SetVolume(volume)));
            fx.push(Effect::SaveConfig);
        }
    }

    fn update_title(&mut self) {
        if let (Some(t), Some(i)) = (&self.track, self.queue.current()) {
            self.marquee
                .set_text(&marquee::track_title(i + 1, &t.title, t.duration));
        }
    }
}

fn track(info: &TrackInfo) -> Track {
    Track {
        title: name(&info.source),
        kbps: info.kbps,
        khz: (info.sample_rate + 500) / 1000,
        channels: info.channels,
        duration: info.duration,
        seekable: info.seekable,
    }
}

/// A file's name without its extension, or a stream's URL (until it sends a title).
fn name(source: &Source) -> String {
    match source {
        Source::File(path) => path
            .file_stem()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .into_owned(),
        Source::Url(url) => url.clone(),
    }
}
