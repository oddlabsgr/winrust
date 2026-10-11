use winrust_app::present::{scale, to_1x, upscale, window_size};
use winrust_skin::Image;

#[test]
fn the_scale_is_the_rounded_os_factor_doubled_on_request() {
    assert_eq!(scale(1.0, false), 1);
    assert_eq!(scale(1.0, true), 2);
    assert_eq!(scale(1.25, false), 1, "fractional factors round, no blur");
    assert_eq!(scale(1.5, false), 2);
    assert_eq!(scale(2.0, true), 4);
    assert_eq!(scale(0.5, false), 1, "never below 1");
}

#[test]
fn the_window_is_the_main_window_scaled() {
    assert_eq!(window_size(false, 1), (275, 116));
    assert_eq!(window_size(true, 2), (550, 28));
}

#[test]
fn pointer_positions_map_to_1x_pixels() {
    assert_eq!(to_1x(10.0, 2), 5);
    assert_eq!(to_1x(11.9, 2), 5);
    assert_eq!(to_1x(-0.5, 2), -1, "left of the window");
    assert_eq!(to_1x(-3.0, 2), -2);
}

#[test]
fn upscaling_repeats_each_pixel_and_drops_alpha() {
    let mut img = Image::new(2, 1, 0xFF11_2233);
    img.set(1, 0, 0xFF44_5566);
    let mut out = vec![7u32; 8];
    upscale(&img, 2, &mut out, 4);
    let (a, b) = (0x0011_2233, 0x0044_5566);
    assert_eq!(out, vec![a, a, b, b, a, a, b, b]);
}

#[test]
fn upscaling_fits_a_buffer_of_another_size() {
    let img = Image::new(2, 1, 0xFF11_2233);
    let mut big = vec![7u32; 6 * 3];
    upscale(&img, 2, &mut big, 6);
    assert_eq!(
        &big[..6],
        &[0x0011_2233, 0x0011_2233, 0x0011_2233, 0x0011_2233, 0, 0]
    );
    assert!(big[12..].iter().all(|p| *p == 0), "past the image: black");
    let mut small = vec![7u32; 1];
    upscale(&img, 2, &mut small, 1);
    assert_eq!(small, vec![0x0011_2233]);
}
