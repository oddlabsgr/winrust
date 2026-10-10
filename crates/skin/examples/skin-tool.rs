//! Skin development tools.
//!
//! `cargo run -p winrust-skin --example skin-tool -- contact <skin.wsz|dir> <out.png>`
//! `cargo run -p winrust-skin --example skin-tool -- debug-wsz <out.wsz>`

use std::path::Path;
use std::process::ExitCode;
use winrust_skin::{Skin, contact_sheet, debug};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["contact", skin, out] => contact(Path::new(skin), Path::new(out)),
        ["debug-wsz", out] => std::fs::write(out, debug::wsz()).map_err(|e| e.to_string()),
        _ => Err("usage: skin-tool contact <skin> <out.png> | debug-wsz <out.wsz>".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn contact(skin: &Path, out: &Path) -> Result<(), String> {
    let skin = Skin::load(skin).map_err(|e| e.to_string())?;
    for warning in &skin.warnings {
        eprintln!("warning: {warning}");
    }
    std::fs::write(out, contact_sheet(&skin).to_png()).map_err(|e| e.to_string())
}
