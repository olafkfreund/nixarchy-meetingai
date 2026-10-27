//! `nixarchy-meetingai mcp`: an MCP server over stdio, a thin client of the
//! daemon socket. Each call opens its own connection, so a restarted daemon is
//! picked up without any state to repair.

use std::time::Duration;

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ContentBlock, Implementation, ListResourceTemplatesResult,
        ListResourcesResult, PaginatedRequestParams, ReadResourceRequestParams,
        ReadResourceResponse, ReadResourceResult, Resource, ResourceContents, ResourceTemplate,
        ServerCapabilities, ServerConfig,
    },
    schemars::JsonSchema,
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{
        UnixStream,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
    time::{Instant, timeout, timeout_at},
};

const DOWN: &str = "The nixarchy-meetingai daemon is not running; start nixarchy-meetingai.service";
const MAX_QUESTION: usize = 8_000;
const MAX_LIMIT: usize = 500;
const ASK_TIMEOUT: Duration = Duration::from_secs(120);
/// Settings an agent may change. `api_key` and `mcp_allow_record` are deliberately absent.
const SETTINGS_ALLOWLIST: [&str; 7] = [
    "meeting_mode",
    "response_pace",
    "chat_provider",
    "chat_model",
    "live_provider",
    "live_model",
    "text_scale",
];

/// Run the MCP server on stdin/stdout until the client disconnects.
pub fn run() -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async {
        let service = Server::new()
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| e.to_string())?;
        service
            .waiting()
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}

// ----- daemon socket client ------------------------------------------------

struct Conn {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
}

impl Conn {
    async fn open() -> Result<Self, String> {
        let path = crate::cli::socket_path().ok_or(DOWN)?;
        let (reader, writer) = UnixStream::connect(path)
            .await
            .map_err(|_| DOWN.to_owned())?
            .into_split();
        Ok(Self {
            reader: BufReader::new(reader),
            writer,
        })
    }

    /// Request id is always 1: a connection carries one request.
    async fn send(&mut self, cmd: &str, args: Value) -> Result<(), String> {
        let line = format!(
            "{}\n",
            json!({"id": 1, "cmd": cmd, "args": args, "origin": "mcp"})
        );
        self.writer
            .write_all(line.as_bytes())
            .await
            .map_err(|_| DOWN.to_owned())
    }

    async fn next(&mut self) -> Result<Value, String> {
        let mut text = String::new();
        match self.reader.read_line(&mut text).await {
            Ok(n) if n > 0 => serde_json::from_str(&text).map_err(|e| e.to_string()),
            _ => Err("The daemon closed the connection.".into()),
        }
    }
}

fn is_reply(v: &Value) -> bool {
    v["id"] == 1 && v.get("ok").is_some()
}

fn reply_result(v: &Value) -> Result<Value, String> {
    if v["ok"] == true {
        Ok(v["data"].clone())
    } else {
        Err(v["error"]
            .as_str()
            .unwrap_or("The daemon refused.")
            .to_owned())
    }
}

/// One command, one reply (events on the connection are ignored).
async fn request(cmd: &str, args: Value) -> Result<Value, String> {
    let mut conn = Conn::open().await?;
    conn.send(cmd, args).await?;
    timeout(Duration::from_secs(30), async {
        loop {
            let v = conn.next().await?;
            if is_reply(&v) {
                return reply_result(&v);
            }
        }
    })
    .await
    .map_err(|_| "The daemon did not answer in time.".to_owned())?
}

