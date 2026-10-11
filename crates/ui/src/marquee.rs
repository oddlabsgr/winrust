//! The scrolling song title (Webamp `Marquee.tsx`): text longer than the marquee loops with a
//! separator, stepping one 5-pixel character every 220 ms. Dragging scrubs it and pauses the
//! stepping until a second after the drag ends. The app drives time with [`Marquee::tick`].

use std::time::Duration;

pub const STEP: Duration = Duration::from_millis(220);
pub const RESUME_AFTER: Duration = Duration::from_secs(1);
pub const MESSAGE_FOR: Duration = Duration::from_secs(3);
pub const SEPARATOR: &str = "  ***  ";
const CHAR_WIDTH: u32 = 5;
/// Texts this long (or longer) do not fit the 154-pixel marquee and scroll.
const LONG: usize = 31;

/// Winamp's title line: `"<n>. <title> (<m:ss>)"`, without the time for streams.
pub fn track_title(number: usize, title: &str, duration: Option<Duration>) -> String {
    match duration {
        Some(d) => {
            let s = d.as_secs();
            format!("{number}. {title} ({}:{:02})", s / 60, s % 60)
        }
        None => format!("{number}. {title}"),
    }
}

fn is_long(text: &str) -> bool {
    text.chars().count() >= LONG
}

/// What the marquee draws: long text twice around the separator, so the loop is seamless;
/// short text padded with spaces to 31 cells, so every cell comes from `text.bmp` (Webamp
/// `loopText`).
pub fn looped(text: &str) -> String {
    if is_long(text) {
        format!("{text}{SEPARATOR}{text}")
    } else {
        format!("{text:<LONG$}")
    }
}

#[derive(Clone, Debug, Default)]
pub struct Marquee {
    text: String,
    /// A message shown instead of the text, with the time it has left.
    message: Option<(String, Duration)>,
    step: u32,
    /// Pixels the text was dragged by, positive to the left.
    scrub: i32,
    since_step: Duration,
    /// Time left before stepping resumes after a drag.
    hold: Duration,
    dragging: bool,
}

impl Marquee {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the text; a different text starts from the beginning.
    pub fn set_text(&mut self, text: &str) {
        if text != self.text {
            self.text = text.to_string();
            self.restart();
        }
    }

    /// Shows `message` instead of the text for [`MESSAGE_FOR`].
    pub fn show_message(&mut self, message: &str) {
        self.message = Some((message.to_string(), MESSAGE_FOR));
        self.restart();
    }

    fn restart(&mut self) {
        self.step = 0;
        self.scrub = 0;
        self.since_step = Duration::ZERO;
    }

    /// The text shown now (a message, or the title).
    pub fn text(&self) -> &str {
        match &self.message {
            Some((message, _)) => message,
            None => &self.text,
        }
    }

    /// How far the looped text is scrolled, in pixels.
    pub fn offset(&self) -> u32 {
        let text = self.text();
        if !is_long(text) {
            return 0;
        }
        let width = ((text.chars().count() + SEPARATOR.len()) as u32 * CHAR_WIDTH) as i64;
        (i64::from(self.step) * i64::from(CHAR_WIDTH) + i64::from(self.scrub)).rem_euclid(width)
            as u32
    }

    /// Advances time by `dt`.
    pub fn tick(&mut self, mut dt: Duration) {
        if let Some((_, left)) = &mut self.message {
            if dt < *left {
                *left -= dt;
            } else {
                self.message = None;
                self.restart();
            }
        }
        if self.dragging {
            return;
        }
        if self.hold > Duration::ZERO {
            if dt < self.hold {
                self.hold -= dt;
                return;
            }
            dt -= self.hold;
            self.hold = Duration::ZERO;
            self.since_step = Duration::ZERO;
        }
        self.since_step += dt;
        while self.since_step >= STEP {
            self.since_step -= STEP;
            self.step += 1;
        }
    }

    /// The pointer dragged the text `dx` pixels (positive to the right).
    pub fn drag(&mut self, dx: i32) {
        self.dragging = true;
        self.scrub -= dx;
    }

    pub fn end_drag(&mut self) {
        self.dragging = false;
        self.hold = RESUME_AFTER;
    }
}
