//! Winrust: the skinned main window. `winrust_app::model` decides everything; this file
//! carries winit's events to it and its effects back out to the window, the engine and the
//! config file (M3 spec §5).

use std::ffi::OsString;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::monitor::MonitorHandle;
use winit::window::{Window, WindowId, WindowLevel};
use winrust_app::config::{self, Area, Config, Debounce};
use winrust_app::input::{self, DoubleClick, Hotkeys, Mods, Wheel};
use winrust_app::model::{Action, Control, Effect, Model};
use winrust_app::{present, sources};
use winrust_audio::{
    AnalyzerReader, Command, CpalSink, Engine, EngineEvent, EngineHandle, PlaybackState, Source,
};
use winrust_skin::{Image, Skin};
use winrust_ui::interaction::Pointer;
use winrust_ui::vis::{Vis, VisSettings};

fn main() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--version" || a == "-V") {
        println!("Winrust {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let mut builder = EventLoop::<EngineEvent>::with_user_event();
    #[cfg(target_os = "linux")]
    {
        // Wayland does not let windows place themselves (umbrella spec §8.3).
        use winit::platform::x11::EventLoopBuilderExtX11;
        builder.with_x11();
    }
    let event_loop = match builder.build() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("winrust: cannot start: {e}");
            std::process::exit(1);
        }
    };
    let mut app = App::new(event_loop.create_proxy(), sources::from_args(args));
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("winrust: {e}");
        std::process::exit(1);
    }
}

struct Win {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    /// Screen pixels per skin pixel.
    scale: u32,
}

struct App {
    model: Model,
    /// Files and URLs from the command line, opened once the window exists.
    startup: Vec<Source>,
    /// `None` when no audio output could start; the window still opens and says why.
    engine: Option<Engine>,
    handle: Option<EngineHandle>,
    analyzer: Option<AnalyzerReader>,
    skin: Arc<Skin>,
    vis: Vis,
    vis_frame: Option<Image>,
    win: Option<Win>,
    os_scale: f64,
    config_dir: Option<PathBuf>,
    save: Debounce,
    /// When the next tick is due.
    wake: Option<Instant>,
    keys: Hotkeys,
    mods: Mods,
    wheel: Wheel,
    clicks: DoubleClick,
    /// The pointer, in 1× window pixels.
    cursor: (i32, i32),
    /// Files dropped in this batch of events; winit reports them one by one.
    dropped: Vec<PathBuf>,
}

impl App {
    fn new(proxy: EventLoopProxy<EngineEvent>, startup: Vec<Source>) -> Self {
        let now = Instant::now();
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(1, |d| d.as_nanos() as u64);
        let mut model = Model::new(seed);
        let config_dir = config::dir();
        let (settings, warning) = config_dir.as_deref().map(config::load).unwrap_or_default();
        settings.apply(&mut model);
        if let Some(warning) = warning {
            model.apply(Action::Message(warning), now);
        }
        let (engine, analyzer) = match Engine::spawn(Box::new(CpalSink::new(None))) {
            Ok(mut engine) => {
                let analyzer = engine.take_analyzer();
                forward_events(&engine, proxy);
                for c in model.engine_settings() {
                    engine.handle().send(c);
                }
                (Some(engine), analyzer)
            }
            Err(e) => {
                model.apply(Action::Message(e.to_string()), now);
                (None, None)
            }
        };
        App {
            handle: engine.as_ref().map(Engine::handle),
            model,
            startup,
            engine,
            analyzer,
            skin: Skin::base(),
            vis: Vis::new(VisSettings::default()),
            vis_frame: None,
            win: None,
            os_scale: 1.0,
            config_dir,
            save: Debounce::default(),
            wake: None,
            keys: Hotkeys::new(),
            mods: Mods::default(),
            wheel: Wheel::default(),
            clicks: DoubleClick::default(),
            cursor: (0, 0),
            dropped: Vec::new(),
        }
    }

    fn scale(&self) -> u32 {
        present::scale(self.os_scale, self.model.double_size)
    }

    fn size(&self) -> (u32, u32) {
        present::window_size(self.model.shade, self.scale())
    }

