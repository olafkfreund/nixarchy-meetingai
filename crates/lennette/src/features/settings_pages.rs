//! Settings pages.
//!
//! Each function renders one settings route. Controls write through
//! [`SettingsState`], which persists immediately so a restart never loses a
//! change.

use gpui::{Context, FontWeight, IntoElement, ParentElement, Styled, div, px};
use lucide_icons::Icon;

use super::controls::{action, keycap, row, section, segmented, switch, value_chip, warning};
use super::dashboard::nav::Route;
use super::icons;
use super::settings::{AutoScreenshot, PrimaryTab, Settings, ThemeChoice};
use super::theme::{Theme, radius, space};

/// Shortcut catalog shown on the shortcuts page, derived from the real bindings
/// that the shortcut layer registers.
pub fn shortcut_catalog() -> Vec<(String, &'static str)> {
    crate::features::shortcuts::Binding::defaults()
        .into_iter()
        .map(|binding| (binding.display, binding.action.label()))
        .collect()
}

/// Settings state shared by every settings page.
pub struct SettingsState {
    pub settings: Settings,
    pub database: Option<super::db::Database>,
    /// Last error from a failed save, surfaced inline.
    pub error: Option<String>,
    /// Notice shown when a setting could not be applied on this platform.
    pub notice: Option<String>,
}

impl SettingsState {
    pub fn new(database: Option<super::db::Database>) -> Self {
        let settings = database
            .as_ref()
            .and_then(|database| Settings::load(database).ok())
            .unwrap_or_default();
        Self {
            settings,
            database,
            error: None,
            notice: None,
        }
    }

    /// Persist the current settings, recording any failure.
    pub fn save(&mut self) {
        let Some(database) = self.database.as_ref() else {
            self.error = Some("The local database is not available.".to_owned());
            return;
        };
        match self.settings.save(database) {
            Ok(()) => self.error = None,
            Err(error) => self.error = Some(format!("Could not save: {error}")),
        }
    }
}

