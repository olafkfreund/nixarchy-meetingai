//! Iconography.
//!
//! Uses the Lucide icon font — the same set the reference UI uses — so marks are
//! consistent vector glyphs rather than emoji. The font is registered once at
//! startup; icons then render as ordinary text tinted by `text_color`.

use gpui::{
    FontWeight, ImageSource, IntoElement, ParentElement, RenderImage, Resource, Styled, div, px,
};

use crate::ui::theme::Theme;

/// Family name reported by the bundled Lucide font.
pub const FAMILY: &str = "lucide";

/// The bundled Lucide TTF, registered with the text system at startup.
pub const FONT_BYTES: &[u8] = lucide_icons::LUCIDE_FONT_BYTES;

/// A single icon at a given point size.
pub fn icon(theme: &Theme, glyph: lucide_icons::Icon, size: f32) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size(px(size + 4.0))
        .font_family(FAMILY)
        .font_weight(FontWeight::NORMAL)
        .text_size(px(size))
        .text_color(theme.muted_foreground)
        .child(char::from(glyph).to_string())
}

/// An icon tinted with an explicit colour, for accents and active states.
pub fn icon_colored(color: gpui::Rgba, glyph: lucide_icons::Icon, size: f32) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size(px(size + 4.0))
        .font_family(FAMILY)
        .text_size(px(size))
        .text_color(color)
        .child(char::from(glyph).to_string())
}

/// Decoded brand logo image, cached in memory for instantaneous rendering.
pub fn brand_logo() -> ImageSource {
    static LOGO: std::sync::LazyLock<Option<std::sync::Arc<RenderImage>>> =
        std::sync::LazyLock::new(|| {
            let bytes = include_bytes!("../../assets/lennette.png");
            let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?;
            let mut rgba = img.into_rgba8();
            // GPUI RenderImage expects BGRA layout.
            for pixel in rgba.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
            let frame = image::Frame::new(rgba);
            Some(std::sync::Arc::new(RenderImage::new([frame])))
        });

    if let Some(render_image) = LOGO.as_ref() {
        ImageSource::Render(render_image.clone())
    } else {
        ImageSource::Resource(Resource::Embedded("lennette.png".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_font_is_present() {
        assert!(FONT_BYTES.len() > 1000, "lucide font should be embedded");
    }

    #[test]
    fn icon_names_are_stable() {
        assert_eq!(format!("{}", lucide_icons::Icon::Mic), "mic");
        assert_eq!(format!("{}", lucide_icons::Icon::Camera), "camera");
        assert_eq!(format!("{}", lucide_icons::Icon::Settings), "settings");
    }

    #[test]
    fn brand_logo_loads_successfully() {
        let source = brand_logo();
        assert!(matches!(source, ImageSource::Render(_)));
    }
}