    fn open_window(&mut self, el: &ActiveEventLoop) -> Result<(), String> {
        let level = if self.model.always_on_top {
            WindowLevel::AlwaysOnTop
        } else {
            WindowLevel::Normal
        };
        let attributes = Window::default_attributes()
            .with_title("Winrust")
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(false)
            .with_window_level(level);
        let window = Rc::new(el.create_window(attributes).map_err(|e| e.to_string())?);
        self.os_scale = window.scale_factor();
        let (w, h) = self.size();
        let _ = window.request_inner_size(PhysicalSize::new(w, h));
        let monitors: Vec<Area> = el.available_monitors().map(|m| area(&m)).collect();
        let primary = el.primary_monitor().map(|m| area(&m));
        let saved = self.model.position.map(|(x, y)| [x, y]);
        if let Some((x, y)) = config::place(saved, (w, h), &monitors, primary) {
            window.set_outer_position(PhysicalPosition::new(x, y));
        }
        let context = softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
        let surface =
            softbuffer::Surface::new(&context, window.clone()).map_err(|e| e.to_string())?;
        window.set_visible(true);
        window.request_redraw();
        self.win = Some(Win {
            window,
            surface,
            scale: self.scale(),
        });
        Ok(())
    }

    fn act(&mut self, el: &ActiveEventLoop, action: Action) {
        let now = Instant::now();
        for effect in self.model.apply(action, now) {
            self.run(el, effect, now);
        }
    }

    fn run(&mut self, el: &ActiveEventLoop, effect: Effect, now: Instant) {
        match effect {
            Effect::Engine(command) => self.send(command),
            Effect::Redraw => self.redraw(),
            Effect::SaveConfig => self.save.poke(now),
            Effect::MoveWindow { dx, dy } => {
                if let Some(win) = &self.win
                    && let Ok(p) = win.window.outer_position()
                {
                    let s = win.scale as i32;
                    win.window
                        .set_outer_position(PhysicalPosition::new(p.x + dx * s, p.y + dy * s));
                }
            }
            Effect::Resize => self.resize(),
            Effect::SetAlwaysOnTop(on) => {
                if let Some(win) = &self.win {
                    win.window.set_window_level(if on {
                        WindowLevel::AlwaysOnTop
                    } else {
                        WindowLevel::Normal
                    });
                }
            }
            Effect::Minimize => {
                if let Some(win) = &self.win {
                    win.window.set_minimized(true);
                }
            }
            Effect::Exit => el.exit(),
        }
    }

    fn send(&self, command: Command) {
        if let Some(handle) = &self.handle {
            handle.send(command);
        }
    }

    fn redraw(&self) {
        if let Some(win) = &self.win {
            win.window.request_redraw();
        }
    }

    /// Fits the window to the scale, shade and double-size settings.
    fn resize(&mut self) {
        let scale = self.scale();
        let (w, h) = self.size();
        if let Some(win) = &mut self.win {
            win.scale = scale;
            let _ = win.window.request_inner_size(PhysicalSize::new(w, h));
            win.window.request_redraw();
        }
    }

    fn draw(&mut self) {
        let Some(win) = &mut self.win else {
            return;
        };
        let size = win.window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        let mut view = self.model.view(Instant::now());
        view.vis = self.model.vis_for(self.vis_frame.as_ref());
        let image = winrust_ui::draw::render(&self.skin, &view, &self.model.interaction);
        if win.surface.resize(w, h).is_err() {
            return;
        }
        if let Ok(mut buffer) = win.surface.buffer_mut() {
            present::upscale(&image, win.scale, &mut buffer, size.width);
            let _ = buffer.present();
        }
    }

    /// The visualizer advances once per frame while playing, keeps its last frame while
    /// paused and goes blank when stopped.
    fn update_vis(&mut self) {
        match self.model.state {
            PlaybackState::Playing | PlaybackState::Buffering => {
                if let Some(analyzer) = &mut self.analyzer {
                    self.vis.settings.mode = self.model.vis_mode;
                    let scope = analyzer.read();
                    self.vis_frame = self.vis.frame(
                        &scope.left,
                        &scope.right,
                        &self.skin.vis_colors,
                        self.model.shade,
                    );
                }
            }
            PlaybackState::Paused => {}
            PlaybackState::Stopped => self.vis_frame = None,
        }
    }

