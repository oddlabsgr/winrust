use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use winrust_audio::Source;
use winrust_playlist::entry::{Item, Tags};
use winrust_playlist::files::{decode, parse_m3u, parse_pls, read, write_m3u8, write_pls};
use winrust_playlist::playlist::Playlist;
use winrust_playlist::title::Template;

fn base() -> PathBuf {
    PathBuf::from("lists")
}

fn item(source: Source, hint: Option<&str>, secs: Option<u64>) -> Item {
    Item {
        source,
        hint: hint.map(String::from),
        duration: secs.map(Duration::from_secs),
    }
}

fn local(name: &str) -> Source {
    Source::File(base().join(name))
}

#[test]
fn m3u_reads_extinf_titles_and_durations() {
    let text = "#EXTM3U\n#EXTINF:123,Artist - Song\nsong.mp3\n#EXTINF:-1,Radio\nhttp://radio.example/live\nplain.mp3\n";
    assert_eq!(
        parse_m3u(text, &base()),
        [
            item(local("song.mp3"), Some("Artist - Song"), Some(123)),
            item(
                Source::Url("http://radio.example/live".into()),
                Some("Radio"),
                None
            ),
            item(local("plain.mp3"), None, None),
        ]
    );
}

#[test]
fn an_extinf_line_describes_only_the_next_location() {
    let text = "#EXTINF:5,A\r\n\r\n# a comment\r\na.mp3\r\nb.mp3\r\n";
    assert_eq!(
        parse_m3u(text, &base()),
        [
            item(local("a.mp3"), Some("A"), Some(5)),
            item(local("b.mp3"), None, None),
        ]
    );
}

#[test]
fn extinf_attributes_and_odd_numbers_are_tolerated() {
    let text = "#extinf:-1 tvg-id=\"x\",Name\nhttp://tv.example/\n#EXTINF:1e300,Huge\nhuge.mp3\n#EXTINF:abc\nbad.mp3\n";
    assert_eq!(
        parse_m3u(text, &base()),
        [
            item(Source::Url("http://tv.example/".into()), Some("Name"), None),
            item(local("huge.mp3"), Some("Huge"), None),
            item(local("bad.mp3"), None, None),
        ]
    );
}

#[test]
fn pls_reads_entries_in_number_order_whatever_the_key_case() {
    let text = "[playlist]\nfile2=b.mp3\nFILE1=a.mp3\nTitle1=A\nlength1=60\nFile4=d.mp3\nTitle3=no file\nNumberOfEntries=3\nVersion=2\n";
    assert_eq!(
        parse_pls(text, &base()),
        [
            item(local("a.mp3"), Some("A"), Some(60)),
            item(local("b.mp3"), None, None),
            item(local("d.mp3"), None, None),
        ]
    );
}

#[test]
fn absolute_paths_stay_and_relative_ones_join_the_playlist_folder() {
    let dir = tempfile::tempdir().unwrap();
    let absolute = dir.path().join("x.mp3");
    let text = format!("{}\nsub/a.mp3\n", absolute.display());
    assert_eq!(
        parse_m3u(&text, &base()),
        [
            item(Source::File(absolute), None, None),
            item(Source::File(base().join("sub").join("a.mp3")), None, None),
        ]
    );
}

#[test]
fn backslashes_separate_folders_on_every_os() {
    assert_eq!(
        parse_m3u("sub\\a.mp3", &base()),
        [item(
            Source::File(base().join("sub").join("a.mp3")),
            None,
            None
        )]
    );
}

#[test]
fn file_urls_become_paths_and_stream_urls_pass_through() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a b.mp3");
    let url = url::Url::from_file_path(&path).unwrap();
    let text = format!("{url}\nHTTPS://radio.example/x\nicy://radio.example/y\n");
    assert_eq!(
        parse_m3u(&text, &base()),
        [
            item(Source::File(path), None, None),
            item(Source::Url("HTTPS://radio.example/x".into()), None, None),
            item(Source::Url("icy://radio.example/y".into()), None, None),
        ]
    );
}

