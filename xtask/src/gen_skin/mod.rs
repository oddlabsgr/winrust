//! Generator of Winrust Base, the default skin (`cargo xtask gen-skin`, spec §7.6): original
//! pixel art drawn by code. Its output is committed under `assets/skin/`.

pub mod draw;
pub mod font;
pub mod icons;
pub mod palette;
pub mod sheets;

use std::path::Path;
use winrust_skin::{Sheet, bmp, viscolor};

/// Winrust Base's playlist colours.
pub const PLEDIT: &str =
    "[Text]\nNormal=#00E000\nCurrent=#FFFFFF\nNormalBG=#000000\nSelectedBG=#6B2A12\nFont=Arial\n";

/// The classic visualization colours, one `r,g,b` line each.
pub fn viscolor_txt() -> String {
    let mut out = String::new();
    for (i, c) in viscolor::DEFAULT_COLORS.iter().enumerate() {
        let what = match i {
            0 => "background",
            1 => "grid dots",
            2..=17 => "spectrum, top to bottom",
            18..=22 => "oscilloscope",
            _ => "peak dots",
        };
        let [_, r, g, b] = c.to_be_bytes();
        out.push_str(&format!("{r},{g},{b}, // {what}\n"));
    }
    out
}

/// Every file of the skin, by name: one BMP per sheet, `pledit.txt` and `viscolor.txt`.
pub fn files() -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = Sheet::ALL
        .iter()
        .zip(sheets::all())
        .map(|(sheet, image)| (format!("{}.bmp", sheet.stem()), bmp::encode24(&image)))
        .collect();
    out.push(("pledit.txt".into(), PLEDIT.as_bytes().to_vec()));
    out.push(("viscolor.txt".into(), viscolor_txt().into_bytes()));
    out
}

/// Writes the skin into `dir`; returns the number of files.
pub fn write(dir: &Path) -> std::io::Result<usize> {
    std::fs::create_dir_all(dir)?;
    let files = files();
    for (name, data) in &files {
        std::fs::write(dir.join(name), data)?;
    }
    Ok(files.len())
}
