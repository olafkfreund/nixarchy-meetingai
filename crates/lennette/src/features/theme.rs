//! Visual tokens.
//!
//! Values are taken from the reference build's stylesheet so the port keeps the
//! same dark surface, muted text and hairline borders. Kept in one place so the
//! whole app draws from a single palette.

use gpui::Rgba;

/// Build an opaque colour from a 24-bit RGB literal at compile time.
const fn rgb_hex(value: u32) -> Rgba {
    Rgba {
        r: ((value >> 16) & 0xff) as f32 / 255.0,
        g: ((value >> 8) & 0xff) as f32 / 255.0,
        b: (value & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

/// Semantic surface and text colours.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub background: Rgba,
    pub card: Rgba,
    pub foreground: Rgba,
    pub muted: Rgba,
    pub muted_foreground: Rgba,
    pub border: Rgba,
    pub input: Rgba,
    pub accent: Rgba,
    pub primary: Rgba,
    pub destructive: Rgba,
    pub success: Rgba,
}

impl Theme {
    /// Dark theme, matching the reference overlay's default appearance.
    pub const DARK: Theme = Theme {
        background: rgb_hex(0x0a0a0a),
        card: rgb_hex(0x18181a),
        foreground: rgb_hex(0xf2f2f2),
        muted: rgb_hex(0x262628),
        muted_foreground: rgb_hex(0xa3a3a3),
        border: rgb_hex(0x2a2a2d),
        input: rgb_hex(0x303034),
        accent: rgb_hex(0x35353a),
        primary: rgb_hex(0xe8e8e8),
        destructive: rgb_hex(0xd35a5a),
        success: rgb_hex(0x6fcf97),
    };

    pub const fn new() -> Self {
        Self::DARK
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::DARK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_maps_channels_correctly() {
        let white = rgb_hex(0xffffff);
        assert_eq!(white.r, 1.0);
        assert_eq!(white.a, 1.0);
        assert_eq!(rgb_hex(0x000000).r, 0.0);
        assert!((rgb_hex(0x808080).r - 0.5019).abs() < 1e-3);
    }
}