#[test]
fn text_is_utf8_after_a_byte_order_mark() {
    assert_eq!(decode(b"\xEF\xBB\xBFcaf\xC3\xA9", true), "café");
}

#[test]
fn a_legacy_m3u_that_is_not_utf8_is_windows_1252() {
    assert_eq!(decode(b"caf\xE9 \x80 \x93x\x94", true), "café € “x”");
    assert_eq!(decode(b"caf\xE9", false), "caf\u{FFFD}");
}

#[test]
fn read_picks_the_format_and_encoding_by_extension() {
    let dir = tempfile::tempdir().unwrap();
    let m3u = dir.path().join("old.M3U");
    fs::write(&m3u, b"#EXTINF:1,Caf\xE9\nsong.mp3\n").unwrap();
    let pls = dir.path().join("list.pls");
    fs::write(&pls, "[playlist]\nFile1=a.mp3\n").unwrap();
    assert_eq!(
        read(&m3u).unwrap(),
        [item(
            Source::File(dir.path().join("song.mp3")),
            Some("Café"),
            Some(1)
        )]
    );
    assert_eq!(
        read(&pls).unwrap(),
        [item(Source::File(dir.path().join("a.mp3")), None, None)]
    );
    assert!(read(&dir.path().join("missing.m3u8")).is_err());
}

/// A playlist with a tagged file in `dir`, an untagged one in a subfolder, a file elsewhere
/// and a stream.
fn sample(dir: &Path, elsewhere: &Path) -> Playlist {
    let mut pl = Playlist::new();
    pl.append(vec![
        item(Source::File(dir.join("a.mp3")), None, Some(61)),
        item(Source::File(dir.join("sub").join("b.mp3")), None, None),
        item(Source::File(elsewhere.join("c.mp3")), None, Some(5)),
        item(Source::Url("http://radio.example/".into()), None, None),
    ]);
    let first = pl.entries()[0].id();
    pl.get_mut(first).unwrap().tags = Some(Tags {
        artist: Some("Artist".into()),
        title: Some("Line\nbreak".into()),
        ..Tags::default()
    });
    pl
}

#[test]
fn m3u8_is_written_with_relative_paths_under_the_base() {
    let dir = PathBuf::from("music");
    let pl = sample(&dir, Path::new("/elsewhere"));
    let text = write_m3u8(&pl.entries()[..1], &Template::default(), Some(&dir));
    assert_eq!(text, "#EXTM3U\n#EXTINF:61,Artist - Line break\na.mp3\n");
    let text = write_m3u8(&pl.entries()[3..], &Template::default(), None);
    assert_eq!(
        text,
        "#EXTM3U\n#EXTINF:-1,http://radio.example/\nhttp://radio.example/\n"
    );
}

#[test]
fn m3u8_and_pls_read_back_what_was_written() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let pl = sample(dir.path(), elsewhere.path());
    let t = Template::default();
    let expected: Vec<Item> = pl
        .entries()
        .iter()
        .map(|e| Item {
            source: e.source.clone(),
            hint: Some(e.title(&t).replace('\n', " ")),
            duration: e.duration,
        })
        .collect();
    for base in [Some(dir.path()), None] {
        let m3u8 = write_m3u8(pl.entries(), &t, base);
        assert_eq!(parse_m3u(&m3u8, dir.path()), expected, "{m3u8}");
        let pls = write_pls(pl.entries(), &t, base);
        assert_eq!(parse_pls(&pls, dir.path()), expected, "{pls}");
    }
}

#[test]
fn an_old_pls_that_is_not_utf8_is_windows_1252_too() {
    // Winamp 2.x wrote PLS files in the ANSI code page.
    let dir = tempfile::tempdir().unwrap();
    let pls = dir.path().join("old.pls");
    fs::write(&pls, b"[playlist]\nFile1=Bj\xF6rk.mp3\n").unwrap();
    assert_eq!(
        read(&pls).unwrap(),
        [item(Source::File(dir.path().join("Björk.mp3")), None, None)]
    );
}
