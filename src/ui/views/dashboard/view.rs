//! Dashboard rendering: sidebar, page header and per-route bodies.

use gpui::{
    Context, FontWeight, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, div, img, px,
};
use lucide_icons::Icon;

use super::nav::{NAV_ITEMS, NavItem};
use super::{Dashboard, PageData};
use crate::features::db::{Database, conversations};
use crate::features::icons;
use crate::features::settings_pages::{self, SettingsState};
use crate::features::theme::{Theme, radius};

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
    /// Inline rename state for the open conversation or meeting.
    renaming: bool,
    rename_buffer: String,
}

impl DashboardView {
    /// Surface colours at 50% opacity for glass aesthetics.
    fn surfaces(&self) -> crate::features::theme::Surfaces {
        self.theme.surfaces(crate::features::theme::DEFAULT_OPACITY)
    }

    pub fn new(database: Option<Database>) -> Self {
        let mut dashboard = Dashboard::new();
        if let Some(database) = database.as_ref() {
            dashboard.navigate(super::nav::Route::Conversations, Some(database));
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
            renaming: false,
            rename_buffer: String::new(),
        }
    }

    fn navigate(&mut self, route: super::nav::Route) {
        self.dashboard.navigate(route, self.database.as_ref());
    }

    /// Navigate to a page, loading its data.
    pub fn navigate_to(&mut self, route: super::nav::Route) {
        self.navigate(route);
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

    /// Begin renaming the open conversation or meeting.
    pub fn start_rename(&mut self) {
        self.renaming = match self.dashboard.route() {
            super::nav::Route::ConversationDetail => self.open_conversation.is_some(),
            super::nav::Route::MeetingDetail => self.open_meeting.is_some(),
            _ => false,
        };
        if self.renaming {
            self.rename_buffer = self.current_title().unwrap_or_default();
        }
    }

    fn current_title(&self) -> Option<String> {
        match self.dashboard.route() {
            super::nav::Route::ConversationDetail => {
                let id = self.open_conversation.clone()?;
                let database = self.database.as_ref()?;
                conversations::list(database.connection(), 500)
                    .ok()?
                    .into_iter()
                    .find(|record| record.id == id)
                    .map(|record| record.title)
            }
            super::nav::Route::MeetingDetail => {
                let id = self.open_meeting.clone()?;
                let database = self.database.as_ref()?;
                crate::features::db::meetings::list(database.connection(), 500)
                    .ok()?
                    .into_iter()
                    .find(|record| record.id == id)
                    .map(|record| record.title)
            }
            _ => None,
        }
    }

    /// Commit the rename and leave edit mode.
    pub fn finish_rename(&mut self) {
        if !self.renaming {
            return;
        }
        let title = self.rename_buffer.trim().to_owned();
        self.renaming = false;
        if title.is_empty() {
            return;
        }
        match self.dashboard.route() {
            super::nav::Route::ConversationDetail => self.rename_open_conversation(&title),
            super::nav::Route::MeetingDetail => self.rename_open_meeting(&title),
            _ => {}
        }
    }

    /// Cancel renaming.
    pub fn cancel_rename(&mut self) {
        self.renaming = false;
        self.rename_buffer.clear();
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
        (self.dashboard.route() == super::nav::Route::DevSpace && !self.renaming) || self.renaming
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

    /// Navigation across the top, as one quiet row of pills.
    ///
    /// Settings live behind the same row rather than in a separate group, so every
    /// destination is one click away and the page needs no header of its own.
    fn top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut row = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .flex_1()
            .min_w(px(0.0))
            .id("dashboard-nav");

        for item in NAV_ITEMS {
            let active = if item.route == super::nav::Route::Settings {
                self.dashboard.route().is_setting()
            } else {
                item.route == self.dashboard.route()
            };
            row = row.child(self.nav_pill(*item, active, cx));
        }

        div()
            .flex()
            .items_center()
            .gap(px(14.0))
            .h(px(52.0))
            .px(px(20.0))
            .border_b_1()
            .border_color(self.surfaces().border())
            .flex_shrink_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .mr(px(8.0))
                    .child(
                        img(icons::brand_logo())
                            .h(px(24.0))
                            .w(px(36.0))
                            .rounded(px(radius::SM)),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(self.theme.foreground)
                            .child("lennette"),
                    ),
            )
            .child(row)
    }

