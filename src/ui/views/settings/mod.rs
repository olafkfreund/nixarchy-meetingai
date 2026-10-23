//! Settings pages.
//!
//! Each function renders one settings route. Controls write through
//! [`SettingsState`], which persists immediately so a restart never loses a
//! change.

use gpui::{
    Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px,
};
use lucide_icons::Icon;

use crate::core::settings::{AutoScreenshot, Settings, ThemeChoice};
use crate::ui::controls::{
    action, keycap, row, section, segmented, slider, switch, value_chip, warning,
};
use crate::ui::dashboard::nav::Route;
use crate::ui::icons;
use crate::ui::theme::scaled;
use crate::ui::theme::{Theme, radius, space};
pub(crate) mod custom;
mod search;

/// Shortcut catalog shown on the shortcuts page, derived from the real bindings
/// that the shortcut layer registers.
pub fn shortcut_catalog() -> Vec<(String, &'static str)> {
    crate::features::shortcuts::Binding::defaults()
        .into_iter()
        .map(|binding| (binding.display, binding.action.label()))
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CredentialField {
    Key,
    Model,
    CustomName,
    CustomEndpoint,
    CustomKey,
    CustomModel,
    CustomHeader,
    CustomPath,
    CustomStream,
}

#[derive(Clone, Copy)]
pub enum CaptureCheck {
    Microphone,
    SystemAudio,
    Screen,
}

pub const KEY_PROVIDERS: &[(&str, &str)] = &[
    ("Gemini", "gemini"),
    ("Merge Gateway", "merge-gateway"),
    ("Deepgram", "deepgram-stt"),
];

/// Settings state shared by every settings page.
pub struct SettingsState {
    pub custom_draft: Option<crate::core::credentials::Credential>,
    pub custom_open: bool,
    pub credential_editors: Vec<(
        CredentialField,
        gpui::Entity<crate::ui::widgets::editor::Editor>,
    )>,
    pub credential_observers: Vec<gpui::Subscription>,
    pub provider_menu: Option<bool>,
    pub provider_menu_bounds: [std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>; 2],
    pub provider_docs: Option<&'static str>,
    pub custom_json_mode: bool,
    pub custom_guide: bool,
    pub custom_json: Option<gpui::Entity<crate::ui::widgets::editor::Editor>>,
    pub devices: crate::audio::devices::Devices,
    pub device_menu: Option<bool>,
    pub devices_loading: bool,
    pub capture_status: Option<String>,
    pub language_menu: bool,
    pub speech_language_menu: bool,
    pub custom_speech_language: bool,
    pub settings: Settings,
    pub baseline: Settings,
    pub database: Option<crate::db::Database>,
    /// Last error from a failed save, surfaced inline.
    pub error: Option<String>,
    /// Notice shown when a setting could not be applied on this platform.
    pub notice: Option<String>,
    /// Provider key being typed on the Dev Space page.
    pub key_input: String,
    /// Which provider that key belongs to.
    pub key_provider: String,
    /// Model override typed alongside the key.
    pub key_model: String,
    pub key_field: Option<CredentialField>,
    pub model_menu_open: bool,
    pub model_options: Vec<String>,
    pub model_loading: bool,
    pub model_status: Option<String>,
    pub replace_key_field: bool,
    /// Result of the last save attempt.
    pub key_status: Option<String>,
    /// Credentials currently stored, refreshed after each change.
    pub stored_chat: Vec<crate::core::credentials::Credential>,
    pub stored_speech: Vec<crate::core::credentials::Credential>,
    /// Vault handle for reading and writing credentials.
    pub vault: Option<crate::core::vault::Vault>,
}

impl SettingsState {
    pub fn new(database: Option<crate::db::Database>) -> Self {
        Self::with_vault(database, crate::core::vault::Vault::open_default().ok())
    }

    fn with_vault(
        database: Option<crate::db::Database>,
        vault: Option<crate::core::vault::Vault>,
    ) -> Self {
        let settings = database
            .as_ref()
            .and_then(|database| Settings::load(database).ok())
            .unwrap_or_default();
        let stored_chat = vault
            .as_ref()
            .and_then(|vault| {
                crate::core::credentials::list(vault, crate::core::credentials::Kind::Chat).ok()
            })
            .unwrap_or_default();
        let stored_speech = vault
            .as_ref()
            .and_then(|vault| {
                crate::core::credentials::list(vault, crate::core::credentials::Kind::Speech).ok()
            })
            .unwrap_or_default();
        let selected = stored_chat
            .iter()
            .find(|credential| credential.custom.is_none() && credential.to_config().is_usable())
            .or_else(|| {
                stored_speech
                    .iter()
                    .find(|credential| credential.custom.is_none())
            });
        let key_provider = selected
            .map(|credential| credential.provider.clone())
            .unwrap_or_else(|| KEY_PROVIDERS[0].1.into());
        let key_model = selected
            .map(|credential| credential.to_config().model)
            .unwrap_or_else(|| crate::ai::models::default_for("gemini").unwrap().to_owned());
        Self {
            custom_draft: None,
            custom_open: false,
            credential_editors: Vec::new(),
            credential_observers: Vec::new(),
            provider_menu: None,
            provider_menu_bounds: Default::default(),
            provider_docs: None,
            custom_json_mode: false,
            custom_guide: false,
            custom_json: None,
            devices: crate::audio::devices::Devices::default(),
            device_menu: None,
            devices_loading: false,
            capture_status: None,
            language_menu: false,
            speech_language_menu: false,
            custom_speech_language: false,
            baseline: settings.clone(),
            settings,
            database,
            error: None,
            notice: None,
            key_input: String::new(),
            key_provider,
            key_model,
            key_field: None,
            model_menu_open: false,
            model_options: Vec::new(),
            model_loading: false,
            model_status: None,
            replace_key_field: false,
            key_status: None,
            stored_chat,
            stored_speech,
            vault,
        }
    }

    pub fn saved_credential(&self) -> Option<&crate::core::credentials::Credential> {
        self.stored_chat
            .iter()
            .chain(self.stored_speech.iter())
            .find(|credential| credential.provider == self.key_provider)
    }

    /// Blank input means reuse the saved key, never copy a masked display value
    /// into the request. Preserve custom endpoints and only replace a typed key.
    pub fn model_discovery_config(&self) -> Result<crate::ai::providers::ProviderConfig, String> {
        use crate::core::credentials::{self, Credential, Kind};
        if crate::ai::providers::ChatProvider::by_id(&self.key_provider).is_none() {
            return Err("Select a chat provider to browse its models.".into());
        }
        let saved = match self.vault.as_ref() {
            Some(vault) => credentials::load(vault, Kind::Chat, &self.key_provider)?,
            None => self
                .stored_chat
                .iter()
                .find(|credential| credential.provider == self.key_provider)
                .cloned(),
        };
        let mut credential = saved.unwrap_or_else(|| Credential::chat(&self.key_provider, ""));
        if !self.key_input.trim().is_empty() {
            credential.api_key = self.key_input.trim().into();
        }
        if !self.key_model.trim().is_empty() {
            credential.model = self.key_model.trim().into();
        }
        let config = credential.to_config();
        if config.api_key.is_empty() {
            return Err(
                "No key is saved for this provider yet. Add its API key once to fetch models."
                    .into(),
            );
        }
        Ok(config)
    }

    /// Persist the current settings, recording any failure.
    pub fn save(&mut self) {
        let Some(database) = self.database.as_ref() else {
            self.error = Some("The local database is not available.".to_owned());
            return;
        };
        match self.settings.save_changed(&self.baseline, database) {
            Ok(()) => {
                self.error = None;
                self.settings = Settings::load(database).unwrap_or_else(|_| self.settings.clone());
                self.baseline = self.settings.clone();
            }
            Err(error) => self.error = Some(format!("Could not save: {error}")),
        }
    }
}

fn page_header(theme: &Theme, title: &str, description: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .text_size(scaled(20.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.foreground)
                .child(title.to_owned()),
        )
        .child(
            div()
                .text_size(scaled(12.0))
                .text_color(theme.muted_foreground)
                .child(description.to_owned()),
        )
}

fn stack(children: Vec<gpui::AnyElement>) -> gpui::AnyElement {
    let mut column = div()
        .flex()
        .flex_col()
        .w_full()
        .min_w(px(0.0))
        .gap(px(20.0));
    for child in children {
        column = column.child(child);
    }
    column.into_any_element()
}

/// App settings: theme, transparency, startup and window behaviour.
pub fn app_settings(
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<crate::ui::dashboard::view::DashboardView>,
) -> gpui::AnyElement {
    let s = &state.settings;

    let theme_options: Vec<(&'static str, bool)> = ThemeChoice::ALL
        .iter()
        .map(|choice| (choice.label(), *choice == s.theme))
        .collect();

    let appearance = section(
        theme,
        "Appearance",
        "Personalize the look and transparency of the overlay.",
    )
    .child(row(
        theme,
        "Theme",
        "Which palette the overlay uses",
        segmented(
            "theme-choice",
            theme,
            &theme_options,
            cx,
            |view: &mut crate::ui::dashboard::view::DashboardView, index, _| {
                view.settings_mut().settings.theme = ThemeChoice::ALL[index];
                view.save_settings();
            },
        ),
    ))
    .child(row(
        theme,
        "Transparency",
        "Lower values let the desktop show through",
        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(slider(
                "transparency-slider",
                theme,
                s.transparency,
                0,
                100,
                cx,
                |view: &mut crate::ui::dashboard::view::DashboardView, new_val, cx| {
                    view.settings_mut().settings.transparency = new_val;
                    view.schedule_settings_save(cx);
                },
            ))
            .child(value_chip(theme, &format!("{}%", s.transparency))),
    ))
    .child(row(
        theme,
        "Text size",
        "Scale text in the overlay and dashboard",
        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(slider(
                "font-scale-slider",
                theme,
                s.font_scale,
                85,
                140,
                cx,
                |view: &mut crate::ui::dashboard::view::DashboardView, new_val, cx| {
                    view.settings_mut().settings.font_scale = new_val;
                    view.schedule_settings_save(cx);
                },
            ))
            .child(value_chip(theme, &format!("{}%", s.font_scale))),
    ))
    .child(row(
        theme,
        "Show borders",
        "Outline Lennette windows and panels",
        switch(
            "show-borders",
            theme,
            s.show_borders,
            cx,
            |view: &mut crate::ui::dashboard::view::DashboardView, _, _| {
                view.settings_mut().settings.show_borders =
                    !view.settings_mut().settings.show_borders;
                view.save_settings();
            },
        ),
    ))
    .child(row(
        theme,
        "Always on top",
        "Keep Lennette above other windows while listening or idle",
        switch(
            "always-on-top",
            theme,
            s.always_on_top,
            cx,
            |view: &mut crate::ui::dashboard::view::DashboardView, _, window| {
                let enabled = !view.settings_mut().settings.always_on_top;
                view.settings_mut().settings.always_on_top = enabled;
                view.save_settings();
                view.apply_always_on_top(enabled, window);
            },
        ),
    ));

    let startup = section(
        theme,
        "Launch and Windows",
        "How lennette behaves when it is not in focus.",
    )
    .child(row(
        theme,
        "Launch on startup",
        "Open lennette when your system starts",
        switch(
            "autostart",
            theme,
            s.autostart,
            cx,
            |view: &mut crate::ui::dashboard::view::DashboardView, _, _| {
                let enabled = !view.settings_mut().settings.autostart;
                view.set_autostart(enabled);
            },
        ),
    ))
    .child(row(
        theme,
        "Startup mode",
        "Choose which Lennette launch opens when you sign in",
        segmented(
            "startup-mode",
            theme,
            &[
                ("Normal", !s.startup_privacy),
                ("Privacy", s.startup_privacy),
            ],
            cx,
            |view: &mut crate::ui::dashboard::view::DashboardView, index, _| {
                view.set_startup_privacy(index == 1);
            },
        ),
    ))
    .child(row(
        theme,
        "Hide app icon",
        "Hide the taskbar or dock icon. Privacy mode also removes the tray icon.",
        switch(
            "hide-app-icon",
            theme,
            s.hide_app_icon,
            cx,
            |view: &mut crate::ui::dashboard::view::DashboardView, _, _| {
                let enabled = view.settings_mut().settings.hide_app_icon;
                view.settings_mut().settings.hide_app_icon = !enabled;
                view.save_settings();
            },
        ),
    ))
    .child(row(
        theme,
        "Reduce motion",
        "Disable transitions and pulses",
        switch(
            "reduce-motion",
            theme,
            s.reduce_motion,
            cx,
            |view: &mut crate::ui::dashboard::view::DashboardView, _, _| {
                let enabled = view.settings_mut().settings.reduce_motion;
                view.settings_mut().settings.reduce_motion = !enabled;
                view.save_settings();
            },
        ),
    ));

    let mut children = vec![appearance.into_any_element(), startup.into_any_element()];
    if let Some(message) = state.error.as_deref() {
        children.insert(0, warning(theme, message).into_any_element());
    }
    if let Some(message) = state.notice.as_deref() {
        children.insert(0, warning(theme, message).into_any_element());
    }
    stack(children)
}

/// Screenshot behaviour.
pub fn screenshot_settings(
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<crate::ui::dashboard::view::DashboardView>,
) -> gpui::AnyElement {
    let s = &state.settings;
    let modes: Vec<(&'static str, bool)> = AutoScreenshot::ALL
        .iter()
        .map(|mode| (mode.label(), *mode == s.auto_screenshot))
        .collect();

    let capture = section(
        theme,
        "Automatic Screenshots",
        "Capture context for questions, or refresh it while recording on an interval.",
    )
    .child(row(
        theme,
        "Mode",
        "Choose when the screen is captured automatically",
        segmented(
            "auto-screenshot",
            theme,
            &modes,
            cx,
            |view: &mut crate::ui::dashboard::view::DashboardView, index, _| {
                view.settings_mut().settings.auto_screenshot = AutoScreenshot::ALL[index];
                view.save_settings();
            },
        ),
    ))
    .child(row(
        theme,
        "Capture delay",
        "Wait before capturing, so menus can close",
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(slider(
                "capture-delay-slider",
                theme,
                s.capture_delay_seconds,
                0,
                60,
                cx,
                |view: &mut crate::ui::dashboard::view::DashboardView, value, cx| {
                    view.settings_mut().settings.capture_delay_seconds = value;
                    view.schedule_settings_save(cx);
                },
            ))
            .child(value_chip(theme, &format!("{}s", s.capture_delay_seconds))),
    ))
    .child(row(
        theme,
        "Interval",
        "Refresh screen context while recording in interval mode",
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(slider(
                "capture-interval-slider",
                theme,
                s.screenshot_interval,
                5,
                600,
                cx,
                |view: &mut crate::ui::dashboard::view::DashboardView, value, cx| {
                    view.settings_mut().settings.screenshot_interval = value;
                    view.schedule_settings_save(cx);
                },
            ))
            .child(value_chip(theme, &format!("{}s", s.screenshot_interval))),
    ))
    .child(row(
        theme,
        "Use image on send",
        "Always attach a fresh screenshot, regardless of mode",
        switch(
            "use-image",
            theme,
            s.use_image_on_send,
            cx,
            |view: &mut crate::ui::dashboard::view::DashboardView, _, _| {
                let enabled = view.settings_mut().settings.use_image_on_send;
                view.settings_mut().settings.use_image_on_send = !enabled;
                view.save_settings();
            },
        ),
    ));

    stack(vec![capture.child(div().py(px(space::SM)).text_size(scaled(11.0)).text_color(theme.muted_foreground)
        .child("Captures use the active monitor and are scaled to at most 1920 pixels. Interval mode refreshes context while recording; it does not send questions.")).into_any_element()])
}

/// Audio devices and speech providers.
pub fn audio_settings(
    view: &crate::ui::dashboard::view::DashboardView,
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<crate::ui::dashboard::view::DashboardView>,
) -> gpui::AnyElement {
    let s = &state.settings;
    let mut microphone = section(
        theme,
        "Microphone",
        "Used by Listen and push to talk. Hold the composer microphone button, release to transcribe, then review before sending.",
    );
    microphone = microphone.child(device_picker(state, theme, true, cx));
    microphone = microphone.child(row(
        theme,
        "Always on top",
        "Keep Lennette above the meeting window while using audio",
        switch(
            "audio-always-on-top",
            theme,
            s.always_on_top,
            cx,
            |view: &mut crate::ui::dashboard::view::DashboardView, _, window| {
                let enabled = !view.settings_mut().settings.always_on_top;
                view.settings_mut().settings.always_on_top = enabled;
                view.save_settings();
                view.apply_always_on_top(enabled, window);
            },
        ),
    ));
    let mut speakers = section(
        theme,
        "System Audio",
        "Capture the speakers or headphones playing the meeting. Enable System Audio in Listen to hear the other participants.",
    );
    speakers = speakers.child(device_picker(state, theme, false, cx));
    let mut providers = section(
        theme,
        "Speech to Text",
        "Choose a configured service. Batch providers transcribe speech after a pause; live providers can show interim words.",
    );
    for credential in &state.stored_speech {
        let provider = credential.provider.clone();
        let model = credential.to_config().model;
        let label = credential
            .custom
            .as_ref()
            .map(|c| c.name.clone())
            .or_else(|| {
                crate::ai::providers::SttProvider::by_id(&provider).map(|p| p.name.to_owned())
            })
            .unwrap_or_else(|| provider.clone());
        let display_model = model.clone();
        providers = providers.child(row(
            theme,
            &label,
            &display_model,
            action(
                gpui::ElementId::Name(format!("pick-stt-{provider}").into()),
                theme,
                if s.speech_provider == provider {
                    "Selected"
                } else {
                    "Choose"
                },
                cx,
                move |view: &mut crate::ui::dashboard::view::DashboardView, _, _| {
                    view.settings_mut().settings.speech_provider = provider.clone();
                    view.settings_mut().settings.speech_model = model.clone();
                    view.save_settings();
                },
            ),
        ));
    }
    providers = providers.child(speech_language_picker(view, state, theme, cx));
    if state.stored_speech.is_empty() {
        providers = providers.child(help_text(
            theme,
            "Add a built-in or custom speech provider in AI & Keys to enable transcription.",
        ));
    }
    let mut access = section(theme, "Access and checks", "Device detection does not prove permission to record. These checks run locally and do not send audio or screenshots to an AI service.")
        .child(row(theme, "Microphone access", "Open the selected microphone briefly and check whether samples arrive", action("test-mic", theme, "Test microphone", cx,
            |view: &mut crate::ui::dashboard::view::DashboardView, cx, _| view.test_capture(CaptureCheck::Microphone, cx))))
        .child(row(theme, "Screen context", "Capture one frame locally, then discard it", action("test-screen", theme, "Test screen capture", cx,
            |view: &mut crate::ui::dashboard::view::DashboardView, cx, _| view.test_capture(CaptureCheck::Screen, cx))));
    access = access.child(row(
        theme,
        "System audio access",
        "Open the selected output for loopback capture, then discard the samples",
        action(
            "test-system-audio",
            theme,
            "Test system audio",
            cx,
            |view: &mut crate::ui::dashboard::view::DashboardView, cx, _| {
                view.test_capture(CaptureCheck::SystemAudio, cx)
            },
        ),
    ));
    #[cfg(windows)]
    {
        access = access.child(row(
            theme,
            "Windows microphone privacy",
            "Allow microphone access and let desktop apps access your microphone",
            action(
                "microphone-privacy",
                theme,
                "Open settings",
                cx,
                |_view: &mut crate::ui::dashboard::view::DashboardView, cx, _| {
                    cx.open_url("ms-settings:privacy-microphone")
                },
            ),
        ));
    }
    #[cfg(target_os = "linux")]
    {
        access = access.child(help_text(theme, "Linux: allow access to the audio server. System audio requires pactl and parec with PulseAudio or PipeWire-Pulse. Wayland screen capture may request permission through your desktop portal; Hyprland uses grim."));
    }
    if let Some(status) = &state.capture_status {
        access = access.child(help_text(theme, status));
    }
    stack(vec![
        microphone.into_any_element(),
        speakers.into_any_element(),
        providers.into_any_element(),
        access.into_any_element(),
    ])
}

fn device_picker(
    state: &SettingsState,
    theme: &Theme,
    input: bool,
    cx: &mut Context<crate::ui::dashboard::view::DashboardView>,
) -> gpui::AnyElement {
    let selected = if input {
        &state.settings.input_device
    } else {
        &state.settings.output_device
    };
    let devices = if input {
        &state.devices.inputs
    } else {
        &state.devices.outputs
    };
    let label = if selected.is_empty() {
        "System default".to_owned()
    } else {
        devices
            .iter()
            .find(|d| &d.id == selected)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| format!("Unavailable: {selected}"))
    };
    let open = state.device_menu == Some(input);
    let mut block = div()
        .flex()
        .flex_col()
        .w_full()
        .items_start()
        .pt(px(10.0))
        .gap(px(8.0))
        .on_mouse_down_out(cx.listener(move |view, _, _, cx| {
            if view.settings_mut().device_menu == Some(input) {
                view.settings_mut().device_menu = None;
                cx.notify();
            }
        }))
        .child(
            div()
                .flex()
                .items_center()
                .w_full()
                .max_w(px(480.0))
                .min_w(px(0.0))
                .gap(px(8.0))
                .child(
                    select_trigger(
                        if input {
                            "microphone-picker"
                        } else {
                            "output-picker"
                        },
                        theme,
                        &label,
                        open,
                        cx,
                        move |view, _, _| {
                            let state = view.settings_mut();
                            state.device_menu = if state.device_menu == Some(input) {
                                None
                            } else {
                                Some(input)
                            };
                            state.speech_language_menu = false;
                        },
                    )
                    .w_auto()
                    .flex_1(),
                )
                .child(
                    div()
                        .id(if input {
                            "refresh-microphones"
                        } else {
                            "refresh-outputs"
                        })
                        .w(px(68.0))
                        .h(px(36.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(9.0))
                        .border_1()
                        .border_color(theme.control_border(false))
                        .bg(theme.input)
                        .text_color(theme.foreground)
                        .text_size(scaled(11.0))
                        .cursor_pointer()
                        .opacity(if state.devices_loading { 0.5 } else { 1.0 })
                        .hover(|s| s.bg(theme.menu_hover()))
                        .child("Refresh")
                        .on_click(cx.listener(|view, _, _, cx| {
                            if !view.settings_mut().devices_loading {
                                view.refresh_audio_devices(cx);
                            }
                            cx.notify();
                        })),
                ),
        );
    if open {
        let mut menu = div()
            .id(if input {
                "microphone-options"
            } else {
                "output-options"
            })
            .w_full()
            .max_w(px(380.0))
            .max_h(px(240.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .p(px(5.0))
            .rounded(px(12.0))
            .bg(theme.popover())
            .border_1()
            .border_color(theme.border)
            .shadow_md()
            .occlude()
            .child(select_option(
                if input {
                    "default-microphone"
                } else {
                    "default-output"
                },
                theme,
                "System default",
                selected.is_empty(),
                cx,
                move |view, _, _| {
                    let state = view.settings_mut();
                    if input {
                        state.settings.input_device.clear();
                    } else {
                        state.settings.output_device.clear();
                    }
                    state.device_menu = None;
                    view.save_settings();
                },
            ));
        for (index, device) in devices.iter().enumerate() {
            let id = device.id.clone();
            let label = format!(
                "{}{}",
                device.name,
                if device.is_default { " (Default)" } else { "" }
            );
            menu = menu.child(select_option(
                gpui::ElementId::Name(format!("device-{input}-{index}").into()),
                theme,
                &label,
                selected == &device.id,
                cx,
                move |view, _, _| {
                    let state = view.settings_mut();
                    if input {
                        state.settings.input_device = id.clone();
                    } else {
                        state.settings.output_device = id.clone();
                    }
                    state.device_menu = None;
                    view.save_settings();
                },
            ));
        }
        block = block.child(crate::ui::widgets::motion::reveal(
            menu,
            "settings-picker-open",
            state.settings.reduce_motion,
            true,
        ));
    }
    if let Some(error) = &state.devices.error {
        block = block.child(warning(theme, error));
    }
    if devices.is_empty() && !state.devices_loading {
        block = block.child(help_text(
            theme,
            "No devices detected. Connect a device, check system access, then Refresh.",
        ));
    }
    block.into_any_element()
}

fn select_trigger(
    id: &'static str,
    theme: &Theme,
    label: &str,
    open: bool,
    cx: &mut Context<crate::ui::dashboard::view::DashboardView>,
    handler: impl Fn(
        &mut crate::ui::dashboard::view::DashboardView,
        &mut Context<crate::ui::dashboard::view::DashboardView>,
        &mut gpui::Window,
    ) + 'static,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(8.0))
        .w_full()
        .min_w(px(0.0))
        .overflow_hidden()
        .h(px(36.0))
        .px(px(10.0))
        .rounded(px(10.0))
        .bg(theme.input)
        .border_1()
        .border_color(if open {
            theme.focus_border()
        } else {
            theme.control_border(false)
        })
        .text_color(theme.foreground)
        .text_size(scaled(12.0))
        .cursor_pointer()
        .hover(|style| style.bg(theme.menu_hover()))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .child(label.to_owned()),
        )
        .child(
            div()
                .w(px(16.0))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .child(icons::icon(
                    theme,
                    if open {
                        Icon::ChevronUp
                    } else {
                        Icon::ChevronDown
                    },
                    14.0,
                )),
        )
        .on_click(cx.listener(move |view, _, window, cx| {
            handler(view, cx, window);
            cx.notify();
        }))
}

fn select_option(
    id: impl Into<gpui::ElementId>,
    theme: &Theme,
    label: &str,
    selected: bool,
    cx: &mut Context<crate::ui::dashboard::view::DashboardView>,
    handler: impl Fn(
        &mut crate::ui::dashboard::view::DashboardView,
        &mut Context<crate::ui::dashboard::view::DashboardView>,
        &mut gpui::Window,
    ) + 'static,
) -> gpui::AnyElement {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(10.0))
        .w_full()
        .min_w(px(0.0))
        .flex_shrink_0()
        .min_h(px(34.0))
        .px(px(10.0))
        .py(px(7.0))
        .rounded(px(8.0))
        .bg(if selected {
            theme.menu_hover()
        } else {
            gpui::transparent_black().into()
        })
        .text_color(theme.foreground)
        .text_size(scaled(12.0))
        .cursor_pointer()
        .hover(|style| style.bg(theme.menu_hover()))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .whitespace_normal()
                .child(label.to_owned()),
        )
        .child(
            div()
                .w(px(14.0))
                .flex_shrink_0()
                .children(selected.then(|| icons::icon(theme, Icon::Check, 14.0))),
        )
        .on_click(cx.listener(move |view, _, window, cx| {
            handler(view, cx, window);
            cx.notify();
        }))
        .into_any_element()
}

const SPEECH_LANGUAGES: &[(&str, &str)] = &[
    ("Provider default", ""),
    ("English", "en"),
    ("Hindi", "hi"),
    ("Spanish", "es"),
    ("French", "fr"),
    ("German", "de"),
    ("Portuguese", "pt"),
    ("Italian", "it"),
    ("Dutch", "nl"),
    ("Japanese", "ja"),
    ("Korean", "ko"),
    ("Chinese", "zh"),
    ("Arabic", "ar"),
    ("Russian", "ru"),
    ("Turkish", "tr"),
    ("Indonesian", "id"),
];

fn speech_language_label(code: &str) -> String {
    SPEECH_LANGUAGES
        .iter()
        .find(|(_, value)| *value == code)
        .map(|(label, _)| (*label).to_owned())
        .unwrap_or_else(|| format!("Custom ({code})"))
}

fn speech_language_picker(
    view: &crate::ui::dashboard::view::DashboardView,
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<crate::ui::dashboard::view::DashboardView>,
) -> gpui::AnyElement {
    let code = &state.settings.speech_language;
    let custom =
        state.custom_speech_language || !SPEECH_LANGUAGES.iter().any(|(_, value)| value == code);
    let label = if state.custom_speech_language
        && SPEECH_LANGUAGES.iter().any(|(_, value)| value == code)
    {
        "Custom language...".into()
    } else {
        speech_language_label(code)
    };
    let mut block = div()
        .flex()
        .flex_col()
        .w_full()
        .items_start()
        .gap(px(8.0))
        .py(px(8.0))
        .child(
            div()
                .text_size(scaled(12.0))
                .text_color(theme.foreground)
                .child("Speech language"),
        )
        .child(div().w_full().max_w(px(380.0)).child(select_trigger(
            "speech-language-picker",
            theme,
            &label,
            state.speech_language_menu,
            cx,
            |view, _, _| {
                let state = view.settings_mut();
                state.speech_language_menu = !state.speech_language_menu;
                state.device_menu = None;
            },
        )))
        .on_mouse_down_out(cx.listener(|view, _, _, cx| {
            if view.settings_mut().speech_language_menu {
                view.settings_mut().speech_language_menu = false;
                cx.notify();
            }
        }));
    if state.speech_language_menu {
        let mut menu = div()
            .id("speech-language-options")
            .w_full()
            .max_w(px(380.0))
            .max_h(px(240.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .p(px(5.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover())
            .shadow_md()
            .occlude();
        for (index, (name, code)) in SPEECH_LANGUAGES.iter().enumerate() {
            menu = menu.child(select_option(
                ("speech-language-option", index),
                theme,
                name,
                !custom && state.settings.speech_language == *code,
                cx,
                move |view, _, _| {
                    let state = view.settings_mut();
                    state.settings.speech_language = (*code).into();
                    state.speech_language_menu = false;
                    state.custom_speech_language = false;
                    view.save_settings();
                },
            ));
        }
        menu = menu.child(select_option(
            "speech-language-custom",
            theme,
            "Add a custom language...",
            custom,
            cx,
            |view, _, _| {
                view.settings_mut().speech_language_menu = false;
                view.settings_mut().custom_speech_language = true;
            },
        ));
        block = block.child(crate::ui::widgets::motion::reveal(
            menu,
            "settings-picker-open",
            state.settings.reduce_motion,
            true,
        ));
    }
    if custom {
        block = block.child(view.editable_setting(
            "speech_language",
            "Custom language code (for example en-US or ta)",
            code,
            cx,
        ));
    }
    block.child(help_text(theme, "Choose a supported language for your speech provider. Availability and language-code support depend on the service and model.")).into_any_element()
}

fn help_text(theme: &Theme, text: &str) -> impl IntoElement {
    div()
        .w_full()
        .min_w(px(0.0))
        .py(px(8.0))
        .text_size(scaled(11.0))
        .text_color(theme.muted_foreground)
        .whitespace_normal()
        .child(text.to_owned())
}

/// Follow-up suggestions and answer style.
pub fn response_settings(
    view: &crate::ui::dashboard::view::DashboardView,
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<crate::ui::dashboard::view::DashboardView>,
) -> gpui::AnyElement {
    let s = &state.settings;

    let follow_ups = section(
        theme,
        "Follow-ups",
        "Suggest what to say next after each answer.",
    )
    .child(row(
        theme,
        "Smart follow-ups",
        "Show suggested next questions above the composer",
        switch(
            "smart-follow-ups",
            theme,
            s.smart_follow_ups,
            cx,
            |view: &mut crate::ui::dashboard::view::DashboardView, _, _| {
                let enabled = view.settings_mut().settings.smart_follow_ups;
                view.settings_mut().settings.smart_follow_ups = !enabled;
                view.save_settings();
            },
        ),
    ));

    let follow_ups = follow_ups.child(
        view.editable_setting("smart_follow_ups_prompt", "Question generation prompt", &s.smart_follow_ups_prompt, cx)
    ).child(help_text(theme, "Follow-up suggestions use an additional AI request after each answer when enabled. Titles use a separate request when a session is named."));
    use crate::core::settings::{AutoResponse, ResponsePace};
    let choices: Vec<_> = AutoResponse::ALL
        .iter()
        .map(|choice| (choice.label(), *choice == s.auto_response))
        .collect();
    let pace: Vec<_> = ResponsePace::ALL
        .iter()
        .map(|choice| (choice.label(), *choice == s.response_pace))
        .collect();
    let automatic = section(theme, "Automatic responses", "When to respond during the conversation. What it says comes from your selected prompt (and its attached files) + the live transcript.")
        .child(segmented("auto-response", theme, &choices, cx, |view: &mut crate::ui::dashboard::view::DashboardView, index, _| {
            view.settings_mut().settings.auto_response = AutoResponse::ALL[index]; view.save_settings();
        }))
        .child(help_text(theme, "When a question is asked: responds when someone asks something. After every pause: responds continuously — uses more AI requests. Off: manual requests only."))
        .child(row(theme, "Pace", "Wait for a complete thought before responding", segmented("response-pace", theme, &pace, cx, |view: &mut crate::ui::dashboard::view::DashboardView, index, _| {
            view.settings_mut().settings.response_pace = ResponsePace::ALL[index]; view.save_settings();
        })))
        .child(help_text(theme, "Each response uses 1 AI request. Short remarks never trigger one on their own. Only the other participants trigger automatic responses; your own speech supplies context."));
    use crate::core::settings::{RESPONSE_LANGUAGES, ResponseLength};
    let length_options: Vec<_> = ResponseLength::ALL
        .iter()
        .map(|length| (length.label(), *length == s.response_length))
        .collect();
    let mut responses = section(theme, "Responses", "Length and language apply to new conversations across all AI providers. An existing conversation keeps the preferences it started with.")
        .child(row(theme, "Response length", "How much explanation an answer should include", segmented("response-length", theme, &length_options, cx,
            |view: &mut crate::ui::dashboard::view::DashboardView, index, _| { view.settings_mut().settings.response_length = ResponseLength::ALL[index]; view.save_settings(); })))
        .child(help_text(theme, "Short: quick answers and summaries. Medium: balanced explanations for tasks. Auto: the AI chooses based on the complexity of your question."))
        .child(row(theme, "Response language", "Language support depends on the selected AI provider", action("response-language", theme, &s.response_language, cx,
            |view: &mut crate::ui::dashboard::view::DashboardView, _, _| { view.settings_mut().language_menu = !view.settings_mut().language_menu; })));
    if state.language_menu {
        let mut languages = div()
            .id("response-language-options")
            .flex()
            .flex_col()
            .max_h(px(240.0))
            .overflow_y_scroll()
            .gap(px(4.0))
            .p(px(8.0))
            .bg(theme.popover())
            .rounded(px(12.0))
            .border_1()
            .border_color(theme.border);
        for (index, language) in RESPONSE_LANGUAGES.iter().enumerate() {
            languages = languages.child(action(
                ("response-language", index),
                theme,
                language,
                cx,
                move |view: &mut crate::ui::dashboard::view::DashboardView, _, _| {
                    view.settings_mut().settings.response_language = (*language).into();
                    view.settings_mut().language_menu = false;
                    view.save_settings();
                },
            ));
        }
        responses = responses.child(crate::ui::widgets::motion::reveal(
            languages,
            "response-language-open",
            state.settings.reduce_motion,
            true,
        ));
    }
    responses = responses.child(row(theme, "Auto-scroll", "Follow new answers and transcript lines as they arrive", switch("auto-scroll", theme, s.auto_scroll, cx,
        |view: &mut crate::ui::dashboard::view::DashboardView, _, _| { view.settings_mut().settings.auto_scroll = !view.settings_mut().settings.auto_scroll; view.save_settings(); })))
        .child(row(theme, "Auto recap when a meeting ends", "Save a summary, decisions and action items beside its transcript", switch("auto-recap", theme, s.auto_recap, cx,
        |view: &mut crate::ui::dashboard::view::DashboardView, _, _| { view.settings_mut().settings.auto_recap = !view.settings_mut().settings.auto_recap; view.save_settings(); })))
        .child(help_text(theme, "Auto recap sends the finished transcript to your selected AI provider in one additional request. It waits for the final words and saves the result to that meeting."));
    stack(vec![
        responses.into_any_element(),
        automatic.into_any_element(),
        follow_ups.into_any_element(),
    ])
}

/// Keyboard shortcuts.
pub fn shortcut_settings(
    theme: &Theme,
    _cx: &mut Context<crate::ui::dashboard::view::DashboardView>,
) -> gpui::AnyElement {
    let mut list = section(
        theme,
        "Global Shortcuts",
        "These work from any application while lennette runs.",
    );
    for (keys, label) in shortcut_catalog() {
        list = list.child(row(
            theme,
            label,
            "Global shortcut",
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .children(keys.split('+').map(|part| keycap(theme, part))),
        ));
    }

    let (_, message) = crate::features::shortcuts::support();
    list = list.child(div().pt(px(space::SM)).child(warning(theme, message)));

    stack(vec![list.into_any_element()])
}

/// Provider credentials: the only place a key enters the app.
pub(crate) fn credentials_section(
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<crate::ui::dashboard::view::DashboardView>,
) -> impl IntoElement {
    let name = KEY_PROVIDERS
        .iter()
        .find(|(_, id)| *id == state.key_provider)
        .map(|(name, _)| *name)
        .unwrap_or("Provider");
    let mut card = custom::editor_header(
        theme,
        name,
        "Connect with an API key and choose a model. Leave the key unchanged to reuse its encrypted saved value.",
    );

    for (field, label) in [
        (CredentialField::Key, "API key"),
        (CredentialField::Model, "Model"),
    ] {
        let chat_model = field == CredentialField::Model
            && crate::ai::providers::ChatProvider::by_id(&state.key_provider).is_some();
        card = card.child(custom::native_field(state, theme, field, label));
        if chat_model {
            card = card.child(div().flex().child(action(
                "browse-provider-models",
                theme,
                "Browse models",
                cx,
                |view: &mut crate::ui::dashboard::view::DashboardView, cx, _| {
                    view.toggle_credential_models(false, cx)
                },
            )));
        }
        if chat_model && state.model_menu_open {
            let mut dropdown = div()
                .id("credential-model-options")
                .flex()
                .flex_col()
                .w_full()
                .max_h(px(220.0))
                .overflow_y_scroll()
                .p(px(6.0))
                .gap(px(4.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover())
                .text_color(theme.foreground)
                .child(action(
                    "refresh-credential-models",
                    theme,
                    if state.model_loading {
                        "Loading models…"
                    } else {
                        "Refresh live models"
                    },
                    cx,
                    |view: &mut crate::ui::dashboard::view::DashboardView, cx, _| {
                        view.toggle_credential_models(true, cx)
                    },
                ));
            if let Some(status) = &state.model_status {
                dropdown = dropdown.child(warning(theme, status));
            }
            for (index, model) in state.model_options.iter().enumerate() {
                let chosen = model.clone();
                dropdown = dropdown.child(
                    div()
                        .id(("credential-model-option", index))
                        .px(px(8.0))
                        .py(px(6.0))
                        .cursor_pointer()
                        .text_size(scaled(12.0))
                        .child(model.clone())
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.settings_mut().key_model = chosen.clone();
                            view.settings_mut().credential_observers.clear();
                            view.settings_mut().credential_editors.clear();
                            view.settings_mut().model_menu_open = false;
                            cx.notify();
                        })),
                );
            }
            dropdown = dropdown.child(action(
                "custom-model-id",
                theme,
                "Enter custom model ID",
                cx,
                |view: &mut crate::ui::dashboard::view::DashboardView, cx, window| {
                    view.settings_mut().model_menu_open = false;
                    if let Some((_, editor)) = view
                        .settings_mut()
                        .credential_editors
                        .iter()
                        .find(|(field, _)| *field == CredentialField::Model)
                    {
                        editor.read(cx).focus(window);
                    }
                },
            ));
            card = card.child(crate::ui::widgets::motion::reveal(
                dropdown,
                "model-picker-open",
                state.settings.reduce_motion,
                true,
            ));
        }
    }
    card = card.child(div().flex().items_center().py(px(8.0)).child(action(
        "save-key",
        theme,
        if state.key_input.trim().is_empty() && state.saved_credential().is_some() {
            "Save changes"
        } else {
            "Save key"
        },
        cx,
        |view: &mut crate::ui::dashboard::view::DashboardView, _, _| {
            view.save_credential();
        },
    )));
    if let Some(status) = state.key_status.as_deref() {
        card = card.child(
            div()
                .text_size(scaled(11.0))
                .text_color(theme.muted_foreground)
                .child(status.to_owned()),
        );
    }

    card = card.child(div().flex().child(action(
        "close-provider-editor",
        theme,
        "Cancel",
        cx,
        |view: &mut crate::ui::dashboard::view::DashboardView, _, _| {
            view.settings_mut().custom_open = false;
            view.settings_mut().key_field = None;
        },
    )));

    card.child(
        div()
            .flex()
            .children(custom::remove_button(state, theme, cx)),
    )
}

/// Provider setup and storage status.
pub fn dev_space(
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<crate::ui::dashboard::view::DashboardView>,
) -> gpui::AnyElement {
    let storage = section(
        theme,
        "Storage",
        "Where lennette keeps your data on this computer.",
    )
    .child(row(
        theme,
        "Database",
        "Conversations, transcripts and settings",
        value_chip(
            theme,
            &state
                .database
                .as_ref()
                .map(|_| "Connected".to_owned())
                .unwrap_or_else(|| "Not available".to_owned()),
        ),
    ))
    .child(row(
        theme,
        "Credential vault",
        "API keys are encrypted with AES-256-GCM",
        value_chip(theme, "Encrypted"),
    ));

    stack(vec![
        custom::providers(state, theme, cx),
        storage.into_any_element(),
    ])
}

/// Render the settings body for a route.
pub fn render(
    view: &crate::ui::dashboard::view::DashboardView,
    route: Route,
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<crate::ui::dashboard::view::DashboardView>,
) -> gpui::AnyElement {
    let active_route = if route.is_setting() {
        route
    } else {
        Route::Settings
    };
    let subtabs = [
        ("General", Route::Settings),
        ("Responses", Route::Responses),
        ("Audio", Route::Audio),
        ("AI & Keys", Route::DevSpace),
        ("Shortcuts", Route::Shortcuts),
    ];
    let surfaces = theme.surfaces(crate::features::theme::DEFAULT_OPACITY);
    let mut tab_bar = div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .p(px(3.0))
        .rounded(px(radius::FULL))
        .border_1()
        .border_color(surfaces.border())
        .bg(surfaces.card());

    for (label, sub_route) in subtabs {
        let is_active = sub_route == active_route;
        let mut chip = div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(26.0))
            .px(px(14.0))
            .rounded(px(radius::FULL))
            .text_size(scaled(11.5))
            .font_weight(if is_active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            })
            .cursor_pointer()
            .text_color(theme.foreground)
            .id(("settings-subtab", sub_route as usize));

        if is_active {
            chip = chip
                .bg(surfaces.muted())
                .border_1()
                .border_color(surfaces.border());
        } else {
            chip = chip.hover(|s| s.bg(surfaces.card()));
        }

        chip = chip.on_click(cx.listener(move |view, _, _, cx| {
            view.navigate_to(sub_route);
            cx.notify();
        }));
        tab_bar = tab_bar.child(chip.child(label));
    }

    let query = view.settings_search_query(cx);
    let body: gpui::AnyElement = if !query.trim().is_empty() {
        search::results(&query, theme, state.settings.reduce_motion, cx)
    } else {
        match active_route {
            Route::Settings => {
                let app = app_settings(state, theme, cx);
                let screenshots = screenshot_settings(state, theme, cx);
                stack(vec![app, screenshots])
            }
            Route::Responses => response_settings(view, state, theme, cx),
            Route::Audio => audio_settings(view, state, theme, cx),
            Route::DevSpace => dev_space(state, theme, cx),
            Route::Shortcuts => shortcut_settings(theme, cx),
            _ => app_settings(state, theme, cx),
        }
    };

    div()
        .flex()
        .flex_col()
        .w_full()
        .min_w(px(0.0))
        .gap(px(24.0))
        .child(
            div()
                .flex()
                .flex_col()
                .w_full()
                .items_start()
                .gap(px(18.0))
                .child(page_header(
                    theme,
                    active_route.title(),
                    active_route.description(),
                ))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .w_full()
                        .child(
                            div()
                                .w(px(420.0))
                                .max_w_full()
                                .flex_shrink_0()
                                .children(view.settings_search_field()),
                        )
                        .children((!query.is_empty()).then(|| {
                            action(
                                "clear-settings-search",
                                theme,
                                "Clear",
                                cx,
                                |view: &mut crate::ui::dashboard::view::DashboardView, cx, _| {
                                    view.clear_settings_search(cx)
                                },
                            )
                        })),
                )
                .child(tab_bar),
        )
        .child(crate::ui::widgets::motion::reveal(
            body,
            ("settings-body", active_route as usize),
            state.settings.reduce_motion,
            false,
        ))
        .into_any_element()
}

