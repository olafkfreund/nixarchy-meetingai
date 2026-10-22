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
}

pub const KEY_PROVIDERS: &[(&str, &str)] = &[
    ("Gemini", "gemini"),
    ("Merge Gateway", "merge-gateway"),
    ("Deepgram", "deepgram-stt"),
];

/// Settings state shared by every settings page.
pub struct SettingsState {
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
        let settings = database
            .as_ref()
            .and_then(|database| Settings::load(database).ok())
            .unwrap_or_default();
        let vault = crate::core::vault::Vault::open_default().ok();
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
        let key_model = stored_chat
            .iter()
            .find(|credential| credential.provider == "gemini")
            .map(|credential| credential.to_config().model)
            .unwrap_or_else(|| crate::ai::models::default_for("gemini").unwrap().to_owned());
        Self {
            baseline: settings.clone(),
            settings,
            database,
            error: None,
            notice: None,
            key_input: String::new(),
            key_provider: crate::ai::providers::CHAT_PROVIDERS[0].id.to_owned(),
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
                let enabled = view.settings_mut().settings.autostart;
                view.settings_mut().settings.autostart = !enabled;
                view.save_settings();
            },
        ),
    ))
    .child(row(
        theme,
        "Always on top",
        "Keep the overlay above other windows",
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
    ))
    .child(row(
        theme,
        "Hide app icon",
        "Keep the window out of the dock or taskbar",
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
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<crate::ui::dashboard::view::DashboardView>,
) -> gpui::AnyElement {
    let s = &state.settings;
    let devices = crate::audio::devices::enumerate();
    let inputs = devices.inputs.clone();
    let outputs = devices.outputs.clone();
    let inputs_empty = inputs.is_empty();
    let outputs_empty = outputs.is_empty();

    let microphone = section(
        theme,
        "Microphone",
        "The input used for push to talk and Listen.",
    )
    .child(row(
        theme,
        "Input device",
        "Leave blank to use the system default",
        value_chip(
            theme,
            if s.input_device.is_empty() {
                "System default"
            } else {
                &s.input_device
            },
        ),
    ));

    let mut microphone = microphone;
    for (index, device) in inputs.iter().take(8).enumerate() {
        microphone = microphone.child(row(
            theme,
            &device.name,
            if device.is_default {
                "Default device"
            } else {
                "Available input"
            },
            action(
                ("pick-input", index),
                theme,
                if s.input_device == device.id {
                    "Selected"
                } else {
                    "Use"
                },
                cx,
                {
                    let id = device.id.clone();
                    move |view: &mut crate::ui::dashboard::view::DashboardView, _, _| {
                        view.settings_mut().settings.input_device = id.clone();
                        view.save_settings();
                    }
                },
            ),
        ));
    }
    if inputs_empty {
        microphone = microphone.child(warning(
            theme,
            "No microphone was detected. Connect one or check your system audio settings.",
        ));
    }

    let mut speakers = section(
        theme,
        "System Audio",
        "The output whose sound is captured for the other side of a call.",
    );
    for (index, device) in outputs.iter().take(8).enumerate() {
        speakers = speakers.child(row(
            theme,
            &device.name,
            if device.is_default {
                "Default device"
            } else {
                "Available output"
            },
            action(
                ("pick-output", index),
                theme,
                if s.output_device == device.id {
                    "Selected"
                } else {
                    "Use"
                },
                cx,
                {
                    let id = device.id.clone();
                    move |view: &mut crate::ui::dashboard::view::DashboardView, _, _| {
                        view.settings_mut().settings.output_device = id.clone();
                        view.save_settings();
                    }
                },
            ),
        ));
    }
    if outputs_empty {
        speakers = speakers.child(warning(
            theme,
            "No output device was detected, so system audio capture is unavailable.",
        ));
    }

    let mut providers = section(
        theme,
        "Speech to Text",
        "Deepgram handles speech. Add its key in AI & Keys to enable recording.",
    );
    for provider in crate::ai::providers::STT_PROVIDERS {
        providers = providers.child(row(
            theme,
            provider.name,
            if s.speech_provider == provider.id {
                "Selected provider"
            } else if provider.default_model.is_empty() {
                "Requires a project id"
            } else {
                provider.default_model
            },
            action(
                gpui::ElementId::Name(format!("pick-stt-{}", provider.id).into()),
                theme,
                if s.speech_provider == provider.id {
                    "Selected"
                } else {
                    "Choose"
                },
                cx,
                move |view: &mut crate::ui::dashboard::view::DashboardView, _, _| {
                    view.settings_mut().settings.speech_provider = provider.id.to_owned();
                    view.settings_mut().settings.speech_model = provider.default_model.to_owned();
                    view.save_settings();
                },
            ),
        ));
    }

    stack(vec![
        microphone.into_any_element(),
        speakers.into_any_element(),
        providers.into_any_element(),
    ])
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
    stack(vec![
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
fn credentials_section(
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<crate::ui::dashboard::view::DashboardView>,
) -> impl IntoElement {
    use crate::core::credentials::Kind;

    let mut card = section(
        theme,
        "Provider Keys",
        "Stored encrypted on this computer. Keys are only sent to the provider you choose.",
    );

    let choices: Vec<(&str, bool)> = KEY_PROVIDERS
        .iter()
        .map(|(name, id)| (*name, *id == state.key_provider))
        .collect();
    card = card.child(row(
        theme,
        "Service",
        "Connect AI answers and speech transcription",
        segmented(
            "cred-provider",
            theme,
            &choices,
            cx,
            |view: &mut crate::ui::dashboard::view::DashboardView, index, _| {
                let id = KEY_PROVIDERS[index].1;
                view.reset_credential_models();
                let state = view.settings_mut();
                state.key_provider = id.to_owned();
                state.key_input.clear();
                state.key_field = None;
                state.key_status = None;
                state.key_model = state
                    .stored_chat
                    .iter()
                    .chain(state.stored_speech.iter())
                    .find(|credential| credential.provider == id)
                    .map(|credential| credential.to_config().model)
                    .unwrap_or_else(|| {
                        crate::ai::models::default_for(id)
                            .or_else(|| {
                                crate::ai::providers::SttProvider::by_id(id)
                                    .map(|p| p.default_model)
                            })
                            .unwrap_or_default()
                            .to_owned()
                    });
            },
        ),
    ));

    for (id, label, field, shown) in [
        (
            "credential-key",
            "API key",
            CredentialField::Key,
            if state.key_input.is_empty() {
                "Click to paste your API key".to_owned()
            } else {
                "•".repeat(state.key_input.chars().count().min(40))
            },
        ),
        (
            "credential-model",
            "Model",
            CredentialField::Model,
            state.key_model.clone(),
        ),
    ] {
        let focused = state.key_field == Some(field);
        let chat_model = field == CredentialField::Model
            && crate::ai::providers::ChatProvider::by_id(&state.key_provider).is_some();
        card = card.child(
            div()
                .flex()
                .flex_col()
                .w_full()
                .max_w(px(480.0))
                .gap(px(5.0))
                .py(px(space::SM))
                .child(
                    div()
                        .text_size(scaled(12.0))
                        .text_color(theme.foreground)
                        .child(label),
                )
                .child(
                    div()
                        .id(id)
                        .h(px(36.0))
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .rounded(px(radius::LG))
                        .border_1()
                        .border_color(if focused {
                            theme.foreground
                        } else {
                            theme.border
                        })
                        .bg(theme.input)
                        .px(px(space::SM))
                        .text_size(scaled(12.0))
                        .text_color(theme.foreground)
                        .cursor_text()
                        .child(div().flex_1().min_w(px(0.0)).truncate().child(shown))
                        .child(if focused { "│" } else { "" })
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |view, _, window, cx| {
                                if chat_model {
                                    view.toggle_credential_models(false, cx);
                                } else {
                                    view.focus_credential_field(field, window, cx);
                                }
                            }),
                        ),
                ),
        );
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
                    view.focus_credential_field(CredentialField::Model, window, cx);
                },
            ));
            card = card.child(dropdown);
        }
    }
    card = card.child(div().flex().items_center().py(px(8.0)).child(action(
        "save-key",
        theme,
        "Save key",
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

    // Stored credentials, with the key masked.
    for credential in state.stored_chat.iter().chain(state.stored_speech.iter()) {
        let label = if credential.kind == Kind::Chat {
            "Chat"
        } else {
            "Speech"
        };
        let provider = credential.provider.clone();
        let kind = credential.kind;
        card = card.child(row(
            theme,
            &format!("{} · {}", provider, label),
            &credential.masked_key(),
            div().flex().items_center().gap(px(6.0)).child(action(
                gpui::ElementId::Name(format!("del-{provider}").into()),
                theme,
                "Remove",
                cx,
                move |view: &mut crate::ui::dashboard::view::DashboardView, _, _| {
                    view.remove_credential(kind, &provider);
                },
            )),
        ));
    }

    if state.stored_chat.is_empty() && state.stored_speech.is_empty() {
        card = card.child(
            div()
                .text_size(scaled(11.0))
                .text_color(theme.muted_foreground)
                .child(
                    "No keys stored yet. Chat and transcription stay disabled until you add one.",
                ),
        );
    }

    card
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
        credentials_section(state, theme, cx).into_any_element(),
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
        ("Profiles", Route::Profiles),
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
            .text_color(if is_active {
                theme.foreground
            } else {
                surfaces.muted_foreground()
            })
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

    let body: gpui::AnyElement = match active_route {
        Route::Settings => {
            let app = app_settings(state, theme, cx);
            let screenshots = screenshot_settings(state, theme, cx);
            let responses = response_settings(view, state, theme, cx);
            stack(vec![app, screenshots, responses])
        }
        Route::Profiles => view.profiles_body(cx),
        Route::Audio => audio_settings(state, theme, cx),
        Route::DevSpace => dev_space(state, theme, cx),
        Route::Shortcuts => shortcut_settings(theme, cx),
        _ => app_settings(state, theme, cx),
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
                .items_start()
                .gap(px(18.0))
                .child(page_header(
                    theme,
                    active_route.title(),
                    active_route.description(),
                ))
                .child(tab_bar),
        )
        .child(body)
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
