//! The overlay window: mode tabs, status, empty state, composer and size row.

use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, Window, div, px, rgb,
};

use super::chat::Conversation;
use super::domain::{Mode, OverlaySize};
use super::theme::Theme;

/// Root view for the overlay window.
pub struct Overlay {
    mode: Mode,
    size: OverlaySize,
    conversation: Conversation,
    theme: Theme,
}

impl Overlay {
    pub fn new() -> Self {
        Self {
            mode: Mode::default(),
            size: OverlaySize::default(),
            conversation: Conversation::new(),
            theme: Theme::new(),
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    pub fn size(&self) -> OverlaySize {
        self.size
    }

    pub fn conversation(&self) -> &Conversation {
        &self.conversation
    }

    pub fn conversation_mut(&mut self) -> &mut Conversation {
        &mut self.conversation
    }

    fn mode_tab(
        &self,
        mode: Mode,
        active: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme;
        let mut tab = div()
            .flex()
            .items_center()
            .gap_1()
            .h(px(28.0))
            .px(px(10.0))
            .rounded(px(8.0))
            .text_size(px(12.5))
            .font_weight(if active {
                gpui::FontWeight::MEDIUM
            } else {
                gpui::FontWeight::NORMAL
            })
            .text_color(if active {
                theme.foreground
            } else {
                theme.muted_foreground
            })
            .cursor_pointer()
            .id(("mode-tab", mode as usize));
        tab = tab.on_click(cx.listener(move |this, _, _, cx| {
            this.set_mode(mode);
            cx.notify();
        }));
        if active {
            tab = tab.bg(theme.muted);
        }
        tab.child(mode.label())
    }

    fn mode_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .p(px(2.0))
            .rounded(px(10.0))
            .bg(rgb(0x141416))
            .child(self.mode_tab(Mode::Ask, self.mode == Mode::Ask, cx))
            .child(self.mode_tab(Mode::Listen, self.mode == Mode::Listen, cx))
    }

    fn status_pill(&self) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(28.0))
            .px(px(10.0))
            .rounded_full()
            .border_1()
            .border_color(theme.border)
            .bg(theme.muted)
            .text_size(px(11.0))
            .text_color(theme.muted_foreground)
            .child(div().size(px(6.0)).rounded_full().bg(theme.success))
            .child("Ready")
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .h(px(44.0))
            .px(px(10.0))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.foreground)
                            .child("lennette"),
                    )
                    .child(self.mode_tabs(cx)),
            )
            .child(div().flex_1())
            .child(self.status_pill())
    }

    fn empty_state(&self) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .flex_1()
            .items_center()
            .justify_center()
            .gap(px(10.0))
            .text_size(px(13.0))
            .text_color(theme.muted_foreground)
            .child("Ask anything — or start listening and I'll follow the conversation.")
    }

    fn composer(&self) -> impl IntoElement {
        let theme = self.theme;
        div()
            .m(px(10.0))
            .h(px(40.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .rounded(px(10.0))
            .border_1()
            .border_color(theme.input)
            .bg(theme.card)
            .text_size(px(13.0))
            .text_color(theme.muted_foreground)
            .child("Ask anything, or paste a screenshot...")
    }

    fn size_row(&self) -> impl IntoElement {
        let theme = self.theme;
        let current = self.size;
        let mut row = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(12.0))
            .pb(px(8.0))
            .text_size(px(9.5))
            .text_color(theme.muted_foreground)
            .child("SIZE:");
        for size in OverlaySize::ALL {
            let active = size == current;
            row = row.child(
                div()
                    .px(px(4.0))
                    .text_color(if active {
                        theme.foreground
                    } else {
                        theme.muted_foreground
                    })
                    .child(size.label()),
            );
        }
        row
    }
}

impl Default for Overlay {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for Overlay {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(self.header(cx))
            .child(self.empty_state())
            .child(self.composer())
            .child(self.size_row())
    }
}
