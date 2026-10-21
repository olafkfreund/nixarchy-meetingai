//! The overlay window.
//!
//! Mirrors the reference overlay: a drag header with brand, icon mode tabs and a
//! live status pill; the Ask body with conversation, follow-ups and input block;
//! a labelled capture/attach cluster; the composer with model, image and send
//! controls; and the size switcher row.

use gpui::{
    Context, FontWeight, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use lucide_icons::Icon;

use super::chat::Conversation;
use super::components::{icon_button, pill, radius, space};
use super::domain::{Mode, OverlaySize, Role};
use super::icons;
use super::theme::Theme;

/// Messages from the network task to the UI task.
enum StreamMsg {
    Delta(String),
    Error(String),
}

/// One transcribed line in the live transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptEntry {
    pub text: String,
    pub start: f64,
}

/// Follow-up suggestions shown above the composer, in the reference's order.
pub const FOLLOW_UPS: [&str; 4] = [
    "What should I say?",
    "Follow-up questions",
    "Fact-check",
    "Recap",
];

/// The tool cluster entries, in the reference's order.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Capture,
    Selection,
    Attach,
    Library,
}

impl Tool {
    const ALL: [Tool; 4] = [Tool::Capture, Tool::Selection, Tool::Attach, Tool::Library];

    fn icon(self) -> Icon {
        match self {
            Tool::Capture => Icon::Monitor,
            Tool::Selection => Icon::Crop,
            Tool::Attach => Icon::Paperclip,
            Tool::Library => Icon::FolderOpen,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Tool::Capture => "Capture",
            Tool::Selection => "Selection",
            Tool::Attach => "Attach",
            Tool::Library => "Library",
        }
    }
}

/// Root view for the overlay window.
pub struct Overlay {
    services: crate::app::Services,
    mode: Mode,
    size: OverlaySize,
    conversation: Conversation,
    theme: Theme,
    /// Provider id backing the active credential.
    model_provider: Option<String>,
    /// Model in use; falls back to the provider's first suggestion.
    model: Option<String>,
    /// Listen/system prompts available as context.
    prompt_choices: Vec<(i64, String, String)>,
    /// Selected prompt id.
    active_prompt: Option<i64>,
    /// Whether the model list is open.
    model_menu_open: bool,
    /// Whether the prompt list is open.
    prompt_menu_open: bool,
    use_image: bool,
    show_settings: bool,
    show_history: bool,
    /// The running capture session, when Listen is active.
    session: Option<crate::features::listen::ListenSession>,
    /// Live microphone level, 0.0 to 1.0.
    level: f32,
    /// Why listening could not start, shown under the header.
    listen_error: Option<String>,
    /// Non-fatal capture notice, e.g. microphone-only fallback.
    listen_notice: Option<String>,
    /// Device that capture actually opened.
    listening_device: Option<String>,
    /// Timer driving the level meter and the elapsed clock.
    pulse: Option<gpui::Task<()>>,
    /// Most recent screenshot, attached to the next message.
    attached_capture: Option<crate::features::capture::CapturedImage>,
    /// Why the last capture failed, shown under the header.
    capture_error: Option<String>,
    /// Live transcript lines for the current Listen session.
    transcript: Vec<TranscriptEntry>,
    /// Meeting row for the running session.
    meeting_id: Option<String>,
    /// Speech provider config, resolved when the session starts.
    speech: Option<crate::features::providers::ProviderConfig>,
    /// Chat session: conversation row plus the streaming answer.
    chat: crate::features::chat_session::ChatSession,
    /// Why the last send failed.
    chat_error: Option<String>,
    /// Utterances waiting to be transcribed.
    pending_audio: Vec<(String, f64, f64)>,
    /// Background transcription task.
    transcription: Option<gpui::Task<()>>,
    /// Background chat stream task.
    chat_stream: Option<gpui::Task<()>>,
    /// Answer text currently streaming, rendered live.
    streaming_text: String,
    /// Generated follow-up questions for this conversation.
    follow_ups: Vec<String>,
    /// Whether a follow-up request is in flight.
    follow_ups_loading: bool,
    /// Background follow-up task.
    follow_up_task: Option<gpui::Task<()>>,
    /// Set when a shortcut asked to focus the composer.
    focus_composer: bool,
    /// Whether the far side of the call is captured too.
    system_audio: bool,
    /// System tray handle, polled alongside shortcuts.
    tray: Option<crate::features::tray::Tray>,
    /// Overlay background opacity, 0.35 to 1.0.
    opacity: f32,
    /// System-wide shortcut listener, polled on the UI thread.
    shortcuts: Option<std::rc::Rc<crate::features::shortcuts::ShortcutListener>>,
    /// Keeps polling for shortcuts on an idle app.
    shortcut_poll: Option<gpui::Task<()>>,
}

