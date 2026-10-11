//! Winrust skin engine: loads classic Winamp 2.x skins (`.wsz`, `.zip` or a directory) and
//! maps their sprites. Pure: no windowing or OS dependencies.

mod base;
pub mod bmp;
pub mod contact;
pub mod debug;
mod files;
pub mod image;
pub mod ini;
pub mod pledit;
pub mod region;
pub mod skin;
pub mod sprites;
pub mod text;
pub mod viscolor;

pub use contact::contact_sheet;
pub use image::{Image, Rect};
pub use skin::{Skin, SkinError, SpriteRef};
pub use sprites::{Sheet, SpriteId};
