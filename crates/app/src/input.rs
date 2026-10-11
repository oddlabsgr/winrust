//! Keyboard, wheel and double-click input: the hotkey table (M3 spec §6) and the small
//! state machines winit leaves to the app. Pure apart from reading winit's key types.

use crate::model::Control;
use std::time::{Duration, Instant};
use winit::event::MouseScrollDelta;
use winit::keyboard::{Key as WinitKey, KeyCode, KeyLocation, NamedKey, PhysicalKey};

/// A key as the hotkey table names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A letter, lowercase.
    Letter(char),
    Left,
    Right,
    Up,
    Down,
    /// A numpad digit, typed with Num Lock on.
    Numpad(u8),
    Escape,
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

const LETTERS: [(KeyCode, char); 26] = [
    (KeyCode::KeyA, 'a'),
    (KeyCode::KeyB, 'b'),
    (KeyCode::KeyC, 'c'),
    (KeyCode::KeyD, 'd'),
    (KeyCode::KeyE, 'e'),
    (KeyCode::KeyF, 'f'),
    (KeyCode::KeyG, 'g'),
    (KeyCode::KeyH, 'h'),
    (KeyCode::KeyI, 'i'),
    (KeyCode::KeyJ, 'j'),
    (KeyCode::KeyK, 'k'),
    (KeyCode::KeyL, 'l'),
    (KeyCode::KeyM, 'm'),
    (KeyCode::KeyN, 'n'),
    (KeyCode::KeyO, 'o'),
    (KeyCode::KeyP, 'p'),
    (KeyCode::KeyQ, 'q'),
    (KeyCode::KeyR, 'r'),
    (KeyCode::KeyS, 's'),
    (KeyCode::KeyT, 't'),
    (KeyCode::KeyU, 'u'),
    (KeyCode::KeyV, 'v'),
    (KeyCode::KeyW, 'w'),
    (KeyCode::KeyX, 'x'),
    (KeyCode::KeyY, 'y'),
    (KeyCode::KeyZ, 'z'),
];

/// A winit key press as a [`Key`]. Letters follow the keyboard layout, as Winamp's do (the
/// AZERTY Z key is Z); when the layout gives no Latin letter (Greek, or a macOS Option
/// combination) the key's position decides. Numpad digits count only with Num Lock on;
/// without it the numpad keys are arrows.
pub fn key(logical: &WinitKey, physical: PhysicalKey, location: KeyLocation) -> Key {
    match logical {
        WinitKey::Named(NamedKey::ArrowLeft) => return Key::Left,
        WinitKey::Named(NamedKey::ArrowRight) => return Key::Right,
        WinitKey::Named(NamedKey::ArrowUp) => return Key::Up,
        WinitKey::Named(NamedKey::ArrowDown) => return Key::Down,
        WinitKey::Named(NamedKey::Escape) => return Key::Escape,
        WinitKey::Character(s) => {
            let mut chars = s.chars();
            if let (Some(c), None) = (chars.next(), chars.next()) {
                if location == KeyLocation::Numpad
                    && let Some(d) = c.to_digit(10)
                {
                    return Key::Numpad(d as u8);
                }
                if c.is_ascii_alphabetic() {
                    return Key::Letter(c.to_ascii_lowercase());
                }
            }
        }
        _ => {}
    }
    match physical {
        PhysicalKey::Code(code) => LETTERS
            .iter()
            .find(|(k, _)| *k == code)
            .map_or(Key::Other, |(_, c)| Key::Letter(*c)),
        PhysicalKey::Unidentified(_) => Key::Other,
    }
}

/// Typing these letters toggles the easter-egg title bar (Webamp `hotkeys.ts`).
const EGG: [char; 8] = ['n', 'u', 'l', 'l', 's', 'o', 'f', 't'];

/// The hotkey table, plus progress through the easter-egg word.
#[derive(Clone, Debug, Default)]
pub struct Hotkeys {
    egg: usize,
}

impl Hotkeys {
    pub fn new() -> Self {
        Self::default()
    }

