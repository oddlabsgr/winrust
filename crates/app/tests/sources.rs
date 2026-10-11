use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use winrust_app::sources::{from_args, from_paths};
use winrust_audio::Source;

#[test]
fn urls_and_files_are_told_apart() {
    let got = from_args([
        OsString::from("HTTPS://radio.example/live"),
        OsString::from("icy://radio.example/x"),
        OsString::from("song.mp3"),
    ]);
    assert_eq!(
        got,
        vec![
            Source::Url("HTTPS://radio.example/live".into()),
            Source::Url("icy://radio.example/x".into()),
            Source::File(PathBuf::from("song.mp3")),
        ]
    );
}

#[test]
fn folders_expand_to_their_audio_files_in_name_order() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("sub")).unwrap();
    for f in ["b.mp3", "A.FLAC", "cover.jpg", "sub/c.ogg", "notes.txt"] {
        fs::write(root.join(f), b"").unwrap();
    }
    assert_eq!(
        from_paths(vec![root.to_path_buf()]),
        vec![
            Source::File(root.join("A.FLAC")),
            Source::File(root.join("b.mp3")),
            Source::File(root.join("sub").join("c.ogg")),
        ]
    );
}

#[test]
fn files_named_directly_are_kept_whatever_they_are() {
    let dir = tempfile::tempdir().unwrap();
    let odd = dir.path().join("track.xyz");
    fs::write(&odd, b"").unwrap();
    let missing = dir.path().join("missing.mp3");
    assert_eq!(
        from_paths(vec![odd.clone(), missing.clone()]),
        vec![Source::File(odd), Source::File(missing)]
    );
}

#[test]
fn folder_arguments_expand_too() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.wav"), b"").unwrap();
    assert_eq!(
        from_args([dir.path().as_os_str().to_owned()]),
        vec![Source::File(dir.path().join("a.wav"))]
    );
}
