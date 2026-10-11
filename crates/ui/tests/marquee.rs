use std::time::Duration;
use winrust_ui::marquee::{self, Marquee};

const STEP: Duration = Duration::from_millis(220);
const LONG: &str = "1. A song title much longer than thirty characters (3:45)";

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn track_titles_read_like_winamps() {
    let title =
        |n, name, secs: Option<u64>| marquee::track_title(n, name, secs.map(Duration::from_secs));
    assert_eq!(
        title(1, "Artist - Title", Some(225)),
        "1. Artist - Title (3:45)"
    );
    assert_eq!(
        title(2, "Long one", Some(75 * 60 + 2)),
        "2. Long one (75:02)"
    );
    // Streams have no duration.
    assert_eq!(title(12, "Radio", None), "12. Radio");
}

#[test]
fn long_text_loops_with_a_separator() {
    assert_eq!(marquee::looped(LONG), format!("{LONG}  ***  {LONG}"));
    // 30 characters fit the 154-pixel marquee; it still draws 31 cells.
    let fits = "x".repeat(30);
    assert_eq!(marquee::looped(&fits), format!("{fits} "));
}

#[test]
fn short_text_stays_put() {
    let mut m = Marquee::new();
    m.set_text("1. Short (1:00)");
    m.tick(STEP * 10);
    m.drag(-12);
    assert_eq!(m.offset(), 0);
    assert_eq!(m.text(), "1. Short (1:00)");
}

#[test]
fn long_text_steps_one_character_every_220_ms() {
    let mut m = Marquee::new();
    m.set_text(LONG);
    m.tick(ms(219));
    assert_eq!(m.offset(), 0);
    m.tick(ms(1));
    assert_eq!(m.offset(), 5);
    m.tick(STEP * 2);
    assert_eq!(m.offset(), 15);
}

#[test]
fn scrolling_wraps_after_the_text_and_separator() {
    let mut m = Marquee::new();
    m.set_text(LONG);
    let chars = LONG.chars().count() as u32 + 7;
    m.tick(STEP * chars);
    assert_eq!(m.offset(), 0);
    m.tick(STEP);
    assert_eq!(m.offset(), 5);
}

#[test]
fn dragging_scrubs_and_stepping_resumes_a_second_after() {
    let mut m = Marquee::new();
    m.set_text(LONG);
    // Dragging left moves the text left.
    m.drag(-12);
    assert_eq!(m.offset(), 12);
    m.tick(STEP * 3);
    assert_eq!(m.offset(), 12);
    m.end_drag();
    m.tick(ms(999));
    assert_eq!(m.offset(), 12);
    m.tick(ms(1) + ms(219));
    assert_eq!(m.offset(), 12);
    m.tick(ms(1));
    assert_eq!(m.offset(), 17);
    // Dragging right past the start wraps around.
    m.drag(20);
    let looped_width = (LONG.chars().count() as u32 + 7) * 5;
    assert_eq!(m.offset(), looped_width - 3);
}

#[test]
fn new_text_starts_from_the_beginning() {
    let mut m = Marquee::new();
    m.set_text(LONG);
    m.tick(STEP * 4);
    m.set_text(LONG);
    assert_eq!(m.offset(), 20);
    m.set_text(&format!("{LONG}!"));
    assert_eq!(m.offset(), 0);
}

#[test]
fn messages_replace_the_text_for_three_seconds() {
    let mut m = Marquee::new();
    m.set_text(LONG);
    m.tick(STEP * 4);
    m.show_message("Cannot open file");
    assert_eq!(m.text(), "Cannot open file");
    assert_eq!(m.offset(), 0);
    m.tick(ms(2999));
    assert_eq!(m.text(), "Cannot open file");
    m.tick(ms(1));
    assert_eq!(m.text(), LONG);
    assert_eq!(m.offset(), 0);
}

#[test]
fn short_text_is_padded_to_31_cells() {
    // Webamp pads to 31 characters, so the whole marquee is drawn from text.bmp.
    assert_eq!(marquee::looped("abc"), format!("abc{}", " ".repeat(28)));
    assert_eq!(marquee::looped(""), " ".repeat(31));
}

#[test]
fn next_change_says_when_the_text_moves_again() {
    let mut m = Marquee::new();
    m.set_text("short");
    assert_eq!(m.next_change(), None, "nothing moves");
    m.set_text(LONG);
    assert_eq!(m.next_change(), Some(STEP));
    m.tick(ms(100));
    assert_eq!(m.next_change(), Some(ms(120)));
    m.drag(5);
    assert_eq!(m.next_change(), None, "held while dragged");
    m.end_drag();
    assert_eq!(m.next_change(), Some(ms(1000) + STEP));
    m.set_text("short");
    m.show_message("hi");
    assert_eq!(m.next_change(), Some(ms(3000)), "the message expires");
}
