//! `pledit.txt`: playlist colours and font.

use crate::ini;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistStyle {
    pub normal: u32,
    pub current: u32,
    pub normal_bg: u32,
    pub selected_bg: u32,
    pub font: String,
}

impl Default for PlaylistStyle {
    fn default() -> Self {
        PlaylistStyle {
            normal: 0xFF00_FF00,
            current: 0xFFFF_FFFF,
            normal_bg: 0xFF00_0000,
            selected_bg: 0xFF00_00FF,
            font: "Arial".into(),
        }
    }
}

/// Reads the `[Text]` section; missing or unreadable entries keep their defaults.
pub fn parse(text: &str) -> PlaylistStyle {
    let mut style = PlaylistStyle::default();
    let ini = ini::parse(text);
    let Some(section) = ini.get("text") else {
        return style;
    };
    let colors = [
        ("normal", &mut style.normal),
        ("current", &mut style.current),
        ("normalbg", &mut style.normal_bg),
        ("selectedbg", &mut style.selected_bg),
    ];
    for (key, slot) in colors {
        if let Some(color) = section.get(key).and_then(|v| color(v)) {
            *slot = color;
        }
    }
    if let Some(font) = section.get("font").filter(|f| !f.is_empty()) {
        style.font = font.clone();
    }
    style
}

/// `#RRGGBB` with the `#` optional and anything past six digits ignored (skins in the wild
/// carry extra characters, which Winamp tolerates); `#RGB` shorthand as in CSS.
fn color(value: &str) -> Option<u32> {
    let hex = value.trim();
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    let digits: String = hex.chars().take(6).collect();
    let full = match digits.len() {
        6 => digits,
        3 => digits.chars().flat_map(|c| [c, c]).collect(),
        _ => return None,
    };
    u32::from_str_radix(&full, 16)
        .ok()
        .map(|rgb| 0xFF00_0000 | rgb)
}
