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

/// Default window tint opacity.
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
    /// Outer window frame, independent of decorative control outlines/opacity.
    pub frame_border: Rgba,
    pub input: Rgba,
    pub accent: Rgba,
    pub primary: Rgba,
    pub destructive: Rgba,
    pub success: Rgba,
    pub warning: Rgba,
}

impl Theme {
    pub fn from_settings(settings: &crate::core::settings::Settings) -> Self {
        let mut theme = Self::from_choice(settings.theme);
        if !settings.show_borders {
            theme.border.a = 0.0;
            theme.frame_border.a = 0.0;
        }
        theme
    }

    /// Glass uses faint neutral fills rather than outlining every nested control.
    pub const GLASS: Theme = Theme {
        background: rgba(0x11, 0x13, 0x18, 0.18),
        card: rgba(0x16, 0x1b, 0x24, 0.12),
        foreground: rgb_hex(0xf5f5f5),
        muted: rgba(0xff, 0xff, 0xff, 0.045),
        muted_foreground: rgb_hex(0xd0d4dc),
        border: rgba(0xff, 0xff, 0xff, 0.0),
        frame_border: rgba(0xff, 0xff, 0xff, 0.18),
        input: rgba(0x0c, 0x11, 0x1b, 0.12),
        accent: rgba(0xff, 0xff, 0xff, 0.09),
        primary: rgb_hex(0xf7f7f7),
        destructive: rgb_hex(0xe5837f),
        success: rgb_hex(0x8fd9b0),
        warning: rgb_hex(0xe8c48c),
    };

    /// Dark theme, matching the reference overlay's default appearance.
    pub const DARK: Theme = Theme {
        background: rgba(0x08, 0x08, 0x08, 0.88),
        card: rgba(0x18, 0x18, 0x18, 0.52),
        foreground: rgb_hex(0xf4f4f4),
        muted: rgba(0xff, 0xff, 0xff, 0.08),
        muted_foreground: rgb_hex(0xa6a6a6),
        border: rgba(0xff, 0xff, 0xff, 0.14),
        frame_border: rgba(0xff, 0xff, 0xff, 0.22),
        input: rgba(0x12, 0x12, 0x12, 0.42),
        accent: rgba(0xff, 0xff, 0xff, 0.12),
        primary: rgb_hex(0xf5f5f5),
        destructive: rgb_hex(0xd35a5a),
        success: rgb_hex(0x6fcf97),
        warning: rgb_hex(0xd8a657),
    };

    /// Light theme: modern clean light palette with crisp contrast.
    pub const LIGHT: Theme = Theme {
        background: rgba(0xf8, 0xf9, 0xfa, 0.95),
        card: rgba(0xff, 0xff, 0xff, 0.95),
        foreground: rgb_hex(0x111827),
        muted: rgba(0x00, 0x00, 0x00, 0.06),
        muted_foreground: rgb_hex(0x6b7280),
        border: rgba(0x00, 0x00, 0x00, 0.14),
        frame_border: rgba(0x00, 0x00, 0x00, 0.20),
        input: rgba(0xff, 0xff, 0xff, 0.90),
        accent: rgba(0x00, 0x00, 0x00, 0.08),
        primary: rgb_hex(0x111827),
        destructive: rgb_hex(0xdc2626),
        success: rgb_hex(0x16a34a),
        warning: rgb_hex(0xd97706),
    };

    /// Primary fills need opaque inverse text; page alpha must never affect glyphs.
    pub fn primary_foreground(self) -> Rgba {
        if self.is_glass() {
            self.foreground
        } else {
            Self::with_alpha(self.background, 1.0)
        }
    }

    pub fn is_glass(self) -> bool {
        self.background.a < 0.2
    }

    /// Cap semantic fills in glass so active/error states remain translucent.
    pub fn fill(self, colour: Rgba) -> Rgba {
        if self.is_glass() {
            Self::with_alpha(colour, colour.a.min(0.13))
        } else {
            colour
        }
    }