impl Overlay {
    pub fn new(
        services: crate::app::Services,
        shortcuts: Option<std::rc::Rc<crate::features::shortcuts::ShortcutListener>>,
        tray: Option<crate::features::tray::Tray>,
    ) -> Self {
        // Read once at startup; changing it writes straight back to settings.
        let opacity =
            crate::features::settings::Settings::load_from_path(services.database_path.as_deref())
                .map(|settings| (settings.transparency as f32 / 100.0).clamp(0.5, 1.0))
                .unwrap_or(1.0);
        // Resolve the active credential and prompts once, so the pickers show
        // real values rather than placeholders.
        let (model_provider, model) = services
            .vault
            .as_ref()
            .as_ref()
            .and_then(|vault| {
                crate::features::credentials::list(vault, crate::features::credentials::Kind::Chat)
                    .ok()
            })
            .and_then(|credentials| {
                credentials.into_iter().find_map(|credential| {
                    let config = credential.to_config();
                    config.is_usable().then(|| {
                        let provider = config.id.clone();
                        let model = Some(config.model).filter(|m| !m.trim().is_empty());
                        (provider, model)
                    })
                })
            })
            .map(|(provider, model)| (Some(provider), model))
            .unwrap_or((None, None));

        let prompt_choices = services
            .database_path
            .as_deref()
            .and_then(|path| crate::features::db::Database::open(path).ok())
            .and_then(|database| {
                crate::features::db::settings::listen_modes(database.connection()).ok()
            })
            .map(|modes| {
                modes
                    .into_iter()
                    .map(|mode| (mode.id, mode.name, mode.prompt))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let active_prompt = prompt_choices.first().map(|(id, _, _)| *id);

        Self {
            services,
            mode: Mode::default(),
            size: OverlaySize::default(),
            conversation: Conversation::new(),
            theme: Theme::new(),
            model_provider,
            model,
            prompt_choices,
            active_prompt,
            model_menu_open: false,
            prompt_menu_open: false,
            use_image: false,
            show_settings: false,
            show_history: false,
            session: None,
            level: 0.0,
            listen_error: None,
            listen_notice: None,
            listening_device: None,
            pulse: None,
            attached_capture: None,
            capture_error: None,
            transcript: Vec::new(),
            meeting_id: None,
            speech: None,
            chat: crate::features::chat_session::ChatSession::new(),
            chat_error: None,
            pending_audio: Vec::new(),
            transcription: None,
            chat_stream: None,
            streaming_text: String::new(),
            opacity,
            shortcuts,
            follow_ups: Vec::new(),
            follow_ups_loading: false,
            follow_up_task: None,
            focus_composer: false,
            system_audio: false,
            tray,
            shortcut_poll: None,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn size(&self) -> OverlaySize {
        self.size
    }

    pub fn conversation(&self) -> &Conversation {
        &self.conversation
    }

    pub fn set_mode(&mut self, mode: Mode) {
        // Leaving Listen stops the session, matching the reference behaviour:
        // the microphone is released and the status returns to Ready.
        if self.mode == Mode::Listen && mode != Mode::Listen {
            self.stop_listening();
        }
        self.mode = mode;
        self.show_settings = false;
        self.show_history = false;
    }

    /// Compact layouts drop tool labels, as the reference does below `md`.
    fn show_tool_labels(&self) -> bool {
        self.size != OverlaySize::Small
    }

    fn status_label(&self) -> (String, gpui::Rgba) {
        let theme = self.theme;
        if let Some(session) = self
            .session
            .as_ref()
            .filter(|session| session.is_capturing())
        {
            let seconds = session.elapsed().as_secs();
            (
                format!("Listening · {:02}:{:02}", seconds / 60, seconds % 60),
                theme.success,
            )
        } else if self.listen_error.is_some() {
            ("Microphone error".to_string(), theme.destructive)
        } else {
            ("Ready".to_string(), theme.muted_foreground)
        }
    }

    /// Seconds elapsed in the current session.
    fn elapsed_seconds(&self) -> f64 {
        self.session
            .as_ref()
            .map(|session| session.elapsed().as_secs_f64())
            .unwrap_or(0.0)
    }

    /// Whether a live session is running.
    pub fn is_listening(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(crate::features::listen::ListenSession::is_capturing)
    }

    /// Start capturing from the configured device.
    pub fn start_listening(&mut self, cx: &mut Context<Self>) {
        if self.is_listening() {
            return;
        }
        self.listen_error = None;
        let device = self
            .services
            .database_path
            .as_ref()
            .and_then(|_| {
                crate::features::settings::Settings::load_from_path(
                    self.services.database_path.as_deref(),
                )
            })
            .map(|settings| settings.input_device)
            .filter(|device| !device.is_empty());

        // Resolve the speech provider from the vault once, at start.
        self.speech = self
            .services
            .vault
            .as_ref()
            .as_ref()
            .and_then(|vault| {
                crate::features::credentials::list(
                    vault,
                    crate::features::credentials::Kind::Speech,
                )
                .ok()
            })
            .and_then(|credentials| {
                credentials
                    .into_iter()
                    .map(|credential| credential.to_config())
                    .find(|config| !config.api_key.trim().is_empty())
            });

        let config = crate::features::listen::SessionConfig {
            device,
            system_audio: self.system_audio,
            speech: self.speech.clone(),
        };
        let (session, events) = crate::features::listen::ListenSession::start(&config);
        self.session = Some(session);
        self.transcript.clear();
        self.meeting_id = self.create_meeting();
        for event in events {
            self.handle_session_event(event);
        }
        self.spawn_transcription(cx);
    }

    /// Transcribe finished utterances in the background as they arrive.
    fn spawn_transcription(&mut self, cx: &mut Context<Self>) {
        let Some(config) = self.speech.clone() else {
            return;
        };
        self.transcription = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(250))
                    .await;
                let (pending, alive) = this
                    .update(cx, |overlay, _| {
                        (overlay.take_pending_audio(), overlay.session.is_some())
                    })
                    .unwrap_or((Vec::new(), false));
                if !alive && pending.is_empty() {
                    break;
                }
                for (wav, start, duration) in pending {
                    let language = None;
                    let result =
                        crate::features::listen::transcribe_utterance(&config, &wav, language)
                            .await;
                    this.update(cx, |overlay, cx| {
                        match result {
                            Ok(text) if !text.trim().is_empty() => {
                                overlay.push_transcript(text, start, duration);
                            }
                            Ok(_) => {}
                            Err(error) => overlay.listen_error = Some(error),
                        }
                        cx.notify();
                    })
                    .ok();
                }
            }
        }));
    }

    fn take_pending_audio(&mut self) -> Vec<(String, f64, f64)> {
        std::mem::take(&mut self.pending_audio)
    }

    fn push_transcript(&mut self, text: String, start: f64, duration: f64) {
        if let Some(meeting_id) = self.meeting_id.as_ref() {
            let path = self.services.database_path.clone();
            if let Some(path) = path
                && let Ok(database) = crate::features::db::Database::open(&path)
            {
                let _ = crate::features::db::meetings::add_line(
                    database.connection(),
                    meeting_id,
                    "them",
                    None,
                    &text,
                    start,
                    start + duration,
                );
            }
        }
        self.transcript.push(TranscriptEntry { text, start });
    }

    /// The configured input device, if one was chosen.
    fn configured_input_device(&self) -> Option<String> {
        crate::features::settings::Settings::load_from_path(self.services.database_path.as_deref())
            .map(|settings| settings.input_device)
            .filter(|device| !device.is_empty())
    }

    /// Create the meeting row that this session's transcript is written into.
    fn create_meeting(&self) -> Option<String> {
        let path = self.services.database_path.as_ref()?;
        let database = crate::features::db::Database::open(path).ok()?;
        // Name it at creation so the list never shows a blank title.
        let title = format!("Session · {}", chrono_like_now());
        crate::features::db::meetings::start(database.connection(), &title, "meeting", "en").ok()
    }

    /// Stop capturing and flush the last utterance.
    pub fn stop_listening(&mut self) {
        if let Some(mut session) = self.session.take() {
            for event in session.stop() {
                self.handle_session_event(event);
            }
        }
        self.level = 0.0;
    }

    /// Tick the session a few times a second so the meter and clock stay live.
    fn start_pulse(&mut self, cx: &mut Context<Self>) {
        if self.pulse.is_some() {
            return;
        }
        self.pulse = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(120))
                    .await;
                let keep_going = this
                    .update(cx, |overlay, cx| {
                        overlay.pump();
                        if overlay.is_listening() {
                            cx.notify();
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        }));
    }

    /// Send the composer contents to the model and stream the answer.
    pub fn send_message(&mut self, cx: &mut Context<Self>) {
        use crate::features::chat_session::{SendError, chat_config};
        self.chat_error = None;

        if self.chat.is_streaming() {
            self.chat_error = Some(SendError::AlreadyStreaming.message());
            return;
        }
        let question = self.conversation.draft().trim().to_owned();
        if question.is_empty() {
            return;
        }
        let Some(config) = chat_config(self.services.vault.as_ref().as_ref()) else {
            self.chat_error = Some(SendError::NoKey.message());
            return;
        };

        let database = self
            .services
            .database_path
            .as_deref()
            .and_then(|path| crate::features::db::Database::open(path).ok());
        let history = match self.chat.begin(database.as_ref(), &question) {
            Ok(history) => history,
            Err(error) => {
                self.chat_error = Some(error.message());
                return;
            }
        };

        // Show the question in the overlay and clear the composer.
        self.conversation.submit();
        self.conversation.set_draft(String::new());
        let images = self
            .attached_capture
            .as_ref()
            .map(|image| vec![image.base64.clone()])
            .unwrap_or_default();
        self.attached_capture = None;

        let context = crate::features::chat_engine::TurnContext {
            system: Some(self.selected_system_prompt()),
            images,
        };
        let cancel = std::sync::Arc::new(tokio::sync::watch::channel(false).1);
        let client = crate::features::chat_engine::http_client();
        let database_path = self.services.database_path.clone();
        let config_clone = config.clone();

        // Deltas cross an unbounded channel; the UI task awaits the next one, so
        // text appears the moment the provider sends it rather than on a timer.
        let (delta_tx, mut delta_rx) = tokio::sync::mpsc::unbounded_channel::<StreamMsg>();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<(bool, Option<String>)>();

        self.chat_stream = Some(cx.spawn(async move |this, cx| {
            loop {
                match delta_rx.recv().await {
                    Some(StreamMsg::Delta(text)) => {
                        // Persist as text arrives, so a crash keeps what was shown.
                        let database = database_path
                            .as_deref()
                            .and_then(|path| crate::features::db::Database::open(path).ok());
                        this.update(cx, move |overlay, cx| {
                            overlay.chat.push_delta(database.as_ref(), &text);
                            overlay.streaming_text = overlay.chat.answer().to_owned();
                            cx.notify();
                        })
                        .ok();
                    }
                    Some(StreamMsg::Error(message)) => {
                        let database = database_path
                            .as_deref()
                            .and_then(|path| crate::features::db::Database::open(path).ok());
                        this.update(cx, move |overlay, cx| {
                            let text = overlay.chat.fail(database.as_ref(), &message);
                            overlay.chat_error = Some(format!("Couldn't get an answer: {message}"));
                            overlay.streaming_text.clear();
                            if !text.is_empty() {
                                overlay
                                    .conversation
                                    .push(crate::features::domain::Message::assistant(text));
                            }
                            cx.notify();
                        })
                        .ok();
                        break;
                    }
                    None => break,
                }
            }
            // The request task reports completion through this oneshot.
            if let Ok((_ok, answer)) = done_rx.await {
                this.update(cx, |overlay, cx| {
                    overlay.chat.finish();
                    let final_text = overlay.chat.answer().to_owned();
                    if !final_text.is_empty() {
                        overlay
                            .conversation
                            .push(crate::features::domain::Message::assistant(final_text));
                    }
                    let _ = answer;
                    overlay.streaming_text.clear();
                    cx.notify();
                })
                .ok();
            }
        }));

        // The network task owns the request and pushes every delta immediately.
        tokio::spawn(async move {
            let result = crate::features::chat_engine::stream_chat(
                &client,
                &config_clone,
                &history,
                &context,
                cancel,
                |delta| {
                    let _ = delta_tx.send(StreamMsg::Delta(delta.to_owned()));
                },
            )
            .await;
            match result {
                Ok(answer) => {
                    let _ = done_tx.send((true, Some(answer)));
                }
                Err(error) => {
                    let message = error.to_string();
                    let _ = delta_tx.send(StreamMsg::Error(message));
                    let _ = done_tx.send((false, None));
                }
            }
        });
    }

    /// The system prompt for the current context tab.
    fn selected_system_prompt(&self) -> String {
        // The chosen listen mode drives answers; the built-in default is only a
        // fallback for a profile with no prompts.
        if let Some(id) = self.active_prompt
            && let Some((_, _, prompt)) = self
                .prompt_choices
                .iter()
                .find(|(prompt_id, _, _)| *prompt_id == id)
        {
            return prompt.clone();
        }
        crate::features::db::seed::DEFAULT_SYSTEM_PROMPT.to_owned()
    }

    /// Capture the screen and hold it for the next message.
    pub fn capture_screen(&mut self) {
        self.capture_error = None;
        let png = false;
        match crate::features::screen::capture(None, None, png) {
            Ok(image) => self.attached_capture = Some(image),
            Err(error) => {
                self.capture_error = Some(format!("Couldn't capture the screen: {error}"))
            }
        }
    }

    /// Discard the attached screenshot.
    pub fn clear_capture(&mut self) {
        self.attached_capture = None;
    }

    fn handle_session_event(&mut self, event: crate::features::listen::SessionEvent) {
        use crate::features::listen::SessionEvent;
        match event {
            SessionEvent::Started { device } => {
                self.listening_device = Some(device);
                self.listen_error = None;
            }
            SessionEvent::Level(level) => self.level = level.clamp(0.0, 1.0),
            SessionEvent::Failed(message) => {
                self.listen_error = Some(message);
                self.listening_device = None;
            }
            SessionEvent::Utterance {
                wav_base64,
                duration,
                ..
            } => {
                if self.speech.is_some() {
                    let start = self.elapsed_seconds();
                    self.pending_audio.push((wav_base64, start, duration));
                }
            }
            SessionEvent::Transcript(line) => {
                if line.is_final && !line.text.trim().is_empty() {
                    self.push_transcript(line.text, line.start, line.end - line.start);
                }
            }
            SessionEvent::Stopped { .. } => {
                self.level = 0.0;
            }
            SessionEvent::Notice(message) => {
                self.listen_notice = Some(message);
            }
        }
    }

    /// Keep polling shortcuts even when no session is running.
    fn ensure_shortcut_poll(&mut self, cx: &mut Context<Self>) {
        if (self.shortcuts.is_none() && self.tray.is_none()) || self.shortcut_poll.is_some() {
            return;
        }
        self.shortcut_poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(150))
                    .await;
                let alive = this
                    .update(cx, |overlay, cx| {
                        overlay.poll_shortcuts();
                        overlay.poll_tray(cx);
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !alive {
                    break;
                }
            }
        }));
    }

    /// Act on any global shortcut pressed since the last frame.
    pub fn poll_shortcuts(&mut self) {
        let Some(listener) = self.shortcuts.clone() else {
            return;
        };
        // Collect first: handling an action mutates the overlay, so the listener
        // must not be borrowed across the loop.
        let mut actions = Vec::new();
        while let Some(action) = listener.try_action() {
            actions.push(action);
        }
        for action in actions {
            self.handle_shortcut(action);
        }
    }

    /// Act on tray clicks and menu choices.
    fn poll_tray(&mut self, cx: &mut Context<Self>) {
        use crate::features::tray::TrayAction;
        let Some(tray) = self.tray.as_ref() else {
            return;
        };
        tray.set_tooltip(if self.is_listening() {
            "lennette · listening"
        } else {
            "lennette"
        });
        let actions = tray.poll();
        for action in actions {
            match action {
                TrayAction::ShowOverlay => cx.activate(true),
                TrayAction::OpenDashboard => {
                    crate::app::open_dashboard(cx, self.services.clone());
                }
                TrayAction::ToggleListening => {
                    if self.is_listening() {
                        self.stop_listening();
                    } else {
                        self.begin_capture();
                    }
                }
                TrayAction::Quit => cx.quit(),
            }
        }
    }

    fn handle_shortcut(&mut self, action: crate::features::shortcuts::Action) {
        use crate::features::shortcuts::Action;
        match action {
            Action::Screenshot => self.capture_screen(),
            Action::ToggleWindow => self.show_settings = !self.show_settings,
            Action::ToggleDashboard => self.show_history = !self.show_history,
            Action::VoiceInput => {
                if self.is_listening() {
                    self.stop_listening();
                } else {
                    self.begin_capture();
                }
            }
            Action::FocusInput => {
                // Bring the overlay forward and put the cursor in the composer.
                self.mode = Mode::Ask;
                self.show_settings = false;
                self.show_history = false;
                self.focus_composer = true;
            }
            Action::SystemAudio => {
                // Toggle capture of the far side of a call alongside the mic.
                self.system_audio = !self.system_audio;
                if self.is_listening() {
                    // Restart so the new source set takes effect immediately.
                    self.stop_listening();
                    self.begin_capture();
                }
            }
        }
    }

    /// Start capture without a UI context (a global shortcut).
    ///
    /// Audio is captured and segmented immediately; transcription begins on the
    /// next frame, when a context is available.
    fn begin_capture(&mut self) {
        if self.is_listening() {
            return;
        }
        self.listen_error = None;
        let device = self.configured_input_device();
        let config = crate::features::listen::SessionConfig {
            device,
            system_audio: self.system_audio,
            speech: None,
        };
        let (session, events) = crate::features::listen::ListenSession::start(&config);
        self.session = Some(session);
        self.transcript.clear();
        self.meeting_id = self.create_meeting();
        for event in events {
            self.handle_session_event(event);
        }
    }

    /// Drain microphone audio. Called on a short timer while listening.
    pub fn pump(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        for event in session.pump() {
            self.handle_session_event(event);
        }
    }

    fn mode_tab(&self, mode: Mode, active: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let icon = match mode {
            Mode::Ask => Icon::MessageSquare,
            Mode::Listen => Icon::Headphones,
        };
        let mut tab = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(24.0))
            .px(px(10.0))
            .rounded(px(radius::SM))
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(if active {
                theme.foreground
            } else {
                theme.muted_foreground
            })
            .cursor_pointer()
            .id(("mode-tab", mode as usize));
        tab = tab.on_click(cx.listener(move |this, _, _, cx| {
            this.set_mode(mode);
            if mode == Mode::Listen {
                this.start_listening(cx);
                this.start_pulse(cx);
            }
            cx.notify();
        }));
        if active {
            tab = tab.bg(theme.background);
        }
        tab.child(icons::icon(&theme, icon, 13.0))
            .child(mode.label())
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let settings_active = self.show_settings;
        let history_active = self.show_history;

        div()
            .flex()
            .items_center()
            .gap(px(space::SM))
            .h(px(44.0))
            .px(px(10.0))
            .border_b_1()
            .border_color(theme.border)
            .bg(Theme::with_alpha(theme.card, self.surface_alpha()))
            // Home mark opens the dashboard.
            .child(icon_button("home", &theme, Icon::House))
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.foreground)
                    .child("lennette"),
            )
            .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .p(px(2.0))
                    .rounded(px(radius::MD))
                    .bg(theme.card)
                    .child(self.mode_tab(Mode::Ask, self.mode == Mode::Ask, cx))
                    .child(self.mode_tab(Mode::Listen, self.mode == Mode::Listen, cx)),
            )
            .child(div().flex_1())
            .child(self.status_pill())
            .child(self.dashboard_button(cx))
            .child(self.settings_button(settings_active, cx))
            .child(self.history_button(history_active, cx))
    }

    fn status_pill(&self) -> impl IntoElement {
        let theme = self.theme;
        let (label, dot) = self.status_label();
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(26.0))
            .px(px(8.0))
            .rounded_full()
            .border_1()
            .border_color(theme.border)
            .bg(theme.muted)
            .text_size(px(10.5))
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme.foreground)
            .child(div().size(px(6.0)).rounded_full().bg(dot))
            .child(label)
    }

    fn dashboard_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let services = self.services.clone();
        div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .h(px(26.0))
            .px(px(8.0))
            .rounded_full()
            .border_1()
            .border_color(theme.border)
            .bg(theme.muted)
            .text_size(px(10.5))
            .text_color(theme.foreground)
            .cursor_pointer()
            .id("open-dashboard")
            .child(icons::icon(&theme, Icon::LayoutDashboard, 11.0))
            .child("Open dashboard")
            .on_click(cx.listener(move |_, _, _, cx| {
                crate::app::open_dashboard(cx, services.clone());
            }))
    }

    fn settings_button(&self, active: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut button = icon_button("toggle-settings", &theme, Icon::Settings).on_click(
            cx.listener(move |this, _, _, cx| {
                this.show_settings = !this.show_settings;
                this.show_history = false;
                cx.notify();
            }),
        );
        if active {
            button = button.bg(theme.accent);
        }
        button
    }

    fn history_button(&self, active: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut button = icon_button("toggle-history", &theme, Icon::History).on_click(
            cx.listener(move |this, _, _, cx| {
                this.show_history = !this.show_history;
                this.show_settings = false;
                cx.notify();
            }),
        );
        if active {
            button = button.bg(theme.accent);
        }
        button
    }

    /// Alpha for opaque surfaces. Kept slightly above the body opacity so text
    /// on the header and composer stays readable at low settings.
    fn surface_alpha(&self) -> f32 {
        (self.opacity * 0.92).clamp(0.30, 1.0)
    }

    /// Ask the model for fresh follow-up questions based on the conversation.
    pub fn generate_follow_ups(&mut self, cx: &mut Context<Self>) {
        use crate::features::chat_session::chat_config;
        let Some(config) = chat_config(self.services.vault.as_ref().as_ref()) else {
            self.chat_error =
                Some("Add an AI provider key in Dev Space to generate follow-ups.".to_owned());
            return;
        };
        if self.follow_ups_loading {
            return;
        }
        self.follow_ups_loading = true;
        self.follow_ups.clear();

        let mut history: Vec<crate::features::domain::Message> =
            self.conversation.messages().to_vec();
        history.push(crate::features::domain::Message::user(
            "Suggest three short follow-up questions the user might ask next. Reply with one              question per line, no numbering, no other text.",
        ));

        let client = crate::features::chat_engine::http_client();
        let cancel = std::sync::Arc::new(tokio::sync::watch::channel(false).1);
        self.follow_up_task = Some(cx.spawn(async move |this, cx| {
            let result = crate::features::chat_engine::stream_chat(
                &client,
                &config,
                &history,
                &crate::features::chat_engine::TurnContext::default(),
                cancel,
                |_| {},
            )
            .await;
            this.update(cx, |overlay, cx| {
                overlay.follow_ups_loading = false;
                match result {
                    Ok(text) => {
                        overlay.follow_ups = text
                            .lines()
                            .map(|line| {
                                line.trim()
                                    .trim_start_matches(['-', '*', '1', '2', '3', '.', ' '])
                                    .trim()
                            })
                            .filter(|line| !line.is_empty() && line.ends_with('?'))
                            .take(3)
                            .map(str::to_owned)
                            .collect();
                        if overlay.follow_ups.is_empty() {
                            overlay.chat_error =
                                Some("The model did not return usable questions.".to_owned());
                        }
                    }
                    Err(error) => {
                        overlay.chat_error = Some(format!("Couldn't generate follow-ups: {error}"))
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// The follow-ups to show: generated ones, else the defaults.
    fn visible_follow_ups(&self) -> Vec<String> {
        if self.follow_ups.is_empty() {
            FOLLOW_UPS.iter().map(|text| (*text).to_owned()).collect()
        } else {
            self.follow_ups.clone()
        }
    }

    /// Change overlay opacity and persist it.
    pub fn set_opacity(&mut self, opacity: f32) {
        // Below 50% the overlay stops being readable over a busy desktop.
        self.opacity = opacity.clamp(0.5, 1.0);
        let percent = (self.opacity * 100.0).round() as u32;
        if let Some(path) = self.services.database_path.as_deref()
            && let Ok(database) = crate::features::db::Database::open(path)
        {
            let _ = crate::features::settings::Settings::set(
                &database,
                "transparency",
                &percent.to_string(),
            );
        }
    }

    fn opacity_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let current = self.opacity;
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(22.0))
                    .rounded(px(radius::SM))
                    .cursor_pointer()
                    .text_color(theme.muted_foreground)
                    .id("opacity-down")
                    .child(icons::icon(&theme, Icon::SunDim, 11.0))
                    .on_click(cx.listener(|this, _, _, cx| {
                        let next = this.opacity - 0.1;
                        this.set_opacity(next);
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .text_size(px(9.5))
                    .text_color(theme.muted_foreground)
                    .child(format!("{:.0}%", current * 100.0)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(22.0))
                    .rounded(px(radius::SM))
                    .cursor_pointer()
                    .text_color(theme.muted_foreground)
                    .id("opacity-up")
                    .child(icons::icon(&theme, Icon::SunMedium, 11.0))
                    .on_click(cx.listener(|this, _, _, cx| {
                        let next = this.opacity + 0.1;
                        this.set_opacity(next);
                        cx.notify();
                    })),
            )
    }

    /// A notice shown under the header: errors, live level, or an attachment.
    fn listen_banner(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let theme = self.theme;
        if let Some(message) = self
            .capture_error
            .as_deref()
            .or(self.listen_error.as_deref())
        {
            return Some(
                div()
                    .mx(px(space::MD))
                    .mt(px(space::SM))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(radius::MD))
                    .border_1()
                    .border_color(theme.destructive)
                    .px(px(space::MD))
                    .py(px(space::SM))
                    .child(icons::icon_colored(
                        theme.destructive,
                        Icon::TriangleAlert,
                        12.0,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(11.0))
                            .text_color(theme.foreground)
                            .child(message.to_owned()),
                    )
                    .into_any_element(),
            );
        }

        if let Some(image) = self.attached_capture.as_ref() {
            let kilobytes = image.base64.len() * 3 / 4 / 1024;
            let clear = div()
                .text_size(px(11.0))
                .text_color(theme.muted_foreground)
                .cursor_pointer()
                .id("clear-capture")
                .child("Remove")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.clear_capture();
                    cx.notify();
                }));
            return Some(
                div()
                    .mx(px(space::MD))
                    .mt(px(space::SM))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(radius::MD))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.card)
                    .px(px(space::MD))
                    .py(px(space::SM))
                    .child(icons::icon(&theme, Icon::Image, 12.0))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(11.0))
                            .text_color(theme.foreground)
                            .child(format!("Screenshot attached · {kilobytes} KB")),
                    )
                    .child(clear)
                    .into_any_element(),
            );
        }

        self.session
            .as_ref()
            .filter(|session| session.is_capturing())
            .map(|_session| {
                let device = self
                    .listening_device
                    .clone()
                    .unwrap_or_else(|| "the system default".to_owned());
                let width = (self.level * 120.0).clamp(2.0, 120.0);
                div()
                    .mx(px(space::MD))
                    .mt(px(space::SM))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(theme.muted_foreground)
                            .child(format!("Recording from {device}")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .w(px(120.0))
                            .h(px(4.0))
                            .rounded_full()
                            .bg(theme.muted)
                            .child(
                                div()
                                    .w(px(width))
                                    .h(px(4.0))
                                    .rounded_full()
                                    .bg(theme.success),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.muted_foreground)
                            .child(format!("{:.0}%", self.level * 100.0)),
                    )
                    .into_any_element()
            })
    }

    fn empty_state(&self) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .flex_1()
            .items_center()
            .justify_center()
            .gap(px(space::SM))
            .px(px(space::XL))
            .child(icons::icon_colored(theme.primary, Icon::Sparkles, 18.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(theme.muted_foreground)
                    .child(if self.mode == Mode::Ask {
                        "Ask anything — or start listening and I'll follow the conversation."
                    } else {
                        "Start listening and I'll follow the conversation."
                    }),
            )
    }

    fn message_list(&self) -> impl IntoElement {
        let theme = self.theme;
        let mut list = div()
            .id("message-list")
            .flex()
            .flex_col()
            .flex_1()
            .gap(px(space::MD))
            .overflow_y_scroll()
            .px(px(space::LG))
            .py(px(space::MD));

        for (index, message) in self.conversation.messages().iter().enumerate() {
            let is_user = message.role == Role::User;
            if is_user {
                list = list.child(
                    div().flex().justify_end().child(
                        div()
                            .id(("message", index))
                            .max_w(px(520.0))
                            .rounded(px(radius::LG))
                            .px(px(space::MD))
                            .py(px(space::SM))
                            .text_size(px(12.5))
                            .text_color(theme.background)
                            .bg(theme.primary)
                            .child(message.content.clone()),
                    ),
                );
            } else {
                // Assistant answers are markdown and render as formatted blocks.
                list = list.child(
                    div()
                        .flex()
                        .justify_start()
                        .child(render_markdown(&theme, &message.content)),
                );
            }
        }

        // The answer currently arriving, rendered as partial markdown.
        if !self.streaming_text.is_empty() {
            list = list.child(
                div()
                    .flex()
                    .justify_start()
                    .gap(px(6.0))
                    .child(render_markdown(&theme, &self.streaming_text))
                    .child(div().size(px(6.0)).rounded_full().bg(theme.success)),
            );
        }
        list
    }

    fn follow_ups(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut chips = div()
            .flex()
            .items_center()
            .gap(px(space::SM))
            .flex_1()
            .id("follow-up-chips")
            .overflow_x_scroll();

        let labels = self.visible_follow_ups();
        for (index, label) in labels.into_iter().enumerate() {
            let chip = pill(("follow-up", index), &theme, false)
                .flex_shrink_0()
                .child(icons::icon(&theme, Icon::ArrowUpRight, 10.0))
                .child(label.clone());
            let send = label;
            chips = chips.child(chip.on_click(cx.listener(move |this, _, _, cx| {
                // Fill the composer and send, so a chip is one click.
                this.conversation.set_draft(send.clone());
                this.send_message(cx);
                cx.notify();
            })));
        }

        div()
            .flex()
            .items_center()
            .gap(px(space::SM))
            .w_full()
            .px(px(space::MD))
            .py(px(space::SM))
            .border_t_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(9.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.muted_foreground)
                    .whitespace_nowrap()
                    .child("FOLLOW-UPS"),
            )
            .child(chips)
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(4.0))
                    .text_size(px(11.0))
                    .text_color(theme.muted_foreground)
                    .cursor_pointer()
                    .id("edit-follow-ups")
                    .child(icons::icon(&theme, Icon::Plus, 10.0))
                    .child("Edit"),
            )
    }

    fn tool_cluster(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let labelled = self.show_tool_labels();
        let mut cluster = div()
            .flex()
            .items_center()
            .gap(px(1.0))
            .p(px(2.0))
            .rounded_full()
            .border_1()
            .border_color(theme.border)
            .bg(theme.muted);

        for (index, tool) in Tool::ALL.into_iter().enumerate() {
            if index > 0 {
                cluster = cluster.child(div().w(px(1.0)).h(px(14.0)).bg(theme.border));
            }
            let mut button = div()
                .flex()
                .items_center()
                .gap(px(5.0))
                .h(px(26.0))
                .rounded_full()
                .cursor_pointer()
                .hover(|style| style.bg(theme.accent))
                .id(("tool", index))
                .child(icons::icon(&theme, tool.icon(), 13.0));
            button = if labelled {
                button.px(px(9.0))
            } else {
                button.w(px(26.0)).justify_center()
            };
            if labelled {
                button = button.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.foreground)
                        .child(tool.label()),
                );
            }
            cluster = cluster.child(button.on_click(cx.listener(move |this, _, _, cx| {
                if tool == Tool::Capture {
                    this.capture_screen();
                }
                cx.notify();
            })));
        }
        cluster
    }

    fn push_to_talk(&self) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(28.0))
            .px(px(10.0))
            .rounded_full()
            .border_1()
            .border_color(theme.border)
            .text_size(px(11.0))
            .text_color(theme.foreground)
            .cursor_pointer()
            .id("push-to-talk")
            .child(icons::icon(&theme, Icon::Mic, 12.0))
            .child(if self.is_listening() {
                "Listening…"
            } else {
                "Push to talk"
            })
            .child(kbd(&theme, "⌃"))
            .child(kbd(&theme, "Space"))
    }

    fn image_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let active = self.use_image;
        let mut button = div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .h(px(28.0))
            .px(px(8.0))
            .rounded(px(radius::MD))
            .border_1()
            .text_size(px(11.0))
            .cursor_pointer()
            .id("toggle-image");
        button = if active {
            button
                .border_color(theme.primary)
                .bg(theme.accent)
                .text_color(theme.primary)
        } else {
            button
                .border_color(theme.border)
                .text_color(theme.muted_foreground)
        };
        button
            .child(icons::icon(&theme, Icon::Image, 12.0))
            .child("Use image")
            .on_click(cx.listener(|this, _, _, cx| {
                this.use_image = !this.use_image;
                cx.notify();
            }))
    }

    /// The model button, showing the real active model.
    fn model_selector(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let label =
            crate::features::models::label(self.model.as_deref(), self.model_provider.as_deref());
        let mut button = div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .h(px(28.0))
            .px(px(8.0))
            .rounded(px(radius::MD))
            .border_1()
            .border_color(if self.model_menu_open {
                theme.primary
            } else {
                theme.border
            })
            .text_size(px(11.5))
            .text_color(theme.muted_foreground)
            .cursor_pointer()
            .id("model-select")
            .child(icons::icon(&theme, Icon::Sparkles, 11.0))
            .child(label)
            .child(icons::icon(&theme, Icon::ChevronDown, 11.0));
        button = button.on_click(cx.listener(|this, _, _, cx| {
            this.model_menu_open = !this.model_menu_open;
            this.prompt_menu_open = false;
            cx.notify();
        }));

        if !self.model_menu_open {
            return button.into_any_element();
        }

        // A list of the provider's suggested models.
        let models = self
            .model_provider
            .as_deref()
            .map(crate::features::models::for_provider)
            .unwrap_or(&[]);
        let mut menu = div()
            .flex()
            .flex_col()
            .gap(px(1.0))
            .rounded(px(radius::MD))
            .border_1()
            .border_color(theme.border)
            .bg(theme.card)
            .p(px(3.0));
        if models.is_empty() {
            menu = menu.child(
                div()
                    .px(px(8.0))
                    .py(px(4.0))
                    .text_size(px(11.0))
                    .text_color(theme.muted_foreground)
                    .child("Add a provider key in Dev Space"),
            );
        }
        for (index, model) in models.iter().enumerate() {
            let active = self.model.as_deref() == Some(*model);
            let mut item = div()
                .flex()
                .items_center()
                .forge_model_item(&theme, active)
                .id(("model-option", index))
                .child(*model);
            item = item.on_click(cx.listener(move |this, _, _, cx| {
                this.select_model(model);
                cx.notify();
            }));
            menu = menu.child(item);
        }

        div()
            .flex()
            .flex_col()
            .items_end()
            .gap(px(4.0))
            .child(button)
            .child(menu)
            .into_any_element()
    }

    /// The context prompt button, listing the listen modes from the database.
    fn prompt_selector(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let label = self
            .active_prompt_name()
            .unwrap_or_else(|| "General".to_owned());
        let mut button = div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .h(px(28.0))
            .px(px(8.0))
            .rounded(px(radius::MD))
            .text_size(px(11.5))
            .text_color(theme.foreground)
            .cursor_pointer()
            .id("prompt-mode")
            .child(icons::icon(&theme, Icon::Sparkles, 11.0))
            .child(label)
            .child(icons::icon(&theme, Icon::ChevronDown, 11.0));
        button = button.on_click(cx.listener(|this, _, _, cx| {
            this.prompt_menu_open = !this.prompt_menu_open;
            this.model_menu_open = false;
            cx.notify();
        }));

        if !self.prompt_menu_open {
            return button.into_any_element();
        }

        let mut menu = div()
            .flex()
            .flex_col()
            .gap(px(1.0))
            .rounded(px(radius::MD))
            .border_1()
            .border_color(theme.border)
            .bg(theme.card)
            .p(px(3.0));
        for (index, (id, name, _)) in self.prompt_choices.clone().into_iter().enumerate() {
            let active = self.active_prompt == Some(id);
            let mut item = div()
                .flex()
                .items_center()
                .forge_model_item(&theme, active)
                .id(("prompt-option", index))
                .child(name);
            item = item.on_click(cx.listener(move |this, _, _, cx| {
                this.active_prompt = Some(id);
                this.prompt_menu_open = false;
                cx.notify();
            }));
            menu = menu.child(item);
        }

        div()
            .flex()
            .flex_col()
            .items_start()
            .gap(px(4.0))
            .child(button)
            .child(menu)
            .into_any_element()
    }

    fn active_prompt_name(&self) -> Option<String> {
        let id = self.active_prompt?;
        self.prompt_choices
            .iter()
            .find(|(prompt_id, _, _)| *prompt_id == id)
            .map(|(_, name, _)| name.clone())
    }

    /// Switch model and persist it to the active credential.
    pub fn select_model(&mut self, model: &str) {
        self.model = Some(model.to_owned());
        self.model_menu_open = false;
        if let Some(vault) = self.services.vault.as_ref().as_ref()
            && let Some(provider) = self.model_provider.clone()
            && let Ok(Some(mut credential)) = crate::features::credentials::load(
                vault,
                crate::features::credentials::Kind::Chat,
                &provider,
            )
        {
            credential.model = model.to_owned();
            let _ = crate::features::credentials::save(vault, &credential);
        }
    }

    fn input_block(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let draft = self.conversation.draft().to_string();
        let can_submit = self.conversation.can_submit();

        div()
            .flex()
            .flex_col()
            .gap(px(space::SM))
            .border_t_1()
            .border_color(theme.border)
            .bg(Theme::with_alpha(theme.card, self.surface_alpha()))
            .px(px(space::MD))
            .py(px(10.0))
            // Push to talk + tool cluster.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(self.push_to_talk())
                    .child(div().flex_1())
                    .child(self.tool_cluster(cx)),
            )
            // Model row: context on the left, model and image on the right.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::SM))
                    .child(self.prompt_selector(cx))
                    .child(div().flex_1())
                    .child(self.image_toggle(cx))
                    .child(self.model_selector(cx)),
            )
            // Composer.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::SM))
                    .h(px(38.0))
                    .px(px(space::MD))
                    .rounded(px(radius::LG))
                    .border_1()
                    .border_color(theme.input)
                    .id("composer")
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.5))
                            .text_color(if draft.is_empty() {
                                theme.muted_foreground
                            } else {
                                theme.foreground
                            })
                            .child(if draft.is_empty() {
                                "Ask anything, or paste a screenshot…".to_string()
                            } else {
                                draft
                            }),
                    )
                    .child(kbd(&theme, "↵"))
                    .child(
                        div()
                            .size(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(radius::MD))
                            .bg(if can_submit {
                                theme.primary
                            } else {
                                theme.muted
                            })
                            .cursor_pointer()
                            .id("send")
                            .child(icons::icon_colored(
                                if can_submit {
                                    theme.background
                                } else {
                                    theme.muted_foreground
                                },
                                Icon::ArrowUp,
                                13.0,
                            ))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.send_message(cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(self.size_row(cx))
    }

    fn size_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut switcher = div()
            .flex()
            .items_center()
            .gap(px(1.0))
            .p(px(2.0))
            .rounded(px(radius::MD))
            .border_1()
            .border_color(theme.border)
            .bg(theme.muted);

        for size in OverlaySize::ALL {
            let active = size == self.size;
            let mut chip = div()
                .flex()
                .items_center()
                .justify_center()
                .h(px(20.0))
                .min_w(px(20.0))
                .px(px(6.0))
                .rounded(px(radius::SM))
                .text_size(px(10.0))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .text_color(if active {
                    theme.foreground
                } else {
                    theme.muted_foreground
                })
                .id(("size", size as usize))
                .child(size.label());
            if active {
                chip = chip.bg(theme.background);
            }
            switcher = switcher.child(chip.on_click(cx.listener(move |this, _, _, cx| {
                this.size = size;
                cx.notify();
            })));
        }

        div()
            .flex()
            .items_center()
            .gap(px(space::SM))
            .w_full()
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(10.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.muted_foreground)
                    .whitespace_nowrap()
                    .child("Size:"),
            )
            .child(switcher)
            .child(div().flex_1())
            .child(self.opacity_controls(cx))
            .child(kbd(&theme, "Ctrl"))
            .child(kbd(&theme, "\\"))
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(10.5))
                    .text_color(theme.muted_foreground)
                    .whitespace_nowrap()
                    .child("hide"),
            )
    }
}

