//! Shared building blocks for the overlay and settings surfaces.
//!
//! Small, composable pieces so screens read as composition rather than a pile of
//! styling. Each helper takes the active [`Theme`] so colour decisions stay in
//! one place.

use gpui::{
    AnyElement, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, Rgba,
    Styled, div, px,
};

use super::theme::Theme;
pub use super::theme::{radius, space};

/// A rounded container used for cards and grouped settings.
pub fn panel(theme: &Theme) -> gpui::Div {
    div()
        .rounded(px(radius::XL))
        .border_1()
        .border_color(theme.border)
        .bg(theme.card)
}

/// A card with a title, optional description and arbitrary body.
pub fn card(
    theme: &Theme,
    title: &'static str,
    description: Option<&'static str>,
    body: impl IntoElement,
) -> impl IntoElement {
    let mut head = div().flex().flex_col().gap(px(2.0)).child(
        div()
            .text_size(px(13.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.foreground)
            .child(title),
    );
    if let Some(description) = description {
        head = head.child(
            div()
                .text_size(px(11.5))
                .text_color(theme.muted_foreground)
                .child(description),
        );
    }
    panel(theme)
        .flex()
        .flex_col()
        .gap(px(space::MD))
        .p(px(space::LG))
        .child(head)
        .child(body)
}

/// A clickable pill, used for follow-up chips and segmented choices.
pub fn pill(
    id: impl Into<ElementId>,
    theme: &Theme,
    surfaces: crate::features::theme::Surfaces,
    active: bool,
) -> gpui::Stateful<gpui::Div> {
    let mut element = div()
        .h(px(26.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .rounded_full()
        .border_1()
        .text_size(px(11.0))
        .cursor_pointer();

    element = if active {
        element
            .bg(surfaces.primary())
            .border_color(theme.primary)
            .text_color(theme.background)
    } else {
        element
            .bg(surfaces.muted())
            .border_color(surfaces.border())
            .text_color(theme.foreground)
    };

    element.id(id)
}

/// A compact icon-sized ghost button.
pub fn icon_button(
    id: impl Into<ElementId>,
    theme: &Theme,
    glyph: lucide_icons::Icon,
) -> gpui::Stateful<gpui::Div> {
    div()
        .size(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::MD))
        .cursor_pointer()
        .hover(|style| style.bg(theme.surfaces(0.6).accent()))
        .id(id)
        .child(crate::features::icons::icon(theme, glyph, 13.0))
}

/// A one-pixel horizontal divider in the theme's border colour.
pub fn divider(theme: &Theme) -> impl IntoElement {
    div().h(px(1.0)).w_full().bg(theme.border)
}

/// A labelled row: title on the left, control on the right. Mirrors the
/// settings list rows in the reference.
pub fn setting_row(
    theme: &Theme,
    title: &'static str,
    description: Option<&'static str>,
    control: AnyElement,
) -> impl IntoElement {
    let mut text = div().flex().flex_col().gap(px(1.0)).child(
        div()
            .text_size(px(12.5))
            .text_color(theme.foreground)
            .child(title),
    );
    if let Some(description) = description {
        text = text.child(
            div()
                .text_size(px(11.0))
                .text_color(theme.muted_foreground)
                .child(description),
        );
    }
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(space::LG))
        .py(px(space::SM))
        .child(text)
        .child(control)
}

/// An inline status message.
pub fn notice(theme: &Theme, colour: Rgba, message: &'static str) -> impl IntoElement {
    div()
        .rounded(px(radius::MD))
        .bg(theme.muted)
        .px(px(space::MD))
        .py(px(space::SM))
        .text_size(px(11.5))
        .text_color(colour)
        .child(message)
}