    fn save_config(&mut self) {
        let Some(dir) = &self.config_dir else {
            return;
        };
        if let Err(e) = config::save(dir, &Config::from_model(&self.model)) {
            let message = format!("Cannot save settings: {e}");
            eprintln!("winrust: {message}");
            self.model.apply(Action::Message(message), Instant::now());
            self.redraw();
        }
    }
}

impl ApplicationHandler<EngineEvent> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.win.is_some() {
            return;
        }
        if let Err(e) = self.open_window(el) {
            eprintln!("winrust: cannot open the window: {e}");
            el.exit();
            return;
        }
        let startup = std::mem::take(&mut self.startup);
        if !startup.is_empty() {
            self.act(el, Action::Open(startup));
        }
    }

    fn user_event(&mut self, el: &ActiveEventLoop, event: EngineEvent) {
        self.act(el, Action::Engine(event));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.act(el, Action::Control(Control::Close)),
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::Focused(focused) => self.act(el, Action::Focus(focused)),
            WindowEvent::Moved(p) => self.act(el, Action::Moved { x: p.x, y: p.y }),
            WindowEvent::ScaleFactorChanged {
                scale_factor,
                mut inner_size_writer,
            } => {
                self.os_scale = scale_factor;
                let (w, h) = self.size();
                let _ = inner_size_writer.request_inner_size(PhysicalSize::new(w, h));
                let scale = self.scale();
                if let Some(win) = &mut self.win {
                    win.scale = scale;
                }
                self.redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let s = self.win.as_ref().map_or(1, |w| w.scale);
                self.cursor = (present::to_1x(position.x, s), present::to_1x(position.y, s));
                let (x, y) = self.cursor;
                self.act(el, Action::Pointer(Pointer::Move { x, y }));
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let (x, y) = self.cursor;
                let pointer = match state {
                    ElementState::Pressed => Pointer::Down {
                        x,
                        y,
                        double: self.clicks.press(Instant::now(), x, y),
                    },
                    ElementState::Released => Pointer::Up { x, y },
                };
                self.act(el, Action::Pointer(pointer));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = self.wheel.steps(delta);
                if steps != 0 {
                    self.act(el, Action::Control(Control::VolumeBy(steps)));
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                let s = m.state();
                self.mods = Mods {
                    ctrl: s.control_key(),
                    alt: s.alt_key(),
                    shift: s.shift_key(),
                };
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } if event.state == ElementState::Pressed => {
                let key = input::key(&event.logical_key, event.physical_key, event.location);
                for control in self.keys.press(key, self.mods, event.repeat) {
                    self.act(el, Action::Control(control));
                }
            }
            WindowEvent::DroppedFile(path) => self.dropped.push(path),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if !self.dropped.is_empty() {
            let paths = std::mem::take(&mut self.dropped);
            self.act(el, Action::Open(sources::from_paths(paths)));
        }
        let now = Instant::now();
        let due = self.wake.is_some_and(|t| now >= t);
        if due {
            self.act(el, Action::Tick);
            self.update_vis();
        }
        // Keep a pending wake unless the model now wants an earlier one, so a stream of
        // input events cannot postpone the animation.
        let wanted = self.model.next_wake(now);
        self.wake = match (self.wake, wanted) {
            (Some(pending), Some(w)) if !due => Some(pending.min(w)),
            _ => wanted,
        };
        if self.save.take(now) {
            self.save_config();
        }
        let next = self.wake.into_iter().chain(self.save.due()).min();
        el.set_control_flow(next.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        if self.save.take_pending() {
            self.save_config();
        }
        // Dropping the engine stops playback before the window goes.
        self.engine.take();
    }
}

/// Engine events wake the event loop from a helper thread.
fn forward_events(engine: &Engine, proxy: EventLoopProxy<EngineEvent>) {
    let events = engine.events().clone();
    let _ = std::thread::Builder::new()
        .name("winrust-events".into())
        .spawn(move || {
            for event in events.iter() {
                if proxy.send_event(event).is_err() {
                    break;
                }
            }
        });
}

fn area(m: &MonitorHandle) -> Area {
    let (p, s) = (m.position(), m.size());
    Area {
        x: p.x,
        y: p.y,
        w: s.width,
        h: s.height,
    }
}
