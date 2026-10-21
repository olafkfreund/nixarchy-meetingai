//! The overlay window.
//!
//! Mirrors the reference overlay: a drag header with brand, icon mode tabs and a
//! live status pill; the Ask body with conversation, follow-ups and input block;
//! a labelled capture/attach cluster; the composer with model, image and send
//! controls; and the size switcher row.

use gpui::{
    Context, FontWeight, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use lucide_icons::Icon;

use super::chat::Conversation;
use super::components::{icon_button, pill, radius, space};
use super::domain::{Mode, OverlaySize, Role};
use super::icons;
use super::theme::Theme;

/// Follow-up suggestions shown above the composer, in the reference's order.
pub const FOLLOW_UPS: [&str; 4] = [
    "What should I say?",
    "Follow-up questions",
    "Fact-check",
    "Recap",
];

/// The tool cluster entries, in the reference's order.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Capture,
    Selection,
    Attach,
    Library,
}

impl Tool {
    const ALL: [Tool; 4] = [Tool::Capture, Tool::Selection, Tool::Attach, Tool::Library];

    fn icon(self) -> Icon {
        match self {
            Tool::Capture => Icon::Monitor,
            Tool::Selection => Icon::Crop,
            Tool::Attach => Icon::Paperclip,
            Tool::Library => Icon::FolderOpen,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Tool::Capture => "Capture",
            Tool::Selection => "Selection",
            Tool::Attach => "Attach",
            Tool::Library => "Library",
        }
    }
}

/// Root view for the overlay window.
pub struct Overlay {
    mode: Mode,
    size: OverlaySize,
    conversation: Conversation,
    theme: Theme,
    selected_model: &'static str,
    prompt_mode: &'static str,
    use_image: bool,
    show_settings: bool,
    show_history: bool,
    listening: bool,
    elapsed_secs: u32,
}

