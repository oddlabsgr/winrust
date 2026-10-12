use std::path::PathBuf;
use winrust_audio::Source;
use winrust_playlist::entry::{Entry, Item, Tags};
use winrust_playlist::playlist::Playlist;
use winrust_playlist::title::Template;

fn tags() -> Tags {
    Tags {
        artist: Some("Artist".into()),
        title: Some("Title".into()),
        album: Some("Album".into()),
        year: Some(1999),
        track: Some(7),
    }
}

fn render(template: &str, tags: &Tags) -> String {
    Template::parse(template).render(tags, "song")
}

fn entry(source: Source, hint: Option<&str>, tags: Option<Tags>) -> Entry {
    let mut pl = Playlist::new();
    pl.append(vec![Item {
        source,
        hint: hint.map(String::from),
        duration: None,
    }]);
    let id = pl.entries()[0].id();
    pl.get_mut(id).unwrap().tags = tags;
    pl.entries()[0].clone()
}

fn song() -> Source {
    Source::File(PathBuf::from("music").join("song.mp3"))
}

#[test]
fn the_default_template_joins_artist_and_title() {
    let t = Template::default();
    assert_eq!(t.render(&tags(), "song"), "Artist - Title");
    let no_artist = Tags {
        artist: None,
        ..tags()
    };
    assert_eq!(t.render(&no_artist, "song"), "Title");
}

#[test]
fn every_field_expands() {
    assert_eq!(
        render(
            "%track%|%artist%|%album%|%year%|%title%|%filename%",
            &tags()
        ),
        "7|Artist|Album|1999|Title|song"
    );
}

#[test]
fn field_names_ignore_case() {
    assert_eq!(render("%ARTIST%/%Title%", &tags()), "Artist/Title");
}

#[test]
fn a_section_vanishes_when_a_field_inside_is_empty() {
    let no_album = Tags {
        album: None,
        ..tags()
    };
    assert_eq!(render("[%album% - ]%title%", &no_album), "Title");
    assert_eq!(render("[%album% - ]%title%", &tags()), "Album - Title");
    assert_eq!(render("[no fields]", &no_album), "no fields");
}

#[test]
fn nested_sections_decide_for_themselves() {
    let t = "[%artist%[ (%year%)] - ]%title%";
    let no_year = Tags {
        year: None,
        ..tags()
    };
    assert_eq!(render(t, &tags()), "Artist (1999) - Title");
    assert_eq!(render(t, &no_year), "Artist - Title");
    let no_artist = Tags {
        artist: None,
        ..tags()
    };
    assert_eq!(render(t, &no_artist), "Title");
}

#[test]
fn doubled_marks_are_literal() {
    assert_eq!(render("%%[[x]]", &tags()), "%[x]");
}

#[test]
fn unknown_fields_expand_to_nothing() {
    assert_eq!(render("%genre%x", &tags()), "x");
    assert_eq!(render("[%genre% - ]%title%", &tags()), "Title");
}

#[test]
fn broken_syntax_is_taken_literally() {
    assert_eq!(render("50% [%title%", &tags()), "50% [Title");
    assert_eq!(render("a]b", &tags()), "a]b");
    assert_eq!(render("%title", &tags()), "%title");
    assert_eq!(render("100%", &tags()), "100%");
    assert_eq!(render("%%%", &tags()), "%%");
    assert_eq!(render("", &tags()), "");
}

#[test]
fn the_title_falls_back_to_the_file_name() {
    let t = Template::default();
    let untitled = Tags {
        title: None,
        ..tags()
    };
    assert_eq!(entry(song(), None, Some(untitled)).title(&t), "song");
    let blank = Template::parse("%genre%");
    assert_eq!(entry(song(), None, Some(tags())).title(&blank), "song");
    assert_eq!(
        entry(song(), None, Some(tags())).title(&t),
        "Artist - Title"
    );
}

#[test]
fn the_playlist_file_title_shows_until_the_tags_are_read() {
    let t = Template::default();
    assert_eq!(
        entry(song(), Some("From the list"), None).title(&t),
        "From the list"
    );
    assert_eq!(entry(song(), None, None).title(&t), "song");
    let read = entry(song(), Some("From the list"), Some(tags()));
    assert_eq!(read.title(&t), "Artist - Title");
}

#[test]
fn a_stream_is_named_by_its_url() {
    let url = Source::Url("http://radio.example/live".into());
    let e = entry(url, None, None);
    assert_eq!(e.filename(), "http://radio.example/live");
    assert_eq!(e.title(&Template::default()), "http://radio.example/live");
}
