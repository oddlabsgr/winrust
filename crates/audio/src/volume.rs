//! Volume and balance curves.

/// Maps the 0..=100 volume slider to a linear gain.
///
/// Squared curve: 50% is about -12 dB, 25% about -24 dB. 0 is silence, 100 is unity.
pub fn slider_to_gain(slider: u8) -> f32 {
    let x = f32::from(slider.min(100)) / 100.0;
    x * x
}

/// Per-channel gains for a balance value in -1.0 (left) ..= 1.0 (right).
///
/// Winamp-style: the far channel is attenuated linearly, the near channel stays at unity.
pub fn balance_gains(balance: f32) -> (f32, f32) {
    let b = balance.clamp(-1.0, 1.0);
    if b >= 0.0 {
        (1.0 - b, 1.0)
    } else {
        (1.0, 1.0 + b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_endpoints() {
        assert_eq!(slider_to_gain(0), 0.0);
        assert_eq!(slider_to_gain(100), 1.0);
        assert_eq!(slider_to_gain(255), 1.0);
    }

    #[test]
    fn volume_half_is_about_minus_12_db() {
        let db = 20.0 * slider_to_gain(50).log10();
        assert!((db - -12.04).abs() < 0.1, "{db}");
    }

    #[test]
    fn balance_center_is_unity() {
        assert_eq!(balance_gains(0.0), (1.0, 1.0));
    }

    #[test]
    fn balance_right_attenuates_left() {
        assert_eq!(balance_gains(0.25), (0.75, 1.0));
        assert_eq!(balance_gains(1.0), (0.0, 1.0));
    }

    #[test]
    fn balance_left_attenuates_right() {
        assert_eq!(balance_gains(-0.5), (1.0, 0.5));
        assert_eq!(balance_gains(-7.0), (1.0, 0.0));
    }
}
