use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use winrust_audio::Source;
use winrust_playlist::entry::Item;
use winrust_playlist::sources::{from_args, from_paths};

fn files(paths: &[PathBuf]) -> Vec<Item> {
    paths
        .iter()
        .map(|p| Source::File(p.clone()).into())
        .collect()
}

fn touch(root: &Path, names: &[&str]) {
    for name in names {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"").unwrap();
    }
}

#[test]
fn urls_and_files_are_told_apart() {
    let got = from_args([
        OsString::from("HTTPS://radio.example/live"),
        OsString::from("icy://radio.example/x"),
        OsString::from("song.mp3"),
    ]);
    assert_eq!(
        got,
        [
            Source::Url("HTTPS://radio.example/live".into()).into(),
            Source::Url("icy://radio.example/x".into()).into(),
            Source::File(std::env::current_dir().unwrap().join("song.mp3")).into(),
        ]
    );
}

#[test]
fn folders_expand_to_their_audio_files_in_name_order() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(
        root,
        &["b.mp3", "A.FLAC", "cover.jpg", "sub/c.ogg", "notes.txt"],
    );
    assert_eq!(
        from_paths(vec![root.to_path_buf()]),
        files(&[
            root.join("A.FLAC"),
            root.join("b.mp3"),
            root.join("sub").join("c.ogg")
        ])
    );
}

#[test]
fn folders_sort_one_name_at_a_time() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, &["Live 2001/a.mp3", "live/b.mp3", "Live/c.mp3"]);
    let got = from_paths(vec![root.to_path_buf()]);
    let names: Vec<_> = got
        .iter()
        .map(|i| match &i.source {
            Source::File(p) => p.strip_prefix(root).unwrap().to_path_buf(),
            Source::Url(_) => unreachable!(),
        })
        .collect();
    let first_folder = names[0].parent().unwrap().to_string_lossy().to_lowercase();
    assert_eq!(first_folder, "live", "{names:?}");
    assert_eq!(names[2], Path::new("Live 2001").join("a.mp3"));
}

#[test]
fn files_named_directly_are_kept_whatever_they_are() {
    let dir = tempfile::tempdir().unwrap();
    let odd = dir.path().join("track.xyz");
    fs::write(&odd, b"").unwrap();
    let missing = dir.path().join("missing.mp3");
    assert_eq!(
        from_paths(vec![odd.clone(), missing.clone()]),
        files(&[odd, missing])
    );
}

#[test]
fn folder_arguments_expand_too() {
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), &["a.wav"]);
    assert_eq!(
        from_args([dir.path().as_os_str().to_owned()]),
        files(&[dir.path().join("a.wav")])
    );
}

#[test]
fn playlist_files_expand_to_their_entries() {
    let dir = tempfile::tempdir().unwrap();
    let list = dir.path().join("mix.M3U8");
    fs::write(&list, "#EXTINF:3,Three\nsub/a.mp3\nhttp://radio.example/\n").unwrap();
    let expected = vec![
        Item {
            source: Source::File(dir.path().join("sub").join("a.mp3")),
            hint: Some("Three".into()),
            duration: Some(std::time::Duration::from_secs(3)),
        },
        Source::Url("http://radio.example/".into()).into(),
    ];
    assert_eq!(from_paths(vec![list.clone()]), expected);
    assert_eq!(from_args([list.into_os_string()]), expected);
}

#[test]
fn a_missing_playlist_file_adds_nothing() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(from_paths(vec![dir.path().join("gone.pls")]), []);
}

#[test]
fn playlist_files_inside_folders_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), &["a.mp3"]);
    fs::write(dir.path().join("list.m3u"), "a.mp3\na.mp3\n").unwrap();
    assert_eq!(
        from_paths(vec![dir.path().to_path_buf()]),
        files(&[dir.path().join("a.mp3")])
    );
}

#[cfg(unix)]
#[test]
fn linked_files_count_and_linked_folders_are_not_followed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(root, &["real/a.mp3"]);
    std::os::unix::fs::symlink(root.join("real").join("a.mp3"), root.join("link.mp3")).unwrap();
    std::os::unix::fs::symlink(root, root.join("real").join("loop")).unwrap();
    assert_eq!(
        from_paths(vec![root.to_path_buf()]),
        files(&[root.join("link.mp3"), root.join("real").join("a.mp3")])
    );
}

#[test]
fn relative_arguments_become_absolute_so_the_autosave_finds_them_again() {
    let cwd = std::env::current_dir().unwrap();
    let got = from_args([OsString::from("sub/x.mp3")]);
    assert_eq!(got, files(&[cwd.join("sub").join("x.mp3")]));
}
