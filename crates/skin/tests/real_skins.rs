//! Loads every skin in `WINRUST_SKINS_DIR` and writes a contact sheet for each, to compare
//! with Webamp by eye. No real skins are committed: they belong to their authors. Run:
//!
//! `WINRUST_SKINS_DIR=<dir> cargo test -p winrust-skin --test real_skins -- --ignored --nocapture`

use std::path::Path;
use winrust_skin::{Skin, contact_sheet};

#[test]
#[ignore = "needs WINRUST_SKINS_DIR"]
fn real_skins_load() {
    let dir = std::env::var_os("WINRUST_SKINS_DIR").expect("set WINRUST_SKINS_DIR");
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("real-skins");
    std::fs::create_dir_all(&out).unwrap();
    let (mut count, mut failures) = (0, Vec::new());
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let is_skin = path.is_dir()
            || path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("wsz") || e.eq_ignore_ascii_case("zip"));
        if !is_skin {
            continue;
        }
        count += 1;
        match Skin::load(&path) {
            Ok(skin) => {
                for w in &skin.warnings {
                    println!("{}: warning: {w}", path.display());
                }
                let name = path.file_stem().unwrap().to_string_lossy();
                let png = contact_sheet(&skin).to_png();
                std::fs::write(out.join(format!("{name}.png")), png).unwrap();
            }
            Err(e) => failures.push(format!("{}: {e}", path.display())),
        }
    }
    println!("{count} skins; contact sheets in {}", out.display());
    assert!(count > 0, "no skins found");
    assert!(failures.is_empty(), "{failures:#?}");
}
