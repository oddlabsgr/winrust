//! Skin development tools.
//!
//! `cargo run -p winrust-ui --example skin-tool -- contact <skin.wsz|dir> <out.png>`
//! `cargo run -p winrust-ui --example skin-tool -- debug-wsz <out.wsz>`
//! `cargo run -p winrust-ui --example skin-tool -- render <skin|base|debug> <state> <out.png>`
//!
//! `render` draws the main window in one state: stopped, playing, paused, buffering or shade.
//! The skin is a `.wsz`, `.zip` or folder, or one of the built-in skins.

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;
use winrust_skin::{Skin, contact_sheet, debug};
use winrust_ui::draw;
use winrust_ui::interaction::Interaction;
use winrust_ui::view::{MainView, PlayState};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["contact", skin, out] => load(skin).and_then(|s| write(out, &contact_sheet(&s))),
        ["debug-wsz", out] => std::fs::write(out, debug::wsz()).map_err(|e| e.to_string()),
        ["render", skin, state, out] => load(skin).and_then(|s| render(&s, state, out)),
        _ => Err(
            "usage: skin-tool contact <skin> <out.png> | debug-wsz <out.wsz> \
                  | render <skin|base|debug> <stopped|playing|paused|buffering|shade> <out.png>"
                .into(),
        ),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// A skin file or folder, or the built-in `base` or `debug` skin.
fn load(skin: &str) -> Result<Arc<Skin>, String> {
    let skin = match skin {
        "base" => return Ok(Skin::base()),
        "debug" => return Ok(Arc::new(Skin::debug())),
        path => Skin::load(Path::new(path)).map_err(|e| e.to_string())?,
    };
    for warning in &skin.warnings {
        eprintln!("warning: {warning}");
    }
    Ok(Arc::new(skin))
}

fn write(out: &str, img: &winrust_skin::Image) -> Result<(), String> {
    std::fs::write(out, img.to_png()).map_err(|e| e.to_string())
}

fn render(skin: &Skin, state: &str, out: &str) -> Result<(), String> {
    let playing = MainView {
        state: PlayState::Playing,
        elapsed: Duration::from_secs(83),
        duration: Some(Duration::from_secs(296)),
        title: "1. Artist - Title (4:56)".into(),
        kbps: Some(128),
        khz: Some(44),
        channels: Some(2),
        volume: 75,
        seekable: true,
        focused: true,
        ..MainView::default()
    };
    let view = match state {
        "stopped" => MainView {
            title: "Winrust".into(),
            focused: true,
            ..MainView::default()
        },
        "playing" => playing,
        "paused" => MainView {
            state: PlayState::Paused,
            ..playing
        },
        "buffering" => MainView {
            state: PlayState::Buffering,
            title: "1. Radio stream".into(),
            duration: None,
            seekable: false,
            ..playing
        },
        "shade" => MainView {
            shade: true,
            ..playing
        },
        other => return Err(format!("unknown state {other}")),
    };
    write(out, &draw::render(skin, &view, &Interaction::new()))
}
