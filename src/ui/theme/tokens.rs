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
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 22.0;
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

/// Build a colour with alpha from a 24-bit literal at compile time.
const fn rgba(r: u32, g: u32, b: u32, a: f32) -> Rgba {
    Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a,
    }
}

/// Default window opacity: half-see-through glass.
pub const DEFAULT_OPACITY: f32 = 0.5;

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
    /// Glass: a soft dark pane with light hairline edges.
    ///
    /// Colours carry their own alpha so the window reads as layered glass rather
    /// than a solid panel, and borders are a light grey rather than white so they
    /// stay hairline at low opacity.
    pub const GLASS: Theme = Theme {
        background: rgba(0x14, 0x15, 0x18, 0.55),
        card: rgba(0xff, 0xff, 0xff, 0.06),
        foreground: rgb_hex(0xf6f7f9),
        muted: rgba(0xff, 0xff, 0xff, 0.09),
        muted_foreground: rgba(0xf3, 0xf5, 0xf8, 0.62),
        border: rgba(0xff, 0xff, 0xff, 0.16),
        input: rgba(0xff, 0xff, 0xff, 0.10),
        accent: rgba(0xff, 0xff, 0xff, 0.14),
        primary: rgb_hex(0xf7f8fa),
        destructive: rgb_hex(0xe5837f),
        success: rgb_hex(0x8fd9b0),
        warning: rgb_hex(0xe8c48c),
    };

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

    pub const DEFAULT: Theme = Self::GLASS;

    pub const fn new() -> Self {
        Self::GLASS
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
        Self::GLASS
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
    ///
    /// The glass palette already carries alpha, so this multiplies rather than
    /// flattening to the window value: a 0.06 card at 50% stays a whisper of
    /// light instead of becoming an opaque block.
    fn fade(self, colour: Rgba) -> Rgba {
        Rgba {
            a: (colour.a * (0.35 + self.opacity * 0.65)).clamp(0.0, 1.0),
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

    pub fn muted_foreground(self) -> Rgba {
        self.fade(self.theme.muted_foreground)
    }

    pub fn border(self) -> Rgba {
        self.fade(self.theme.border)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surfaces_preserve_glass_layering() {
        let theme = Theme::GLASS;
        let half = theme.surfaces(0.5);
        // A glass card keeps its own low alpha, so it stays a whisper of light
        // rather than turning into an opaque block.
        assert!(half.card().a < 0.1, "card alpha {}", half.card().a);
        assert!(half.background().a < 0.5);
        // Primary stays readable at low opacity rather than vanishing.
        assert!(half.primary().a > 0.5);
        // Borders stay visible enough to read as an edge.
        assert!(half.border().a > 0.05);
    }

    #[test]
    fn full_opacity_keeps_the_declared_alpha() {
        let theme = Theme::GLASS;
        let full = theme.surfaces(1.0);
        assert!((full.card().a - theme.card.a).abs() < 1e-6);
        assert!((full.background().a - theme.background.a).abs() < 1e-6);
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
