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
        let mut card = section(
            theme,
            if speech {
                "Speech providers"
            } else {
                "AI providers"
            },
            if speech {
                "Create and manage speech endpoints, authentication, upload formats and live transcription."
            } else {
                "Create and manage AI endpoints, authentication and response formats. Chat requests use the OpenAI messages format."
            },
        );
        card = card.child(action(
            if speech {
                "new-custom-stt"
            } else {
                "new-custom-ai"
            },
            theme,
            "Add custom provider",
            cx,
            move |view: &mut Dashboard, _, _| view.edit_custom_provider(None, speech),
        ));
        let credentials = if speech {
            &state.stored_speech
        } else {
            &state.stored_chat
        };
        for credential in credentials {
            let label = credential
                .custom
                .as_ref()
                .map(|c| c.name.clone())
                .unwrap_or_else(|| credential.provider.clone());
            let id = credential.provider.clone();
            let model = credential.to_config().model;
            let selected = if speech {
                state.settings.speech_provider == id
            } else {
                state.settings.chat_provider == id
            };
            let mut buttons = div().flex().gap(px(6.0));
            buttons = buttons.child(action(
                gpui::ElementId::Name(format!("choose-{id}").into()),
                theme,
                if selected { "Selected" } else { "Choose" },
                cx,
                move |view: &mut Dashboard, _, _| {
                    let state = view.settings_mut();
                    if speech {
                        state.settings.speech_provider = id.clone();
                        state.settings.speech_model = model.clone();
                    } else {
                        state.settings.chat_provider = id.clone();
                    }
                    view.save_settings();
                },
            ));
            if credential.custom.is_some() {
                let draft = credential.clone();
                buttons = buttons.child(action(
                    gpui::ElementId::Name(format!("edit-{}", credential.provider).into()),
                    theme,
                    "Edit",
                    cx,
                    move |view: &mut Dashboard, _, _| {
                        view.edit_custom_provider(Some(draft.clone()), speech)
                    },
                ));
                let id = credential.provider.clone();
                buttons = buttons.child(action(
                    gpui::ElementId::Name(format!("remove-{id}").into()),
                    theme,
                    "Remove",
                    cx,
                    move |view: &mut Dashboard, _, _| {
                        view.remove_credential(if speech { Kind::Speech } else { Kind::Chat }, &id)
                    },
                ));
            }
            card = card.child(row(theme, &label, &credential.to_config().model, buttons));
        }
        sections.push(card.into_any_element());
    }
    stack(sections)
}

pub fn dialog(
    state: &SettingsState,
    theme: &Theme,
    cx: &mut Context<Dashboard>,
) -> Option<gpui::AnyElement> {
    if let Some(draft) = &state.custom_draft {
        let custom = draft.custom.as_ref().expect("custom provider draft");
        let speech = draft.kind == Kind::Speech;
        let mut form = section(
            theme,
            if speech {
                "Custom speech provider"
            } else {
                "Custom AI provider"
            },
            "Use the full request endpoint. Leave the key blank while editing to keep its saved value. Keys and custom settings stay encrypted.",
        );
        for (field, label, value) in [
            (CredentialField::CustomName, "Name", custom.name.clone()),
            (
                CredentialField::CustomEndpoint,
                "HTTP endpoint",
                draft.base_url.clone(),
            ),
            (
                CredentialField::CustomKey,
                "API key / auth value",
                if draft.api_key.is_empty() {
                    "Leave blank to keep a saved key".into()
                } else {
                    "•".repeat(draft.api_key.chars().count().min(40))
                },
            ),
            (
                CredentialField::CustomModel,
                "Model ID",
                draft.model.clone(),
            ),
            (
                CredentialField::CustomHeader,
                "Authentication header",
                custom.auth_header.clone(),
            ),
            (
                CredentialField::CustomPath,
                "Response JSON pointer",
                custom.response_path.clone(),
            ),
        ] {
            form = form.child(field_input(state, theme, field, label, value, cx));
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
                "Requires the OpenAI chat SSE protocol"
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
            form = form.child(field_input(
                state,
                theme,
                CredentialField::CustomStream,
                "WebSocket endpoint (ws:// or wss://)",
                custom.stream_url.clone(),
                cx,
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
        if let Some(status) = &state.key_status {
            form = form.child(help_text(theme, status));
        }
        return Some(form.into_any_element());
    }
    None
}

fn field_input(
    state: &SettingsState,
    theme: &Theme,
    field: CredentialField,
    label: &'static str,
    value: String,
    cx: &mut Context<Dashboard>,
) -> gpui::AnyElement {
    let focused = state.key_field == Some(field);
    div()
        .flex()
        .flex_col()
        .gap(px(5.0))
        .py(px(7.0))
        .child(div().text_size(scaled(12.0)).child(label))
        .child(
            div()
                .id(gpui::ElementId::Name(
                    format!("custom-field-{}", field as usize).into(),
                ))
                .min_h(px(36.0))
                .w_full()
                .px(px(10.0))
                .py(px(8.0))
                .rounded(px(radius::LG))
                .border_1()
                .border_color(if focused {
                    theme.focus_border()
                } else {
                    theme.control_border(false)
                })
                .bg(theme.input)
                .text_size(scaled(12.0))
                .text_color(theme.foreground)
                .cursor_text()
                .child(if value.is_empty() {
                    "Click to enter a value".to_owned()
                } else {
                    value
                })
                .child(if focused { "│" } else { "" })
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |view, _, window, cx| {
                        view.focus_credential_field(field, window, cx)
                    }),
                ),
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
            },
        ),
    )
    .into_any_element()
}
