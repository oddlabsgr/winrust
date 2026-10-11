//! Where the main window's widgets are, at 1× (Webamp `css/main-window.css` at 999f567).

use winrust_skin::Rect;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mode {
    #[default]
    Normal,
    /// The 14-pixel "windowshade" bar.
    Shade,
}

impl Mode {
    /// Window size at 1×.
    pub fn size(self) -> (u32, u32) {
        match self {
            Mode::Normal => (275, 116),
            Mode::Shade => (275, 14),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Widget {
    TitleBar,
    Options,
    Minimize,
    Shade,
    Close,
    ClutterO,
    ClutterA,
    ClutterI,
    ClutterD,
    ClutterV,
    Time,
    Vis,
    Marquee,
    Volume,
    Balance,
    Eq,
    Playlist,
    Seek,
    Previous,
    Play,
    Pause,
    Stop,
    Next,
    Eject,
    Shuffle,
    Repeat,
    About,
}

/// How a widget takes the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Shows pressed while held; fires when released over it.
    Button,
    /// A thumb dragged along a track.
    Slider,
    /// Dragging it moves something else: the window, or the title text.
    Drag,
}

impl Widget {
    pub fn kind(self) -> Kind {
        match self {
            Widget::Volume | Widget::Balance | Widget::Seek => Kind::Slider,
            Widget::TitleBar | Widget::Marquee => Kind::Drag,
            _ => Kind::Button,
        }
    }
}

const fn r(x: u32, y: u32, w: u32, h: u32) -> Rect {
    Rect::new(x, y, w, h)
}

/// Normal-mode widgets, topmost first.
const NORMAL: [(Widget, Rect); 27] = [
    (Widget::Options, r(6, 3, 9, 9)),
    (Widget::Minimize, r(244, 3, 9, 9)),
    (Widget::Shade, r(254, 3, 9, 9)),
    (Widget::Close, r(264, 3, 9, 9)),
    (Widget::TitleBar, r(0, 0, 275, 14)),
    (Widget::ClutterO, r(10, 25, 8, 8)),
    (Widget::ClutterA, r(10, 33, 8, 7)),
    (Widget::ClutterI, r(10, 40, 8, 7)),
    (Widget::ClutterD, r(10, 47, 8, 8)),
    (Widget::ClutterV, r(10, 55, 8, 7)),
    (Widget::Time, r(39, 26, 59, 13)),
    (Widget::Vis, r(24, 43, 76, 16)),
    (Widget::Marquee, r(111, 24, 154, 12)),
    // The volume input is 3 pixels shorter than its 68-pixel background (Webamp).
    (Widget::Volume, r(107, 57, 65, 13)),
    (Widget::Balance, r(177, 57, 38, 13)),
    (Widget::Eq, r(219, 58, 23, 12)),
    (Widget::Playlist, r(242, 58, 23, 12)),
    (Widget::Seek, r(16, 72, 248, 10)),
    (Widget::Previous, r(16, 88, 23, 18)),
    (Widget::Play, r(39, 88, 23, 18)),
    (Widget::Pause, r(62, 88, 23, 18)),
    (Widget::Stop, r(85, 88, 23, 18)),
    (Widget::Next, r(108, 88, 22, 18)),
    (Widget::Eject, r(136, 89, 22, 16)),
    // Repeat overlaps shuffle by one column and sits on top, as in Webamp.
    (Widget::Repeat, r(210, 89, 28, 15)),
    (Widget::Shuffle, r(164, 89, 47, 15)),
    (Widget::About, r(253, 91, 13, 15)),
];

/// Shade-mode widgets, topmost first.
const SHADE: [(Widget, Rect); 14] = [
    (Widget::Options, r(6, 3, 9, 9)),
    (Widget::Minimize, r(244, 3, 9, 9)),
    (Widget::Shade, r(254, 3, 9, 9)),
    (Widget::Close, r(264, 3, 9, 9)),
    (Widget::Vis, r(79, 5, 38, 5)),
    (Widget::Time, r(127, 4, 30, 6)),
    (Widget::Previous, r(169, 2, 7, 10)),
    (Widget::Play, r(176, 2, 10, 10)),
    (Widget::Pause, r(186, 2, 9, 10)),
    (Widget::Stop, r(195, 2, 9, 10)),
    (Widget::Next, r(204, 2, 10, 10)),
    (Widget::Eject, r(215, 2, 10, 10)),
    (Widget::Seek, r(226, 4, 17, 7)),
    (Widget::TitleBar, r(0, 0, 275, 14)),
];

/// The widgets of a mode, topmost first.
pub fn widgets(mode: Mode) -> &'static [(Widget, Rect)] {
    match mode {
        Mode::Normal => &NORMAL,
        Mode::Shade => &SHADE,
    }
}

/// Where `widget` is in `mode`, if it is there.
pub fn rect(mode: Mode, widget: Widget) -> Option<Rect> {
    widgets(mode)
        .iter()
        .find(|(w, _)| *w == widget)
        .map(|&(_, r)| r)
}

/// Whether the 1× point (x, y) is inside `r`.
pub fn contains(r: Rect, x: i32, y: i32) -> bool {
    let inside = |p: i32, start: u32, len: u32| p >= start as i32 && p < (start + len) as i32;
    inside(x, r.x, r.w) && inside(y, r.y, r.h)
}

/// The topmost widget at the 1× point (x, y).
pub fn hit_test(mode: Mode, x: i32, y: i32) -> Option<Widget> {
    widgets(mode)
        .iter()
        .find(|&&(_, r)| contains(r, x, y))
        .map(|&(w, _)| w)
}

/// Width of a slider's thumb (0 for other widgets).
pub fn thumb_width(mode: Mode, widget: Widget) -> u32 {
    match (mode, widget) {
        (_, Widget::Volume | Widget::Balance) => 14,
        (Mode::Normal, Widget::Seek) => 29,
        (Mode::Shade, Widget::Seek) => 3,
        _ => 0,
    }
}
