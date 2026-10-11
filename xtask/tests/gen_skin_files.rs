use std::path::Path;
use winrust_skin::pledit::{self, PlaylistStyle};
use winrust_skin::{Sheet, bmp, viscolor};
use xtask::gen_skin;

#[test]
fn one_bitmap_per_sheet_plus_two_text_files() {
    let names: Vec<String> = gen_skin::files().into_iter().map(|(n, _)| n).collect();
    let mut want: Vec<String> = Sheet::ALL
        .iter()
        .map(|s| format!("{}.bmp", s.stem()))
        .collect();
    want.extend(["pledit.txt".into(), "viscolor.txt".into()]);
    assert_eq!(names, want);
}

#[test]
fn bitmaps_decode_at_their_sheet_size() {
    for (sheet, (name, data)) in Sheet::ALL.iter().zip(gen_skin::files()) {
        let img = bmp::decode(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!((img.width, img.height), sheet.size(), "{name}");
    }
}

#[test]
fn text_files_parse_as_intended() {
    assert_eq!(
        pledit::parse(gen_skin::PLEDIT),
        PlaylistStyle {
            normal: 0xFF00_E000,
            current: 0xFFFF_FFFF,
            normal_bg: 0xFF00_0000,
            selected_bg: 0xFF6B_2A12,
            font: "Arial".into(),
        }
    );
    assert_eq!(
        viscolor::parse(&gen_skin::viscolor_txt()),
        viscolor::DEFAULT_COLORS
    );
}

#[test]
fn committed_skin_is_up_to_date() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/skin");
    let files = gen_skin::files();
    for (name, data) in &files {
        let committed = std::fs::read(dir.join(name)).unwrap_or_default();
        assert!(
            committed == *data,
            "assets/skin/{name} is stale: run `cargo xtask gen-skin` and commit the result"
        );
    }
    let on_disk = std::fs::read_dir(&dir).unwrap().count();
    assert_eq!(
        on_disk,
        files.len(),
        "assets/skin holds files the generator does not make"
    );
}
