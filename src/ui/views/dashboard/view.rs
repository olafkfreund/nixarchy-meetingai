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
}

#[path = "profiles.rs"]
mod profiles;
#[path = "prompts.rs"]
mod prompts;

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
    devices_task: Option<gpui::Task<()>>,
    capture_task: Option<gpui::Task<()>>,
    native_icon_hidden: Option<bool>,
    update_task: Option<gpui::Task<()>>,
    update_status: Option<String>,
    update_available: bool,
    recap_task: Option<gpui::Task<()>>,
    prompt_edit: Option<prompts::PromptDraft>,
    prompt_search: Option<gpui::Entity<crate::ui::widgets::editor::Editor>>,
    prompt_observer: Option<gpui::Subscription>,
    prompt_menu: Option<i64>,
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
            devices_task: None,
            capture_task: None,
            native_icon_hidden: None,
            update_task: None,
            update_status: None,
            update_available: false,
            recap_task: None,
            prompt_edit: None,
            prompt_search: None,
            prompt_observer: None,
            prompt_menu: None,
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
        self.prompt_edit = None;
        self.prompt_menu = None;
        self.settings.device_menu = None;
        self.settings.speech_language_menu = false;
        self.settings.language_menu = false;
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

    pub fn set_autostart(&mut self, enabled: bool) {
        match crate::platform::system::startup::set_enabled(
            enabled,
            self.settings.settings.startup_privacy,
        ) {
            Ok(()) => {
                self.settings.settings.autostart = enabled;
                self.save_settings();
                self.settings.notice = None;
            }
            Err(error) => self.settings.notice = Some(error),
        }
    }

    fn check_updates(&mut self, cx: &mut Context<Self>) {
        if self.update_task.is_some() {
            return;
        }
        self.update_status = Some("Checking GitHub…".into());
        self.update_available = false;
        self.update_task = Some(cx.spawn(async move |this, cx| {
            let result = crate::platform::system::updates::latest().await.and_then(|release| {
                let newer = crate::platform::system::updates::newer(env!("CARGO_PKG_VERSION"), &release.tag_name)?;
                Ok((newer, release.tag_name))
            });
            this.update(cx, |this, cx| {
                this.update_task = None;
                this.update_status = Some(match result {
                    Ok((true, version)) => { this.update_available = true; format!("Update available: {version}. Open the release to download the installer for your system.") }
                    Ok((false, _)) => "You're up to date with the latest stable GitHub release.".into(),
                    Err(error) => error,
                });
                cx.notify();
            }).ok();
        }));
    }

    fn request_recap(&mut self, cx: &mut Context<Self>) {
        if self.recap_task.is_some() {
            return;
        }
        let Some(id) = self.open_meeting.clone() else {
            return;
        };
        let Some(path) = self
            .database
            .as_ref()
            .and_then(|db| db.connection().path())
            .map(std::path::PathBuf::from)
        else {
            return;
        };
        let config = self
            .settings
            .stored_chat
            .iter()
            .find(|credential| credential.provider == self.settings.settings.chat_provider)
            .or_else(|| {
                self.settings
                    .stored_chat
                    .iter()
                    .find(|credential| credential.to_config().is_usable())
            })
            .map(|c| c.to_config());
        let language = self.settings.settings.response_language.clone();
        self.recap_task = Some(cx.spawn(async move |this, cx| {
            let _ = crate::ai::recap::generate(&path, &id, config, &language).await;
            this.update(cx, |this, cx| {
                this.recap_task = None;
                cx.notify();
            })
            .ok();
        }));
    }

    pub fn set_startup_privacy(&mut self, private: bool) {
        if self.settings.settings.autostart
            && let Err(error) = crate::platform::system::startup::set_enabled(true, private)
        {
            self.settings.notice = Some(error);
            return;
        }
        self.settings.settings.startup_privacy = private;
        self.save_settings();
    }

    pub fn refresh_audio_devices(&mut self, cx: &mut Context<Self>) {
        if self.settings.devices_loading {
            return;
        }
        self.settings.devices_loading = true;
        let task = cx
            .background_executor()
            .spawn(async { crate::audio::devices::enumerate() });
        self.devices_task = Some(cx.spawn(async move |this, cx| {
            let devices = task.await;
            this.update(cx, |this, cx| {
                this.settings.devices = devices;
                this.settings.devices_loading = false;
                cx.notify();
            })
            .ok();
        }));
    }

    pub fn test_capture(&mut self, check: settings_pages::CaptureCheck, cx: &mut Context<Self>) {
        if self.capture_task.is_some() {
            return;
        }
        self.settings.capture_status = Some("Checking local access…".into());
        let device = self.settings.settings.input_device.clone();
        let output = self.settings.settings.output_device.clone();
        let task = cx.background_executor().spawn(async move {
            if matches!(check, settings_pages::CaptureCheck::Screen) {
                crate::platform::screen::capture_monitor(None).map(|frame| format!("Screen capture works: {} × {}. The test frame was discarded.", frame.width(), frame.height())).map_err(|e| format!("Screen capture failed: {e}. Check your desktop capture permissions."))
            } else if matches!(check, settings_pages::CaptureCheck::SystemAudio) {
                crate::audio::system_audio::SystemAudio::start((!output.is_empty()).then_some(output.as_str())).map(|mut capture| {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    let samples = capture.take(32_000); let name = capture.source().to_owned(); let running = capture.is_running(); capture.stop();
                    if !running { format!("System audio stopped unexpectedly: {name}.") } else if samples.is_empty() { format!("{name} opened. Play meeting audio and test again to confirm samples arrive.") } else { format!("System audio access works: {name}. Test audio was discarded.") }
                }).map_err(|e| format!("System audio access failed: {e}. Check your selected output and audio server."))
            } else {
                crate::audio::Microphone::start((!device.is_empty()).then_some(device.as_str())).map(|mic| {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    let samples = mic.take(32_000); let name = mic.device_name().to_owned(); mic.stop();
                    if samples.is_empty() { format!("{name} opened, but no samples arrived. Check the device and microphone privacy settings.") } else { format!("Microphone access works: {name}. Test audio was discarded.") }
                }).map_err(|e| format!("Microphone access failed: {e}. Check your device and microphone privacy settings."))
            }
        });
        self.capture_task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                this.settings.capture_status = Some(result.unwrap_or_else(|e| e));
                this.capture_task = None;
                cx.notify();
            })
            .ok();
        }));
    }

    pub fn edit_custom_provider(
        &mut self,
        credential: Option<crate::core::credentials::Credential>,
        speech: bool,
    ) {
        use crate::ai::catalog::custom::CustomProvider;
        let mut credential = credential.unwrap_or_else(|| {
            let id = format!("custom-{}", uuid::Uuid::new_v4());
            let mut credential = if speech {
                crate::core::credentials::Credential::speech(id, "")
            } else {
                crate::core::credentials::Credential::chat(id, "")
            };
            credential.custom = Some(if speech {
                CustomProvider {
                    name: "Custom speech".into(),
                    request_format: "multipart-wav".into(),
                    response_path: "/text".into(),
                    supports_images: false,
                    ..Default::default()
                }
            } else {
                CustomProvider::default()
            });
            credential
        });
        credential.api_key.clear();
        self.settings.custom_draft = Some(credential);
        self.settings.key_field = None;
        self.settings.key_status = None;
    }

    pub fn save_custom_provider(&mut self) {
        use crate::core::credentials;
        let Some(mut credential) = self.settings.custom_draft.clone() else {
            return;
        };
        let Some(vault) = &self.settings.vault else {
            self.settings.key_status = Some("The credential vault is unavailable.".into());
            return;
        };
        if credential.api_key.is_empty()
            && let Ok(Some(saved)) = credentials::load(vault, credential.kind, &credential.provider)
        {
            credential.api_key = saved.api_key;
        }
        match credentials::save(vault, &credential) {
            Ok(()) => {
                if credential.kind == credentials::Kind::Speech
                    && self.settings.settings.speech_provider == credential.provider
                {
                    self.settings.settings.speech_model = credential.model.clone();
                    self.save_settings();
                }
                self.settings.custom_draft = None;
                self.settings.key_field = None;
                self.settings.key_status =
                    Some("Custom provider saved in the encrypted vault.".into());
                self.reload_credentials();
            }
            Err(error) => self.settings.key_status = Some(error),
        }
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
                if kind == credentials::Kind::Speech
                    && (self.settings.settings.speech_provider.is_empty()
                        || self.settings.settings.speech_provider == provider)
                {
                    self.settings.settings.speech_provider = provider;
                    self.settings.settings.speech_model = credential.model;
                    self.save_settings();
                }
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
            settings_pages::CredentialField::CustomName => self
                .settings
                .custom_draft
                .as_mut()?
                .custom
                .as_mut()
                .map(|c| &mut c.name),
            settings_pages::CredentialField::CustomEndpoint => {
                Some(&mut self.settings.custom_draft.as_mut()?.base_url)
            }
            settings_pages::CredentialField::CustomKey => {
                Some(&mut self.settings.custom_draft.as_mut()?.api_key)
            }
            settings_pages::CredentialField::CustomModel => {
                Some(&mut self.settings.custom_draft.as_mut()?.model)
            }
            settings_pages::CredentialField::CustomHeader => self
                .settings
                .custom_draft
                .as_mut()?
                .custom
                .as_mut()
                .map(|c| &mut c.auth_header),
            settings_pages::CredentialField::CustomPath => self
                .settings
                .custom_draft
                .as_mut()?
                .custom
                .as_mut()
                .map(|c| &mut c.response_path),
            settings_pages::CredentialField::CustomStream => self
                .settings
                .custom_draft
                .as_mut()?
                .custom
                .as_mut()
                .map(|c| &mut c.stream_url),
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
            .id("dashboard-titlebar")
            .window_control_area(gpui::WindowControlArea::Drag)
            .cursor(gpui::CursorStyle::OpenHand)
            .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                crate::platform::window::start_drag(window);
                cx.stop_propagation();
            })
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
                    .occlude()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
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
            .occlude()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
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
                .border_color(Theme::with_alpha(theme.destructive, 0.22))
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
                    .hover(|s| {
                        s.bg(surfaces.muted())
                            .border_color(theme.control_border(true))
                    })
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
            .child(div().flex().flex_wrap().gap(px(8.0)).justify_center()
                .child(action_button("about-website", &theme, "Visit Lennette ↗", cx, |_, cx, _| cx.open_url("https://lennette.itssingularity.com")))
                .child(action_button("about-check-updates", &theme, if self.update_task.is_some() { "Checking…" } else { "Check for updates" }, cx, |this, cx, _| this.check_updates(cx)))
                .children(self.update_available.then(|| action_button("about-open-update", &theme, "View update ↗", cx, |_, cx, _| cx.open_url("https://github.com/parazeeknova/lennette/releases/latest")))))
            .children(self.update_status.as_ref().map(|status| div().w_full().text_center().text_size(scaled(12.0)).text_color(theme.muted_foreground).whitespace_normal().child(status.clone())))
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
                        "Built for Windows and Linux with Rust and GPUI. Your provider keys are stored in the \
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
        let mut transcript = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(260.0))
            .gap(px(12.0));
        for line in lines {
            let who = match line.who.as_str() {
                "me" => "You".to_owned(),
                _ => match line.speaker {
                    Some(speaker) => format!("Speaker {}", speaker + 1),
                    None => "Them".to_owned(),
                },
            };
            let stamp = crate::features::db::meetings::format_duration(line.start_time as i64);
            transcript = transcript.child(
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
        let recap = self
            .database
            .as_ref()
            .zip(self.open_meeting.as_deref())
            .and_then(|(db, id)| {
                crate::db::meetings::recap(db.connection(), id)
                    .ok()
                    .flatten()
            });
        let mut panel = div()
            .w(px(300.0))
            .min_w(px(250.0))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .p(px(16.0))
            .rounded(px(14.0))
            .border_1()
            .border_color(theme.border)
            .bg(self.surfaces().card())
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(scaled(15.0))
                    .child("Meeting recap"),
            );
        if let Some(recap) = recap {
            let text = match recap.status.as_str() {
                "ready" => recap.content,
                "pending" => "Generating a recap from the finished transcript…".into(),
                _ => format!("The recap could not be generated: {}", recap.error),
            };
            panel = panel.child(crate::ui::overlay::render_markdown(
                &theme,
                self.surfaces(),
                &text,
            ));
        } else {
            panel = panel.child(div().text_size(scaled(12.0)).text_color(theme.muted_foreground).whitespace_normal().child("No recap saved yet. Enable auto recap in Response settings, or generate one here."));
        }
        panel = panel.child(action_button(
            "regenerate-recap",
            &theme,
            if self.recap_task.is_some() {
                "Generating…"
            } else {
                "Generate / retry recap"
            },
            cx,
            |this, cx, _| this.request_recap(cx),
        ));
        list.child(
            div()
                .flex()
                .flex_wrap()
                .items_start()
                .gap(px(18.0))
                .child(transcript)
                .child(panel),
        )
        .into_any_element()
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
        if self.dashboard.route() == super::nav::Route::Prompts && self.prompt_search.is_none() {
            self.init_prompt_search(cx);
        }
        if self.dashboard.route() == super::nav::Route::About && self.update_status.is_none() {
            self.check_updates(cx);
        }
        if self.dashboard.route() == super::nav::Route::Audio && self.devices_task.is_none() {
            self.refresh_audio_devices(cx);
        }
        let hidden = self.settings.settings.hide_app_icon || crate::cli::private_overlay_enabled();
        if self.native_icon_hidden != Some(hidden) {
            self.settings.notice = match crate::platform::window::set_icon_hidden(window, hidden) {
                crate::platform::window::Applied::Unsupported if hidden => Some("This compositor controls dock visibility. Use its window rules to hide Lennette's app icon.".into()),
                crate::platform::window::Applied::Failed => Some("The desktop could not update the app icon visibility.".into()),
                _ => self.settings.notice.take(),
            };
            self.native_icon_hidden = Some(hidden);
        }
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
            .relative()
            .font_family(crate::ui::theme::ui_font_family())
            .flex()
            .size_full()
            .rounded(px(radius::XL))
            .p(px(1.0))
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
                    "enter" => {
                        if this.settings.custom_draft.is_some() {
                            this.save_custom_provider();
                        } else {
                            this.save_credential();
                        }
                    }
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
            .child(crate::ui::widgets::window_frame::window_edge(theme))
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
