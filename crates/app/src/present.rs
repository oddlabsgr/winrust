//! Getting the 1× compositor image onto the screen: integer scaling (M3 spec §5.2) and the
//! pixel copy into the window's buffer.

use winrust_skin::Image;
use winrust_ui::layout::Mode;

/// Screen pixels per skin pixel: the OS scale factor rounded to a whole number (at least 1,
/// so pixels stay sharp), doubled in double-size mode.
pub fn scale(os_factor: f64, double_size: bool) -> u32 {
    let os = (os_factor.round() as u32).max(1);
    if double_size { os * 2 } else { os }
}

/// The window's size in physical pixels.
pub fn window_size(shade: bool, scale: u32) -> (u32, u32) {
    let mode = if shade { Mode::Shade } else { Mode::Normal };
    let (w, h) = mode.size();
    (w * scale, h * scale)
}

/// A physical pointer coordinate in 1× window pixels; positions left of or above the window
/// (while a drag holds the pointer) are negative.
pub fn to_1x(physical: f64, scale: u32) -> i32 {
    (physical / f64::from(scale.max(1))).floor() as i32
}

/// Copies `src` into a `width`-wide buffer, each pixel as a `scale`×`scale` block, in the
/// `0RGB` format windows take. Buffer pixels past the image are black.
pub fn upscale(src: &Image, scale: u32, dst: &mut [u32], width: u32) {
    let scale = scale.max(1);
    for (y, row) in dst.chunks_mut(width.max(1) as usize).enumerate() {
        let sy = y as u32 / scale;
        for (x, px) in row.iter_mut().enumerate() {
            let sx = x as u32 / scale;
            *px = if sx < src.width && sy < src.height {
                src.pixels[(sy * src.width + sx) as usize] & 0x00FF_FFFF
            } else {
                0
            };
        }
    }
}
