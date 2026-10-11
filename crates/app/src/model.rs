//! The app's state and rules (umbrella spec §5.3): every input becomes an [`Action`];
//! [`Model::apply`] updates the state and returns the [`Effect`]s for the app to carry out.
//! Pure, so it is tested without a window or an audio device.

use crate::queue::{Queue, Requeue};
use std::time::{Duration, Instant};
use winrust_audio::{Command, EngineEvent, PlaybackState, Source, TrackInfo};
use winrust_skin::Image;
use winrust_ui::interaction::{Interaction, Pointer, UiEvent};
use winrust_ui::layout::Widget;
use winrust_ui::marquee::{self, Marquee};
use winrust_ui::view::{MainView, PlayState, TimeMode};
use winrust_ui::vis::VisMode;

/// How often the window redraws while something animates.
pub const FRAME: Duration = Duration::from_nanos(1_000_000_000 / 60);
/// Shift+V's fadeout.
const FADE: Duration = Duration::from_secs(1);
/// Where Windows reports a minimized window; not a position worth saving.
const MINIMIZED_AT: i32 = -32000;

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
    /// When a fadeout (Shift+V) started.
    fade: Option<Instant>,
    /// A fadeout lowered the engine's volume; it comes back once playback has stopped or
    /// paused, so the last of the audio never plays at full volume.
    faded: bool,
    /// When playback last paused; the time blinks from then.
    paused_at: Option<Instant>,
    last_tick: Option<Instant>,
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
            fade: None,
            faded: false,
            paused_at: None,
            last_tick: None,
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
        // Nothing moves while idle, so the animation clock skips idle time; otherwise the
        // first tick after a quiet spell would advance by all of it (and end a new message).
        if !matches!(action, Action::Tick) && self.next_wake(now).is_none() {
            self.last_tick = Some(now);
        }
        let mut fx = Vec::new();
        // Whether the window shows something new, beyond what `fx` already implies.
        let changed = match action {
            // A dropped folder without audio files leaves what is playing alone.
            Action::Open(sources) if sources.is_empty() => {
                self.marquee.show_message("No audio files found");
                true
            }
            Action::Open(sources) => {
                if let Some(i) = self.queue.replace(sources) {
                    self.load(i, &mut fx);
                }
                !fx.is_empty()
            }
            Action::Control(c) => {
                self.control(c, now, &mut fx);
                !fx.is_empty()
            }
            Action::Pointer(p) => self.pointer(p, now, &mut fx),
            Action::Engine(e) => {
                self.engine(e, now, &mut fx);
                true
            }
            Action::Focus(focused) => {
                self.focused = focused;
                if !focused {
                    self.cancel_pointer();
                }
                true
            }
            Action::Moved { x, y } => {
                if (x, y) != (MINIMIZED_AT, MINIMIZED_AT) {
                    self.position = Some((x, y));
                    fx.push(Effect::SaveConfig);
                }
                false
            }
            Action::Message(text) => {
                self.marquee.show_message(&text);
                true
            }
            Action::Tick => self.tick(now, &mut fx),
        };
        if changed && !fx.contains(&Effect::Redraw) {
            fx.push(Effect::Redraw);
        }
        fx
    }

    /// What the main window shows at `now`; the app adds the visualizer frame.
    pub fn view(&self, now: Instant) -> MainView {
        let track = self.track.as_ref();
        MainView {
            state: match self.state {
                PlaybackState::Stopped => PlayState::Stopped,
                PlaybackState::Playing => PlayState::Playing,
                PlaybackState::Paused => PlayState::Paused,
                PlaybackState::Buffering => PlayState::Buffering,
            },
            elapsed: self.elapsed,
            duration: track.and_then(|t| t.duration),
            time_mode: self.time_mode,
            blink_hidden: self.blink_hidden(now),
            title: self.marquee.text().to_string(),
            title_offset: self.marquee.offset(),
            kbps: track.and_then(|t| t.kbps),
            khz: track.map(|t| t.khz),
            channels: track.map(|t| t.channels),
            volume: self.volume,
            balance: self.balance,
            seekable: track.is_some_and(|t| t.seekable),
            shuffle: self.queue.shuffle,
            repeat: self.queue.repeat,
            eq: self.eq,
            playlist: self.playlist,
            focused: self.focused,
            shade: self.shade,
            double_size: self.double_size,
            always_on_top: self.always_on_top,
            easter_egg: self.easter_egg,
            vis: None,
        }
    }

    /// The visualizer frame to draw: the latest one while playing or paused, if it fits the
    /// window's mode (a frame made before shade mode toggled does not); none when stopped.
    pub fn vis_for(&self, frame: Option<&Image>) -> Option<Image> {
        let fits = if self.shade { (38, 5) } else { (76, 16) };
        frame
            .filter(|f| self.state != PlaybackState::Stopped && (f.width, f.height) == fits)
            .cloned()
    }

    /// When the next [`Action::Tick`] is due; `None` sleeps until some other input.
    pub fn next_wake(&self, now: Instant) -> Option<Instant> {
        if matches!(
            self.state,
            PlaybackState::Playing | PlaybackState::Buffering
        ) || self.fade.is_some()
        {
            return Some(now + FRAME);
        }
        let blink = self
            .paused_at
            .filter(|_| self.state == PlaybackState::Paused)
            .map(|at| at + Duration::from_secs(now.saturating_duration_since(at).as_secs() + 1));
        let marquee = self
            .marquee
            .next_change()
            .map(|d| self.last_tick.unwrap_or(now) + d);
        blink.into_iter().chain(marquee).min()
    }

    fn blink_hidden(&self, now: Instant) -> bool {
        self.state == PlaybackState::Paused
            && self
                .paused_at
                .is_some_and(|at| now.saturating_duration_since(at).as_secs() % 2 == 1)
    }

    fn tick(&mut self, now: Instant, fx: &mut Vec<Effect>) -> bool {
        let last = self.last_tick.replace(now).unwrap_or(now);
        let shown = |m: &Self, at| {
            (
                m.marquee.text().to_string(),
                m.marquee.offset(),
                m.blink_hidden(at),
            )
        };
        let before = shown(self, last);
        self.marquee.tick(now.saturating_duration_since(last));
        if let Some(start) = self.fade {
            let t = now.saturating_duration_since(start);
            if t >= FADE {
                self.stop(fx);
            } else {
                let level = f32::from(self.volume) * (1.0 - t.as_secs_f32() / FADE.as_secs_f32());
                fx.push(Effect::Engine(Command::SetVolume(level.round() as u8)));
                self.faded = true;
            }
        }
        let animating = matches!(
            self.state,
            PlaybackState::Playing | PlaybackState::Buffering
        );
        animating || shown(self, now) != before
    }

    fn pointer(&mut self, p: Pointer, now: Instant, fx: &mut Vec<Effect>) -> bool {
        let shown = |i: &Interaction| (i.pressed(), i.sliding());
        let before = shown(&self.interaction);
        let view = self.view(now);
        let event = self.interaction.pointer(p, &view);
        if let Some(e) = event {
            self.ui_event(e, now, fx);
        }
        event.is_some() || shown(&self.interaction) != before
    }

    fn cancel_pointer(&mut self) {
        if let Some(UiEvent::DragTitleEnd) = self.interaction.cancel() {
            self.marquee.end_drag();
        }
    }

    fn ui_event(&mut self, event: UiEvent, now: Instant, fx: &mut Vec<Effect>) {
        match event {
            UiEvent::Clicked(widget) => self.clicked(widget, now, fx),
            UiEvent::Volume(v) => self.set_volume(v, fx),
            UiEvent::Balance(b) => {
                if b != self.balance {
                    self.balance = b;
                    fx.push(Effect::Engine(Command::SetBalance(f32::from(b) / 100.0)));
                    fx.push(Effect::SaveConfig);
                }
            }
            UiEvent::Seek(fraction) => {
                if let Some(d) = self.track.as_ref().and_then(|t| t.duration) {
                    self.seek(d.mul_f32(fraction.clamp(0.0, 1.0)), fx);
                }
            }
            UiEvent::MoveWindow { dx, dy } => fx.push(Effect::MoveWindow { dx, dy }),
            UiEvent::ToggleShade => self.control(Control::ToggleShade, now, fx),
            UiEvent::DragTitle(dx) => self.marquee.drag(dx),
            UiEvent::DragTitleEnd => self.marquee.end_drag(),
        }
    }

    fn clicked(&mut self, widget: Widget, now: Instant, fx: &mut Vec<Effect>) {
        let control = match widget {
            Widget::Previous => Control::Previous,
            Widget::Play => Control::Play,
            Widget::Pause => Control::Pause,
            Widget::Stop => Control::Stop,
            Widget::Next => Control::Next,
            Widget::Shuffle => Control::ToggleShuffle,
            Widget::Repeat => Control::ToggleRepeat,
            Widget::Eq => Control::ToggleEq,
            Widget::Playlist => Control::TogglePlaylist,
            Widget::Minimize => Control::Minimize,
            Widget::Shade => Control::ToggleShade,
            Widget::Close => Control::Close,
            Widget::ClutterA => Control::ToggleAlwaysOnTop,
            Widget::ClutterD => Control::ToggleDoubleSize,
            Widget::Time => Control::ToggleTimeMode,
            Widget::Vis => {
                self.vis_mode = self.vis_mode.next();
                fx.push(Effect::SaveConfig);
                return;
            }
            // Menus, the file picker and the about box arrive in M7.
            Widget::Options
            | Widget::ClutterO
            | Widget::ClutterI
            | Widget::ClutterV
            | Widget::Eject
            | Widget::About
            | Widget::TitleBar
            | Widget::Marquee
            | Widget::Volume
            | Widget::Balance
            | Widget::Seek => return,
        };
        self.control(control, now, fx);
    }

    fn control(&mut self, c: Control, now: Instant, fx: &mut Vec<Effect>) {
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
                Playing | Buffering => {
                    // The volume comes back with the Paused state.
                    fx.push(Effect::Engine(Command::Pause));
                    self.fade = None;
                }
                Paused => fx.push(Effect::Engine(Command::Play)),
                Stopped => {}
            },
            Control::Stop => self.stop(fx),
            Control::FadeStop => match self.state {
                Playing => {
                    self.fade.get_or_insert(now);
                }
                _ => self.stop(fx),
            },
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
            Control::ToggleTimeMode => {
                self.time_mode = match self.time_mode {
                    TimeMode::Elapsed => TimeMode::Remaining,
                    TimeMode::Remaining => TimeMode::Elapsed,
                };
                fx.push(Effect::SaveConfig);
            }
            Control::ToggleDoubleSize => {
                self.double_size = !self.double_size;
                fx.push(Effect::Resize);
                fx.push(Effect::SaveConfig);
            }
            Control::ToggleAlwaysOnTop => {
                self.always_on_top = !self.always_on_top;
                fx.push(Effect::SetAlwaysOnTop(self.always_on_top));
                fx.push(Effect::SaveConfig);
            }
            Control::ToggleShade => {
                // A drag in one layout means nothing in the other.
                self.cancel_pointer();
                self.shade = !self.shade;
                fx.push(Effect::Resize);
            }
            Control::ToggleEq => {
                self.eq = !self.eq;
                fx.push(Effect::Redraw);
            }
            Control::TogglePlaylist => {
                self.playlist = !self.playlist;
                fx.push(Effect::Redraw);
            }
            Control::ToggleEasterEgg => {
                self.easter_egg = !self.easter_egg;
                fx.push(Effect::Redraw);
            }
            Control::Minimize => fx.push(Effect::Minimize),
            Control::Close => fx.push(Effect::Exit),
        }
    }

    fn engine(&mut self, event: EngineEvent, now: Instant, fx: &mut Vec<Effect>) {
        match event {
            EngineEvent::State(state) => {
                if state == PlaybackState::Paused && self.state != state {
                    self.paused_at = Some(now);
                }
                if matches!(state, PlaybackState::Stopped | PlaybackState::Paused) {
                    self.fade = None;
                    self.restore_volume(fx);
                }
                self.state = state;
            }
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
        self.end_fade(fx);
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
        self.fade = None;
        fx.push(Effect::Engine(Command::Stop));
        if self.state == PlaybackState::Stopped {
            // Nothing plays, and no Stopped state will come to put the volume back.
            self.restore_volume(fx);
        }
    }

    /// Ends a fadeout because another track starts: the volume comes back with it.
    fn end_fade(&mut self, fx: &mut Vec<Effect>) {
        self.fade = None;
        self.restore_volume(fx);
    }

    fn restore_volume(&mut self, fx: &mut Vec<Effect>) {
        if std::mem::take(&mut self.faded) {
            fx.push(Effect::Engine(Command::SetVolume(self.volume)));
        }
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