    pub fn primary_fill(self) -> Rgba {
        self.fill(self.primary)
    }

    /// Glass controls use their fill for separation; other themes keep outlines.
    pub fn control_border(self, active: bool) -> Rgba {
        if self.is_glass() || !active {
            self.border
        } else {
            self.primary
        }
    }

    /// A quiet focus cue remains available even when decorative borders are off.
    pub fn focus_border(self) -> Rgba {
        if self.is_glass() {
            Self::with_alpha(self.foreground, 0.22)
        } else {
            self.primary
        }
    }

    /// Popups follow the same transparency policy as the rest of the app.
    pub fn popover(self) -> Rgba {
        if self.is_glass() {
            Self::with_alpha(self.background, 0.78)
        } else {
            Self::with_alpha(self.card, 0.98)
        }
    }

    pub fn menu_hover(self) -> Rgba {
        if self.is_glass() {
            self.accent
        } else {
            self.muted
        }
    }

    pub const DEFAULT: Theme = Self::DARK;

    pub const fn new() -> Self {
        Self::DARK
    }

    pub fn from_choice(choice: crate::core::models::settings::ThemeChoice) -> Self {
        match choice {
            crate::core::models::settings::ThemeChoice::Dark => Self::DARK,
            crate::core::models::settings::ThemeChoice::Light => Self::LIGHT,
            crate::core::models::settings::ThemeChoice::Glass => Self::GLASS,
            crate::core::models::settings::ThemeChoice::System => Self::DARK,
        }
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
    ///
    /// The glass palette already carries alpha, so this multiplies rather than
    /// flattening to the window value: a 0.06 card at 50% stays a whisper of
    /// light instead of becoming an opaque block.
    fn fade(self, colour: Rgba) -> Rgba {
        if colour.a == 0.0 {
            return colour;
        }
        // Below the old 35% floor a surface kept a visible tint, so 0% never
        // reached a truly clear window. Scale straight from the chosen value.
        Rgba {
            a: (colour.a * self.opacity).clamp(0.0, 1.0),
            ..colour
        }
    }

    /// Surfaces that carry text keep a floor so labels stay readable even when
    /// the background is fully transparent. Only the page behind them clears.
    fn fade_readable(self, colour: Rgba) -> Rgba {
        if colour.a == 0.0 {
            return colour;
        }
        let weight = 0.35 + self.opacity * 0.65;
        Rgba {
            a: (colour.a * weight).clamp(0.0, 1.0),
            ..colour
        }
    }

    /// The page behind everything. This is the only surface that goes fully
    /// clear at 0%, so the window can sit invisibly over the desktop.
    pub fn background(self) -> Rgba {
        self.fade(self.theme.background)
    }

    /// Glass panels keep a faint tint; other themes retain a legibility floor.
    pub fn card(self) -> Rgba {
        if self.theme.is_glass() {
            self.theme.card
        } else {
            self.fade_readable(self.theme.card)
        }
    }

    pub fn muted(self) -> Rgba {
        if self.theme.is_glass() {
            self.theme.muted
        } else {
            self.fade_readable(self.theme.muted)
        }
    }

    pub fn accent(self) -> Rgba {
        if self.theme.is_glass() {
            self.theme.accent
        } else {
            self.fade_readable(self.theme.accent)
        }
    }

    pub fn input(self) -> Rgba {
        if self.theme.is_glass() {
            self.theme.input
        } else {
            self.fade_readable(self.theme.input)
        }
    }

    /// Primary actions use a soft translucent fill in glass.
    pub fn primary(self) -> Rgba {
        if self.theme.is_glass() {
            return self.theme.primary_fill();
        }
        let boosted = (self.opacity + 0.35).clamp(0.0, 1.0);
        Rgba {
            a: boosted,
            ..self.theme.primary
        }
    }

    pub fn muted_foreground(self) -> Rgba {
        if self.theme.is_glass() {
            self.theme.muted_foreground
        } else {
            self.fade(self.theme.muted_foreground)
        }
    }

    pub fn border(self) -> Rgba {
        if self.theme.is_glass() {
            self.theme.border
        } else {
            self.fade(self.theme.border)
        }
    }
}

/// Stable code metrics and programming ligatures in fenced and inline code.
pub fn code_font_family() -> gpui::SharedString {
    "FiraCode Nerd Font".into()
}

/// Bundled proportional UI face, consistent even when the desktop uses a code font.
pub fn ui_font_family() -> gpui::SharedString {
    "Satoshi".into()
}

/// Base em size. Every text size in the UI is expressed in rems against this,
/// so one value scales the whole interface.
pub const BASE_REM: f32 = 16.0;

/// The em size for a stored font-scale percentage.
///
/// GPUI resolves `text_size` written in rems against the window's rem size, so
/// setting this on the window scales labels as well as their line heights.
pub fn rem_size_for(font_scale: u32) -> gpui::Pixels {
    gpui::px(BASE_REM * font_scale.clamp(85, 140) as f32 / 100.0)
}

/// A pixel size expressed as a scale-independent length.
pub fn scaled(font_size: f32) -> gpui::Rems {
    gpui::rems(font_size / BASE_REM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_scale_moves_rem_size_and_stays_readable() {
        assert_eq!(rem_size_for(100), gpui::px(16.0));
        assert_eq!(rem_size_for(125), gpui::px(20.0));
        // Out-of-range values from hand-edited settings are clamped, never tiny.
        assert_eq!(rem_size_for(10), gpui::px(16.0 * 0.85));
        assert_eq!(rem_size_for(500), gpui::px(16.0 * 1.40));
        // A 12px label stays 12px at 100% and grows with the setting.
        assert_eq!(scaled(12.0), gpui::rems(0.75));
    }

    #[test]
    fn glass_keeps_soft_fills_and_opaque_text_without_decorative_outlines() {
        let theme = Theme::GLASS;
        for opacity in [0.0, 0.5, 1.0] {
            let surfaces = theme.surfaces(opacity);
            for colour in [
                surfaces.card(),
                surfaces.muted(),
                surfaces.input(),
                surfaces.accent(),
                surfaces.primary(),
                theme.menu_hover(),
                theme.fill(theme.destructive),
            ] {
                assert!(colour.a > 0.0 && colour.a <= 0.13);
            }
            assert_eq!(surfaces.border().a, 0.0);
            assert_eq!(theme.control_border(true).a, 0.0);
            assert!(theme.focus_border().a > 0.0);
            assert!(surfaces.background().a <= 0.18);
            assert!(theme.popover().a > surfaces.card().a);
            assert_eq!(theme.primary_foreground(), theme.foreground);
            assert_eq!(theme.foreground.a, 1.0);
        }
        assert!(Theme::DARK.primary_fill().a > 0.0);
        assert!(Theme::DARK.popover().a > 0.0);
        assert!(Theme::LIGHT.primary_fill().a > 0.0);
    }

    #[test]
    fn border_preference_keeps_focus_and_error_colours() {
        let settings = crate::core::settings::Settings {
            show_borders: false,
            ..Default::default()
        };
        let theme = Theme::from_settings(&settings);
        assert_eq!(theme.surfaces(1.0).border().a, 0.0);
        assert!(theme.primary.a > 0.0);
        assert!(theme.destructive.a > 0.0);
    }

    #[test]
    fn window_frame_respects_border_preference_in_every_theme() {
        use crate::core::models::settings::ThemeChoice;
        for choice in [ThemeChoice::Glass, ThemeChoice::Dark, ThemeChoice::Light] {
            let mut settings = crate::core::settings::Settings {
                theme: choice,
                show_borders: true,
                transparency: 0,
                ..Default::default()
            };
            assert!(Theme::from_settings(&settings).frame_border.a > 0.0);
            settings.show_borders = false;
            assert_eq!(Theme::from_settings(&settings).frame_border.a, 0.0);
        }
        assert_eq!(Theme::GLASS.control_border(true).a, 0.0);
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