/// Send `chat.send` and follow its turn until `answer.done` or `answer.error`.
async fn ask_daemon(question: String) -> Result<Value, String> {
    let want_suggestions = request("settings.get", json!({})).await?["smart_follow_ups"] != false;
    let mut conn = Conn::open().await?;
    conn.send("chat.send", json!({"text": question})).await?;
    let mut turn: Option<u64> = None;
    let mut early: Vec<Value> = Vec::new();
    let (mut answer, mut provider, mut model) = (String::new(), String::new(), String::new());
    let mut suggestions: Vec<String> = Vec::new();
    let mut finished = false;
    let mut partial = false;
    let mut deadline = Instant::now() + ASK_TIMEOUT;
    loop {
        let v = match timeout_at(deadline, conn.next()).await {
            Ok(v) => v?,
            Err(_) => {
                partial = !finished;
                break;
            }
        };
        let mut events = Vec::new();
        if is_reply(&v) {
            turn = Some(
                reply_result(&v)?["turn"]
                    .as_u64()
                    .ok_or("no turn in the reply")?,
            );
            events.append(&mut early);
        } else if turn.is_none() && v.get("turn").is_some() {
            early.push(v);
            continue;
        } else {
            events.push(v);
        }
        for e in events {
            match e["event"].as_str() {
                Some("answer.delta") if e["turn"].as_u64() == turn => {
                    answer.push_str(e["text"].as_str().unwrap_or_default());
                }
                Some("answer.done") if e["turn"].as_u64() == turn => {
                    provider = e["provider"].as_str().unwrap_or_default().to_owned();
                    model = e["model"].as_str().unwrap_or_default().to_owned();
                    finished = true;
                    // shortcut: follow-ups arrive on their own clock; wait 8 s, or 0.5 s once they start.
                    deadline = Instant::now() + Duration::from_secs(8);
                }
                Some("answer.error") if e["turn"].as_u64() == turn => {
                    return Err(e["message"]
                        .as_str()
                        .unwrap_or("The answer failed.")
                        .to_owned());
                }
                Some("suggestion") if finished => {
                    if suggestions.is_empty() {
                        deadline = deadline.min(Instant::now() + Duration::from_millis(500));
                    }
                    suggestions.extend(e["text"].as_str().map(str::to_owned));
                }
                _ => {}
            }
        }
        if finished && (!want_suggestions || suggestions.len() >= 3) {
            break;
        }
    }
    Ok(
        json!({"answer": answer, "provider": provider, "model": model,
              "suggestions": suggestions, "partial": partial}),
    )
}

// ----- formatting ------------------------------------------------------------

fn ok(text: impl Into<String>, data: Value) -> CallToolResult {
    let mut result = CallToolResult::success(vec![ContentBlock::text(text.into())]);
    result.structured_content = Some(data);
    result
}

fn fail(text: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(text.into())])
}

fn done(result: Result<(String, Value), String>) -> CallToolResult {
    match result {
        Ok((text, data)) => ok(text, data),
        Err(error) => fail(error),
    }
}

fn line_text(l: &Value) -> String {
    let who = l["who"].as_str().unwrap_or("?");
    let name = l["speaker"].as_str().filter(|s| !s.is_empty());
    let who = name.map_or_else(|| who.to_owned(), |n| format!("{who} ({n})"));
    format!(
        "[{}] {} {:.0}s: {}",
        l["line"],
        who,
        l["t0"].as_f64().unwrap_or(0.0),
        l["text"].as_str().unwrap_or_default()
    )
}

fn lines_text(lines: &[Value]) -> String {
    if lines.is_empty() {
        return "No transcript lines.".into();
    }
    lines.iter().map(line_text).collect::<Vec<_>>().join("\n")
}

/// Page through `transcript.get`, keeping the last `tail` lines when given.
async fn all_lines(
    meeting: Option<&str>,
    tail: Option<usize>,
) -> Result<(Value, Vec<Value>), String> {
    let (mut after, mut lines, mut id) = (0i64, Vec::<Value>::new(), Value::Null);
    // shortcut: 400 pages (200k lines) at most, then the rest is dropped.
    for _ in 0..400 {
        let mut args = json!({"after_line": after, "limit": MAX_LIMIT});
        if let Some(m) = meeting {
            args["meeting"] = m.into();
        }
        let page = request("transcript.get", args).await?;
        id = page["meeting"].clone();
        let rows = page["lines"].as_array().cloned().unwrap_or_default();
        let Some(last) = rows.last().and_then(|r| r["line"].as_i64()) else {
            break;
        };
        let full = rows.len() == MAX_LIMIT;
        after = last;
        lines.extend(rows);
        if let Some(n) = tail
            && lines.len() > n
        {
            lines.drain(..lines.len() - n);
        }
        if !full {
            break;
        }
    }
    Ok((id, lines))
}