/// A quiet hint used where a page has nothing to show yet.
pub fn hint(theme: &Theme, message: &str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .text_size(scaled(11.5))
        .text_color(theme.muted_foreground)
        .child(icons::icon(theme, Icon::Info, 12.0))
        .child(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_chat_key_is_selected_and_reused_without_pasting_it_again() {
        use crate::core::credentials::{self, Credential, Kind};
        let path =
            std::env::temp_dir().join(format!("lennette-saved-model-key-{}", uuid::Uuid::new_v4()));
        let vault = crate::core::vault::Vault::open(&path).unwrap();
        let mut saved = Credential::chat("merge-gateway", " saved-api-key ");
        saved.model = "my-custom-model".into();
        saved.base_url = "http://127.0.0.1:1234/v1/chat/completions".into();
        credentials::save(&vault, &saved).unwrap();
        let mut state = SettingsState::with_vault(None, Some(vault));
        assert_eq!(state.key_provider, "merge-gateway");
        assert!(state.key_input.is_empty());
        assert_eq!(state.key_model, "my-custom-model");
        let config = state.model_discovery_config().unwrap();
        assert_eq!(config.api_key, "saved-api-key");
        assert_eq!(config.base_url, saved.base_url);
        assert!(state.saved_credential().is_some());
        state.key_input = " replacement-key ".into();
        assert_eq!(
            state.model_discovery_config().unwrap().api_key,
            "replacement-key"
        );
        assert_eq!(
            credentials::load(state.vault.as_ref().unwrap(), Kind::Chat, "merge-gateway")
                .unwrap()
                .unwrap()
                .api_key,
            saved.api_key
        );
        // A key for one service must never be reused for a different service.
        state.key_input.clear();
        state.key_provider = "gemini".into();
        assert!(state.model_discovery_config().is_err());
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn shortcut_catalog_is_complete() {
        let shortcuts = shortcut_catalog();
        assert_eq!(shortcuts.len(), 6);
        for (keys, label) in shortcuts {
            assert!(!label.is_empty());
            assert!(keys.contains('+') || keys.contains('\\'));
        }
    }

    #[test]
    fn state_loads_defaults_without_a_database() {
        let state = SettingsState::new(None);
        assert_eq!(state.settings.theme, ThemeChoice::Dark);
        assert!(state.error.is_none());
    }

    #[test]
    fn save_without_a_database_reports_an_error() {
        let mut state = SettingsState::new(None);
        state.save();
        assert!(state.error.is_some());
    }
}