fn page_header(theme: &Theme, title: &str, description: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .pt(px(20.0))
        .pb(px(14.0))
        .child(
            div()
                .text_size(px(17.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.foreground)
                .child(title.to_owned()),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(theme.muted_foreground)
                .child(description.to_owned()),
        )
}

fn stack(children: Vec<gpui::AnyElement>) -> impl IntoElement {
    let mut column = div().flex().flex_col().gap(px(12.0)).pb(px(24.0));
    for child in children {
        column = column.child(child);
    }
    column
}

/// App settings: theme, transparency, startup and window behaviour.
pub fn app_settings(
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<super::dashboard::view::DashboardView>,
) -> impl IntoElement {
    let s = &state.settings;

    let theme_options: Vec<(&'static str, bool)> = ThemeChoice::ALL
        .iter()
        .map(|choice| (choice.label(), *choice == s.theme))
        .collect();

    let mut appearance = section(
        theme,
        "Theme Customization",
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
            |view: &mut super::dashboard::view::DashboardView, index, _| {
                view.settings_mut().settings.theme = ThemeChoice::ALL[index];
                view.save_settings();
            },
        ),
    ))
    .child(row(
        theme,
        "Transparency",
        "Lower values let the desktop show through",
        value_chip(theme, &format!("{}%", s.transparency)),
    ));

    for (offset, step) in [(-10i32, "Less"), (10, "More")] {
        let _ = (offset, step);
    }
    appearance = appearance.child(
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .pb(px(space::SM))
            .child(action(
                "transparency-down",
                theme,
                "Less",
                cx,
                |view: &mut super::dashboard::view::DashboardView, _, _| {
                    let value = view.settings_mut().settings.transparency;
                    view.settings_mut().settings.transparency = value.saturating_sub(10);
                    view.save_settings();
                },
            ))
            .child(action(
                "transparency-up",
                theme,
                "More",
                cx,
                |view: &mut super::dashboard::view::DashboardView, _, _| {
                    let value = view.settings_mut().settings.transparency;
                    view.settings_mut().settings.transparency = (value + 10).min(100);
                    view.save_settings();
                },
            )),
    );

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
            |view: &mut super::dashboard::view::DashboardView, _, _| {
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
            |view: &mut super::dashboard::view::DashboardView, _, window| {
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
            |view: &mut super::dashboard::view::DashboardView, _, _| {
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
            |view: &mut super::dashboard::view::DashboardView, _, _| {
                let enabled = view.settings_mut().settings.reduce_motion;
                view.settings_mut().settings.reduce_motion = !enabled;
                view.save_settings();
            },
        ),
    ));

    let tab_options: Vec<(&'static str, bool)> = PrimaryTab::ALL
        .iter()
        .map(|tab| (tab.label(), *tab == s.primary_tab))
        .collect();

    let overlay = section(
        theme,
        "Overlay Tabs",
        "Which pane the overlay opens on. Changes apply immediately.",
    )
    .child(row(
        theme,
        "Opening tab",
        "The tab selected when the overlay appears",
        segmented(
            "primary-tab",
            theme,
            &tab_options,
            cx,
            |view: &mut super::dashboard::view::DashboardView, index, _| {
                view.settings_mut().settings.primary_tab = PrimaryTab::ALL[index];
                view.save_settings();
            },
        ),
    ));

    let mut children = vec![
        appearance.into_any_element(),
        startup.into_any_element(),
        overlay.into_any_element(),
    ];
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
    cx: &mut Context<super::dashboard::view::DashboardView>,
) -> impl IntoElement {
    let s = &state.settings;
    let modes: Vec<(&'static str, bool)> = AutoScreenshot::ALL
        .iter()
        .map(|mode| (mode.label(), *mode == s.auto_screenshot))
        .collect();

    let capture = section(
        theme,
        "Automatic Screenshots",
        "When a screenshot is taken without asking.",
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
            |view: &mut super::dashboard::view::DashboardView, index, _| {
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
            .child(action(
                "delay-down",
                theme,
                "−",
                cx,
                |view: &mut super::dashboard::view::DashboardView, _, _| {
                    let value = view.settings_mut().settings.capture_delay_seconds;
                    view.settings_mut().settings.capture_delay_seconds = value.saturating_sub(1);
                    view.save_settings();
                },
            ))
            .child(value_chip(theme, &format!("{}s", s.capture_delay_seconds)))
            .child(action(
                "delay-up",
                theme,
                "+",
                cx,
                |view: &mut super::dashboard::view::DashboardView, _, _| {
                    let value = view.settings_mut().settings.capture_delay_seconds;
                    view.settings_mut().settings.capture_delay_seconds = (value + 1).min(60);
                    view.save_settings();
                },
            )),
    ))
    .child(row(
        theme,
        "Interval",
        "Only used when the mode is On an interval",
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(action(
                "interval-down",
                theme,
                "−",
                cx,
                |view: &mut super::dashboard::view::DashboardView, _, _| {
                    let value = view.settings_mut().settings.screenshot_interval;
                    view.settings_mut().settings.screenshot_interval =
                        value.saturating_sub(5).max(5);
                    view.save_settings();
                },
            ))
            .child(value_chip(theme, &format!("{}s", s.screenshot_interval)))
            .child(action(
                "interval-up",
                theme,
                "+",
                cx,
                |view: &mut super::dashboard::view::DashboardView, _, _| {
                    let value = view.settings_mut().settings.screenshot_interval;
                    view.settings_mut().settings.screenshot_interval = (value + 5).min(600);
                    view.save_settings();
                },
            )),
    ))
    .child(row(
        theme,
        "Use image on send",
        "Attach a fresh screenshot to every message",
        switch(
            "use-image",
            theme,
            s.use_image_on_send,
            cx,
            |view: &mut super::dashboard::view::DashboardView, _, _| {
                let enabled = view.settings_mut().settings.use_image_on_send;
                view.settings_mut().settings.use_image_on_send = !enabled;
                view.save_settings();
            },
        ),
    ));

    stack(vec![capture.into_any_element()])
}

/// Region capture behaviour.
pub fn capture_settings(
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<super::dashboard::view::DashboardView>,
) -> impl IntoElement {
    let s = &state.settings;

    let regions = section(
        theme,
        "Region Capture",
        "Select an area once and reuse it for timed captures.",
    )
    .child(row(
        theme,
        "Automatic captures",
        "Take a capture without asking first",
        switch(
            "capture-auto",
            theme,
            s.auto_screenshot != AutoScreenshot::Off,
            cx,
            |view: &mut super::dashboard::view::DashboardView, _, _| {
                let current = view.settings_mut().settings.auto_screenshot;
                view.settings_mut().settings.auto_screenshot = if current == AutoScreenshot::Off {
                    AutoScreenshot::EveryQuestion
                } else {
                    AutoScreenshot::Off
                };
                view.save_settings();
            },
        ),
    ))
    .child(
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .py(px(space::SM))
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(theme.muted_foreground)
                    .child(
                        "Captures are scaled so the longest edge is at most 1920 pixels before \
                         they are sent.",
                    ),
            )
            .child(warning(
                theme,
                "Region selection needs a display server that allows window placement. On \
                 compositors that restrict it, selection may fail and the full screen is used.",
            )),
    );

    stack(vec![regions.into_any_element()])
}

/// Audio devices and speech providers.
pub fn audio_settings(
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<super::dashboard::view::DashboardView>,
) -> impl IntoElement {
    let s = &state.settings;
    let devices = super::devices::enumerate();
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
                    move |view: &mut super::dashboard::view::DashboardView, _, _| {
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
                    move |view: &mut super::dashboard::view::DashboardView, _, _| {
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
        "Bring your own speech provider. Keys are stored in the encrypted vault.",
    );
    for provider in super::providers::STT_PROVIDERS {
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
                move |view: &mut super::dashboard::view::DashboardView, _, _| {
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

/// Follow-up suggestions and answer style.
pub fn response_settings(
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<super::dashboard::view::DashboardView>,
) -> impl IntoElement {
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
            |view: &mut super::dashboard::view::DashboardView, _, _| {
                let enabled = view.settings_mut().settings.smart_follow_ups;
                view.settings_mut().settings.smart_follow_ups = !enabled;
                view.save_settings();
            },
        ),
    ))
    .child(
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .py(px(space::SM))
            .child(
                div()
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.foreground)
                    .child("Instruction"),
            )
            .child(
                div()
                    .rounded(px(radius::MD))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.muted)
                    .px(px(space::MD))
                    .py(px(space::SM))
                    .text_size(px(11.5))
                    .text_color(theme.muted_foreground)
                    .child(s.smart_follow_ups_prompt.clone()),
            ),
    );

    stack(vec![follow_ups.into_any_element()])
}

