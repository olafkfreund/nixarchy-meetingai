//! Shared window edge, painted last so content cannot cover the two hairlines.
use gpui::{BorderStyle, IntoElement, Styled, canvas, px, quad, transparent_black};

use crate::ui::theme::{Theme, radius};

/// Both strokes use the same curve, offset by exactly one device pixel. The
/// canvas has no hitbox, so it cannot intercept resizing, dragging or clicks.
pub fn window_edge(theme: Theme) -> impl IntoElement {
    surface_edge(theme, radius::XL)
}

/// The same paired edge for the session pill and composer group.
pub fn surface_edge(theme: Theme, radius: f32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let hairline = px(1.0 / window.scale_factor());
            let outer_radius = px(radius);
            window.paint_quad(quad(
                bounds,
                outer_radius,
                transparent_black(),
                hairline,
                theme.frame_outer_border,
                BorderStyle::Solid,
            ));
            window.paint_quad(quad(
                bounds.inset(hairline),
                outer_radius - hairline,
                transparent_black(),
                hairline,
                theme.frame_border,
                BorderStyle::Solid,
            ));
        },
    )
    .absolute()
    .inset_0()
}