    /// What a key press does. A held key (`repeat`) only keeps seeking and changing the
    /// volume.
    pub fn press(&mut self, key: Key, mods: Mods, repeat: bool) -> Vec<Control> {
        let mut out: Vec<Control> = control(key, mods)
            .filter(|c| !repeat || matches!(c, Control::SeekBy(_) | Control::VolumeBy(_)))
            .into_iter()
            .collect();
        // As Webamp: every key but Escape either continues the word or starts it over.
        if key != Key::Escape {
            self.egg = if key == Key::Letter(EGG[self.egg]) {
                self.egg + 1
            } else {
                0
            };
            if self.egg == EGG.len() {
                self.egg = 0;
                out.push(Control::ToggleEasterEgg);
            }
        }
        out
    }
}

fn control(key: Key, mods: Mods) -> Option<Control> {
    use Control::*;
    if mods.ctrl {
        return match key {
            Key::Letter('v') => Some(StopAfterCurrent),
            Key::Letter('t') => Some(ToggleTimeMode),
            Key::Letter('d') => Some(ToggleDoubleSize),
            Key::Letter('a') => Some(ToggleAlwaysOnTop),
            Key::Letter('w') => Some(ToggleShade),
            _ => None,
        };
    }
    if mods.alt {
        return match key {
            Key::Letter('g') => Some(ToggleEq),
            Key::Letter('e') => Some(TogglePlaylist),
            _ => None,
        };
    }
    Some(match key {
        Key::Letter('z') | Key::Numpad(4) => Previous,
        Key::Letter('x') | Key::Numpad(5) => Play,
        Key::Letter('c') => Pause,
        Key::Letter('v') if mods.shift => FadeStop,
        Key::Letter('v') => Stop,
        Key::Letter('b') | Key::Numpad(6) => Next,
        Key::Left | Key::Numpad(7) => SeekBy(-5),
        Key::Right | Key::Numpad(9) => SeekBy(5),
        Key::Up | Key::Numpad(8) => VolumeBy(1),
        Key::Down | Key::Numpad(2) => VolumeBy(-1),
        Key::Numpad(1) => Jump(-10),
        Key::Numpad(3) => Jump(10),
        Key::Letter('r') => ToggleRepeat,
        Key::Letter('s') => ToggleShuffle,
        _ => return None,
    })
}

/// Touchpads scroll in pixels: this many make one volume step.
const PIXELS_PER_STEP: f64 = 20.0;

/// The mouse wheel as volume steps: one per notch, fractions carried over.
#[derive(Clone, Debug, Default)]
pub struct Wheel {
    lines: f64,
    pixels: f64,
}

impl Wheel {
    pub fn steps(&mut self, delta: MouseScrollDelta) -> i32 {
        let (acc, per_step) = match delta {
            MouseScrollDelta::LineDelta(_, y) => {
                self.lines += f64::from(y);
                (&mut self.lines, 1.0)
            }
            MouseScrollDelta::PixelDelta(p) => {
                self.pixels += p.y;
                (&mut self.pixels, PIXELS_PER_STEP)
            }
        };
        let steps = (*acc / per_step).trunc();
        *acc -= steps * per_step;
        steps as i32
    }
}

/// Windows' default double-click time.
const DOUBLE_CLICK: Duration = Duration::from_millis(500);
/// How far (1× pixels) the second press may land from the first.
const DOUBLE_CLICK_SLOP: i32 = 4;

/// Spots the second press of a double-click (winit reports single presses only).
#[derive(Clone, Debug, Default)]
pub struct DoubleClick {
    last: Option<(Instant, i32, i32)>,
}

impl DoubleClick {
    pub fn press(&mut self, now: Instant, x: i32, y: i32) -> bool {
        let double = self.last.is_some_and(|(at, lx, ly)| {
            now.saturating_duration_since(at) <= DOUBLE_CLICK
                && (x - lx).abs() <= DOUBLE_CLICK_SLOP
                && (y - ly).abs() <= DOUBLE_CLICK_SLOP
        });
        self.last = (!double).then_some((now, x, y));
        double
    }
}
