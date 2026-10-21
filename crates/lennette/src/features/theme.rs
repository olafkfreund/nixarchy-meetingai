//! Visual tokens.
//!
//! Values are taken from the reference build's stylesheet so the port keeps the
//! same dark surface, muted text and hairline borders. Kept in one place so the
//! whole app draws from a single palette.

use gpui::Rgba;

/// Vertical spacing steps used across the app.
pub mod space {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 24.0;
}

/// Border radii, matching the reference's `rounded-lg` / `rounded-xl` scale.
pub mod radius {
    pub const SM: f32 = 6.0;
    pub const MD: f32 = 8.0;
    pub const LG: f32 = 10.0;
    pub const XL: f32 = 12.0;
    pub const FULL: f32 = 999.0;
}

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
    pub warning: Rgba,
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
        warning: rgb_hex(0xd8a657),
    };

    pub const DEFAULT: Theme = Self::DARK;

    pub const fn new() -> Self {
        Self::DARK
    }
}

impl Theme {
    /// Surface colours at a given window opacity.
    ///
    /// Every panel, pill and button must use these rather than the raw tokens,
    /// otherwise opaque children float over a see-through window.
    pub fn surfaces(self, opacity: f32) -> Surfaces {
        Surfaces {
            opacity,
            theme: self,
        }
    }

    /// The same colour at a given opacity, for see-through surfaces.
    pub fn with_alpha(colour: Rgba, alpha: f32) -> Rgba {
        Rgba {
            a: alpha.clamp(0.0, 1.0),
            ..colour
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::DARK
    }
}

/// A theme viewed at a specific window opacity.
#[derive(Debug, Clone, Copy)]
pub struct Surfaces {
    pub opacity: f32,
    pub theme: Theme,
}

impl Surfaces {
    /// Scale a colour's alpha by the window opacity.
    fn fade(self, colour: Rgba) -> Rgba {
        Rgba {
            a: (colour.a * self.opacity).clamp(0.0, 1.0),
            ..colour
        }
    }

    pub fn background(self) -> Rgba {
        self.fade(self.theme.background)
    }

    pub fn card(self) -> Rgba {
        self.fade(self.theme.card)
    }

    pub fn muted(self) -> Rgba {
        self.fade(self.theme.muted)
    }

    pub fn accent(self) -> Rgba {
        self.fade(self.theme.accent)
    }

    pub fn input(self) -> Rgba {
        self.fade(self.theme.input)
    }

    /// The primary fill stays mostly opaque so labels keep their contrast.
    pub fn primary(self) -> Rgba {
        let boosted = (self.opacity + 0.35).clamp(0.0, 1.0);
        Rgba {
            a: boosted,
            ..self.theme.primary
        }
    }

    pub fn border(self) -> Rgba {
        self.fade(self.theme.border)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surfaces_scale_alpha_and_keep_primary_readable() {
        let theme = Theme::DARK;
        let half = theme.surfaces(0.5);
        assert!((half.card().a - 0.5).abs() < 1e-6);
        assert!((half.background().a - 0.5).abs() < 1e-6);
        // Primary stays readable at low opacity rather than vanishing.
        assert!(half.primary().a > 0.5);
        let full = theme.surfaces(1.0);
        assert!((full.card().a - 1.0).abs() < 1e-6);
    }

    #[test]
    fn hex_maps_channels_correctly() {
        let white = rgb_hex(0xffffff);
        assert_eq!(white.r, 1.0);
        assert_eq!(white.a, 1.0);
        assert_eq!(rgb_hex(0x000000).r, 0.0);
        assert!((rgb_hex(0x808080).r - 0.5019).abs() < 1e-3);
    }
}