/// Local time as `HH:MM` without pulling in a date library.
fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    // Derive the wall clock from the local offset the OS reports.
    let local = seconds as i64 + local_offset_seconds();
    let day_seconds = local.rem_euclid(86_400);
    format!("{:02}:{:02}", day_seconds / 3600, (day_seconds % 3600) / 60)
}

/// The system UTC offset in seconds, read once from `date`.
fn local_offset_seconds() -> i64 {
    static OFFSET: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *OFFSET.get_or_init(|| {
        std::process::Command::new("date")
            .arg("+%z")
            .output()
            .ok()
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .and_then(|text| {
                let text = text.trim().to_owned();
                if text.len() < 5 {
                    return None;
                }
                let sign = if text.starts_with('-') { -1 } else { 1 };
                let hours: i64 = text[1..3].parse().ok()?;
                let minutes: i64 = text[3..5].parse().ok()?;
                Some(sign * (hours * 3600 + minutes * 60))
            })
            .unwrap_or(0)
    })
}

/// Menu row styling shared by the two pickers.
trait ModelItemStyle: Sized {
    fn forge_model_item(self, theme: &Theme, active: bool) -> Self;
}

impl ModelItemStyle for gpui::Div {
    fn forge_model_item(mut self, theme: &Theme, active: bool) -> Self {
        self = self
            .h(px(24.0))
            .px(px(8.0))
            .rounded(px(5.0))
            .text_size(px(11.0))
            .cursor_pointer()
            .text_color(if active {
                theme.foreground
            } else {
                theme.muted_foreground
            });
        if active {
            self = self.bg(theme.muted);
        }
        self
    }
}