    /// One navigation pill.
    fn nav_pill(&self, item: NavItem, active: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let surfaces = self.surfaces();
        let mut pill = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(32.0))
            .px(px(14.0))
            .flex_shrink_0()
            .rounded(px(radius::FULL))
            .text_size(px(12.0))
            .font_weight(if active {
                FontWeight::MEDIUM
            } else {
                FontWeight::NORMAL
            })
            .cursor_pointer()
            .text_color(if active {
                theme.foreground
            } else {
                surfaces.muted_foreground()
            })
            .border_1()
            .border_color(if active {
                surfaces.border()
            } else {
                gpui::Rgba {
                    a: 0.0,
                    ..surfaces.border()
                }
            })
            .hover(|style| style.bg(surfaces.muted()).border_color(surfaces.border()))
            .id(("nav", item.route as usize))
            .child(icons::icon(&theme, item.icon, 13.0))
            .child(item.label);
        if active {
            pill = pill.bg(surfaces.muted());
        }
        pill.on_click(cx.listener(move |this, _, _, cx| {
            this.navigate(item.route);
            cx.notify();
        }))
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
        let surfaces = self.surfaces();
        div()
            .rounded(px(radius::LG))
            .border_1()
            .border_color(surfaces.border())
            .bg(surfaces.card())
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
        let surfaces = self.surfaces();
        let mut list = div()
            .flex()
            .flex_col()
            .rounded(px(radius::LG))
            .border_1()
            .border_color(surfaces.border())
            .bg(surfaces.card())
            .overflow_hidden();
        for record in records {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(14.0))
                    .py(px(10.0))
                    .border_b_1()
                    .border_color(surfaces.border())
                    .cursor_pointer()
                    .hover(|style| style.bg(surfaces.muted()))
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
            let surfaces = self.surfaces();
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .rounded(px(radius::LG))
                    .border_1()
                    .border_color(surfaces.border())
                    .bg(surfaces.card())
                    .hover(|style| style.bg(surfaces.muted()))
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
        let surfaces = self.surfaces();
        let mut list = div()
            .flex()
            .flex_col()
            .rounded(px(radius::LG))
            .border_1()
            .border_color(surfaces.border())
            .bg(surfaces.card())
            .overflow_hidden();
        for meeting in meetings {
            let duration = crate::features::db::meetings::format_duration(meeting.duration_seconds);
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(14.0))
                    .py(px(10.0))
                    .border_b_1()
                    .border_color(surfaces.border())
                    .cursor_pointer()
                    .hover(|style| style.bg(surfaces.muted()))
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
                            .text_color(theme.muted_foreground)
                            .child(duration),
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
                    "rename-meeting",
                    &theme,
                    if self.renaming { "Save name" } else { "Rename" },
                    cx,
                    |this: &mut Self, _, _| {
                        if this.renaming {
                            this.finish_rename();
                        } else {
                            this.start_rename();
                        }
                    },
                ))
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
        if self.renaming {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(theme.primary)
                    .bg(theme.card)
                    .px(px(10.0))
                    .py(px(6.0))
                    .text_size(px(12.0))
                    .text_color(theme.foreground)
                    .child(if self.rename_buffer.is_empty() {
                        "Type a name, Enter to save".to_owned()
                    } else {
                        self.rename_buffer.clone()
                    }),
            );
        }
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
                        "rename-conversation",
                        &theme,
                        if self.renaming { "Save name" } else { "Rename" },
                        cx,
                        |this: &mut Self, _, _| {
                            if this.renaming {
                                this.finish_rename();
                            } else {
                                this.start_rename();
                            }
                        },
                    ))
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
        if self.renaming {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(theme.primary)
                    .bg(theme.card)
                    .px(px(10.0))
                    .py(px(6.0))
                    .text_size(px(12.0))
                    .text_color(theme.foreground)
                    .child(if self.rename_buffer.is_empty() {
                        "Type a name, Enter to save".to_owned()
                    } else {
                        self.rename_buffer.clone()
                    }),
            );
        }
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
            super::nav::Route::Prompts => self.prompts_body().into_any_element(),
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
    let surfaces = theme.surfaces(crate::features::theme::DEFAULT_OPACITY);
    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(26.0))
        .px(px(10.0))
        .rounded(px(radius::SM))
        .border_1()
        .border_color(surfaces.border())
        .bg(surfaces.card())
        .text_size(px(11.0))
        .text_color(theme.foreground)
        .cursor_pointer()
        .hover(|style| style.bg(surfaces.muted()).border_color(theme.primary))
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
        let surfaces = self.surfaces();
        div()
            .flex()
            .size_full()
            .rounded(px(radius::XL))
            .border_1()
            .border_color(surfaces.border())
            .bg(surfaces.background())
            .text_color(theme.foreground)
            .overflow_hidden()
            // Dev Space owns the keyboard so the key field can be typed into.
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if !this.wants_key_input() {
                    return;
                }
                if this.renaming {
                    match event.keystroke.key.as_str() {
                        "backspace" => {
                            this.rename_buffer.pop();
                        }
                        "enter" => this.finish_rename(),
                        "escape" => this.cancel_rename(),
                        key if key.len() == 1 && !event.keystroke.modifiers.platform => {
                            this.rename_buffer.push_str(key)
                        }
                        _ => {}
                    }
                    cx.notify();
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
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(self.top_bar(cx))
                    .child({
                        let mut content = div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h(px(0.0))
                            .id("dashboard-content")
                            .overflow_y_scroll()
                            .px(px(20.0))
                            .pb(px(20.0));
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
