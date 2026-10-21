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
    /// Meeting whose transcript is open, when viewing a detail page.
    open_meeting: Option<String>,
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
            open_meeting: None,
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

    /// Store the key typed on the Dev Space page.
    pub fn save_credential(&mut self) {
        use crate::features::credentials::{self, Credential};
        let Some(vault) = self.settings.vault.as_ref() else {
            self.settings.key_status = Some("The credential vault is unavailable.".to_owned());
            return;
        };
        let provider = self.settings.key_provider.clone();
        let mut credential = Credential::chat(provider, self.settings.key_input.trim());
        credential.model = self.settings.key_model.trim().to_owned();
        match credentials::save(vault, &credential) {
            Ok(()) => {
                self.settings.key_input.clear();
                self.settings.key_status = Some("Key saved.".to_owned());
                self.reload_credentials();
            }
            Err(error) => self.settings.key_status = Some(error),
        }
    }

    /// Remove a stored credential.
    pub fn remove_credential(&mut self, kind: crate::features::credentials::Kind, provider: &str) {
        use crate::features::credentials;
        let Some(vault) = self.settings.vault.as_ref() else {
            return;
        };
        match credentials::remove(vault, kind, provider) {
            Ok(()) => {
                self.settings.key_status = Some(format!("Removed {provider}."));
                self.reload_credentials();
            }
            Err(error) => self.settings.key_status = Some(error),
        }
    }

    fn reload_credentials(&mut self) {
        use crate::features::credentials::{self, Kind};
        let Some(vault) = self.settings.vault.as_ref() else {
            return;
        };
        self.settings.stored_chat = credentials::list(vault, Kind::Chat).unwrap_or_default();
        self.settings.stored_speech = credentials::list(vault, Kind::Speech).unwrap_or_default();
    }

    /// Rename the open conversation from the detail page.
    pub fn rename_open_conversation(&mut self, title: &str) {
        let (Some(database), Some(id)) = (self.database.as_ref(), self.open_conversation.clone())
        else {
            return;
        };
        let _ = conversations::rename(database.connection(), &id, title.trim());
        self.dashboard
            .navigate(super::nav::Route::Conversations, self.database.as_ref());
    }

    /// Delete the conversation that is currently listed or open.
    pub fn delete_conversation(&mut self, id: &str) {
        let Some(database) = self.database.as_ref() else {
            return;
        };
        if conversations::delete(database.connection(), id).is_ok() {
            if self.open_conversation.as_deref() == Some(id) {
                self.open_conversation = None;
            }
            self.dashboard
                .navigate(super::nav::Route::Conversations, self.database.as_ref());
        }
    }

    /// Write the open conversation to a markdown file and report the path.
    pub fn export_conversation(&mut self) -> Option<std::path::PathBuf> {
        let id = self.open_conversation.clone()?;
        let database = self.database.as_ref()?;
        let records = conversations::messages(database.connection(), &id).ok()?;
        let title = conversations::list(database.connection(), 500)
            .ok()?
            .into_iter()
            .find(|record| record.id == id)
            .map(|record| record.title)
            .unwrap_or_else(|| "Conversation".to_owned());

        let mut markdown = format!("# {title}\n\n");
        for record in records {
            let who = if record.role == "user" {
                "You"
            } else {
                "Assistant"
            };
            markdown.push_str(&format!("**{who}**\n\n{}\n\n---\n\n", record.content));
        }

        let directory = self
            .database
            .as_ref()
            .and_then(|_| directories::ProjectDirs::from("dev", "lennette", "lennette"))
            .map(|dirs| dirs.data_dir().to_path_buf())
            .unwrap_or_else(std::env::temp_dir);
        let safe: String = title
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == ' ' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let path = directory.join(format!("{}-{}.md", safe.trim(), &id[..8.min(id.len())]));
        std::fs::write(&path, markdown).ok()?;
        self.settings.notice = Some(format!("Exported to {}", path.display()));
        Some(path)
    }

    /// Rename the open meeting.
    pub fn rename_open_meeting(&mut self, title: &str) {
        let (Some(database), Some(id)) = (self.database.as_ref(), self.open_meeting.clone()) else {
            return;
        };
        let _ = crate::features::db::meetings::rename(database.connection(), &id, title.trim());
        self.dashboard
            .navigate(super::nav::Route::Meetings, self.database.as_ref());
    }

    /// Delete a meeting and its transcript.
    pub fn delete_meeting(&mut self, id: &str) {
        let Some(database) = self.database.as_ref() else {
            return;
        };
        if crate::features::db::meetings::delete(database.connection(), id).is_ok() {
            if self.open_meeting.as_deref() == Some(id) {
                self.open_meeting = None;
            }
            self.dashboard
                .navigate(super::nav::Route::Meetings, self.database.as_ref());
        }
    }

    /// Feed a typed character into the key field.
    pub fn type_into_key_field(&mut self, text: &str) {
        self.settings.key_input.push_str(text);
    }

    /// Delete the last character of the key field.
    pub fn backspace_key_field(&mut self) {
        self.settings.key_input.pop();
    }

    /// Whether keystrokes should go to the key field.
    pub fn wants_key_input(&self) -> bool {
        self.dashboard.route() == super::nav::Route::DevSpace
    }

    /// Apply "always on top" to the real window and report when it cannot.
    pub fn apply_always_on_top(&mut self, enabled: bool, window: &mut gpui::Window) {
        use crate::features::window::{self, Applied};

        self.settings.notice = match window::set_always_on_top(window, enabled) {
            Applied::Yes => None,
            Applied::Unsupported => Some(
                "Always on top is not supported by this display server. The setting is saved \
                 and applies wherever it is supported."
                    .to_owned(),
            ),
            Applied::Failed => Some("The window manager refused the change.".to_owned()),
        };
    }

    /// Open a meeting's transcript.
    pub fn open_meeting(&mut self, id: impl Into<String>) {
        self.open_meeting = Some(id.into());
        self.dashboard
            .navigate(super::nav::Route::MeetingDetail, self.database.as_ref());
    }

    fn meeting_lines(&self) -> Vec<crate::features::db::meetings::LineRecord> {
        let (Some(database), Some(id)) = (self.database.as_ref(), self.open_meeting.as_ref())
        else {
            return Vec::new();
        };
        crate::features::db::meetings::lines(database.connection(), id).unwrap_or_default()
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

    fn meetings_body(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let PageData::Meetings(meetings) = self.dashboard.data() else {
            return self
                .empty_note("No meetings recorded yet.")
                .into_any_element();
        };
        if meetings.is_empty() {
            return self
                .empty_note("No meetings yet. Start a Listen session and it will appear here as it happens.")
                .into_any_element();
        }
        let mut list = div()
            .flex()
            .flex_col()
            .rounded(px(12.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.card);
        for meeting in meetings {
            let duration = crate::features::db::meetings::format_duration(meeting.duration_seconds);
            let live = meeting.ended_at.is_none();
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
                        format!("meeting-{}", meeting.id).into(),
                    ))
                    .on_click(cx.listener({
                        let id = meeting.id.clone();
                        move |this, _, _, cx| {
                            this.open_meeting(id.clone());
                            cx.notify();
                        }
                    }))
                    .child(icons::icon(&theme, Icon::Headphones, 13.0))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.5))
                            .text_color(theme.foreground)
                            .child(if meeting.title.is_empty() {
                                "Untitled session".to_owned()
                            } else {
                                meeting.title.clone()
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(if live {
                                theme.success
                            } else {
                                theme.muted_foreground
                            })
                            .child(if live { "Live".to_owned() } else { duration }),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.muted_foreground)
                            .child(format!("{} lines", meeting.line_count)),
                    ),
            );
        }
        list.into_any_element()
    }

    fn meeting_detail(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let delete_id = self.open_meeting.clone();
        let actions: gpui::AnyElement = match delete_id.clone() {
            Some(delete_id) => div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .pb(px(8.0))
                .child(super::view::action_button(
                    "delete-meeting",
                    &theme,
                    "Delete",
                    cx,
                    move |this: &mut Self, _, _| {
                        this.delete_meeting(&delete_id);
                    },
                ))
                .into_any_element(),
            None => div().into_any_element(),
        };
        let lines = self.meeting_lines();
        if lines.is_empty() {
            return self
                .empty_note("Pick a meeting from the list to read its transcript.")
                .into_any_element();
        }
        let mut list = div().flex().flex_col().gap(px(6.0)).child(actions);
        for line in lines {
            let who = match line.who.as_str() {
                "me" => "You".to_owned(),
                _ => match line.speaker {
                    Some(speaker) => format!("Speaker {}", speaker + 1),
                    None => "Them".to_owned(),
                },
            };
            let stamp = crate::features::db::meetings::format_duration(line.start_time as i64);
            list = list.child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(10.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.card)
                    .px(px(12.0))
                    .py(px(8.0))
                    .child(
                        div()
                            .w(px(72.0))
                            .flex_shrink_0()
                            .text_size(px(10.5))
                            .text_color(theme.muted_foreground)
                            .child(format!("{stamp} {who}")),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.5))
                            .text_color(theme.foreground)
                            .child(line.text),
                    ),
            );
        }
        list.into_any_element()
    }

    fn conversation_detail(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let id = self.open_conversation.clone();
        let actions: gpui::AnyElement = match id.clone() {
            Some(id) => {
                let export_id = id.clone();
                let delete_id = id.clone();
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .pb(px(8.0))
                    .child(super::view::action_button(
                        "export-conversation",
                        &theme,
                        "Export",
                        cx,
                        move |this: &mut Self, _, _| {
                            let _ = export_id;
                            this.export_conversation();
                        },
                    ))
                    .child(super::view::action_button(
                        "delete-conversation",
                        &theme,
                        "Delete",
                        cx,
                        move |this: &mut Self, _, _| {
                            this.delete_conversation(&delete_id);
                        },
                    ))
                    .into_any_element()
            }
            None => div().into_any_element(),
        };
        let messages = self.conversation_messages();
        if messages.is_empty() {
            return self
                .empty_note("Pick a conversation from the list to read it here.")
                .into_any_element();
        }
        let mut list = div().flex().flex_col().gap(px(8.0)).child(actions);
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
            super::nav::Route::ConversationDetail => self.conversation_detail(cx),
            super::nav::Route::Meetings => self.meetings_body(cx),
            super::nav::Route::MeetingDetail => self.meeting_detail(cx),
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

/// A small secondary button used by detail pages.
pub fn action_button<V: 'static>(
    id: impl Into<gpui::ElementId>,
    theme: &Theme,
    label: &'static str,
    cx: &mut Context<V>,
    handler: impl Fn(&mut V, &mut Context<V>, &mut gpui::Window) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(26.0))
        .px(px(10.0))
        .rounded(px(7.0))
        .border_1()
        .border_color(theme.border)
        .bg(theme.card)
        .text_size(px(11.0))
        .text_color(theme.foreground)
        .cursor_pointer()
        .hover(|style| style.bg(theme.muted))
        .id(id)
        .child(label)
        .on_click(cx.listener(move |view, _, window, cx| {
            handler(view, cx, window);
            cx.notify();
        }))
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
            // Dev Space owns the keyboard so the key field can be typed into.
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if !this.wants_key_input() {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "backspace" => this.backspace_key_field(),
                    "enter" => this.save_credential(),
                    "v" if event.keystroke.modifiers.platform => {
                        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                            let cleaned = text.trim();
                            if !cleaned.is_empty() && cleaned.len() <= 200 {
                                this.type_into_key_field(cleaned);
                            }
                        }
                    }
                    key => {
                        if key.len() == 1 && !event.keystroke.modifiers.platform {
                            this.type_into_key_field(key);
                        }
                    }
                }
                cx.notify();
            }))
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
