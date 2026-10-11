use std::time::{Duration, Instant};
use winit::dpi::PhysicalPosition;
use winit::event::MouseScrollDelta;
use winit::keyboard::{Key as WinitKey, KeyCode, KeyLocation, NamedKey, PhysicalKey};
use winrust_app::input::{DoubleClick, Hotkeys, Key, Mods, Wheel, key};
use winrust_app::model::Control;

fn char_key(s: &str, code: KeyCode, location: KeyLocation) -> Key {
    key(
        &WinitKey::Character(s.into()),
        PhysicalKey::Code(code),
        location,
    )
}

const PLAIN: Mods = Mods {
    ctrl: false,
    alt: false,
    shift: false,
};
const CTRL: Mods = Mods {
    ctrl: true,
    ..PLAIN
};
const ALT: Mods = Mods { alt: true, ..PLAIN };
const SHIFT: Mods = Mods {
    shift: true,
    ..PLAIN
};

#[test]
fn letters_follow_the_layout_and_fall_back_to_the_key_position() {
    use KeyLocation::Standard;
    // AZERTY: the key labelled Z sits where QWERTY has W.
    assert_eq!(char_key("z", KeyCode::KeyW, Standard), Key::Letter('z'));
    assert_eq!(char_key("V", KeyCode::KeyV, Standard), Key::Letter('v'));
    // Greek layout, or macOS Option combinations: no Latin letter, use the position.
    assert_eq!(char_key("ζ", KeyCode::KeyZ, Standard), Key::Letter('z'));
    assert_eq!(char_key("©", KeyCode::KeyG, Standard), Key::Letter('g'));
    assert_eq!(char_key("1", KeyCode::Digit1, Standard), Key::Other);
}

#[test]
fn numpad_digits_count_only_with_num_lock() {
    assert_eq!(
        char_key("4", KeyCode::Numpad4, KeyLocation::Numpad),
        Key::Numpad(4)
    );
    // Num Lock off: the numpad 4 key is an arrow.
    let arrow = key(
        &WinitKey::Named(NamedKey::ArrowLeft),
        PhysicalKey::Code(KeyCode::Numpad4),
        KeyLocation::Numpad,
    );
    assert_eq!(arrow, Key::Left);
    let escape = key(
        &WinitKey::Named(NamedKey::Escape),
        PhysicalKey::Code(KeyCode::Escape),
        KeyLocation::Standard,
    );
    assert_eq!(escape, Key::Escape);
}

#[test]
fn the_hotkey_table() {
    let table = [
        (Key::Letter('z'), PLAIN, Control::Previous),
        (Key::Numpad(4), PLAIN, Control::Previous),
        (Key::Letter('x'), PLAIN, Control::Play),
        (Key::Numpad(5), PLAIN, Control::Play),
        (Key::Letter('c'), PLAIN, Control::Pause),
        (Key::Letter('v'), PLAIN, Control::Stop),
        (Key::Letter('v'), SHIFT, Control::FadeStop),
        (Key::Letter('v'), CTRL, Control::StopAfterCurrent),
        (Key::Letter('b'), PLAIN, Control::Next),
        (Key::Numpad(6), PLAIN, Control::Next),
        (Key::Left, PLAIN, Control::SeekBy(-5)),
        (Key::Numpad(7), PLAIN, Control::SeekBy(-5)),
        (Key::Right, PLAIN, Control::SeekBy(5)),
        (Key::Numpad(9), PLAIN, Control::SeekBy(5)),
        (Key::Up, PLAIN, Control::VolumeBy(1)),
        (Key::Numpad(8), PLAIN, Control::VolumeBy(1)),
        (Key::Down, PLAIN, Control::VolumeBy(-1)),
        (Key::Numpad(2), PLAIN, Control::VolumeBy(-1)),
        (Key::Numpad(1), PLAIN, Control::Jump(-10)),
        (Key::Numpad(3), PLAIN, Control::Jump(10)),
        (Key::Letter('r'), PLAIN, Control::ToggleRepeat),
        (Key::Letter('s'), PLAIN, Control::ToggleShuffle),
        (Key::Letter('t'), CTRL, Control::ToggleTimeMode),
        (Key::Letter('d'), CTRL, Control::ToggleDoubleSize),
        (Key::Letter('a'), CTRL, Control::ToggleAlwaysOnTop),
        (Key::Letter('w'), CTRL, Control::ToggleShade),
        (Key::Letter('g'), ALT, Control::ToggleEq),
        (Key::Letter('e'), ALT, Control::TogglePlaylist),
        // Shift does not change the other keys.
        (Key::Letter('b'), SHIFT, Control::Next),
    ];
    for (k, mods, control) in table {
        assert_eq!(
            Hotkeys::new().press(k, mods, false),
            vec![control],
            "{k:?} {mods:?}"
        );
    }
}

