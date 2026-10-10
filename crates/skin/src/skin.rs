//! A loaded skin: decoded sheets, playlist style, visualization colours, window regions and
//! cursors. Whatever a skin lacks comes from the base skin.

use crate::bmp;
use crate::files::SkinFiles;
use crate::image::{Image, Rect};
use crate::ini::decode_text;
use crate::pledit::{self, PlaylistStyle};
use crate::region::{self, Regions};
use crate::sprites::{Sheet, SpriteId};
use crate::viscolor;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// Largest skin archive accepted.
pub const MAX_ARCHIVE: u64 = 64 << 20;

/// Cursor files Winamp 2.x reads (`<NAME>.cur`; may hold animated RIFF data).
pub const CURSORS: [&str; 24] = [
    "CLOSE", "EQCLOSE", "EQNORMAL", "EQSLID", "EQTITLE", "MAINMENU", "MMENU", "MIN", "NORMAL",
    "PCLOSE", "PNORMAL", "POSBAR", "PSIZE", "PTBAR", "PVSCROLL", "PWINBUT", "PWSNORM", "PWSSIZE",
    "SONGNAME", "TITLEBAR", "VOLBAL", "WINBUT", "WSNORMAL", "WSPOSBAR",
];

#[derive(Debug, thiserror::Error)]
pub enum SkinError {
    #[error("cannot read {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("not a skin: {0}")]
    NotASkin(String),
}

struct SheetImage {
    /// Exactly `Sheet::size()` (cropped or padded), so every sprite rectangle is inside it.
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
    /// Loads a `.wsz`/`.zip` file or a skin directory.
    pub fn load(path: &Path) -> Result<Skin, SkinError> {
        let io = |source| SkinError::Io {
            path: path.to_path_buf(),
            source,
        };
        if path.is_dir() {
            return Self::from_files(SkinFiles::from_dir(path).map_err(io)?);
        }
        let len = std::fs::metadata(path).map_err(io)?.len();
        if len > MAX_ARCHIVE {
            return Err(SkinError::NotASkin(format!("{len} bytes is too large")));
        }
        Self::from_zip_bytes(&std::fs::read(path).map_err(io)?)
    }

    pub fn from_zip_bytes(bytes: &[u8]) -> Result<Skin, SkinError> {
        Self::from_files(SkinFiles::from_zip(bytes).map_err(SkinError::NotASkin)?)
    }

    fn from_files(mut files: SkinFiles) -> Result<Skin, SkinError> {
        if files.is_empty() {
            return Err(SkinError::NotASkin("no skin files found".into()));
        }
        let mut warnings = std::mem::take(&mut files.warnings);
        let mut sheets: Vec<Option<Arc<SheetImage>>> = vec![None; Sheet::ALL.len()];
        for sheet in Sheet::ALL {
            let Some((name, data)) = files.image(sheet.stem()) else {
                continue;
            };
            match bmp::decode_any(data) {
                Ok(img) => {
                    let (w, h) = sheet.size();
                    sheets[sheet as usize] = Some(Arc::new(SheetImage {
                        width: img.width,
                        height: img.height,
                        image: img.fitted(w, h),
                    }));
                }
                Err(e) => warnings.push(format!("{name}: {e}")),
            }
        }
        // Without balance.bmp, Winamp draws the balance slider from volume.bmp.
        if sheets[Sheet::Balance as usize].is_none() {
            sheets[Sheet::Balance as usize] = sheets[Sheet::Volume as usize].clone();
        }
        let cursors = CURSORS
            .iter()
            .filter_map(|&c| Some((c, files.take(&format!("{}.cur", c.to_ascii_lowercase()))?)))
            .collect();
        let text = |name: &str| files.get(name).map(decode_text);
        Ok(Skin {
            sheets,
            fallback: Some(Skin::base()),
            playlist: text("pledit.txt")
                .map(|t| pledit::parse(&t))
                .unwrap_or_default(),
            vis_colors: text("viscolor.txt")
                .map(|t| viscolor::parse(&t))
                .unwrap_or(viscolor::DEFAULT_COLORS),
            regions: text("region.txt")
                .map(|t| region::parse(&t))
                .unwrap_or_default(),
            cursors,
            warnings,
        })
    }

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