/// Render markdown into styled GPUI blocks.
pub fn render_markdown(theme: &Theme, text: &str) -> gpui::AnyElement {
    use crate::features::markdown::{Block, Span};

    fn span_element(theme: &Theme, span: &Span) -> gpui::AnyElement {
        let mut text = div().text_size(px(12.5)).child(span.text.clone());
        let colour = if span.style.code {
            theme.success
        } else if span.style.strikethrough {
            theme.muted_foreground
        } else {
            theme.foreground
        };
        text = text.text_color(colour);
        if span.style.bold {
            text = text.font_weight(FontWeight::SEMIBOLD);
        }
        if span.style.italic {
            text = text.italic();
        }
        if span.style.strikethrough {
            text = text.line_through();
        }
        if span.style.code {
            text = text
                .font_family("monospace")
                .px(px(3.0))
                .rounded(px(3.0))
                .bg(theme.card);
        }
        text.into_any_element()
    }

    let mut column = div().flex().flex_col().gap(px(6.0)).max_w(px(520.0));

    for block in crate::features::markdown::parse(text) {
        let element: gpui::AnyElement = match block {
            Block::Heading { level, spans } => {
                let size = match level {
                    1 => 16.0,
                    2 => 14.5,
                    3 => 13.5,
                    _ => 12.5,
                };
                let mut row = div()
                    .flex()
                    .flex_wrap()
                    .text_size(px(size))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.foreground);
                for span in &spans {
                    row = row.child(span_element(theme, span));
                }
                row.into_any_element()
            }
            Block::Paragraph { spans } => {
                let mut row = div().flex().flex_wrap().items_baseline();
                for span in &spans {
                    row = row.child(span_element(theme, span));
                }
                row.into_any_element()
            }
            Block::Quote { spans } => {
                let mut row = div()
                    .flex()
                    .flex_wrap()
                    .pl(px(8.0))
                    .border_l_1()
                    .border_color(theme.accent)
                    .text_color(theme.muted_foreground);
                for span in &spans {
                    row = row.child(span_element(theme, span));
                }
                row.into_any_element()
            }
            Block::ListItem { marker, spans, .. } => {
                let mut row = div().flex().flex_wrap().gap(px(6.0)).pl(px(4.0));
                row = row.child(
                    div()
                        .text_size(px(12.5))
                        .text_color(theme.muted_foreground)
                        .child(marker),
                );
                for span in &spans {
                    row = row.child(span_element(theme, span));
                }
                row.into_any_element()
            }
            Block::Code { language, text } => {
                let mut panel = div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.card)
                    .px(px(10.0))
                    .py(px(8.0));
                if let Some(language) = language {
                    panel = panel.child(
                        div()
                            .text_size(px(9.5))
                            .text_color(theme.muted_foreground)
                            .child(language),
                    );
                }
                panel
                    .child(
                        div()
                            .font_family("monospace")
                            .text_size(px(11.5))
                            .text_color(theme.foreground)
                            .child(text),
                    )
                    .into_any_element()
            }
            Block::Table { headers, rows } => {
                // Fixed column widths keep every row aligned instead of letting
                // the longest cell decide the layout.
                let columns = headers
                    .len()
                    .max(rows.iter().map(Vec::len).max().unwrap_or(1));
                let cell_width = px((520.0 / columns.max(1) as f32).clamp(70.0, 180.0));
                let mut table = div()
                    .flex()
                    .flex_col()
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(theme.border);

                let mut header_row = div().flex().bg(theme.card);
                for index in 0..columns {
                    header_row = header_row.child(
                        div()
                            .w(cell_width)
                            .flex_shrink_0()
                            .px(px(8.0))
                            .py(px(5.0))
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.foreground)
                            .child(headers.get(index).cloned().unwrap_or_default()),
                    );
                }
                table = table.child(header_row);

                for row in &rows {
                    let mut cells = div().flex().border_t_1().border_color(theme.border);
                    for index in 0..columns {
                        cells = cells.child(
                            div()
                                .w(cell_width)
                                .flex_shrink_0()
                                .px(px(8.0))
                                .py(px(4.0))
                                .text_size(px(11.5))
                                .text_color(theme.muted_foreground)
                                .child(row.get(index).cloned().unwrap_or_default()),
                        );
                    }
                    table = table.child(cells);
                }
                table.into_any_element()
            }
            Block::Rule => div()
                .h(px(1.0))
                .w_full()
                .bg(theme.border)
                .into_any_element(),
        };
        column = column.child(element);
    }

    column.into_any_element()
}

/// A small keyboard chip.
fn kbd(theme: &Theme, label: &'static str) -> impl IntoElement {
    div()
        .rounded(px(4.0))
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted)
        .px(px(4.0))
        .text_size(px(9.5))
        .text_color(theme.muted_foreground)
        .child(label)
}

impl Render for Overlay {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_shortcut_poll(cx);
        let theme = self.theme;
        let mut body = div().flex().flex_col().flex_1();
        if self.conversation.is_empty() {
            body = body.child(self.empty_state());
        } else {
            body = body.child(self.message_list());
        }

        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(gpui::Rgba {
                a: self.opacity,
                ..theme.background
            })
            .text_color(theme.foreground)
            .child(self.header(cx))
            .children(self.listen_banner(cx))
            // The body owns the remaining height so the empty state can centre in
            // it, and follow-ups sit directly above the input block.
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .child(body)
                    .child(self.follow_ups(cx)),
            )
            .child(self.input_block(cx))
    }
}
