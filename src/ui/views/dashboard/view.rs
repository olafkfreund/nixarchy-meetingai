//! Dashboard rendering: sidebar, page header and per-route bodies.

use gpui::{
    AppContext, Context, FocusHandle, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Render, StatefulInteractiveElement, Styled, StyledImage, Window, div, img, px,
};
use lucide_icons::Icon;

use super::nav::{NAV_ITEMS, NavItem};
use super::{Dashboard, PageData};
use crate::features::db::{Database, conversations};
use crate::features::icons;
use crate::features::settings_pages::{self, SettingsState};
use crate::features::theme::{Theme, radius};
use crate::ui::theme::scaled;

#[derive(Clone, PartialEq)]
enum EditTarget {
    Setting(&'static str),
    Prompt(i64, String),
}

#[path = "profiles.rs"]
mod profiles;

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
    edit_target: Option<EditTarget>,
    profile_edit: Option<profiles::ProfileDraft>,
    editor: Option<gpui::Entity<crate::ui::widgets::editor::Editor>>,
    rename_buffer: String,
    credential_focus: Option<FocusHandle>,
    settings_save: Option<gpui::Task<()>>,
    model_task: Option<gpui::Task<()>>,
    /// Background task naming meetings that still carry a timestamp title.
    title_task: Option<gpui::Task<()>>,
    /// Chat provider used to name old sessions, when one is configured.
    title_config: Option<crate::features::providers::ProviderConfig>,
}

impl DashboardView {
    /// Surface colours at current transparency opacity.
    fn surfaces(&self) -> crate::features::theme::Surfaces {
        let opacity = (self.settings.settings.transparency as f32 / 100.0).clamp(0.0, 1.0);
        self.theme.surfaces(opacity)
    }

    pub fn new(database: Option<Database>) -> Self {
        Self::with_titles(database, None)
    }

    /// Build the view, optionally with a chat provider for naming old sessions.
    pub fn with_titles(
        database: Option<Database>,
        title_config: Option<crate::features::providers::ProviderConfig>,
    ) -> Self {
        let mut dashboard = Dashboard::new();
        if let Some(database) = database.as_ref() {
            dashboard.navigate(super::nav::Route::Conversations, Some(database));
        }
        let settings = SettingsState::new(database.as_ref().map(|database| {
            // A second connection keeps settings writes off the list queries.
            Database::open(database.connection().path().unwrap_or_default())
                .unwrap_or_else(|_| Database::open_in_memory().expect("in-memory database"))
        }));
        let theme = Theme::from_settings(&settings.settings);
        Self {
            dashboard,
            database,
            theme,
            settings,
            open_conversation: None,
            open_meeting: None,
            renaming: false,
            edit_target: None,
            profile_edit: None,
            editor: None,
            rename_buffer: String::new(),
            credential_focus: None,
            settings_save: None,
            model_task: None,
            title_task: None,
            title_config,
        }
    }

    fn navigate(&mut self, route: super::nav::Route) {
        if self.settings_save.take().is_some() {
            self.save_settings();
        }
        if let Some(database) = self.settings.database.as_ref()
            && let Ok(settings) = crate::core::settings::Settings::load(database)
        {
            self.settings.baseline = settings.clone();
            self.settings.settings = settings;
        }
        self.renaming = false;
        self.edit_target = None;
        self.profile_edit = None;
        self.editor = None;
        self.rename_buffer.clear();
        self.settings.key_field = None;
        self.reset_credential_models();
        self.settings.notice = None;
        self.dashboard.navigate(route, self.database.as_ref());
    }

    /// Navigate to a page, loading its data.
    pub fn navigate_to(&mut self, route: super::nav::Route) {
        self.navigate(route);
    }

    /// Navigate, then start any follow-up work the page needs.
    fn navigate_with(&mut self, route: super::nav::Route, cx: &mut Context<Self>) {
        self.navigate(route);
        if route == super::nav::Route::Meetings {
            self.name_untitled_meetings(cx);
        }
    }

    /// Give older sessions real titles, using the configured chat model.
    ///
    /// Only meetings still titled `Session · HH:MM` are touched, and only when a
    /// provider is available; otherwise the timestamp title simply stays.
    fn name_untitled_meetings(&mut self, cx: &mut Context<Self>) {
        if self.title_task.is_some() {
            return;
        }
        let Some(path) = self
            .database
            .as_ref()
            .and_then(|database| database.connection().path())
            .map(str::to_owned)
        else {
            return;
        };
        let Some(config) = self.title_config.clone() else {
            return;
        };
        self.title_task = Some(cx.spawn(async move |this, cx| {
            let Some(database) = crate::db::Database::open(&path)
                .ok()
                .map(std::sync::Arc::new)
            else {
                return;
            };
            let renamed = crate::ai::titling::name_untitled_meetings(&database, config).await;
            if renamed > 0 {
                this.update(cx, |view, cx| {
                    view.title_task = None;
                    view.dashboard
                        .navigate(super::nav::Route::Meetings, view.database.as_ref());
                    cx.notify();
                })
                .ok();
            } else {
                let _ = cx.refresh();
            }
        }));
    }

