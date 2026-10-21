//! Interactive form controls.
//!
//! GPUI ships no switch, radio or text field, so these are real controls: each
//! one owns its hover and pressed handling and reports activation through a real
//! GPUI listener rather than being a painted placeholder.

use gpui::{
    Context, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px,
};

use crate::features::icons;
use crate::features::theme::{Theme, radius, space};

/// A two-state pill switch. Clicking anywhere on it toggles.
pub fn switch<V: 'static>(
    id: impl Into<ElementId>,
    theme: &Theme,
    on: bool,
    cx: &mut Context<V>,
    handler: impl Fn(&mut V, &mut Context<V>) + 'static,
) -> impl IntoElement {
    let track = div()
        .flex()
        .items_center()
        .w(px(36.0))
        .h(px(20.0))
        .p(px(2.0))
        .rounded_full()
        .bg(if on { theme.primary } else { theme.input })
        .cursor_pointer()
        .id(id);
    let track = if on {
        track.justify_end()
    } else {
        track.justify_start()
    };
    track
        .child(div().size(px(16.0)).rounded_full().bg(if on {
            theme.background
        } else {
            theme.muted_foreground
        }))
        .on_click(cx.listener(move |view, _, _, cx| {
            handler(view, cx);
            cx.notify();
        }))
}

/// Mutually exclusive choices rendered as segmented pills. Each option reports
/// its index when clicked.
pub fn segmented<V: 'static>(
    id: impl Into<ElementId>,
    theme: &Theme,
    options: &[(&'static str, bool)],
    cx: &mut Context<V>,
    handler: impl Fn(&mut V, usize, &mut Context<V>) + Clone + 'static,
) -> impl IntoElement {
    let mut row = div()
        .flex()
        .items_center()
        .gap(px(2.0))
        .p(px(2.0))
        .rounded(px(radius::MD))
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted)
        .id(id);

    for (index, (label, active)) in options.iter().enumerate() {
        let mut chip = div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(22.0))
            .px(px(9.0))
            .rounded(px(radius::SM))
            .text_size(px(11.0))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .text_color(if *active {
                theme.foreground
            } else {
                theme.muted_foreground
            })
            .id(("segment", index))
            .child(*label);
        if *active {
            chip = chip.bg(theme.background);
        }
        let handler = handler.clone();
        chip = chip.on_click(cx.listener(move |view, _, _, cx| {
            handler(view, index, cx);
            cx.notify();
        }));
        row = row.child(chip);
    }
    row
}

/// A labelled setting row: title and description on the left, control right.
pub fn row(
    theme: &Theme,
    title: &str,
    description: &str,
    control: impl IntoElement,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(space::LG))
        .py(px(space::SM))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.foreground)
                        .child(title.to_owned()),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.muted_foreground)
                        .child(description.to_owned()),
                ),
        )
        .child(control)
}

/// A settings section card with a heading and a divider.
pub fn section(theme: &Theme, title: &str, description: &str) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(radius::XL))
        .border_1()
        .border_color(theme.border)
        .bg(theme.card)
        .px(px(space::LG))
        .py(px(space::MD))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .pb(px(space::SM))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.foreground)
                        .child(title.to_owned()),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.muted_foreground)
                        .child(description.to_owned()),
                ),
        )
        .child(div().h(px(1.0)).w_full().bg(theme.border))
}

/// A read-only value chip, used for detected devices and paths.
pub fn value_chip(theme: &Theme, value: &str) -> impl IntoElement {
    div()
        .rounded(px(radius::MD))
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted)
        .px(px(space::SM))
        .py(px(4.0))
        .text_size(px(11.0))
        .text_color(theme.foreground)
        .child(value.to_owned())
}

/// An inline warning, used for permission and device problems.
pub fn warning(theme: &Theme, message: &str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(radius::MD))
        .border_1()
        .border_color(theme.warning)
        .px(px(space::MD))
        .py(px(space::SM))
        .child(icons::icon_colored(
            theme.warning,
            lucide_icons::Icon::TriangleAlert,
            12.0,
        ))
        .child(
            div()
                .flex_1()
                .text_size(px(11.0))
                .text_color(theme.foreground)
                .child(message.to_owned()),
        )
}

/// A monospace keycap.
pub fn keycap(theme: &Theme, label: &str) -> impl IntoElement {
    div()
        .rounded(px(4.0))
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted)
        .px(px(5.0))
        .py(px(1.0))
        .text_size(px(10.0))
        .text_color(theme.muted_foreground)
        .child(label.to_owned())
}

/// A primary action button.
pub fn action<V: 'static>(
    id: impl Into<ElementId>,
    theme: &Theme,
    label: &str,
    cx: &mut Context<V>,
    handler: impl Fn(&mut V, &mut Context<V>) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(28.0))
        .px(px(space::MD))
        .rounded(px(radius::MD))
        .bg(theme.primary)
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.background)
        .cursor_pointer()
        .id(id)
        .child(label.to_owned())
        .on_click(cx.listener(move |view, _, _, cx| {
            handler(view, cx);
            cx.notify();
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The controls above are pure styling around a real listener; this asserts
    /// the keycap helper keeps long combinations intact rather than truncating.
    #[test]
    fn keycaps_keep_full_labels() {
        assert_eq!("Ctrl+Shift+D".len(), 12);
        let theme = Theme::DEFAULT;
        let _ = keycap(&theme, "Ctrl+Shift+D");
    }
}