/// Keyboard shortcuts.
pub fn shortcut_settings(
    theme: &Theme,
    _cx: &mut Context<super::dashboard::view::DashboardView>,
) -> impl IntoElement {
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

/// Developer space: local CLI detection.
pub fn dev_space(
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<super::dashboard::view::DashboardView>,
) -> impl IntoElement {
    let detected = super::cli::detect();
    let s = &state.settings;

    let mut clis = section(
        theme,
        "Local AI CLIs",
        "Drive an assistant you already have installed instead of a hosted model.",
    );
    for spec in super::cli::CLIS {
        let found = detected.get(spec.id).cloned();
        let selected = s.cli_connection == spec.id;
        let description = found
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Not found on PATH".to_owned());
        let control: gpui::AnyElement = match found {
            Some(_) => action(
                gpui::ElementId::Name(format!("pick-cli-{}", spec.id).into()),
                theme,
                if selected { "Connected" } else { "Connect" },
                cx,
                move |view: &mut super::dashboard::view::DashboardView, _, _| {
                    view.settings_mut().settings.cli_connection = spec.id.to_owned();
                    view.save_settings();
                },
            )
            .into_any_element(),
            None => value_chip(theme, "Not installed").into_any_element(),
        };
        clis = clis.child(row(theme, spec.name, &description, control));
    }

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

    stack(vec![clis.into_any_element(), storage.into_any_element()])
}

/// Render the settings body for a route.
pub fn render(
    route: Route,
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<super::dashboard::view::DashboardView>,
) -> gpui::AnyElement {
    let (title, description) = (route.title(), route.description());
    let body: gpui::AnyElement = match route {
        Route::Settings => app_settings(state, theme, cx).into_any_element(),
        Route::Screenshot => screenshot_settings(state, theme, cx).into_any_element(),
        Route::Capture => capture_settings(state, theme, cx).into_any_element(),
        Route::Audio => audio_settings(state, theme, cx).into_any_element(),
        Route::Responses => response_settings(state, theme, cx).into_any_element(),
        Route::Shortcuts => shortcut_settings(theme, cx).into_any_element(),
        Route::DevSpace => dev_space(state, theme, cx).into_any_element(),
        _ => div().into_any_element(),
    };

    div()
        .flex()
        .flex_col()
        .child(page_header(theme, title, description))
        .child(body)
        .into_any_element()
}

/// A quiet hint used where a page has nothing to show yet.
pub fn hint(theme: &Theme, message: &str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .text_size(px(11.5))
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
