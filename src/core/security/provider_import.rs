//! Reviewable custom-provider imports. Parsing never writes a credential or sends a request.
use super::credentials::{Credential, Kind};
use crate::ai::catalog::custom::CustomProvider;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Import {
    kind: Kind,
    name: String,
    endpoint: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    model: String,
    method: Option<String>,
    auth: Option<String>,
    auth_header: Option<String>,
    request_format: Option<String>,
    response_format: Option<String>,
    response_path: Option<String>,
    streaming: Option<bool>,
    stream_url: Option<String>,
    supports_images: Option<bool>,
}

pub fn parse(text: &str, draft: &Credential) -> Result<Credential, String> {
    if text.len() > 65_536 {
        return Err("Keep provider JSON below 64 KB.".into());
    }
    let imported: Import = serde_json::from_str(text).map_err(|_| {
        "Invalid provider JSON. Check the guide, field names and value types.".to_owned()
    })?;
    if imported.kind != draft.kind || draft.custom.is_none() {
        return Err("Use the matching AI or speech provider editor for this JSON.".into());
    }
    let speech = imported.kind == Kind::Speech;
    let mut custom = CustomProvider {
        name: imported.name,
        request_format: if speech { "multipart-wav" } else { "openai" }.into(),
        response_path: if speech {
            "/text"
        } else {
            "/choices/0/message/content"
        }
        .into(),
        supports_images: !speech,
        ..Default::default()
    };
    macro_rules! apply {
        ($($field:ident),+) => { $(if let Some(value) = imported.$field { custom.$field = value; })+ };
    }
    apply!(
        method,
        auth,
        auth_header,
        request_format,
        response_format,
        response_path,
        streaming,
        stream_url,
        supports_images
    );
    let mut result = draft.clone();
    result.custom = Some(custom);
    result.base_url = imported.endpoint.trim().into();
    result.model = imported.model.trim().into();
    if !imported.api_key.trim().is_empty() {
        result.api_key = imported.api_key.trim().into();
    }
    // A missing secret can be entered after review, or reused from the encrypted vault on Save.
    let mut validation = result.clone();
    if validation.api_key.is_empty() {
        validation.api_key = "pending-key".into();
    }
    validation.validate()?;
    Ok(result)
}

pub fn guide(speech: bool) -> String {
    let template = serde_json::json!({
        "kind": if speech { "speech" } else { "chat" },
        "name": "My provider",
        "endpoint": if speech { "https://example.com/v1/audio/transcriptions" } else { "https://example.com/v1/chat/completions" },
        "api_key": "",
        "model": "model-id",
        "method": "POST",
        "auth": "bearer",
        "auth_header": "Authorization",
        "request_format": if speech { "multipart-wav" } else { "openai" },
        "response_format": "json",
        "response_path": if speech { "/text" } else { "/choices/0/message/content" },
        "streaming": false,
        "stream_url": "",
        "supports_images": !speech
    });
    format!(
        "Fill in this nixarchy-meetingai custom-provider JSON using the provider's official API documentation. Return one JSON object without Markdown fences. Do not invent endpoints or protocol support. Leave api_key blank; the user can enter it privately in nixarchy-meetingai.\n\nRequired: kind, name, endpoint; chat also needs model. Allowed methods: POST, PUT, PATCH. Auth: none, bearer, token, header (verbatim key in auth_header). Request format: openai or anthropic for chat; raw-wav, multipart-wav or json-wav for speech. Response: json with a JSON pointer to text, or text. Chat streaming requires OpenAI SSE and /choices/0/message/content, or native Anthropic SSE and /content/0/text. Speech streaming requires a Deepgram-compatible two-channel PCM WebSocket stream_url (ws/wss). Do not enable streaming for other protocols. Blank keys preserve an existing key. No other fields are accepted. Applying JSON only fills the form; Save persists it.\n\n{}",
        serde_json::to_string_pretty(&template).expect("static JSON")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn draft(speech: bool) -> Credential {
        let mut credential = if speech {
            Credential::speech("custom-test", "")
        } else {
            Credential::chat("custom-test", "")
        };
        credential.custom = Some(CustomProvider::default());
        credential
    }
    #[test]
    fn imports_chat_for_review_and_preserves_identity_and_secret() {
        let mut draft = draft(false);
        draft.api_key = "existing-secret".into();
        let imported = parse(
            r#"{"kind":"chat","name":"Example","endpoint":"https://example.com/chat","model":"m"}"#,
            &draft,
        )
        .unwrap();
        assert_eq!(imported.provider, draft.provider);
        assert_eq!(imported.api_key, "existing-secret");
        assert_eq!(imported.custom.unwrap().request_format, "openai");
    }
    #[test]
    fn speech_defaults_and_guide_examples_are_valid() {
        for speech in [true, false] {
            let guide = guide(speech);
            let json = &guide[guide.find('{').unwrap()..];
            let imported = parse(json, &draft(speech)).unwrap();
            assert_eq!(
                imported.custom.unwrap().request_format,
                if speech { "multipart-wav" } else { "openai" }
            );
            assert!(imported.api_key.is_empty());
        }
    }
    #[test]
    fn rejects_mismatches_unknown_fields_bad_types_and_unsupported_protocols() {
        for json in [
            r#"{"kind":"speech","name":"X","endpoint":"https://example.com"}"#,
            r#"{"kind":"chat","name":"X","endpoint":"https://example.com","model":"m","extra":"secret-value"}"#,
            r#"{"kind":"chat","name":"X","endpoint":"https://example.com","model":"m","streaming":"secret-value"}"#,
            r#"{"kind":"chat","name":"X","endpoint":"file:///secret-value","model":"m"}"#,
            r#"{"kind":"chat","name":"X","endpoint":"https://example.com","model":"m","request_format":"unknown"}"#,
            r#"{"kind":"chat","name":"X","endpoint":"https://example.com","model":"m","streaming":true,"response_path":"/text"}"#,
        ] {
            let error = parse(json, &draft(false)).unwrap_err();
            assert!(!error.contains("secret-value"));
        }
    }
}