// ----- tools -------------------------------------------------------------------

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct TranscriptArgs {
    /// Meeting id from meetings_list; omit for the current session.
    meeting: Option<String>,
    /// Only lines after this line number (the `line` of the last one you saw).
    after_line: Option<i64>,
    /// Maximum lines to return (default 200, at most 500).
    limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct AskArgs {
    /// The question to ask the meeting copilot (at most 8000 characters).
    question: String,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct ListArgs {
    /// Only meetings whose title contains this text.
    query: Option<String>,
    /// Maximum meetings to return (default 20, at most 100).
    limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct MeetingArgs {
    /// Meeting id from meetings_list.
    meeting: String,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct SettingArgs {
    /// One of meeting_mode, response_pace, chat_provider, chat_model, live_provider, live_model, text_scale.
    key: String,
    /// The new value (text_scale is a number from 80 to 160).
    value: Value,
}

#[derive(Clone)]
pub struct Server {
    tool_router: ToolRouter<Self>,
}

impl Server {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router]
impl Server {
    #[tool(
        description = "Say whether a meeting is being recorded and what the copilot is doing now.",
        annotations(read_only_hint = true)
    )]
    async fn meeting_status(&self) -> CallToolResult {
        done(
            async {
                let state = request("state.get", json!({})).await?;
                let now = request("now.get", json!({})).await?;
                let text = format!(
                    "Recording: {}. Copilot status: {}.",
                    state["recording"],
                    now["status"].as_str().unwrap_or("idle")
                );
                Ok((text, json!({"state": state, "now": now})))
            }
            .await,
        )
    }

    #[tool(
        description = "Read transcript lines of the current or a saved meeting; poll with after_line for live updates.",
        annotations(read_only_hint = true)
    )]
    async fn transcript_get(&self, Parameters(a): Parameters<TranscriptArgs>) -> CallToolResult {
        if a.limit.is_some_and(|l| l > MAX_LIMIT) {
            return fail(format!("limit must be at most {MAX_LIMIT}."));
        }
        let mut args = json!({});
        if let Some(m) = a.meeting {
            args["meeting"] = m.into();
        }
        if let Some(n) = a.after_line {
            args["after_line"] = n.into();
        }
        if let Some(n) = a.limit {
            args["limit"] = n.into();
        }
        done(request("transcript.get", args).await.map(|data| {
            let lines = data["lines"].as_array().cloned().unwrap_or_default();
            (lines_text(&lines), data)
        }))
    }

    #[tool(
        description = "Get the copilot's latest question, answer and suggested follow-ups.",
        annotations(read_only_hint = true)
    )]
    async fn current_answer(&self) -> CallToolResult {
        done(request("now.get", json!({})).await.map(|now| {
            let answer = now["answer"].as_str().unwrap_or_default();
            let text = if answer.is_empty() {
                "No answer yet."
            } else {
                answer
            };
            (text.to_owned(), now)
        }))
    }

    #[tool(
        description = "Ask the meeting copilot a question and wait for its answer (up to 2 minutes).",
        annotations(read_only_hint = false)
    )]
    async fn ask(&self, Parameters(a): Parameters<AskArgs>) -> CallToolResult {
        let question = a.question.trim().to_owned();
        if question.is_empty() || question.chars().count() > MAX_QUESTION {
            return fail(format!("question must be 1 to {MAX_QUESTION} characters."));
        }
        done(ask_daemon(question).await.map(|data| {
            let mut text = data["answer"].as_str().unwrap_or_default().to_owned();
            if data["partial"] == true {
                text.push_str("\n[Partial: the answer was still streaming.]");
            }
            (text, data)
        }))
    }

    #[tool(
        description = "List saved meetings, newest first, optionally filtered by title.",
        annotations(read_only_hint = true)
    )]
    async fn meetings_list(&self, Parameters(a): Parameters<ListArgs>) -> CallToolResult {
        done(request("session.list", json!({})).await.map(|all| {
            let query = a.query.unwrap_or_default().to_lowercase();
            let meetings: Vec<Value> = all
                .as_array()
                .into_iter()
                .flatten()
                .filter(|m| m["kind"] == "meeting")
                .filter(|m| {
                    m["title"]
                        .as_str()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&query)
                })
                .take(a.limit.unwrap_or(20).clamp(1, 100))
                .cloned()
                .collect();
            let text = meetings
                .iter()
                .map(|m| {
                    format!(
                        "{}  {}",
                        m["id"].as_str().unwrap_or_default(),
                        m["title"].as_str().unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            (text, json!({"meetings": meetings}))
        }))
    }

    #[tool(
        description = "Get the notes (recap, or rolling notes so far) of a meeting.",
        annotations(read_only_hint = true)
    )]
    async fn meeting_notes(&self, Parameters(a): Parameters<MeetingArgs>) -> CallToolResult {
        done(
            request("notes.get", json!({"meeting": a.meeting}))
                .await
                .map(|d| (d["text"].as_str().unwrap_or_default().to_owned(), d)),
        )
    }

    #[tool(
        description = "Regenerate the notes of a meeting in the background.",
        annotations(read_only_hint = false)
    )]
    async fn notes_regenerate(&self, Parameters(a): Parameters<MeetingArgs>) -> CallToolResult {
        done(
            request("notes.regenerate", json!({"meeting": a.meeting}))
                .await
                .map(|d| ("Regenerating the notes.".into(), d)),
        )
    }

    #[tool(
        description = "Start a fresh, empty session (not while recording).",
        annotations(read_only_hint = false)
    )]
    async fn session_new(&self) -> CallToolResult {
        done(
            request("session.new", json!({}))
                .await
                .map(|d| ("Started a new session.".into(), d)),
        )
    }

    #[tool(
        description = "Reopen a saved meeting as the current session (not while recording).",
        annotations(read_only_hint = false)
    )]
    async fn session_restore(&self, Parameters(a): Parameters<MeetingArgs>) -> CallToolResult {
        done(
            request("session.restore", json!({"id": a.meeting}))
                .await
                .map(|d| ("Restored the session.".into(), d)),
        )
    }

    #[tool(
        description = "Start recording a meeting; refused unless the user enabled agent recording.",
        annotations(read_only_hint = false)
    )]
    async fn record_start(&self) -> CallToolResult {
        done(
            request("record.start", json!({}))
                .await
                .map(|d| ("Recording started.".into(), d)),
        )
    }

    #[tool(
        description = "Stop the current recording and finish its notes.",
        annotations(read_only_hint = false)
    )]
    async fn record_stop(&self) -> CallToolResult {
        done(
            request("record.stop", json!({}))
                .await
                .map(|d| ("Recording stopped.".into(), d)),
        )
    }

    #[tool(
        description = "Change one safe setting: meeting_mode, response_pace, chat_provider, chat_model, live_provider, live_model or text_scale.",
        annotations(read_only_hint = false)
    )]
    async fn settings_set(&self, Parameters(a): Parameters<SettingArgs>) -> CallToolResult {
        if !SETTINGS_ALLOWLIST.contains(&a.key.as_str()) {
            return fail(format!(
                "Agents may not change '{}'. Allowed: {}.",
                a.key,
                SETTINGS_ALLOWLIST.join(", ")
            ));
        }
        done(
            request("settings.set", json!({"key": a.key, "value": a.value}))
                .await
                .map(|d| (format!("Set {}.", a.key), d)),
        )
    }
}

