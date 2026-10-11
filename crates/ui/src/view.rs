//! What the main window shows: a snapshot the app builds each frame, plus the formatting
//! rules for the time, slider and bitrate displays (Webamp's, at 999f567).

use std::time::Duration;
use winrust_skin::Image;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlayState {
    #[default]
    Stopped,
    Playing,
    Paused,
    /// A stream is filling its buffer.
    Buffering,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimeMode {
    #[default]
    Elapsed,
    Remaining,
}

#[derive(Clone, Debug, Default)]
pub struct MainView {
    pub state: PlayState,
    pub elapsed: Duration,
    /// `None` for streams, or while unknown.
    pub duration: Option<Duration>,
    pub time_mode: TimeMode,
    /// While paused the time blinks: true during the hidden half of each 2-second cycle.
    pub blink_hidden: bool,
    /// Scrolling text and how far it has scrolled, in pixels (see `Marquee`).
    pub title: String,
    pub title_offset: u32,
    pub kbps: Option<u32>,
    pub khz: Option<u32>,
    /// The source's channel count.
    pub channels: Option<u16>,
    /// 0..=100.
    pub volume: u8,
    /// -100 (left) ..= 100 (right).
    pub balance: i8,
    pub seekable: bool,
    pub shuffle: bool,
    pub repeat: bool,
    /// Lit state of the EQ and playlist buttons.
    pub eq: bool,
    pub playlist: bool,
    pub focused: bool,
    pub shade: bool,
    pub double_size: bool,
    pub always_on_top: bool,
    /// The "NO LLAMAS WERE HARMED"-style easter-egg title bar.
    pub easter_egg: bool,
    /// The visualizer's latest frame (76×16, or 38×5 in shade mode); `None` draws nothing.
    pub vis: Option<Image>,
}

/// The time display: four digits (m m : s s) and whether the minus sign shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeDigits {
    pub minus: bool,
    pub digits: [u8; 4],
}

/// Elapsed time, or remaining time with a minus sign when the duration is known. Minutes
/// above 99 show as hours:minutes.
pub fn time_digits(elapsed: Duration, duration: Option<Duration>, mode: TimeMode) -> TimeDigits {
    let (secs, minus) = match (mode, duration) {
        (TimeMode::Remaining, Some(d)) => (d.saturating_sub(elapsed).as_secs(), true),
        _ => (elapsed.as_secs(), false),
    };
    let (hi, lo) = if secs / 60 > 99 {
        ((secs / 3600).min(99), secs / 60 % 60)
    } else {
        (secs / 60, secs % 60)
    };
    let digit = |n: u64| (n % 10) as u8;
    TimeDigits {
        minus,
        digits: [digit(hi / 10), digit(hi), digit(lo / 10), digit(lo)],
    }
}

/// Volume slider frame (Webamp `MainVolume.tsx`).
pub fn volume_frame(volume: u8) -> u32 {
    let sprite = (f32::from(volume.min(100)) * 28.0 / 100.0).round() as u32;
    sprite.saturating_sub(1).min(27)
}

/// Balance slider frame (Webamp `MainBalance.tsx`).
pub fn balance_frame(balance: i8) -> u32 {
    u32::from(balance.unsigned_abs().min(100)) * 27 / 100
}

/// Left edge of a slider's thumb within its track, for a value fraction 0..=1.
pub fn thumb_offset(track_width: u32, thumb_width: u32, fraction: f32) -> u32 {
    ((track_width - thumb_width) as f32 * fraction.clamp(0.0, 1.0)).round() as u32
}

pub fn volume_fraction(volume: u8) -> f32 {
    f32::from(volume.min(100)) / 100.0
}

pub fn balance_fraction(balance: i8) -> f32 {
    (f32::from(balance.clamp(-100, 100)) + 100.0) / 200.0
}

/// The seek bar's position: whole seconds over the duration, in whole percent (Webamp
/// `Position.tsx`).
pub fn seek_fraction(elapsed: Duration, duration: Duration) -> f32 {
    if duration.is_zero() {
        return 0.0;
    }
    let percent = elapsed.as_secs() as f64 / duration.as_secs_f64() * 100.0;
    (percent.round().min(100.0) / 100.0) as f32
}

/// The balance clips to the centre within 25% of it (Webamp `setBalance`).
pub fn snap_balance(balance: i8) -> i8 {
    if balance.unsigned_abs() < 25 {
        0
    } else {
        balance
    }
}

pub fn volume_text(volume: u8) -> String {
    format!("Volume: {volume}%")
}

pub fn balance_text(balance: i8) -> String {
    match balance {
        0 => "Balance: Center".into(),
        b if b > 0 => format!("Balance: {b}% Right"),
        b => format!("Balance: {}% Left", b.unsigned_abs()),
    }
}

/// What the title shows while the seek bar is dragged to `fraction` of `duration`.
pub fn seek_text(fraction: f32, duration: Duration) -> String {
    let percent = (fraction.clamp(0.0, 1.0) * 100.0).round() as u64;
    let to = duration.as_secs() * percent / 100;
    format!(
        "Seek to: {}/{} ({percent}%)",
        clock(to),
        clock(duration.as_secs())
    )
}

fn clock(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

/// Kilobits per second in the 3-character cell.
pub fn kbps_text(kbps: u32) -> String {
    format!("{kbps:>3}").chars().take(3).collect()
}

/// Kilohertz in the 2-character cell.
pub fn khz_text(khz: u32) -> String {
    format!("{khz:>2}").chars().take(2).collect()
}
