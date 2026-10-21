//! Dashboard rendering: sidebar, page header and per-route bodies.

use gpui::{
    Context, FontWeight, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use lucide_icons::Icon;

use super::nav::{NavItem, SECTIONS};
use super::{Dashboard, PageData};
use crate::features::db::{Database, conversations};
use crate::features::icons;
use crate::features::settings_pages::{self, SettingsState};
use crate::features::theme::Theme;

/// Shared dashboard handle. Wrapped so the view owns it.
pub struct DashboardView {
    dashboard: Dashboard,
    database: Option<Database>,
    theme: Theme,
    settings: SettingsState,
    /// Conversation whose transcript is open, when viewing a detail page.
    open_conversation: Option<String>,
}

impl DashboardView {
    pub fn new(database: Option<Database>) -> Self {
        let mut dashboard = Dashboard::new();
        if let Some(database) = database.as_ref() {
            dashboard.navigate(super::nav::Route::Home, Some(database));
        }
        let settings = SettingsState::new(database.as_ref().map(|database| {
            // A second connection keeps settings writes off the list queries.
            Database::open(database.connection().path().unwrap_or_default())
                .unwrap_or_else(|_| Database::open_in_memory().expect("in-memory database"))
        }));
        Self {
            dashboard,
            database,
            theme: Theme::new(),
            settings,
            open_conversation: None,
        }
    }

    fn sidebar_item(
        &self,
        item: NavItem,
        active: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let mut entry = div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .h(px(34.0))
            .px(px(10.0))
            .rounded(px(8.0))
            .text_size(px(12.5))
            .cursor_pointer()
            .text_color(if active {
                theme.foreground
            } else {
                theme.muted_foreground
            })
            .hover(|style| style.bg(theme.muted))
            .id(("nav", item.route as usize))
            .child(icons::icon(&theme, item.icon, 13.0))
            .child(item.label);
        if active {
            entry = entry.bg(theme.muted).font_weight(FontWeight::MEDIUM);
        }
        entry.on_click(cx.listener(move |this, _, _, cx| {
            this.navigate(item.route);
            cx.notify();
        }))
    }

    fn navigate(&mut self, route: super::nav::Route) {
        self.dashboard.navigate(route, self.database.as_ref());
    }

    /// Mutable settings state, for page controls.
    pub fn settings_mut(&mut self) -> &mut SettingsState {
        &mut self.settings
    }

    /// Persist settings after a control changes.
    pub fn save_settings(&mut self) {
        self.settings.save();
    }

    /// Open a conversation's transcript.
    pub fn open_conversation(&mut self, id: impl Into<String>) {
        self.open_conversation = Some(id.into());
        self.dashboard.navigate(
            super::nav::Route::ConversationDetail,
            self.database.as_ref(),
        );
    }

    fn conversation_messages(&self) -> Vec<conversations::MessageRecord> {
        let (Some(database), Some(id)) = (self.database.as_ref(), self.open_conversation.as_ref())
        else {
            return Vec::new();
        };
        conversations::messages(database.connection(), id).unwrap_or_default()
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut nav = div()
            .flex()
            .flex_col()
            .flex_1()
            .gap(px(2.0))
            .px(px(8.0))
            .py(px(12.0));

        for (section, items) in SECTIONS {
            nav = nav.child(
                div()
                    .px(px(10.0))
                    .pt(px(10.0))
                    .pb(px(4.0))
                    .text_size(px(9.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.muted_foreground)
                    .child(*section),
            );
            for item in *items {
                nav = nav.child(self.sidebar_item(*item, item.route == self.dashboard.route(), cx));
            }
        }

        div()
            .flex()
            .flex_col()
            .w(px(224.0))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.card)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(16.0))
                    .pt(px(14.0))
                    .pb(px(10.0))
                    .child(icons::icon_colored(theme.primary, Icon::Sparkles, 16.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.foreground)
                                    .child("lennette"),
                            )
                            .child(
                                div()
                                    .text_size(px(9.5))
                                    .text_color(theme.muted_foreground)
                                    .child(concat!("v", env!("CARGO_PKG_VERSION"))),
                            ),
                    ),
            )
            .child(nav)
    }

    fn page_header(&self) -> impl IntoElement {
        let theme = self.theme;
        let route = self.dashboard.route();
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .pt(px(24.0))
            .pb(px(16.0))
            .child(
                div()
                    .text_size(px(18.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.foreground)
                    .child(route.title()),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(theme.muted_foreground)
                    .child(route.description()),
            )
    }

    fn error_banner(&self) -> Option<impl IntoElement> {
        let theme = self.theme;
        self.dashboard.error().map(|message| {
            div()
                .mt(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(theme.destructive)
                .px(px(12.0))
                .py(px(8.0))
                .text_size(px(12.0))
                .text_color(theme.destructive)
                .child(message.to_owned())
        })
    }

    fn empty_note(&self, message: &'static str) -> impl IntoElement {
        let theme = self.theme;
        div()
            .rounded(px(12.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.card)
            .px(px(16.0))
            .py(px(28.0))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(theme.muted_foreground)
                    .child(message),
            )
    }

    fn conversations_body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let records = self.dashboard.filtered_conversations();
        if records.is_empty() {
            return div().child(self.empty_note(
                "No conversations yet. Ask something from the overlay and it will show up here.",
            ));
        }
        let mut list = div()
            .flex()
            .flex_col()
            .rounded(px(12.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.card);
        for record in records {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(14.0))
                    .py(px(10.0))
                    .border_b_1()
                    .border_color(theme.border)
                    .cursor_pointer()
                    .id(gpui::ElementId::Name(
                        format!("conversation-{}", record.id).into(),
                    ))
                    .on_click(cx.listener({
                        let id = record.id.clone();
                        move |this, _, _, cx| {
                            this.open_conversation(id.clone());
                            cx.notify();
                        }
                    }))
                    .child(icons::icon(&theme, Icon::MessagesSquare, 13.0))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.5))
                            .text_color(theme.foreground)
                            .child(record.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.muted_foreground)
                            .child(format!("{} messages", record.message_count)),
                    ),
            );
        }
        list
    }

    fn files_body(&self) -> impl IntoElement {
        let theme = self.theme;
        let PageData::Files(files) = self.dashboard.data() else {
            return div().child(self.empty_note("No files yet."));
        };
        if files.is_empty() {
            return div().child(
                self.empty_note(
                    "No files yet. Documents you attach are kept here after processing.",
                ),
            );
        }
        let mut list = div()
            .flex()
            .flex_col()
            .rounded(px(12.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.card);
        for file in files {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(14.0))
                    .py(px(10.0))
                    .border_b_1()
                    .border_color(theme.border)
                    .child(icons::icon(&theme, Icon::FileText, 13.0))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.5))
                            .text_color(theme.foreground)
                            .child(file.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.muted_foreground)
                            .child(format!("{} pages", file.pages)),
                    ),
            );
        }
        list
    }

    fn prompts_body(&self) -> impl IntoElement {
        let theme = self.theme;
        let PageData::Prompts(prompts) = self.dashboard.data() else {
            return div().child(self.empty_note("No prompts loaded."));
        };
        let mut list = div().flex().flex_col().gap(px(8.0));
        for prompt in prompts {
            let mode = prompt
                .listen_slot
                .map(|slot| format!("Listen mode {slot}"))
                .unwrap_or_else(|| "Prompt".to_owned());
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.card)
                    .px(px(14.0))
                    .py(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.foreground)
                                    .child(prompt.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(theme.muted_foreground)
                                    .child(mode),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(theme.muted_foreground)
                            .child(truncate(&prompt.prompt, 220)),
                    ),
            );
        }
        list
    }

    fn settings_body(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        settings_pages::render(self.dashboard.route(), &self.settings, &self.theme, cx)
    }

    fn conversation_detail(&self) -> gpui::AnyElement {
        let theme = self.theme;
        let messages = self.conversation_messages();
        if messages.is_empty() {
            return self
                .empty_note("Pick a conversation from the list to read it here.")
                .into_any_element();
        }
        let mut list = div().flex().flex_col().gap(px(8.0));
        for message in messages {
            let is_user = message.role == "user";
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.card)
                    .px(px(12.0))
                    .py(px(8.0))
                    .max_w(px(680.0))
                    .child(
                        div()
                            .text_size(px(9.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .child(if is_user { "You" } else { "Assistant" }),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(theme.foreground)
                            .child(message.content),
                    ),
            );
        }
        list.into_any_element()
    }

    fn body(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        match self.dashboard.route() {
            super::nav::Route::Conversations => self.conversations_body(cx).into_any_element(),
            super::nav::Route::ConversationDetail => self.conversation_detail(),
            super::nav::Route::Files => self.files_body().into_any_element(),
            super::nav::Route::Prompts => self.prompts_body().into_any_element(),
            super::nav::Route::Home => {
                let theme = self.theme;
                let mut grid = div().flex().flex_wrap().gap(px(10.0));
                for (_, items) in SECTIONS {
                    for item in *items {
                        grid = grid.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(4.0))
                                .w(px(220.0))
                                .rounded(px(12.0))
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.card)
                                .p(px(12.0))
                                .child(icons::icon(&theme, item.icon, 14.0))
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(theme.foreground)
                                        .child(item.label),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(theme.muted_foreground)
                                        .child(item.description),
                                ),
                        );
                    }
                }
                grid.into_any_element()
            }
            _ => self.settings_body(cx),
        }
    }
}

fn truncate(text: &str, limit: usize) -> String {
    let mut out: String = text.chars().take(limit).collect();
    if text.chars().count() > limit {
        out.push('…');
    }
    out
}

impl Render for DashboardView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(self.sidebar(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .id("dashboard-content")
                    .overflow_y_scroll()
                    .px(px(24.0))
                    .child(self.page_header())
                    .child({
                        let mut content = div().flex().flex_col();
                        if let Some(banner) = self.error_banner() {
                            content = content.child(banner);
                        }
                        content.child(self.body(cx))
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_adds_ellipsis_only_when_needed() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("exactlyten", 10), "exactlyten");
        assert_eq!(truncate("a longer string", 5), "a lon…");
    }
}
