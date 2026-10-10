#![allow(dead_code)]

use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use winrust_skin::{Image, bmp};

pub fn solid_bmp(w: u32, h: u32, color: u32) -> Vec<u8> {
    bmp::encode24(&Image::new(w, h, color))
}

/// A deflated zip of (name, contents) entries, in order.
pub fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
    out.into_inner()
}

/// Compares `img` with `tests/golden/<name>.png`. `WINRUST_BLESS=1` rewrites the file.
pub fn assert_golden(name: &str, img: &Image) {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "golden",
        &format!("{name}.png"),
    ]
    .iter()
    .collect();
    if std::env::var_os("WINRUST_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, img.to_png()).unwrap();
        return;
    }
    let expected = std::fs::read(&path)
        .ok()
        .and_then(|png| bmp::decode_any(&png).ok())
        .unwrap_or_else(|| panic!("missing {}; run with WINRUST_BLESS=1", path.display()));
    if expected != *img {
        let actual = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.actual.png"));
        std::fs::write(&actual, img.to_png()).unwrap();
        let differ = if (expected.width, expected.height) == (img.width, img.height) {
            let n = expected
                .pixels
                .iter()
                .zip(&img.pixels)
                .filter(|(a, b)| a != b)
                .count();
            format!("{n} pixels differ")
        } else {
            format!(
                "size {}x{} != {}x{}",
                img.width, img.height, expected.width, expected.height
            )
        };
        panic!(
            "golden {name}: {differ}; actual image at {}",
            actual.display()
        );
    }
}
