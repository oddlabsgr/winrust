//! One drawing function per sheet. Sprites are drawn at their rectangles from the sprite
//! table, so the art always lines up with what the loader reads.

pub mod controls;
pub mod lcd;
pub mod windows;

use super::draw::*;
use super::palette::*;
use winrust_skin::{Image, Rect, Sheet, SpriteId};

pub fn r(id: SpriteId) -> Rect {
    id.def().rect
}

pub fn canvas(sheet: Sheet) -> Image {
    let (w, h) = sheet.size();
    Image::new(w, h, SHADOW)
}

/// A push button: raised or pressed face; the icon shifts one pixel when pressed.
pub fn button(img: &mut Image, b: Rect, down: bool, icon: impl FnOnce(&mut Image, Rect, u32)) {
    if down {
        pressed(img, b);
        icon(img, rect(b.x + 1, b.y + 1, b.w, b.h), COPPER);
    } else {
        raised(img, b);
        icon(img, b, INK);
    }
}

/// A 9×9 title button baked into a bar at `x`: Winamp shows the bar's own pixels until
/// the button is first clicked.
pub fn bar_button(img: &mut Image, bar: Rect, x: u32, icon: &[&str]) {
    button(img, rect(bar.x + x, bar.y + 3, 9, 9), false, |img, b, c| {
        bitmap_centered(img, b, icon, c)
    });
}

pub fn knob(img: &mut Image, b: Rect, active: bool, vertical_grip: bool) {
    if active {
        vgradient(img, b, COPPER, RUST);
        frame(img, b, COPPER, RUST_DARK);
    } else {
        raised(img, b);
    }
    let grip = if active { RUST_DARK } else { RUST };
    if vertical_grip {
        let cx = b.x + b.w / 2;
        for dx in [0u32, 2, 4] {
            vline(img, cx - 2 + dx, b.y + 3, b.h.saturating_sub(6), grip);
        }
    } else {
        let cy = b.y + b.h / 2;
        hline(img, b.x + 3, cy, b.w.saturating_sub(6), grip);
    }
}

pub fn title_stripes(img: &mut Image, x: u32, y: u32, w: u32, active: bool) {
    let (c, dark) = if active {
        (RUST, RUST_DARK)
    } else {
        (STRIPE_IDLE, STRIPE_IDLE_DARK)
    };
    for row in [0, 2, 4] {
        hline(img, x, y + row, w, c);
        hline(img, x, y + row + 1, w, dark);
    }
}

/// A 14-pixel title bar with the title centred between stripes.
pub fn title_bar(
    img: &mut Image,
    b: Rect,
    title: &str,
    active: bool,
    stripes_from: u32,
    stripes_to: u32,
) {
    vgradient(img, b, BODY_TOP, BODY_BOTTOM);
    frame(img, b, BEVEL_LIGHT, BEVEL_DARK);
    let w = text_width(title);
    let tx = b.x + (b.w - w) / 2;
    title_stripes(
        img,
        b.x + stripes_from,
        b.y + 4,
        tx - 4 - (b.x + stripes_from),
        active,
    );
    title_stripes(
        img,
        tx + w + 5,
        b.y + 4,
        b.x + stripes_to - (tx + w + 5),
        active,
    );
    label(
        img,
        tx,
        b.y + 4,
        title,
        if active { COPPER } else { INK_DIM },
    );
}

pub fn body(img: &mut Image, b: Rect) {
    vgradient(img, b, BODY_TOP, BODY_BOTTOM);
    frame(img, b, BEVEL_LIGHT, BEVEL_DARK);
}

/// All sheets in `Sheet::ALL` order.
pub fn all() -> Vec<Image> {
    Sheet::ALL
        .iter()
        .map(|sheet| match sheet {
            Sheet::Main => windows::main(),
            Sheet::Cbuttons => controls::cbuttons(),
            Sheet::Titlebar => controls::titlebar(),
            Sheet::Numbers => lcd::numbers(),
            Sheet::NumsEx => lcd::nums_ex(),
            Sheet::Text => lcd::text(),
            Sheet::Posbar => controls::posbar(),
            Sheet::Volume => controls::volume(),
            Sheet::Balance => controls::balance(),
            Sheet::Monoster => lcd::monoster(),
            Sheet::Playpaus => lcd::playpaus(),
            Sheet::Shufrep => controls::shufrep(),
            Sheet::Eqmain => windows::eqmain(),
            Sheet::EqEx => windows::eq_ex(),
            Sheet::Pledit => windows::pledit(),
        })
        .collect()
}
