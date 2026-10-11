//! Pointer handling for the main window: press, drag and release become [`UiEvent`]s, and
//! the compositor asks which button shows pressed and which slider is being dragged.

use crate::layout::{self, Kind, Mode, Widget};
use crate::view::{self, MainView, PlayState};
use winrust_skin::Rect;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pointer {
    /// The left button went down at a 1× window point; `double` marks the second press of
    /// a double-click.
    Down {
        x: i32,
        y: i32,
        double: bool,
    },
    Move {
        x: i32,
        y: i32,
    },
    Up {
        x: i32,
        y: i32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UiEvent {
    /// A button was released over itself.
    Clicked(Widget),
    Volume(u8),
    Balance(i8),
    /// Seek to this fraction of the track; sent when the seek bar is released.
    Seek(f32),
    /// The pointer is this far (1× pixels) from where the window was grabbed: move the window
    /// by this much from where it is now. Positions are window-relative, so once the window
    /// has followed, the next event reports only the new travel.
    MoveWindow {
        dx: i32,
        dy: i32,
    },
    /// The title bar was double-clicked.
    ToggleShade,
    /// The title text was dragged this many pixels since the last event.
    DragTitle(i32),
    DragTitleEnd,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Grab {
    #[default]
    None,
    Button {
        widget: Widget,
        over: bool,
    },
    /// `offset` is where the thumb was taken, from its left edge.
    Slider {
        widget: Widget,
        offset: i32,
        fraction: f32,
    },
    Window {
        x: i32,
        y: i32,
    },
    Title {
        x: i32,
    },
}

const TITLE_BUTTONS: [Widget; 4] = [
    Widget::Options,
    Widget::Minimize,
    Widget::Shade,
    Widget::Close,
];

#[derive(Clone, Debug, Default)]
pub struct Interaction {
    grab: Grab,
    /// Which title buttons have been pressed at least once.
    clicked: [bool; 4],
}

impl Interaction {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pointer(&mut self, event: Pointer, view: &MainView) -> Option<UiEvent> {
        let mode = if view.shade {
            Mode::Shade
        } else {
            Mode::Normal
        };
        match event {
            Pointer::Down { x, y, double } => self.down(mode, x, y, double, view),
            Pointer::Move { x, y } => self.moved(mode, x, y),
            Pointer::Up { x, y } => self.up(mode, x, y),
        }
    }

    /// The button drawn pressed: held, with the pointer over it.
    pub fn pressed(&self) -> Option<Widget> {
        match self.grab {
            Grab::Button { widget, over: true } => Some(widget),
            _ => None,
        }
    }

    /// The slider being dragged, with its value as a fraction of its track.
    pub fn sliding(&self) -> Option<(Widget, f32)> {
        match self.grab {
            Grab::Slider {
                widget, fraction, ..
            } => Some((widget, fraction)),
            _ => None,
        }
    }

    /// Ends any press or drag without its usual effect: no click, no seek. Call it when the
    /// window loses the pointer mid-drag (focus or capture loss), or a later hover would
    /// still drive the grabbed slider. A title-text drag still ends with `DragTitleEnd`, so
    /// the title resumes scrolling.
    pub fn cancel(&mut self) -> Option<UiEvent> {
        match std::mem::take(&mut self.grab) {
            Grab::Title { .. } => Some(UiEvent::DragTitleEnd),
            _ => None,
        }
    }

    /// Whether a title button (options, minimize, shade, close) has been pressed yet. Until
    /// then the title bar shows through it (Webamp `ClickedDiv.tsx`).
    pub fn title_button_clicked(&self, widget: Widget) -> bool {
        TITLE_BUTTONS
            .iter()
            .position(|&w| w == widget)
            .is_some_and(|i| self.clicked[i])
    }

    fn down(
        &mut self,
        mode: Mode,
        x: i32,
        y: i32,
        double: bool,
        view: &MainView,
    ) -> Option<UiEvent> {
        self.grab = Grab::None;
        let Some(widget) = layout::hit_test(mode, x, y) else {
            // Winamp moves the main window from any spot that is not a control.
            let (w, h) = mode.size();
            if layout::contains(Rect::new(0, 0, w, h), x, y) {
                self.grab = Grab::Window { x, y };
            }
            return None;
        };
        if double && widget == Widget::TitleBar {
            return Some(UiEvent::ToggleShade);
        }
        match widget.kind() {
            Kind::Button => {
                if let Some(i) = TITLE_BUTTONS.iter().position(|&w| w == widget) {
                    self.clicked[i] = true;
                }
                self.grab = Grab::Button { widget, over: true };
                None
            }
            Kind::Drag if widget == Widget::TitleBar => {
                self.grab = Grab::Window { x, y };
                None
            }
            Kind::Drag => {
                self.grab = Grab::Title { x };
                Some(UiEvent::DragTitle(0))
            }
            Kind::Slider => {
                let fraction = slider_value(widget, view)?;
                let track = layout::rect(mode, widget)?;
                let thumb = layout::thumb_width(mode, widget);
                let left = (track.x + view::thumb_offset(track.w, thumb, fraction)) as i32;
                let on_thumb = x >= left && x < left + thumb as i32;
                let offset = if on_thumb {
                    x - left
                } else {
                    (thumb / 2) as i32
                };
                self.grab = Grab::Slider {
                    widget,
                    offset,
                    fraction,
                };
                if on_thumb { None } else { self.slide(mode, x) }
            }
        }
    }

    fn moved(&mut self, mode: Mode, x: i32, y: i32) -> Option<UiEvent> {
        match self.grab {
            Grab::None => None,
            Grab::Button { widget, .. } => {
                let over = layout::rect(mode, widget).is_some_and(|r| layout::contains(r, x, y));
                self.grab = Grab::Button { widget, over };
                None
            }
            Grab::Slider { .. } => self.slide(mode, x),
            Grab::Window { x: gx, y: gy } => Some(UiEvent::MoveWindow {
                dx: x - gx,
                dy: y - gy,
            }),
            Grab::Title { x: last } => {
                self.grab = Grab::Title { x };
                (x != last).then_some(UiEvent::DragTitle(x - last))
            }
        }
    }

    fn up(&mut self, mode: Mode, x: i32, y: i32) -> Option<UiEvent> {
        match std::mem::take(&mut self.grab) {
            Grab::Button { widget, .. } => layout::rect(mode, widget)
                .filter(|&r| layout::contains(r, x, y))
                .map(|_| UiEvent::Clicked(widget)),
            Grab::Slider {
                widget: Widget::Seek,
                fraction,
                ..
            } => Some(UiEvent::Seek(fraction)),
            Grab::Title { .. } => Some(UiEvent::DragTitleEnd),
            _ => None,
        }
    }

    /// Moves the grabbed thumb so the grab point is under `x`; reports a changed volume or
    /// balance (the seek bar reports on release).
    fn slide(&mut self, mode: Mode, x: i32) -> Option<UiEvent> {
        let Grab::Slider {
            widget,
            offset,
            fraction,
        } = self.grab
        else {
            return None;
        };
        let track = layout::rect(mode, widget)?;
        let travel = track.w - layout::thumb_width(mode, widget);
        let new = ((x - offset - track.x as i32) as f32 / travel as f32).clamp(0.0, 1.0);
        self.grab = Grab::Slider {
            widget,
            offset,
            fraction: new,
        };
        let after = slider_event(widget, new);
        if after != slider_event(widget, fraction) {
            after
        } else {
            None
        }
    }
}

/// A slider's value as a fraction of its track; `None` while the seek bar is disabled.
fn slider_value(widget: Widget, view: &MainView) -> Option<f32> {
    match widget {
        Widget::Volume => Some(view::volume_fraction(view.volume)),
        Widget::Balance => Some(view::balance_fraction(view.balance)),
        Widget::Seek => {
            let enabled = view.seekable && view.state != PlayState::Stopped;
            let duration = view.duration.filter(|_| enabled)?;
            Some(view::seek_fraction(view.elapsed, duration))
        }
        _ => None,
    }
}

fn slider_event(widget: Widget, fraction: f32) -> Option<UiEvent> {
    match widget {
        Widget::Volume => Some(UiEvent::Volume((fraction * 100.0).round() as u8)),
        Widget::Balance => {
            let balance = (fraction * 200.0 - 100.0).round() as i8;
            Some(UiEvent::Balance(view::snap_balance(balance)))
        }
        _ => None,
    }
}