#[test]
fn keys_for_later_milestones_do_nothing() {
    for (k, mods) in [
        (Key::Letter('l'), PLAIN),
        (Key::Numpad(0), PLAIN),
        (Key::Letter('j'), PLAIN),
        (Key::Letter('r'), CTRL),
        (Key::Letter('w'), ALT),
        (Key::Letter('z'), CTRL),
        (Key::Other, PLAIN),
    ] {
        assert_eq!(
            Hotkeys::new().press(k, mods, false),
            vec![],
            "{k:?} {mods:?}"
        );
    }
}

#[test]
fn held_keys_repeat_only_seeking_and_volume() {
    let mut keys = Hotkeys::new();
    assert_eq!(keys.press(Key::Up, PLAIN, true), vec![Control::VolumeBy(1)]);
    assert_eq!(
        keys.press(Key::Right, PLAIN, true),
        vec![Control::SeekBy(5)]
    );
    assert_eq!(keys.press(Key::Letter('s'), PLAIN, true), vec![]);
    assert_eq!(keys.press(Key::Letter('x'), PLAIN, true), vec![]);
}

#[test]
fn typing_nullsoft_toggles_the_easter_egg() {
    let mut keys = Hotkeys::new();
    let mut last = Vec::new();
    for c in "nullsoft".chars() {
        last = keys.press(Key::Letter(c), PLAIN, false);
        if c == 's' {
            assert_eq!(last, vec![Control::ToggleShuffle], "S still shuffles");
        }
    }
    assert_eq!(last, vec![Control::ToggleEasterEgg]);
}

#[test]
fn escape_is_ignored_and_other_keys_restart_the_word() {
    let mut keys = Hotkeys::new();
    let mut type_ = |k: Key| keys.press(k, PLAIN, false);
    for c in "nu".chars() {
        type_(Key::Letter(c));
    }
    type_(Key::Escape);
    for c in "llsof".chars() {
        type_(Key::Letter(c));
    }
    assert_eq!(type_(Key::Letter('t')), vec![Control::ToggleEasterEgg]);
    for c in "nulx".chars() {
        type_(Key::Letter(c));
    }
    for c in "lsof".chars() {
        type_(Key::Letter(c));
    }
    assert_eq!(type_(Key::Letter('t')), vec![], "the x broke the word");
}

#[test]
fn the_wheel_steps_once_per_notch() {
    let mut wheel = Wheel::default();
    assert_eq!(wheel.steps(MouseScrollDelta::LineDelta(0.0, 1.0)), 1);
    assert_eq!(wheel.steps(MouseScrollDelta::LineDelta(0.0, -2.0)), -2);
    assert_eq!(wheel.steps(MouseScrollDelta::LineDelta(0.0, 0.5)), 0);
    assert_eq!(wheel.steps(MouseScrollDelta::LineDelta(0.0, 0.5)), 1);
    let px = |y| MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, y));
    assert_eq!(wheel.steps(px(12.0)), 0);
    assert_eq!(wheel.steps(px(12.0)), 1, "20 pixels per step");
    assert_eq!(wheel.steps(px(-50.0)), -2);
}

#[test]
fn double_clicks_are_two_presses_close_in_time_and_place() {
    let t0 = Instant::now();
    let ms = |n| t0 + Duration::from_millis(n);
    let mut dc = DoubleClick::default();
    assert!(!dc.press(ms(0), 10, 10));
    assert!(dc.press(ms(300), 12, 11));
    assert!(!dc.press(ms(400), 12, 11), "a third press starts over");
    assert!(!dc.press(ms(1000), 12, 11), "too late");
    assert!(!dc.press(ms(1100), 40, 11), "too far");
}
