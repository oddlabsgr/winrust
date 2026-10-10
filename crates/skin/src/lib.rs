//! Winrust skin engine: loads classic Winamp 2.x skins (`.wsz`, `.zip` or a directory) and
//! maps their sprites. Pure: no windowing or OS dependencies.

pub mod bmp;
pub mod image;
pub mod sprites;
pub mod text;

pub use image::{Image, Rect};
pub use sprites::{Sheet, SpriteId};