    /// Mutable settings state, for page controls.
    pub fn settings_mut(&mut self) -> &mut SettingsState {
        &mut self.settings
    }

    /// Persist settings after a control changes.
    pub fn save_settings(&mut self) {
        self.settings.save();
        self.theme = Theme::from_settings(&self.settings.settings);
    }

    pub fn schedule_settings_save(&mut self, cx: &mut Context<Self>) {
        self.settings_save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(100))
                .await;
            this.update(cx, |this, _| {
                this.save_settings();
                this.settings_save = None;
            })
            .ok();
        }));
    }

    pub fn edit_setting(&mut self, key: &'static str, text: &str, cx: &mut Context<Self>) {
        self.edit_target = Some(EditTarget::Setting(key));
        self.editor =
            Some(cx.new(|cx| crate::ui::widgets::editor::Editor::new(text, self.theme, cx)));
    }

    fn save_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_ref() else {
            return;
        };
        let text = editor.read(cx).input.text().trim().to_owned();
        let Some(target) = self.edit_target.as_ref() else {
            return;
        };
        let Some(database) = self.database.as_ref() else {
            return;
        };
        let result = match target {
            EditTarget::Setting(key) => crate::db::settings::set(database.connection(), key, &text),
            EditTarget::Prompt(id, name) => {
                if text.is_empty() {
                    self.settings.error = Some("A response prompt cannot be empty.".into());
                    return;
                }
                crate::db::settings::update_prompt(database.connection(), *id, name, &text)
            }
        };
        if let Err(error) = result {
            self.settings.error = Some(error.to_string());
            return;
        }
        self.settings.settings =
            crate::core::settings::Settings::load(database).unwrap_or_default();
        self.settings.baseline = self.settings.settings.clone();
        self.dashboard
            .navigate(self.dashboard.route(), Some(database));
        self.editor = None;
        self.edit_target = None;
        self.settings.error = None;
    }

    pub fn editable_setting(
        &self,
        key: &'static str,
        label: &'static str,
        text: &str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        self.editable_text(EditTarget::Setting(key), label, text, cx)
    }

    fn editable_text(
        &self,
        target: EditTarget,
        label: &'static str,
        text: &str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let active = self.edit_target.as_ref() == Some(&target);
        let mut block = div()
            .flex()
            .flex_col()
            .items_start()
            .flex_shrink_0()
            .w_full()
            .gap(px(8.0))
            .py(px(8.0))
            .child(
                div()
                    .text_size(scaled(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(label),
            );
        if active {
            block = block.children(self.editor.clone()).child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(action_button(
                        "save-text-editor",
                        &self.theme,
                        "Save",
                        cx,
                        |this, cx, _| this.save_editor(cx),
                    ))
                    .child(action_button(
                        "cancel-text-editor",
                        &self.theme,
                        "Cancel",
                        cx,
                        |this, _, _| {
                            this.edit_target = None;
                            this.editor = None;
                        },
                    )),
            );
        } else {
            let initial = text.to_owned();
            let id = match &target {
                EditTarget::Setting(key) => format!("edit-{key}"),
                EditTarget::Prompt(id, _) => format!("edit-prompt-{id}"),
            };
            block = block
                .child(
                    div()
                        .text_size(scaled(12.0))
                        .text_color(self.theme.muted_foreground)
                        .whitespace_normal()
                        .child(if text.is_empty() {
                            "Nothing added yet.".to_owned()
                        } else {
                            text.chars().take(350).collect()
                        }),
                )
                .child(action_button(
                    gpui::ElementId::Name(id.into()),
                    &self.theme,
                    "Edit",
                    cx,
                    move |this, cx, _| {
                        this.edit_target = Some(target.clone());
                        this.editor = Some(cx.new(|cx| {
                            crate::ui::widgets::editor::Editor::new(&initial, this.theme, cx)
                        }));
                    },
                ));
        }
        block.into_any_element()
    }

    pub fn reset_credential_models(&mut self) {
        self.model_task = None;
        self.settings.model_menu_open = false;
        self.settings.model_loading = false;
        self.settings.model_options.clear();
        self.settings.model_status = None;
    }

    pub fn toggle_credential_models(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.settings.model_loading {
            return;
        }
        self.settings.model_menu_open = force || !self.settings.model_menu_open;
        if !self.settings.model_menu_open {
            return;
        }
        let config = match self.settings.model_discovery_config() {
            Ok(config) => config,
            Err(error) => {
                self.settings.model_options = vec![self.settings.key_model.clone()];
                self.settings.model_status = Some(error);
                return;
            }
        };
        self.settings.model_options = vec![self.settings.key_model.clone()];
        if let Some(cache) = crate::ai::models::cached(
            self.database
                .as_ref()
                .and_then(|db| db.connection().path())
                .map(std::path::Path::new),
            &config,
        ) {
            self.settings.model_options.extend(cache.models);
        }
        self.settings.model_options.sort();
        self.settings.model_options.dedup();
        let path = self
            .database
            .as_ref()
            .and_then(|db| db.connection().path())
            .map(std::path::PathBuf::from);
        self.settings.model_loading = true;
        self.settings.model_status = None;
        self.model_task = Some(cx.spawn(async move |this, cx| {
            let result = crate::ai::models::discover(path, config, force).await;
            this.update(cx, |this, cx| {
                this.settings.model_loading = false;
                match result {
                    Ok(mut models) => {
                        models.push(this.settings.key_model.clone());
                        models.sort();
                        models.dedup();
                        this.settings.model_options = models;
                    }
                    Err(error) => this.settings.model_status = Some(error),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// Store the key typed on the Dev Space page.
    pub fn save_credential(&mut self) {
        use crate::features::credentials::{self, Credential};
        let Some(vault) = self.settings.vault.as_ref() else {
            self.settings.key_status = Some("The credential vault is unavailable.".to_owned());
            return;
        };
        let provider = self.settings.key_provider.clone();
        let kind = if crate::ai::providers::SttProvider::by_id(&provider).is_some() {
            credentials::Kind::Speech
        } else {
            credentials::Kind::Chat
        };
        let mut credential = credentials::load(vault, kind, &provider)
            .ok()
            .flatten()
            .unwrap_or_else(|| {
                if kind == credentials::Kind::Speech {
                    Credential::speech(&provider, "")
                } else {
                    Credential::chat(&provider, "")
                }
            });
        if !self.settings.key_input.trim().is_empty() {
            credential.api_key = self.settings.key_input.trim().to_owned();
        }
        credential.model = self.settings.key_model.trim().to_owned();
        match credentials::save(vault, &credential) {
            Ok(()) => {
                let reused_key = self.settings.key_input.trim().is_empty();
                self.settings.key_input.clear();
                self.settings.key_status = Some(
                    if reused_key {
                        "Provider settings saved using your saved key."
                    } else {
                        "Key saved in the encrypted vault."
                    }
                    .to_owned(),
                );
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

    pub fn focus_credential_field(
        &mut self,
        field: settings_pages::CredentialField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings.key_field = Some(field);
        self.settings.replace_key_field = false;
        let focus = self
            .credential_focus
            .get_or_insert_with(|| cx.focus_handle());
        window.focus(focus);
        cx.notify();
    }

    fn credential_field_mut(&mut self) -> Option<&mut String> {
        match self.settings.key_field? {
            settings_pages::CredentialField::Key => Some(&mut self.settings.key_input),
            settings_pages::CredentialField::Model => Some(&mut self.settings.key_model),
        }
    }

    pub fn type_into_key_field(&mut self, text: &str) {
        if self.settings.key_field == Some(settings_pages::CredentialField::Key) {
            self.reset_credential_models();
        }
        let replace = std::mem::take(&mut self.settings.replace_key_field);
        if let Some(field) = self.credential_field_mut() {
            if replace {
                field.clear();
            }
            field.push_str(text);
        }
    }

    pub fn backspace_key_field(&mut self) {
        if self.settings.key_field == Some(settings_pages::CredentialField::Key) {
            self.reset_credential_models();
        }
        let replace = std::mem::take(&mut self.settings.replace_key_field);
        if let Some(field) = self.credential_field_mut() {
            if replace {
                field.clear();
            } else {
                field.pop();
            }
        }
    }

    pub fn wants_key_input(&self) -> bool {
        (self.dashboard.route() == super::nav::Route::DevSpace && self.settings.key_field.is_some())
            || self.renaming
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
        self.renaming = false;
        self.settings.notice = None;
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
    fn top_bar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                    || matches!(
                        (item.route, self.dashboard.route()),
                        (
                            super::nav::Route::Meetings,
                            super::nav::Route::MeetingDetail
                        ) | (
                            super::nav::Route::Conversations,
                            super::nav::Route::ConversationDetail
                        )
                    )
            };
            row = row.child(self.nav_pill(
                *item,
                active,
                window.viewport_size().width < px(860.0),
                cx,
            ));
        }

        div()
            .flex()
            .items_center()
            .gap(px(14.0))
            .h(px(52.0))
            .px(px(20.0))
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
                            .text_size(scaled(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(self.theme.foreground)
                            .child("lennette"),
                    ),
            )
            .child(row)
            .child(
                div()
                    .id("close-dashboard")
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(30.0))
                    .flex_shrink_0()
                    .rounded_full()
                    .cursor_pointer()
                    .hover(|s| s.bg(self.surfaces().muted()))
                    .child(icons::icon(&self.theme, Icon::X, 16.0))
                    .on_click(|_, window, _| window.remove_window()),
            )
    }

    /// One navigation pill.
    fn nav_pill(
        &self,
        item: NavItem,
        active: bool,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
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
            .text_size(scaled(12.0))
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
            .children((!compact).then_some(item.label))
            .tooltip(move |_, cx| {
                cx.new(|_| NavTooltip {
                    text: item.label.into(),
                    theme,
                })
                .into()
            });
        if active {
            pill = pill.bg(surfaces.muted());
        }
        pill.on_click(cx.listener(move |this, _, _, cx| {
            this.navigate_with(item.route, cx);
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
                .text_size(scaled(12.0))
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
                    .text_size(scaled(12.5))
                    .text_color(theme.muted_foreground)
                    .child(message),
            )
    }

    fn conversations_body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let surfaces = self.surfaces();
        let records = self.dashboard.filtered_conversations();
        let count = records.len();
        let mut list = div().flex().flex_col().gap(px(10.0)).pb(px(12.0)).child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .pb(px(8.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(5.0))
                        .child(
                            div()
                                .text_size(scaled(20.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Conversations"),
                        )
                        .child(
                            div()
                                .text_size(scaled(12.0))
                                .text_color(theme.muted_foreground)
                                .child(format!(
                                    "{count} saved {}",
                                    if count == 1 {
                                        "conversation"
                                    } else {
                                        "conversations"
                                    }
                                )),
                        ),
                )
                .child(action_button(
                    "refresh-conversations",
                    &theme,
                    "Refresh",
                    cx,
                    |this: &mut Self, _, _| this.navigate(super::nav::Route::Conversations),
                )),
        );
        if records.is_empty() {
            return list.child(
                div()
                    .pt(px(40.0))
                    .pb(px(40.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.0))
                    .child(icons::icon(&theme, Icon::MessagesSquare, 28.0))
                    .child(
                        div()
                            .text_size(scaled(14.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child("Your conversations will appear here"),
                    )
                    .child(
                        div()
                            .text_size(scaled(12.0))
                            .text_color(theme.muted_foreground)
                            .child("Ask a question in the overlay to start one."),
                    ),
            );
        }
        let now = crate::ai::session::now_millis();
        for record in records {
            let preview = record
                .preview
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with("```"))
                .take(2)
                .collect::<Vec<_>>()
                .join(" ")
                .replace("**", "")
                .replace('`', "");
            list = list.child(
                div()
                    .id(gpui::ElementId::Name(
                        format!("conversation-{}", record.id).into(),
                    ))
                    .flex()
                    .items_center()
                    .gap(px(14.0))
                    .p(px(16.0))
                    .rounded(px(14.0))
                    .border_1()
                    .border_color(surfaces.border())
                    .bg(surfaces.card())
                    .cursor_pointer()
                    .hover(|s| s.bg(surfaces.muted()).border_color(theme.muted_foreground))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(36.0))
                            .flex_shrink_0()
                            .rounded(px(11.0))
                            .bg(surfaces.accent())
                            .child(icons::icon(&theme, Icon::MessagesSquare, 16.0)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .gap(px(6.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(12.0))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .truncate()
                                            .text_size(scaled(13.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(record.title.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .text_size(scaled(10.5))
                                            .text_color(theme.muted_foreground)
                                            .child(relative_time(record.updated_at, now)),
                                    ),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .truncate()
                                    .text_size(scaled(11.5))
                                    .text_color(theme.muted_foreground)
                                    .child(if preview.is_empty() {
                                        "No messages yet".into()
                                    } else {
                                        preview
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(scaled(10.0))
                                    .text_color(theme.muted_foreground)
                                    .child(format!(
                                        "{} {}",
                                        record.message_count,
                                        if record.message_count == 1 {
                                            "message"
                                        } else {
                                            "messages"
                                        }
                                    )),
                            ),
                    )
                    .child(icons::icon(&theme, Icon::ChevronRight, 15.0))
                    .on_click(cx.listener({
                        let id = record.id.clone();
                        move |this, _, _, cx| {
                            this.open_conversation(id.clone());
                            cx.notify();
                        }
                    })),
            );
        }
        list
    }

    fn prompts_body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let PageData::Prompts(prompts) = self.dashboard.data() else {
            return div().child(self.empty_note("No prompts loaded."));
        };
        let mut list = div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.page_heading(
                "Prompts",
                "Instructions used by each meeting mode. Choose a mode in the overlay.",
            ));
        for prompt in prompts {
            let mode = prompt
                .listen_slot
                .map(|_| "Meeting mode".to_owned())
                .unwrap_or_else(|| "Prompt".to_owned());
            let surfaces = self.surfaces();
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
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
                                    .text_size(scaled(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.foreground)
                                    .child(prompt.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(scaled(10.0))
                                    .text_color(theme.muted_foreground)
                                    .child(mode),
                            ),
                    )
                    .child(self.editable_text(
                        EditTarget::Prompt(prompt.id, prompt.name.clone()),
                        "Response instruction",
                        &prompt.prompt,
                        cx,
                    )),
            );
        }
        list
    }

    fn settings_body(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        settings_pages::render(
            self,
            self.dashboard.route(),
            &self.settings,
            &self.theme,
            cx,
        )
    }

    fn page_heading(&self, title: &str, subtitle: &str) -> gpui::AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .pb(px(8.0))
            .child(
                div()
                    .text_size(scaled(20.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title.to_owned()),
            )
            .child(
                div()
                    .text_size(scaled(12.0))
                    .text_color(self.theme.muted_foreground)
                    .child(subtitle.to_owned()),
            )
            .into_any_element()
    }

    fn meetings_body(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let surfaces = self.surfaces();
        let mut list = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(self.page_heading(
                "Meetings",
                "Saved sessions, speaker transcripts and context for your next conversation.",
            ));
        let PageData::Meetings(meetings) = self.dashboard.data() else {
            return list
                .child(self.empty_note("No meetings recorded yet."))
                .into_any_element();
        };
        if meetings.is_empty() {
            return list
                .child(
                    self.empty_note("Start listening in the overlay to save your first meeting."),
                )
                .into_any_element();
        }
        for meeting in meetings {
            let id = meeting.id.clone();
            list = list.child(
                div()
                    .id(gpui::ElementId::Name(format!("meeting-{id}").into()))
                    .flex()
                    .items_center()
                    .gap(px(14.0))
                    .p(px(16.0))
                    .rounded(px(14.0))
                    .border_1()
                    .border_color(surfaces.border())
                    .bg(surfaces.card())
                    .cursor_pointer()
                    .hover(|s| s.bg(surfaces.muted()))
                    .child(icons::icon(&theme, Icon::Headphones, 20.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .gap(px(7.0))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(scaled(14.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(if meeting.title.is_empty() {
                                        "Untitled session".to_owned()
                                    } else {
                                        meeting.title.clone()
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(scaled(11.5))
                                    .text_color(theme.muted_foreground)
                                    .child(format!(
                                        "{} · {} · {} lines{}",
                                        meeting.started_at,
                                        crate::features::db::meetings::format_duration(
                                            meeting.duration_seconds
                                        ),
                                        meeting.line_count,
                                        if meeting.ended_at.is_none() {
                                            " · In progress"
                                        } else {
                                            ""
                                        }
                                    )),
                            ),
                    )
                    .child(icons::icon(&theme, Icon::ChevronRight, 15.0))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_meeting(id.clone());
                        cx.notify();
                    })),
            );
        }
        list.into_any_element()
    }

    fn about_body(&self, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let asset =
            |name: &'static str| gpui::ImageSource::Resource(gpui::Resource::Embedded(name.into()));
        let theme = self.theme;
        let surfaces = self.surfaces();
        let version = env!("CARGO_PKG_VERSION");

        let viewport = window.viewport_size();
        // Art heights follow the window rather than a fixed pixel size, so the
        // page stays a band instead of filling the screen when it grows.
        let header_height = (viewport.height * 0.28).clamp(px(140.0), px(230.0));
        let footer_height = (viewport.height * 0.15).clamp(px(76.0), px(140.0));
        // Side-by-side icon and copy stop working once the column gets tight.
        let available = (viewport.width - px(48.0)).max(px(180.0));
        let narrow = available < px(580.0);
        // One width for every block. The dashboard content is a centred flex
        // column, so a fixed width centres here; auto margins do not exist in
        // this GPUI version, and `w_full` alone would just follow the edge.
        let column = available.min(px(620.0));
        let header_width = (header_height * 2.0).min(column);

        let heading = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .gap(px(4.0))
            .child(
                div()
                    .text_size(scaled(21.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.foreground)
                    .whitespace_normal()
                    .child("About Lennette"),
            )
            .child(
                div()
                    .text_size(scaled(12.5))
                    .line_height(scaled(17.0))
                    .text_color(theme.muted_foreground)
                    .whitespace_normal()
                    .child("An ambient meeting and discussion copilot."),
            )
            .child(
                div()
                    .text_size(scaled(11.0))
                    .text_color(theme.muted_foreground)
                    .child(format!("Version {version}")),
            );

        let identity = div()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(16.0))
            .w_full()
            .children((!narrow).then(|| {
                img(asset("lennette_app_icon.png"))
                    .w(px(72.0))
                    .h(px(72.0))
                    .flex_shrink_0()
                    .object_fit(gpui::ObjectFit::Contain)
            }))
            .child(heading);

        let description = div()
            .flex()
            .items_center()
            .gap(px(16.0))
            .p(px(16.0))
            .w_full()
            .rounded(px(16.0))
            .bg(surfaces.card())
            .children((!narrow).then(|| {
                img(asset("lennette.png"))
                    .w(px(84.0))
                    .h(px(70.0))
                    .flex_shrink_0()
                    .object_fit(gpui::ObjectFit::Contain)
            }))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(scaled(13.0))
                    .line_height(scaled(20.0))
                    .whitespace_normal()
                    .text_color(theme.foreground)
                    .child(
                        "Listen to your microphone and call audio, follow speaker transcripts, and \
                         get streamed suggestions for what to say next. Add screenshots, files or \
                         past transcripts to give each conversation more context. Choose your own \
                         AI and speech providers in Settings.",
                    ),
            );

        let content = div()
            .flex()
            .flex_col()
            .w(column)
            .flex_shrink_0()
            .min_w(px(0.0))
            .gap(px(18.0))
            .pb(px(4.0))
            // Every child shares the column width, so all edges line up.
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .w_full()
                    .h(header_height)
                    .flex_shrink_0()
                    .rounded(px(16.0))
                    .overflow_hidden()
                    .child(
                        img(asset("lennette_header.png"))
                            .w(header_width)
                            .h(header_width / 2.0)
                            .flex_shrink_0()
                            .object_fit(gpui::ObjectFit::Contain),
                    ),
            )
            .child(div().w_full().flex_shrink_0().child(identity))
            .child(div().w_full().flex_shrink_0().child(description))
            .child(
                div()
                    .flex()
                    .w_full()
                    .items_center()
                    .justify_center()
                    .child(action_button(
                        "about-github",
                        &self.theme,
                        "View Lennette on GitHub ↗",
                        cx,
                        |_, cx, _| cx.open_url("https://github.com/parazeeknova/lennette"),
                    )),
            )
            .child(
                div()
                    .w_full()
                    .text_center()
                    .text_size(scaled(12.0))
                    .line_height(scaled(18.0))
                    .whitespace_normal()
                    .text_color(theme.muted_foreground)
                    .child(
                        "Built for Linux with Rust and GPUI. Your provider keys are stored in the \
                         encrypted local vault. Selected context is sent to your configured AI \
                         provider when you ask a question.",
                    ),
            )
            .child(
                img(asset("lennette_footer.png"))
                    .w_full()
                    .h(footer_height)
                    .flex_shrink_0()
                    .object_fit(gpui::ObjectFit::Contain),
            );
        div()
            .flex()
            .flex_col()
            .items_center()
            .w_full()
            .flex_shrink_0()
            .child(content)
            .into_any_element()
    }

    fn meeting_detail(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let delete_id = self.open_meeting.clone();
        let actions: gpui::AnyElement = match delete_id.clone() {
            Some(delete_id) => div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .pb(px(8.0))
                .child(action_button(
                    "back-meetings",
                    &theme,
                    "← Meetings",
                    cx,
                    |this, _, _| this.navigate(super::nav::Route::Meetings),
                ))
                .child(super::view::action_button(
                    "rename-meeting",
                    &theme,
                    if self.renaming { "Save name" } else { "Rename" },
                    cx,
                    |this: &mut Self, cx, window| {
                        if this.renaming {
                            this.finish_rename();
                        } else {
                            this.start_rename();
                            let focus = this
                                .credential_focus
                                .get_or_insert_with(|| cx.focus_handle());
                            window.focus(focus);
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
        let mut list =
            div()
                .flex()
                .flex_col()
                .gap(px(16.0))
                .child(actions)
                .child(self.page_heading(
                    &self.current_title().unwrap_or_else(|| "Meeting".into()),
                    &format!(
                        "{} transcript {}",
                        lines.len(),
                        if lines.len() == 1 { "line" } else { "lines" }
                    ),
                ));
        if lines.is_empty() {
            return list
                .child(self.empty_note("This meeting has no transcript yet."))
                .into_any_element();
        }
        if self.renaming {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(theme.focus_border())
                    .bg(theme.card)
                    .px(px(10.0))
                    .py(px(6.0))
                    .text_size(scaled(12.0))
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
                    .py(px(14.0))
                    .child(
                        div()
                            .w(px(105.0))
                            .flex_shrink_0()
                            .text_size(scaled(10.5))
                            .text_color(theme.muted_foreground)
                            .child(format!("{stamp} {who}")),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(scaled(12.5))
                            .text_color(theme.foreground)
                            .child(line.text),
                    ),
            );
        }
        list.into_any_element()
    }

    fn conversation_detail(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let surfaces = self.surfaces();
        let Some(id) = self.open_conversation.clone() else {
            return self
                .empty_note("Choose a conversation to read it.")
                .into_any_element();
        };
        let messages = self.conversation_messages();
        let title = self
            .current_title()
            .unwrap_or_else(|| "Conversation".into());
        let mut list = div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(18.0))
            .pb(px(24.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .child(
                        div()
                            .id("back-to-conversations")
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .h(px(30.0))
                            .cursor_pointer()
                            .text_size(scaled(11.5))
                            .text_color(theme.muted_foreground)
                            .hover(|s| s.text_color(theme.foreground))
                            .child(icons::icon(&theme, Icon::ArrowLeft, 13.0))
                            .child("Conversations")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.navigate(super::nav::Route::Conversations);
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(action_button(
                                "rename-conversation",
                                &theme,
                                if self.renaming { "Save name" } else { "Rename" },
                                cx,
                                |this: &mut Self, cx, window| {
                                    if this.renaming {
                                        this.finish_rename();
                                    } else {
                                        this.start_rename();
                                        let focus = this
                                            .credential_focus
                                            .get_or_insert_with(|| cx.focus_handle());
                                        window.focus(focus);
                                    }
                                },
                            ))
                            .child(action_button(
                                "export-conversation",
                                &theme,
                                "Export",
                                cx,
                                |this: &mut Self, _, _| {
                                    this.export_conversation();
                                },
                            ))
                            .child(action_button(
                                "delete-conversation",
                                &theme,
                                "Delete",
                                cx,
                                move |this: &mut Self, _, _| this.delete_conversation(&id),
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .pb(px(8.0))
                    .child(
                        div()
                            .text_size(scaled(20.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(scaled(11.0))
                            .text_color(theme.muted_foreground)
                            .child(format!("{} messages", messages.len())),
                    ),
            );
        if let Some(notice) = &self.settings.notice {
            list = list.child(
                div()
                    .text_size(scaled(11.0))
                    .text_color(theme.muted_foreground)
                    .child(notice.clone()),
            );
        }
        if self.renaming {
            list = list.child(
                div()
                    .rounded(px(9.0))
                    .border_1()
                    .border_color(theme.focus_border())
                    .bg(surfaces.input())
                    .p(px(10.0))
                    .text_size(scaled(13.0))
                    .child(if self.rename_buffer.is_empty() {
                        "Type a name, Enter to save".into()
                    } else {
                        self.rename_buffer.clone()
                    }),
            );
        }
        if messages.is_empty() {
            return list
                .child(self.empty_note("This conversation has no messages yet."))
                .into_any_element();
        }
        for message in messages {
            let is_user = message.role == "user";
            let content = if is_user {
                div()
                    .text_size(scaled(13.0))
                    .child(message.content)
                    .into_any_element()
            } else {
                crate::ui::overlay::render_markdown(&theme, surfaces, &message.content)
            };
            list = list.child(
                div()
                    .id(gpui::ElementId::Name(
                        format!("saved-message-{}", message.id).into(),
                    ))
                    .flex()
                    .w_full()
                    .justify_center()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .w_full()
                            .max_w(px(760.0))
                            .min_w(px(0.0))
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .text_size(scaled(10.5))
                                    .text_color(theme.muted_foreground)
                                    .child(icons::icon(
                                        &theme,
                                        if is_user {
                                            Icon::UserRound
                                        } else {
                                            Icon::Sparkles
                                        },
                                        12.0,
                                    ))
                                    .child(if is_user { "You" } else { "Lennette" }),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .min_w(px(0.0))
                                    .rounded(px(12.0))
                                    .p(px(14.0))
                                    .bg(if is_user {
                                        surfaces.accent()
                                    } else {
                                        surfaces.card()
                                    })
                                    .child(content),
                            ),
                    ),
            );
        }
        list.into_any_element()
    }

    fn body(&self, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        match self.dashboard.route() {
            super::nav::Route::Conversations => self.conversations_body(cx).into_any_element(),
            super::nav::Route::ConversationDetail => self.conversation_detail(cx),
            super::nav::Route::Meetings => self.meetings_body(cx),
            super::nav::Route::MeetingDetail => self.meeting_detail(cx),
            super::nav::Route::Prompts => self.prompts_body(cx).into_any_element(),
            super::nav::Route::About => self.about_body(window, cx),
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
        .bg(surfaces.muted())
        .text_size(scaled(11.0))
        .text_color(theme.foreground)
        .cursor_pointer()
        .hover(|style| {
            style
                .bg(surfaces.muted())
                .border_color(theme.control_border(true))
        })
        .id(id)
        .child(label)
        .on_click(cx.listener(move |view, _, window, cx| {
            handler(view, cx, window);
            cx.notify();
        }))
}

fn relative_time(timestamp: i64, now: i64) -> String {
    let seconds = now.saturating_sub(timestamp).max(0) / 1000;
    match seconds {
        0..60 => "Just now".into(),
        60..3600 => format!("{}m ago", seconds / 60),
        3600..86400 => format!("{}h ago", seconds / 3600),
        _ => format!("{}d ago", seconds / 86400),
    }
}

#[cfg(test)]
fn truncate(text: &str, limit: usize) -> String {
    let mut out: String = text.chars().take(limit).collect();
    if text.chars().count() > limit {
        out.push('…');
    }
    out
}

impl Render for DashboardView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Text sizes are written in rems, so this one value scales the window.
        window.set_rem_size(crate::ui::theme::rem_size_for(
            self.settings.settings.font_scale,
        ));
        self.theme = Theme::from_settings(&self.settings.settings);
        let theme = self.theme;
        let surfaces = self.surfaces();
        let focus = self
            .credential_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        div()
            .track_focus(&focus)
            .font_family(crate::ui::theme::ui_font_family())
            .flex()
            .size_full()
            .rounded(px(radius::XL))
            .border_1()
            .border_color(theme.frame_border)
            .bg(
                if self.settings.settings.theme == crate::core::models::settings::ThemeChoice::Glass
                {
                    gpui::Rgba {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 0.0,
                    }
                } else {
                    surfaces.background()
                },
            )
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
                    "escape" => this.settings.key_field = None,
                    "a" if event.keystroke.modifiers.platform
                        || event.keystroke.modifiers.control =>
                    {
                        this.settings.replace_key_field = true;
                    }
                    "v" if event.keystroke.modifiers.platform
                        || event.keystroke.modifiers.control =>
                    {
                        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                            let cleaned = text.trim();
                            if !cleaned.is_empty() && cleaned.len() <= 4096 {
                                this.type_into_key_field(cleaned);
                            }
                        }
                    }
                    key => {
                        if !event.keystroke.modifiers.platform && !event.keystroke.modifiers.control
                        {
                            if let Some(text) = event.keystroke.key_char.as_deref() {
                                this.type_into_key_field(text);
                            } else if key.len() == 1 {
                                this.type_into_key_field(key);
                            }
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
                    .child(self.top_bar(window, cx))
                    .child({
                        let mut content = div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h(px(0.0))
                            .id("dashboard-content")
                            .overflow_y_scroll()
                            .p(px(24.0))
                            .items_center();
                        if let Some(banner) = self.error_banner() {
                            content = content.child(banner);
                        }
                        content.child(
                            div()
                                .w_full()
                                .max_w(px(960.0))
                                .flex_shrink_0()
                                .min_w(px(0.0))
                                .child(self.body(window, cx)),
                        )
                    }),
            )
    }
}

impl Drop for DashboardView {
    fn drop(&mut self) {
        if self.settings_save.is_some() {
            self.settings.save();
        }
    }
}

struct NavTooltip {
    text: gpui::SharedString,
    theme: Theme,
}
impl Render for NavTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.0))
            .py(px(5.0))
            .rounded(px(6.0))
            .bg(self.theme.card)
            .text_color(self.theme.foreground)
            .text_size(scaled(11.0))
            .child(self.text.clone())
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
