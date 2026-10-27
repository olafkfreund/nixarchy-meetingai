//! Wire types for the newline-delimited JSON socket.
//!
//! A request is `{"id":n,"cmd":"…","args":{…}}`; its reply is
//! `{"id":n,"ok":true,"data":…}` or `{"id":n,"ok":false,"error":"…"}`. Events go
//! to every client as `{"event":"…",…}`. The shapes follow what `shell/Client.qml`
//! sends and expects.

use serde::Deserialize;
use serde_json::{Value, json};

/// A request line longer than this closes the client.
pub const MAX_LINE: usize = 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub id: Value,
    pub cmd: String,
    #[serde(default)]
    pub args: Value,
    /// Who sent it; `"mcp"` for the MCP server.
    #[serde(default)]
    pub origin: Option<String>,
}

pub fn reply(id: &Value, result: Result<Value, String>) -> String {
    let value = match result {
        Ok(data) => json!({"id": id, "ok": true, "data": data}),
        Err(error) => json!({"id": id, "ok": false, "error": error}),
    };
    format!("{value}\n")
}

pub fn event(name: &str, fields: Value) -> Value {
    let mut value = fields;
    if let Some(map) = value.as_object_mut() {
        map.insert("event".into(), name.into());
    }
    value
}

pub fn parse<T: for<'de> Deserialize<'de>>(args: Value) -> Result<T, String> {
    let args = if args.is_null() { json!({}) } else { args };
    serde_json::from_value(args).map_err(|error| format!("bad arguments: {error}"))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ChatSend {
    pub text: String,
    pub attachments: Vec<String>,
    pub screenshot: bool,
    pub no_context: bool,
}

#[derive(Debug, Deserialize)]
pub struct IdArg {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct MeetingArg {
    pub meeting: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RegionArg {
    pub region: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct TranscriptGet {
    pub meeting: Option<String>,
    pub after_line: Option<i64>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct PathArg {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct ProviderArg {
    pub provider: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct QueryArg {
    pub query: String,
}

#[derive(Debug, Deserialize)]
pub struct NameArg {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct SettingSet {
    pub key: String,
    pub value: Value,
    #[serde(default)]
    pub provider: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PromptArg {
    pub mode: String,
    pub kind: String,
    #[serde(default)]
    pub text: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_carry_the_request_id() {
        let ok = reply(&json!(7), Ok(json!({"a": 1})));
        assert!(ok.ends_with('\n'));
        let ok: Value = serde_json::from_str(&ok).unwrap();
        assert_eq!(ok, json!({"id": 7, "ok": true, "data": {"a": 1}}));
        let err = reply(&json!(8), Err("nope".into()));
        assert!(err.contains("\"ok\":false") && err.contains("nope"));
    }

    #[test]
    fn events_are_tagged() {
        assert_eq!(event("state", json!({"recording": true}))["event"], "state");
    }

    #[test]
    fn missing_args_parse_as_defaults() {
        let args: ChatSend = parse(Value::Null).unwrap();
        assert!(args.text.is_empty() && !args.no_context);
    }
}
