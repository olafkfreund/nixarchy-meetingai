//! User-created AI and speech provider forms.
use super::*;
use crate::core::credentials::Kind;
type Dashboard = crate::ui::dashboard::view::DashboardView;

pub fn providers(
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<Dashboard>,
) -> gpui::AnyElement {
    let mut sections = Vec::new();
    for speech in [false, true] {
        let credentials = if speech {
            &state.stored_speech
        } else {
            &state.stored_chat
        };
        let mut card = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w(px(0.0))
            .gap(px(14.0))
            .p(px(18.0))
            .rounded(px(14.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme
                .surfaces(crate::features::theme::DEFAULT_OPACITY)
                .card())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(5.0))
                            .child(
                                div()
                                    .text_size(scaled(15.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(if speech {
                                        "Speech providers"
                                    } else {
                                        "AI providers"
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(scaled(11.0))
                                    .text_color(theme.muted_foreground)
                                    .child(if speech {
                                        "Transcription, microphone and meeting audio."
                                    } else {
                                        "Answers, models and screen context."
                                    }),
                            ),
                    )
                    .child(preset_trigger(state, theme, speech, cx))
                    .child(div().flex().flex_shrink_0().child(action(
                        if speech {
                            "new-custom-stt"
                        } else {
                            "new-custom-ai"
                        },
                        theme,
                        "+ Add custom provider",
                        cx,
                        move |view: &mut Dashboard, _, _| view.edit_custom_provider(None, speech),
                    ))),
            );
        for &(name, id) in KEY_PROVIDERS {
            let is_speech = crate::ai::providers::SttProvider::by_id(id).is_some();
            if is_speech != speech {
                continue;
            }
            let saved = credentials
                .iter()
                .find(|credential| credential.provider == id);
            card = card.child(provider_row(state, theme, name, id, saved, speech, cx));
        }
        for credential in credentials {
            if KEY_PROVIDERS
                .iter()
                .any(|(_, id)| *id == credential.provider)
            {
                continue;
            }
            let name = credential
                .custom
                .as_ref()
                .map(|c| c.name.as_str())
                .unwrap_or(&credential.provider);
            card = card.child(provider_row(
                state,
                theme,
                name,
                &credential.provider,
                Some(credential),
                speech,
                cx,
            ));
        }
        if !credentials
            .iter()
            .any(|credential| credential.custom.is_some())
        {
            card = card.child(div().text_size(scaled(11.0)).text_color(theme.muted_foreground)
                .child("Use Add custom provider to connect your own endpoint. You can fill the form or paste JSON."));
        }
        sections.push(card.into_any_element());
    }
    let mut body = div().flex().flex_col().gap(px(18.0))
        .child(div().flex().items_center().gap(px(8.0))
            .child(icons::icon(theme, Icon::LockKeyhole, 14.0))
            .child(div().text_size(scaled(11.0)).text_color(theme.muted_foreground)
                .child("Keys stay encrypted on this computer. Manage a provider to replace its key or model.")))
        .children(sections);
    if let Some(status) = &state.key_status {
        body = body.child(help_text(theme, status));
    }
    body.children(preset_menu(state, theme, cx))
        .into_any_element()
}

fn preset_trigger(
    state: &SettingsState,
    theme: &Theme,
    speech: bool,
    cx: &mut Context<Dashboard>,
) -> gpui::AnyElement {
    let bounds = state.provider_menu_bounds[speech as usize].clone();
    let open = state.provider_menu == Some(speech);
    div()
        .id(if speech {
            "speech-provider-more"
        } else {
            "ai-provider-more"
        })
        .relative()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(5.0))
        .h(px(28.0))
        .px(px(9.0))
        .rounded(px(8.0))
        .bg(theme.input)
        .border_1()
        .border_color(theme.control_border(open))
        .cursor_pointer()
        .text_size(scaled(11.0))
        .child("More")
        .child(icons::icon(theme, Icon::ChevronDown, 12.0))
        .on_click(cx.listener(move |view, _, _, cx| {
            view.settings_mut().provider_menu = if open { None } else { Some(speech) };
            cx.notify();
        }))
        .child(
            gpui::canvas(|_, _, _| (), move |value, _, _, _| bounds.set(value))
                .absolute()
                .inset_0(),
        )
        .into_any_element()
}

fn preset_menu(
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<Dashboard>,
) -> Option<gpui::AnyElement> {
    let speech = state.provider_menu?;
    let bounds = state.provider_menu_bounds[speech as usize].clone();
    let mut menu = div()
        .id("popular-provider-menu")
        .flex()
        .flex_col()
        .gap(px(3.0))
        .p(px(6.0))
        .w(px(220.0))
        .max_h(px(300.0))
        .overflow_y_scroll()
        .rounded(px(11.0))
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover())
        .shadow_md()
        .occlude()
        .on_mouse_down_out(
            cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                if !bounds.get().contains(&event.position) {
                    view.settings_mut().provider_menu = None;
                    cx.notify();
                }
            }),
        )
        .child(
            div()
                .p(px(7.0))
                .text_size(scaled(10.0))
                .text_color(theme.muted_foreground)
                .child("Connect a popular service"),
        );
    for (index, preset) in crate::ai::catalog::presets::PRESETS
        .iter()
        .enumerate()
        .filter(|(_, preset)| preset.speech == speech)
    {
        menu = menu.child(
            div()
                .id(("provider-preset", index))
                .flex()
                .items_center()
                .flex_shrink_0()
                .h(px(30.0))
                .px(px(8.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .text_size(scaled(12.0))
                .text_color(theme.foreground)
                .hover(|style| style.bg(theme.menu_hover()))
                .child(preset.name)
                .on_click(cx.listener(move |view, _, _, cx| {
                    let preset = &crate::ai::catalog::presets::PRESETS[index];
                    view.edit_custom_provider(Some(preset.draft()), speech);
                    view.settings_mut().provider_docs = Some(preset.docs);
                    cx.notify();
                })),
        );
    }
    let bounds = state.provider_menu_bounds[speech as usize].get();
    Some(
        gpui::deferred(
            gpui::anchored()
                .position(gpui::point(bounds.left(), bounds.bottom() + px(5.0)))
                .snap_to_window_with_margin(px(8.0))
                .child(crate::ui::widgets::motion::reveal(
                    menu,
                    "popular-provider-menu-open",
                    state.settings.reduce_motion,
                    true,
                )),
        )
        .with_priority(20)
        .into_any_element(),
    )
}

fn provider_row(
    state: &SettingsState,
    theme: &Theme,
    name: &str,
    id: &str,
    credential: Option<&crate::core::credentials::Credential>,
    speech: bool,
    cx: &mut Context<Dashboard>,
) -> gpui::AnyElement {
    let selected = if speech {
        state.settings.speech_provider == id
    } else {
        state.settings.chat_provider == id
    };
    let connected = credential.is_some();
    let mut buttons = div().flex().flex_shrink_0().items_center().gap(px(6.0));
    if let Some(credential) = credential {
        let chosen = credential.clone();
        buttons = buttons.child(action(
            gpui::ElementId::Name(format!("choose-{id}").into()),
            theme,
            if selected { "Selected" } else { "Use provider" },
            cx,
            move |view: &mut Dashboard, _, _| {
                if speech {
                    view.settings_mut().settings.speech_provider = chosen.provider.clone();
                    view.settings_mut().settings.speech_model = chosen.to_config().model;
                } else {
                    view.settings_mut().settings.chat_provider = chosen.provider.clone();
                }
                view.save_settings();
            },
        ));
    }
    let draft = credential.cloned();
    let provider = id.to_owned();
    buttons = buttons.child(action(
        gpui::ElementId::Name(format!("manage-{id}").into()),
        theme,
        if connected { "Manage" } else { "Connect" },
        cx,
        move |view: &mut Dashboard, _, _| {
            if draft.as_ref().is_some_and(|draft| draft.custom.is_some()) {
                view.edit_custom_provider(draft.clone(), speech);
            } else {
                view.edit_builtin_provider(&provider);
            }
        },
    ));
    let detail = credential
        .map(|c| {
            let model = c.to_config().model;
            format!(
                "Connected  ·  {}  ·  {}",
                if model.is_empty() {
                    "Default model"
                } else {
                    &model
                },
                if c.api_key.is_empty() {
                    "No key required".into()
                } else {
                    c.masked_key()
                }
            )
        })
        .unwrap_or_else(|| "Not connected".into());
    div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .w_full()
        .min_w(px(0.0))
        .p(px(12.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(theme.control_border(false))
        .bg(theme.input)
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(5.0))
                .child(
                    div()
                        .text_size(scaled(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .truncate()
                        .child(name.to_owned()),
                )
                .child(
                    div()
                        .text_size(scaled(10.5))
                        .text_color(theme.muted_foreground)
                        .truncate()
                        .child(detail),
                ),
        )
        .child(buttons)
        .into_any_element()
}

/// Removal lives inside the editor, keeping the provider list focused on selection.
pub(super) fn remove_button(
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<Dashboard>,
) -> Option<gpui::AnyElement> {
    let draft = state.custom_draft.as_ref();
    let id = draft
        .map(|d| d.provider.clone())
        .unwrap_or_else(|| state.key_provider.clone());
    let saved = state
        .stored_chat
        .iter()
        .chain(state.stored_speech.iter())
        .find(|c| c.provider == id)?;
    let kind = saved.kind;
    Some(
        action(
            "disconnect-provider",
            theme,
            "Remove provider",
            cx,
            move |view: &mut Dashboard, _, _| {
                view.remove_credential(kind, &id);
                view.settings_mut().custom_open = false;
                view.settings_mut().key_field = None;
            },
        )
        .into_any_element(),
    )
}

pub(super) fn editor_header(theme: &Theme, title: &str, description: &str) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .w_full()
        .min_w(px(0.0))
        .gap(px(12.0))
        .child(
            div()
                .text_size(scaled(17.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_owned()),
        )
        .child(help_text(theme, description))
}

pub fn dialog(
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<Dashboard>,
) -> Option<gpui::AnyElement> {
    if let Some(draft) = &state.custom_draft {
        let custom = draft.custom.as_ref().expect("custom provider draft");
        let speech = draft.kind == Kind::Speech;
        let mut form = editor_header(
            theme,
            if speech {
                "Custom speech provider"
            } else {
                "Custom AI provider"
            },
            "Use the full request endpoint. Leave the key blank while editing to keep its saved value. Keys and custom settings stay encrypted.",
        );
        if let Some(url) = state.provider_docs {
            form = form.child(div().flex().child(action(
                "preset-docs",
                theme,
                "Provider documentation",
                cx,
                move |_: &mut Dashboard, cx, _| cx.open_url(url),
            )));
        }
        form = form.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(segmented(
                    "provider-entry-mode",
                    theme,
                    &[
                        ("Form", !state.custom_json_mode),
                        ("Paste JSON", state.custom_json_mode),
                    ],
                    cx,
                    |view: &mut Dashboard, index, _| {
                        view.settings_mut().custom_json_mode = index == 1;
                        view.settings_mut().key_field = None;
                    },
                ))
                .child(action(
                    "provider-json-info",
                    theme,
                    "i  JSON guide",
                    cx,
                    |view: &mut Dashboard, _, _| {
                        view.settings_mut().custom_guide = !view.settings_mut().custom_guide;
                    },
                )),
        );
        if state.custom_guide {
            form = form.child(div().flex().flex_col().gap(px(8.0)).p(px(12.0))
                .rounded(px(10.0)).bg(theme.input)
                .child(help_text(theme, "Paste one JSON object using kind, name, endpoint, api_key and model. The guide includes every supported field, formats and a ready-to-fill template for your AI agent. Apply JSON fills the form for review; Save stores the provider."))
                .child(div().flex().child(action("copy-provider-json-guide", theme, "Copy guide + template", cx,
                    move |view: &mut Dashboard, cx, _| {
                        view.settings_mut().key_status = Some("Guide and template copied.".into());
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(crate::core::security::provider_import::guide(speech)));
                    }))));
        }
        if state.custom_json_mode {
            form = form.child(help_text(theme, "Paste your configuration, then apply it to review the fields. Leave api_key blank to keep a saved key."))
                .children(state.custom_json.clone())
                .child(div().flex().gap(px(8.0))
                    .child(action("apply-provider-json", theme, "Apply JSON", cx,
                        |view: &mut Dashboard, cx, _| view.apply_provider_json(cx)))
                    .child(action("cancel-provider-json", theme, "Cancel", cx,
                        |view: &mut Dashboard, _, _| view.settings_mut().custom_open = false)));
            if let Some(status) = &state.key_status {
                form = form.child(help_text(theme, status));
            }
            return Some(form.into_any_element());
        }
        for (field, label) in [
            (CredentialField::CustomName, "Name"),
            (CredentialField::CustomEndpoint, "HTTP endpoint"),
            (CredentialField::CustomKey, "API key / auth value"),
            (CredentialField::CustomModel, "Model ID"),
            (CredentialField::CustomHeader, "Authentication header"),
            (CredentialField::CustomPath, "Response JSON pointer"),
        ] {
            form = form.child(native_field(state, theme, field, label));
        }
        form = form.child(choice(
            theme,
            "Request method",
            &custom.method,
            &["POST", "PUT", "PATCH"],
            "custom-method",
            cx,
            |c, value| c.method = value,
        ));
        form = form.child(choice(
            theme,
            "Authentication",
            &custom.auth,
            &["none", "bearer", "token", "header"],
            "custom-auth",
            cx,
            |c, value| c.auth = value,
        ));
        if !speech {
            form = form.child(choice(
                theme,
                "Chat request format",
                &custom.request_format,
                &["openai", "anthropic"],
                "custom-wire",
                cx,
                |custom, value| {
                    custom.request_format = value;
                    custom.response_path = if custom.request_format == "anthropic" {
                        "/content/0/text"
                    } else {
                        "/choices/0/message/content"
                    }
                    .into();
                },
            ));
        }
        if speech {
            form = form.child(choice(
                theme,
                "Audio upload",
                &custom.request_format,
                &["raw-wav", "multipart-wav", "json-wav"],
                "custom-upload",
                cx,
                |c, value| c.request_format = value,
            ));
            form = form.child(help_text(theme, "Raw WAV uploads audio/wav. Multipart sends file and model fields. JSON sends base64 audio, model and language fields."));
        }
        form = form.child(choice(
            theme,
            "Response format",
            &custom.response_format,
            &["json", "text"],
            "custom-response",
            cx,
            |c, value| c.response_format = value,
        ));
        form = form.child(row(
            theme,
            "Streaming",
            if speech {
                "Requires the Deepgram two-channel PCM WebSocket protocol"
            } else {
                if custom.request_format == "anthropic" {
                    "Uses Anthropic Messages SSE"
                } else {
                    "Uses OpenAI chat SSE"
                }
            },
            switch(
                "custom-streaming",
                theme,
                custom.streaming,
                cx,
                |view: &mut Dashboard, _, _| {
                    if let Some(custom) = view
                        .settings_mut()
                        .custom_draft
                        .as_mut()
                        .and_then(|d| d.custom.as_mut())
                    {
                        custom.streaming = !custom.streaming;
                    }
                },
            ),
        ));
        if speech && custom.streaming {
            form = form.child(native_field(
                state,
                theme,
                CredentialField::CustomStream,
                "WebSocket endpoint (ws:// or wss://)",
            ));
            form = form.child(help_text(theme, "Keep the HTTP endpoint too: push to talk uses it for finished WAV recordings. Live mode sends 16 kHz, 16-bit stereo PCM (microphone left, system audio right)."));
        }
        if !speech {
            form = form.child(row(
                theme,
                "Image context",
                "This endpoint accepts OpenAI image_url content",
                switch(
                    "custom-images",
                    theme,
                    custom.supports_images,
                    cx,
                    |view: &mut Dashboard, _, _| {
                        if let Some(custom) = view
                            .settings_mut()
                            .custom_draft
                            .as_mut()
                            .and_then(|d| d.custom.as_mut())
                        {
                            custom.supports_images = !custom.supports_images;
                        }
                    },
                ),
            ));
        }
        form = form.child(
            div()
                .flex()
                .gap(px(8.0))
                .child(action(
                    "save-custom-provider",
                    theme,
                    "Save provider",
                    cx,
                    |view: &mut Dashboard, _, _| view.save_custom_provider(),
                ))
                .child(action(
                    "cancel-custom-provider",
                    theme,
                    "Cancel",
                    cx,
                    |view: &mut Dashboard, _, _| {
                        view.settings_mut().custom_open = false;
                        view.settings_mut().key_field = None;
                    },
                )),
        );
        form = form.child(div().flex().children(remove_button(state, theme, cx)));
        if let Some(status) = &state.key_status {
            form = form.child(help_text(theme, status));
        }
        return Some(form.into_any_element());
    }
    None
}

pub(super) fn native_field(
    state: &SettingsState,
    theme: &Theme,
    field: CredentialField,
    label: &'static str,
) -> gpui::AnyElement {
    div()
        .flex()
        .flex_col()
        .w_full()
        .min_w(px(0.0))
        .gap(px(6.0))
        .py(px(4.0))
        .child(
            div()
                .text_size(scaled(12.0))
                .text_color(theme.foreground)
                .child(label),
        )
        .children(
            state
                .credential_editors
                .iter()
                .find(|(key, _)| *key == field)
                .map(|(_, editor)| editor.clone()),
        )
        .into_any_element()
}

fn choice(
    theme: &Theme,
    label: &'static str,
    selected: &str,
    values: &'static [&'static str],
    id: &'static str,
    cx: &mut Context<Dashboard>,
    set: fn(&mut crate::ai::catalog::custom::CustomProvider, String),
) -> gpui::AnyElement {
    let options: Vec<_> = values
        .iter()
        .map(|value| (*value, *value == selected))
        .collect();
    row(
        theme,
        label,
        "",
        segmented(
            id,
            theme,
            &options,
            cx,
            move |view: &mut Dashboard, index, _| {
                if let Some(custom) = view
                    .settings_mut()
                    .custom_draft
                    .as_mut()
                    .and_then(|d| d.custom.as_mut())
                {
                    set(custom, values[index].into());
                }
                if id == "custom-wire" {
                    view.settings_mut().credential_observers.clear();
                    view.settings_mut().credential_editors.clear();
                }
            },
        ),
    )
    .into_any_element()
}
