//! Winrust skin engine: loads classic Winamp 2.x skins (`.wsz`, `.zip` or a directory) and
//! maps their sprites. Pure: no windowing or OS dependencies.

pub mod bmp;
pub mod debug;
pub mod image;
pub mod ini;
pub mod pledit;
pub mod region;
pub mod skin;
pub mod sprites;
pub mod text;
pub mod viscolor;

pub use image::{Image, Rect};
pub use skin::{Skin, SpriteRef};
pub use sprites::{Sheet, SpriteId};
