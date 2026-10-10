//! A loaded skin: decoded sheets, playlist style, visualization colours, window regions and
//! cursors. Whatever a skin lacks comes from the base skin.

use crate::image::{Image, Rect};
use crate::pledit::PlaylistStyle;
use crate::region::Regions;
use crate::sprites::{Sheet, SpriteId};
use crate::viscolor;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

struct SheetImage {
    /// Padded to at least `Sheet::size()`, so every sprite rectangle is inside it.
    image: Image,
    /// Size as decoded, before padding.
    width: u32,
    height: u32,
}

/// Where to blit a sprite from.
#[derive(Clone, Copy, Debug)]
pub struct SpriteRef<'a> {
    pub image: &'a Image,
    pub rect: Rect,
}

pub struct Skin {
    sheets: Vec<Option<Arc<SheetImage>>>,
    fallback: Option<Arc<Skin>>,
    pub playlist: PlaylistStyle,
    pub vis_colors: [u32; 24],
    pub regions: Regions,
    /// Raw `.cur` file contents by cursor name (decoded by the app).
    pub cursors: BTreeMap<&'static str, Vec<u8>>,
    /// Problems that did not stop loading (unreadable bitmaps, bad archive entries).
    pub warnings: Vec<String>,
}

impl Skin {
    /// A skin with every sheet given, in `Sheet::ALL` order, and nothing to fall back on.
    pub(crate) fn complete(sheets: Vec<Image>) -> Skin {
        Skin {
            sheets: sheets
                .into_iter()
                .map(|image| {
                    Some(Arc::new(SheetImage {
                        width: image.width,
                        height: image.height,
                        image,
                    }))
                })
                .collect(),
            fallback: None,
            playlist: PlaylistStyle::default(),
            vis_colors: viscolor::DEFAULT_COLORS,
            regions: Regions::default(),
            cursors: BTreeMap::new(),
            warnings: Vec::new(),
        }
    }

    /// The built-in skin used for anything a skin lacks. Until the Winrust Base skin is
    /// generated (M2 part 2) this is the debug skin.
    pub fn base() -> Arc<Skin> {
        static BASE: OnceLock<Arc<Skin>> = OnceLock::new();
        BASE.get_or_init(|| Arc::new(Skin::debug())).clone()
    }

    /// The sprite's pixels: from this skin when its sheet reaches the sprite (missing
    /// pixels of a partly covered sprite are black), otherwise from the base skin.
    pub fn sprite(&self, id: SpriteId) -> SpriteRef<'_> {
        let def = id.def();
        match &self.sheets[def.sheet as usize] {
            Some(s) if def.rect.x < s.width && def.rect.y < s.height => SpriteRef {
                image: &s.image,
                rect: def.rect,
            },
            _ => self
                .fallback
                .as_ref()
                .expect("only complete skins have no fallback")
                .sprite(id),
        }
    }

    /// A whole sheet (e.g. `text.bmp` for [`crate::text::glyph`] cells).
    pub fn sheet(&self, sheet: Sheet) -> &Image {
        match &self.sheets[sheet as usize] {
            Some(s) => &s.image,
            None => self
                .fallback
                .as_ref()
                .expect("only complete skins have no fallback")
                .sheet(sheet),
        }
    }

    /// Whether the skin supplied this sheet (balance counts when derived from volume).
    pub fn has_sheet(&self, sheet: Sheet) -> bool {
        self.sheets[sheet as usize].is_some()
    }

    /// The time display uses `nums_ex.bmp` (which has a minus sign) when the skin has it,
    /// or when it has neither digit sheet; otherwise `numbers.bmp`.
    pub fn uses_nums_ex(&self) -> bool {
        self.has_sheet(Sheet::NumsEx) || !self.has_sheet(Sheet::Numbers)
    }
}
