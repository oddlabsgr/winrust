//! Winrust's main window without any windowing: widget layout, hit-testing, pointer
//! interaction, the compositor, the scrolling title and the visualizer. Pure: it depends on
//! the skin engine only.

pub mod interaction;
pub mod layout;
pub mod marquee;
pub mod view;
pub mod vis;