impl Overlay {
    pub fn new() -> Self {
        Self {
            mode: Mode::default(),
            size: OverlaySize::default(),
            conversation: Conversation::new(),
            theme: Theme::new(),
            selected_model: "Select a model",
            prompt_mode: "General",
            use_image: false,
            show_settings: false,
            show_history: false,
            listening: false,
            elapsed_secs: 0,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn size(&self) -> OverlaySize {
        self.size
    }

    pub fn conversation(&self) -> &Conversation {
        &self.conversation
    }

    pub fn is_listening(&self) -> bool {
        self.listening
    }

    pub fn set_mode(&mut self, mode: Mode) {
        // Leaving Listen pauses the session, matching the reference behaviour.
        if self.mode == Mode::Listen && mode != Mode::Listen {
            self.listening = false;
        }
        self.mode = mode;
        self.show_settings = false;
        self.show_history = false;
    }

    /// Compact layouts drop tool labels, as the reference does below `md`.
    fn show_tool_labels(&self) -> bool {
        self.size != OverlaySize::Small
    }

    fn status_label(&self) -> (String, gpui::Rgba) {
        let theme = self.theme;
        if self.listening && self.mode == Mode::Listen {
            (
                format!(
                    "Listening · {:02}:{:02}",
                    self.elapsed_secs / 60,
                    self.elapsed_secs % 60
                ),
                theme.success,
            )
        } else {
            ("Ready".to_string(), theme.muted_foreground)
        }
    }

    fn mode_tab(&self, mode: Mode, active: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let icon = match mode {
            Mode::Ask => Icon::MessageSquare,
            Mode::Listen => Icon::Headphones,
        };
        let mut tab = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(24.0))
            .px(px(10.0))
            .rounded(px(radius::SM))
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
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
            tab = tab.bg(theme.background);
        }
        tab.child(icons::icon(&theme, icon, 13.0))
            .child(mode.label())
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let settings_active = self.show_settings;
        let history_active = self.show_history;

        div()
            .flex()
            .items_center()
            .gap(px(space::SM))
            .h(px(44.0))
            .px(px(10.0))
            .border_b_1()
            .border_color(theme.border)
            // Home mark opens the dashboard.
            .child(icon_button("home", &theme, Icon::House))
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.foreground)
                    .child("lennette"),
            )
            .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .p(px(2.0))
                    .rounded(px(radius::MD))
                    .bg(theme.card)
                    .child(self.mode_tab(Mode::Ask, self.mode == Mode::Ask, cx))
                    .child(self.mode_tab(Mode::Listen, self.mode == Mode::Listen, cx)),
            )
            .child(div().flex_1())
            .child(self.status_pill())
            .child(self.dashboard_button())
            .child(self.settings_button(settings_active, cx))
            .child(self.history_button(history_active, cx))
    }

    fn status_pill(&self) -> impl IntoElement {
        let theme = self.theme;
        let (label, dot) = self.status_label();
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(26.0))
            .px(px(8.0))
            .rounded_full()
            .border_1()
            .border_color(theme.border)
            .bg(theme.muted)
            .text_size(px(10.5))
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme.foreground)
            .child(div().size(px(6.0)).rounded_full().bg(dot))
            .child(label)
    }

    fn dashboard_button(&self) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .h(px(26.0))
            .px(px(8.0))
            .rounded_full()
            .border_1()
            .border_color(theme.border)
            .bg(theme.muted)
            .text_size(px(10.5))
            .text_color(theme.foreground)
            .cursor_pointer()
            .id("open-dashboard")
            .child(icons::icon(&theme, Icon::LayoutDashboard, 11.0))
            .child("Open dashboard")
    }

    fn settings_button(&self, active: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut button = icon_button("toggle-settings", &theme, Icon::Settings).on_click(
            cx.listener(move |this, _, _, cx| {
                this.show_settings = !this.show_settings;
                this.show_history = false;
                cx.notify();
            }),
        );
        if active {
            button = button.bg(theme.accent);
        }
        button
    }

    fn history_button(&self, active: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut button = icon_button("toggle-history", &theme, Icon::History).on_click(
            cx.listener(move |this, _, _, cx| {
                this.show_history = !this.show_history;
                this.show_settings = false;
                cx.notify();
            }),
        );
        if active {
            button = button.bg(theme.accent);
        }
        button
    }

    fn empty_state(&self) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .flex_1()
            .items_center()
            .justify_center()
            .gap(px(space::SM))
            .px(px(space::XL))
            .child(icons::icon_colored(theme.primary, Icon::Sparkles, 18.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(theme.muted_foreground)
                    .child(if self.mode == Mode::Ask {
                        "Ask anything — or start listening and I'll follow the conversation."
                    } else {
                        "Start listening and I'll follow the conversation."
                    }),
            )
    }

    fn message_list(&self) -> impl IntoElement {
        let theme = self.theme;
        let mut list = div()
            .id("message-list")
            .flex()
            .flex_col()
            .flex_1()
            .gap(px(space::MD))
            .overflow_y_scroll()
            .px(px(space::LG))
            .py(px(space::MD));

        for (index, message) in self.conversation.messages().iter().enumerate() {
            let is_user = message.role == Role::User;
            let bubble = div()
                .id(("message", index))
                .max_w(px(520.0))
                .rounded(px(radius::LG))
                .px(px(space::MD))
                .py(px(space::SM))
                .text_size(px(12.5))
                .text_color(if is_user {
                    theme.background
                } else {
                    theme.foreground
                })
                .bg(if is_user { theme.primary } else { theme.muted })
                .child(message.content.clone());

            list = list.child(if is_user {
                div().flex().justify_end().child(bubble)
            } else {
                div().flex().justify_start().child(bubble)
            });
        }
        list
    }

    fn follow_ups(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut chips = div()
            .flex()
            .items_center()
            .gap(px(space::SM))
            .flex_1()
            .id("follow-up-chips")
            .overflow_x_scroll();

        for (index, label) in FOLLOW_UPS.iter().enumerate() {
            let chip = pill(("follow-up", index), &theme, false)
                .flex_shrink_0()
                .child(icons::icon(&theme, Icon::ArrowUpRight, 10.0))
                .child(*label);
            chips = chips.child(chip.on_click(cx.listener(move |this, _, _, cx| {
                this.conversation.set_draft(*label);
                cx.notify();
            })));
        }

        div()
            .flex()
            .items_center()
            .gap(px(space::SM))
            .w_full()
            .px(px(space::MD))
            .py(px(space::SM))
            .border_t_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(9.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.muted_foreground)
                    .whitespace_nowrap()
                    .child("FOLLOW-UPS"),
            )
            .child(chips)
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(4.0))
                    .text_size(px(11.0))
                    .text_color(theme.muted_foreground)
                    .cursor_pointer()
                    .id("edit-follow-ups")
                    .child(icons::icon(&theme, Icon::Plus, 10.0))
                    .child("Edit"),
            )
    }

    fn tool_cluster(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let labelled = self.show_tool_labels();
        let mut cluster = div()
            .flex()
            .items_center()
            .gap(px(1.0))
            .p(px(2.0))
            .rounded_full()
            .border_1()
            .border_color(theme.border)
            .bg(theme.muted);

        for (index, tool) in Tool::ALL.into_iter().enumerate() {
            if index > 0 {
                cluster = cluster.child(div().w(px(1.0)).h(px(14.0)).bg(theme.border));
            }
            let mut button = div()
                .flex()
                .items_center()
                .gap(px(5.0))
                .h(px(26.0))
                .rounded_full()
                .cursor_pointer()
                .hover(|style| style.bg(theme.accent))
                .id(("tool", index))
                .child(icons::icon(&theme, tool.icon(), 13.0));
            button = if labelled {
                button.px(px(9.0))
            } else {
                button.w(px(26.0)).justify_center()
            };
            if labelled {
                button = button.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.foreground)
                        .child(tool.label()),
                );
            }
            cluster = cluster.child(button.on_click(cx.listener(|_, _, _, cx| {
                cx.notify();
            })));
        }
        cluster
    }

    fn push_to_talk(&self) -> impl IntoElement {
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
            .text_size(px(11.0))
            .text_color(theme.foreground)
            .cursor_pointer()
            .id("push-to-talk")
            .child(icons::icon(&theme, Icon::Mic, 12.0))
            .child(if self.listening {
                "Listening…"
            } else {
                "Push to talk"
            })
            .child(kbd(&theme, "⌃"))
            .child(kbd(&theme, "Space"))
    }

    fn image_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let active = self.use_image;
        let mut button = div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .h(px(28.0))
            .px(px(8.0))
            .rounded(px(radius::MD))
            .border_1()
            .text_size(px(11.0))
            .cursor_pointer()
            .id("toggle-image");
        button = if active {
            button
                .border_color(theme.primary)
                .bg(theme.accent)
                .text_color(theme.primary)
        } else {
            button
                .border_color(theme.border)
                .text_color(theme.muted_foreground)
        };
        button
            .child(icons::icon(&theme, Icon::Image, 12.0))
            .child("Use image")
            .on_click(cx.listener(|this, _, _, cx| {
                this.use_image = !this.use_image;
                cx.notify();
            }))
    }

    fn model_selector(&self) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .h(px(28.0))
            .px(px(8.0))
            .rounded(px(radius::MD))
            .border_1()
            .border_color(theme.border)
            .text_size(px(11.5))
            .text_color(theme.muted_foreground)
            .cursor_pointer()
            .id("model-select")
            .child(icons::icon(&theme, Icon::Sparkles, 11.0))
            .child(self.selected_model)
            .child(icons::icon(&theme, Icon::ChevronDown, 11.0))
    }

    fn input_block(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let draft = self.conversation.draft().to_string();
        let can_submit = self.conversation.can_submit();

        div()
            .flex()
            .flex_col()
            .gap(px(space::SM))
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.card)
            .px(px(space::MD))
            .py(px(10.0))
            // Push to talk + tool cluster.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(self.push_to_talk())
                    .child(div().flex_1())
                    .child(self.tool_cluster(cx)),
            )
            // Model row: context on the left, model and image on the right.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::SM))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .h(px(28.0))
                            .px(px(8.0))
                            .rounded(px(radius::MD))
                            .text_size(px(11.5))
                            .text_color(theme.foreground)
                            .cursor_pointer()
                            .id("prompt-mode")
                            .child(icons::icon(&theme, Icon::Sparkles, 11.0))
                            .child(self.prompt_mode)
                            .child(icons::icon(&theme, Icon::ChevronDown, 11.0)),
                    )
                    .child(div().flex_1())
                    .child(self.image_toggle(cx))
                    .child(self.model_selector()),
            )
            // Composer.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::SM))
                    .h(px(38.0))
                    .px(px(space::MD))
                    .rounded(px(radius::LG))
                    .border_1()
                    .border_color(theme.input)
                    .id("composer")
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.5))
                            .text_color(if draft.is_empty() {
                                theme.muted_foreground
                            } else {
                                theme.foreground
                            })
                            .child(if draft.is_empty() {
                                "Ask anything, or paste a screenshot…".to_string()
                            } else {
                                draft
                            }),
                    )
                    .child(kbd(&theme, "↵"))
                    .child(
                        div()
                            .size(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(radius::MD))
                            .bg(if can_submit {
                                theme.primary
                            } else {
                                theme.muted
                            })
                            .cursor_pointer()
                            .id("send")
                            .child(icons::icon_colored(
                                if can_submit {
                                    theme.background
                                } else {
                                    theme.muted_foreground
                                },
                                Icon::ArrowUp,
                                13.0,
                            ))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.conversation.submit();
                                cx.notify();
                            })),
                    ),
            )
            .child(self.size_row(cx))
    }

    fn size_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut switcher = div()
            .flex()
            .items_center()
            .gap(px(1.0))
            .p(px(2.0))
            .rounded(px(radius::MD))
            .border_1()
            .border_color(theme.border)
            .bg(theme.muted);

        for size in OverlaySize::ALL {
            let active = size == self.size;
            let mut chip = div()
                .flex()
                .items_center()
                .justify_center()
                .h(px(20.0))
                .min_w(px(20.0))
                .px(px(6.0))
                .rounded(px(radius::SM))
                .text_size(px(10.0))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .text_color(if active {
                    theme.foreground
                } else {
                    theme.muted_foreground
                })
                .id(("size", size as usize))
                .child(size.label());
            if active {
                chip = chip.bg(theme.background);
            }
            switcher = switcher.child(chip.on_click(cx.listener(move |this, _, _, cx| {
                this.size = size;
                cx.notify();
            })));
        }

        div()
            .flex()
            .items_center()
            .gap(px(space::SM))
            .w_full()
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(10.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.muted_foreground)
                    .whitespace_nowrap()
                    .child("Size:"),
            )
            .child(switcher)
            .child(div().flex_1())
            .child(kbd(&theme, "Ctrl"))
            .child(kbd(&theme, "\\"))
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(10.5))
                    .text_color(theme.muted_foreground)
                    .whitespace_nowrap()
                    .child("hide"),
            )
    }
}

/// A small keyboard chip.
fn kbd(theme: &Theme, label: &'static str) -> impl IntoElement {
    div()
        .rounded(px(4.0))
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted)
        .px(px(4.0))
        .text_size(px(9.5))
        .text_color(theme.muted_foreground)
        .child(label)
}

impl Default for Overlay {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for Overlay {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut body = div().flex().flex_col().flex_1();
        if self.conversation.is_empty() {
            body = body.child(self.empty_state());
        } else {
            body = body.child(self.message_list());
        }

        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(self.header(cx))
            // The body owns the remaining height so the empty state can centre in
            // it, and follow-ups sit directly above the input block.
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .child(body)
                    .child(self.follow_ups(cx)),
            )
            .child(self.input_block(cx))
    }
}