// ----- resources -----------------------------------------------------------------

fn internal(message: String) -> McpError {
    McpError::internal_error(message, None)
}

fn markdown(lines: &[Value]) -> String {
    lines
        .iter()
        .map(|l| format!("- {}", line_text(l)))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::new(
            "nixarchy-meetingai",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(
            "Live meeting copilot. Use meeting_status first; poll transcript_get with after_line during a meeting.",
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let mut resources = Vec::new();
        let current = request("transcript.get", json!({"limit": 1}))
            .await
            .map_err(internal)?;
        if !current["meeting"].is_null() {
            resources.push(
                Resource::new("meeting://current/transcript", "Current meeting transcript")
                    .with_mime_type("text/markdown"),
            );
        }
        let all = request("session.list", json!({})).await.map_err(internal)?;
        for m in all
            .as_array()
            .into_iter()
            .flatten()
            .filter(|m| m["kind"] == "meeting")
            .take(20)
        {
            let (id, title) = (
                m["id"].as_str().unwrap_or_default(),
                m["title"].as_str().unwrap_or_default(),
            );
            resources.push(
                Resource::new(
                    format!("meeting://{id}/transcript"),
                    format!("{title} (transcript)"),
                )
                .with_mime_type("text/markdown"),
            );
            resources.push(
                Resource::new(format!("meeting://{id}/notes"), format!("{title} (notes)"))
                    .with_mime_type("text/markdown"),
            );
        }
        Ok(ListResourcesResult::with_all_items(resources))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let template = |uri: &str, name: &str| {
            ResourceTemplate::new(uri, name).with_mime_type("text/markdown")
        };
        Ok(ListResourceTemplatesResult::with_all_items(vec![
            template(
                "meeting://current/transcript",
                "Current meeting transcript (last 200 lines)",
            ),
            template("meeting://{id}/transcript", "Transcript of a saved meeting"),
            template("meeting://{id}/notes", "Notes of a saved meeting"),
        ]))
    }

    async fn read_resource(
        &self,
        params: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let uri = params.uri;
        let parts = uri
            .strip_prefix("meeting://")
            .and_then(|r| r.split_once('/'));
        let text = match parts {
            Some(("current", "transcript")) => {
                markdown(&all_lines(None, Some(200)).await.map_err(internal)?.1)
            }
            Some((id, "transcript")) if id != "current" => {
                markdown(&all_lines(Some(id), None).await.map_err(internal)?.1)
            }
            Some((id, "notes")) => {
                let d = request("notes.get", json!({"meeting": id}))
                    .await
                    .map_err(internal)?;
                d["text"].as_str().unwrap_or_default().to_owned()
            }
            _ => {
                return Err(McpError::resource_not_found(
                    format!("Unknown resource {uri}"),
                    None,
                ));
            }
        };
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(text, uri).with_mime_type("text/markdown"),
        ])
        .into())
    }
}
