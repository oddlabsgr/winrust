//! Winrust Base colours: gunmetal body, black LCD with green text, rust accents.

pub const BODY_TOP: u32 = 0xFF44_4A57;
pub const BODY_BOTTOM: u32 = 0xFF33_3843;
pub const BEVEL_LIGHT: u32 = 0xFF6A_7282;
pub const BEVEL_DARK: u32 = 0xFF1E_2128;
pub const SHADOW: u32 = 0xFF14_161B;

pub const FACE_TOP: u32 = 0xFF55_5C6B;
pub const FACE_BOTTOM: u32 = 0xFF3A_404C;
pub const FACE_PRESSED_TOP: u32 = 0xFF2C_3039;
pub const FACE_PRESSED_BOTTOM: u32 = 0xFF3A_3F4B;

pub const GROOVE: u32 = 0xFF1A_1C22;
pub const LCD_BG: u32 = 0xFF00_0000;
pub const LCD_ON: u32 = 0xFF00_E000;
pub const LCD_DIM: u32 = 0xFF0C_3A0C;
pub const LCD_GRID: u32 = 0xFF10_2410;

pub const INK: u32 = 0xFFD2_D6DE;
pub const INK_DIM: u32 = 0xFF8A_909C;
pub const RUST: u32 = 0xFFB7_410E;
pub const RUST_DARK: u32 = 0xFF6E_2608;
pub const COPPER: u32 = 0xFFE0_7B39;

/// Title bar of an inactive window: stripes go grey.
pub const STRIPE_IDLE: u32 = 0xFF4E_5462;
pub const STRIPE_IDLE_DARK: u32 = 0xFF2A_2E37;

/// Level colours for sliders: green (low) to rust red (high).
pub fn level(i: u32, of: u32) -> u32 {
    let t = i as f32 / of.max(1) as f32;
    if t < 0.5 {
        super::draw::mix(0xFF2E_C82E, 0xFFE0_C030, t * 2.0)
    } else {
        super::draw::mix(0xFFE0_C030, 0xFFD0_3A10, (t - 0.5) * 2.0)
    }
}
