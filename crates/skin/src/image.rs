//! Images and rectangles. Pixels are `u32` in `0xAARRGGBB` order.

/// Opaque black: what a skin shows where its bitmap has no pixels.
pub const BLACK: u32 = 0xFF00_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub const fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Rect { x, y, w, h }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

impl Image {
    pub fn new(width: u32, height: u32, fill: u32) -> Self {
        Image {
            width,
            height,
            pixels: vec![fill; width as usize * height as usize],
        }
    }

    /// Pixel at (x, y); black outside the image.
    pub fn get(&self, x: u32, y: u32) -> u32 {
        if x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize]
        } else {
            BLACK
        }
    }

    /// Sets (x, y); ignored outside the image.
    pub fn set(&mut self, x: u32, y: u32, color: u32) {
        if x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize] = color;
        }
    }

    /// Fills `r`, clipped to the image.
    pub fn fill_rect(&mut self, r: Rect, color: u32) {
        for y in r.y..r.y.saturating_add(r.h).min(self.height) {
            for x in r.x..r.x.saturating_add(r.w).min(self.width) {
                self.set(x, y, color);
            }
        }
    }

    /// Copies the `from` area of `src` to (x, y). Source pixels outside `src` are black;
    /// destination pixels outside `self` are dropped.
    pub fn blit(&mut self, src: &Image, from: Rect, x: u32, y: u32) {
        for dy in 0..from.h {
            for dx in 0..from.w {
                let c = src.get(from.x + dx, from.y + dy);
                self.set(x + dx, y + dy, c);
            }
        }
    }

    /// The image grown to at least `width`×`height`; new pixels are black.
    pub fn padded(&self, width: u32, height: u32) -> Image {
        if self.width >= width && self.height >= height {
            return self.clone();
        }
        let mut out = Image::new(self.width.max(width), self.height.max(height), BLACK);
        out.blit(self, Rect::new(0, 0, self.width, self.height), 0, 0);
        out
    }
}
