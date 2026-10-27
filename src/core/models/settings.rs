//! User settings.
//!
//! A typed view over the `app_settings` key/value table. Every value round-trips
//! through here, so defaults, parsing and validation live in one place instead of
//! being repeated in each page.

use crate::features::db::{Database, Result, settings as store};

/// Overlay theme choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeChoice {
    #[default]
    Dark,
    Light,
    Glass,
    System,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 4] = [
        ThemeChoice::Dark,
        ThemeChoice::Light,
        ThemeChoice::Glass,
        ThemeChoice::System,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            ThemeChoice::Dark => "Dark",
            ThemeChoice::Light => "Light",
            ThemeChoice::Glass => "Glass",
            ThemeChoice::System => "System",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            ThemeChoice::Dark => "dark",
            ThemeChoice::Light => "light",
            ThemeChoice::Glass => "glass",
            ThemeChoice::System => "system",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "light" => ThemeChoice::Light,
            "glass" => ThemeChoice::Glass,
            "system" => ThemeChoice::System,
            _ => ThemeChoice::Dark,
        }
    }
}

/// When screenshots are taken automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutoScreenshot {
    #[default]
    Off,
    EveryQuestion,
    Interval,
    Once,
}

impl AutoScreenshot {
    pub const ALL: [AutoScreenshot; 4] = [
        AutoScreenshot::Off,
        AutoScreenshot::EveryQuestion,
        AutoScreenshot::Interval,
        AutoScreenshot::Once,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            AutoScreenshot::Off => "Off",
            AutoScreenshot::EveryQuestion => "Every question",
            AutoScreenshot::Interval => "On an interval",
            AutoScreenshot::Once => "Next question only",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            AutoScreenshot::Off => "off",
            AutoScreenshot::EveryQuestion => "always",
            AutoScreenshot::Interval => "interval",
            AutoScreenshot::Once => "once",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "always" => AutoScreenshot::EveryQuestion,
            "interval" => AutoScreenshot::Interval,
            "once" => AutoScreenshot::Once,
            _ => AutoScreenshot::Off,
        }
    }
}

/// Which pane the overlay opens on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrimaryTab {
    #[default]
    Ask,
    Listen,
}

impl PrimaryTab {
    pub const ALL: [PrimaryTab; 2] = [PrimaryTab::Ask, PrimaryTab::Listen];

    pub const fn label(self) -> &'static str {
        match self {
            PrimaryTab::Ask => "Ask",
            PrimaryTab::Listen => "Listen",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            PrimaryTab::Ask => "ask",
            PrimaryTab::Listen => "listen",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "listen" => PrimaryTab::Listen,
            _ => PrimaryTab::Ask,
        }
    }
}

/// Which remote utterances may request an automatic answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutoResponse {
    Off,
    #[default]
    Questions,
    Pauses,
}

impl AutoResponse {
    pub const ALL: [Self; 3] = [Self::Off, Self::Questions, Self::Pauses];
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Questions => "When a question is asked",
            Self::Pauses => "After every pause",
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Questions => "questions",
            Self::Pauses => "pauses",
        }
    }
    fn parse(value: &str) -> Self {
        match value {
            "off" => Self::Off,
            "pauses" => Self::Pauses,
            _ => Self::Questions,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResponsePace {
    Fast,
    #[default]
    Balanced,
    Relaxed,
}
impl ResponsePace {
    pub const ALL: [Self; 3] = [Self::Fast, Self::Balanced, Self::Relaxed];
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fast => "Fast",
            Self::Balanced => "Balanced",
            Self::Relaxed => "Relaxed",
        }
    }
    pub fn delay(self) -> std::time::Duration {
        std::time::Duration::from_millis(match self {
            Self::Fast => 400,
            Self::Balanced => 1000,
            Self::Relaxed => 2000,
        })
    }
    fn parse(value: &str) -> Self {
        match value {
            "Fast" => Self::Fast,
            "Relaxed" => Self::Relaxed,
            _ => Self::Balanced,
        }
    }
}

pub const FOLLOW_UP_PROMPT: &str = "Using the latest exchange and the user's meeting context, suggest three distinct, useful questions the user could ask next. Prefer clarifications, concrete next steps or deeper follow-ups. Do not answer the questions, repeat something already answered, or invent facts. Return one short question per line, without numbering or commentary.";

/// Global answer detail preference, captured when a new conversation starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResponseLength {
    Short,
    Medium,
    #[default]
    Auto,
}
impl ResponseLength {
    pub const ALL: [Self; 3] = [Self::Short, Self::Medium, Self::Auto];
    pub fn label(self) -> &'static str {
        match self {
            Self::Short => "Short",
            Self::Medium => "Medium",
            Self::Auto => "Auto",
        }
    }
    fn parse(value: &str) -> Self {
        match value {
            "Short" => Self::Short,
            "Medium" => Self::Medium,
            _ => Self::Auto,
        }
    }
    pub fn instruction(self) -> &'static str {
        match self {
            Self::Short => {
                "Keep the answer short: lead with the direct answer, use only essential supporting points, and avoid a long introduction."
            }
            Self::Medium => {
                "Use balanced detail: explain the answer and key reasoning with enough context to act, without unnecessary repetition."
            }
            Self::Auto => {
                "Choose the response length to match the complexity of the question: brief for simple questions, more detailed when the task needs explanation."
            }
        }
    }
}

pub const RESPONSE_LANGUAGES: &[&str] = &[
    "Auto",
    "English",
    "Hindi",
    "Bengali",
    "Tamil",
    "Telugu",
    "Marathi",
    "Gujarati",
    "Kannada",
    "Malayalam",
    "Punjabi",
    "Urdu",
    "Spanish",
    "French",
    "German",
    "Italian",
    "Portuguese",
    "Dutch",
    "Polish",
    "Russian",
    "Ukrainian",
    "Arabic",
    "Turkish",
    "Chinese",
    "Japanese",
    "Korean",
    "Indonesian",
    "Vietnamese",
];
pub fn response_instruction(length: ResponseLength, language: &str) -> String {
    let language = if language == "Auto" || language.trim().is_empty() {
        "Use the language of the user's latest question unless they ask for another language."
            .to_owned()
    } else {
        format!(
            "Write your response in {language}, except code, names and quoted source material where translation would change their meaning."
        )
    };
    format!(
        "Global response preferences: {} {language}",
        length.instruction()
    )
}

/// All persisted settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub theme: ThemeChoice,
    pub transparency: u32,
    /// UI text size as a percentage of the base scale (85 to 140).
    pub font_scale: u32,
    pub show_borders: bool,
    pub autostart: bool,
    pub chat_provider: String,
    pub always_on_top: bool,
    pub reduce_motion: bool,
    pub overlay_size: String,
    pub primary_tab: PrimaryTab,
    pub capture_delay_seconds: u32,
    pub auto_screenshot: AutoScreenshot,
    pub screenshot_interval: u32,
    pub use_image_on_send: bool,
    pub input_device: String,
    pub output_device: String,
    pub speech_provider: String,
    pub speech_model: String,
    pub speech_language: String,
    pub response_length: ResponseLength,
    pub response_language: String,
    pub auto_scroll: bool,
    pub auto_recap: bool,
    pub smart_follow_ups: bool,
    pub smart_follow_ups_prompt: String,
    pub cli_connection: String,
    pub personal_context: String,
    pub meeting_context: String,
    pub auto_response: AutoResponse,
    pub response_pace: ResponsePace,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::Dark,
            transparency: 50,
            font_scale: 100,
            show_borders: true,
            autostart: false,
            chat_provider: String::new(),
            always_on_top: true,
            reduce_motion: false,
            overlay_size: "sm".to_owned(),
            primary_tab: PrimaryTab::Ask,
            capture_delay_seconds: 0,
            auto_screenshot: AutoScreenshot::Off,
            screenshot_interval: 30,
            use_image_on_send: false,
            input_device: String::new(),
            output_device: String::new(),
            speech_provider: "deepgram-stt".into(),
            speech_model: String::new(),
            speech_language: String::new(),
            response_length: ResponseLength::Auto,
            response_language: "Auto".into(),
            auto_scroll: true,
            auto_recap: false,
            smart_follow_ups: true,
            smart_follow_ups_prompt: FOLLOW_UP_PROMPT.to_owned(),
            cli_connection: String::new(),
            personal_context: String::new(),
            meeting_context: String::new(),
            auto_response: AutoResponse::default(),
            response_pace: ResponsePace::default(),
        }
    }
}

/// Keys used in `app_settings`.
mod keys {
    pub const THEME: &str = "theme";
    pub const TRANSPARENCY: &str = "transparency";
    pub const FONT_SCALE: &str = "font_scale";
    pub const SHOW_BORDERS: &str = "show_borders";
    pub const AUTOSTART: &str = "autostart";
    pub const ALWAYS_ON_TOP: &str = "always_on_top";
    pub const REDUCE_MOTION: &str = "reduce_motion";
    pub const OVERLAY_SIZE: &str = "overlay_size";
    pub const PRIMARY_TAB: &str = "primary_tab";
    pub const CAPTURE_DELAY: &str = "capture_delay_seconds";
    pub const AUTO_SCREENSHOT: &str = "auto_screenshot";
    pub const SCREENSHOT_INTERVAL: &str = "screenshot_interval";
    pub const USE_IMAGE_ON_SEND: &str = "use_image_on_send";
    pub const INPUT_DEVICE: &str = "input_device";
    pub const OUTPUT_DEVICE: &str = "output_device";
    pub const SPEECH_PROVIDER: &str = "speech_provider";
    pub const SPEECH_MODEL: &str = "speech_model";
    pub const SMART_FOLLOW_UPS: &str = "smart_follow_ups";
    pub const SMART_FOLLOW_UPS_PROMPT: &str = "smart_follow_ups_prompt";
    pub const CLI_CONNECTION: &str = "cli_connection";
}

impl Settings {
    /// Load every setting, falling back to defaults for missing or malformed values.
    pub fn load(database: &Database) -> Result<Self> {
        let raw = store::all(database.connection())?;
        let defaults = Self::default();
        let get = |key: &str| raw.get(key).map(String::as_str);
        let flag = |key: &str, fallback: bool| get(key).map(|v| v == "true").unwrap_or(fallback);
        let number = |key: &str, fallback: u32| {
            get(key)
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(fallback)
        };
        let text = |key: &str, fallback: &str| {
            get(key)
                .map(str::to_owned)
                .unwrap_or_else(|| fallback.to_owned())
        };

        Ok(Self {
            theme: get(keys::THEME)
                .map(ThemeChoice::parse)
                .unwrap_or(defaults.theme),
            transparency: number(keys::TRANSPARENCY, defaults.transparency).min(100),
            // Clamped so the layout never collapses or overflows its panels.
            font_scale: number(keys::FONT_SCALE, defaults.font_scale).clamp(85, 140),
            show_borders: flag(keys::SHOW_BORDERS, defaults.show_borders),
            autostart: flag(keys::AUTOSTART, defaults.autostart),
            chat_provider: text("chat_provider", &defaults.chat_provider),
            always_on_top: flag(keys::ALWAYS_ON_TOP, defaults.always_on_top),
            reduce_motion: flag(keys::REDUCE_MOTION, defaults.reduce_motion),
            overlay_size: text(keys::OVERLAY_SIZE, &defaults.overlay_size),
            primary_tab: get(keys::PRIMARY_TAB)
                .map(PrimaryTab::parse)
                .unwrap_or(defaults.primary_tab),
            capture_delay_seconds: number(keys::CAPTURE_DELAY, defaults.capture_delay_seconds)
                .min(60),
            auto_screenshot: get(keys::AUTO_SCREENSHOT)
                .map(AutoScreenshot::parse)
                .unwrap_or(defaults.auto_screenshot),
            screenshot_interval: number(keys::SCREENSHOT_INTERVAL, defaults.screenshot_interval)
                .clamp(5, 600),
            use_image_on_send: flag(keys::USE_IMAGE_ON_SEND, defaults.use_image_on_send),
            input_device: text(keys::INPUT_DEVICE, &defaults.input_device),
            output_device: text(keys::OUTPUT_DEVICE, &defaults.output_device),
            speech_provider: text(keys::SPEECH_PROVIDER, &defaults.speech_provider),
            speech_model: text(keys::SPEECH_MODEL, &defaults.speech_model),
            speech_language: text("speech_language", &defaults.speech_language),
            response_length: get("response_length")
                .map(ResponseLength::parse)
                .unwrap_or_default(),
            response_language: text("response_language", &defaults.response_language),
            auto_scroll: flag("auto_scroll", defaults.auto_scroll),
            auto_recap: flag("auto_recap", defaults.auto_recap),
            smart_follow_ups: flag(keys::SMART_FOLLOW_UPS, defaults.smart_follow_ups),
            smart_follow_ups_prompt: match get(keys::SMART_FOLLOW_UPS_PROMPT) {
                None | Some("Suggest three short follow-up questions.") => {
                    FOLLOW_UP_PROMPT.to_owned()
                }
                Some(prompt) => prompt.to_owned(),
            },
            cli_connection: text(keys::CLI_CONNECTION, &defaults.cli_connection),
            personal_context: text("personal_context", "").chars().take(16_000).collect(),
            meeting_context: text("meeting_context", "").chars().take(16_000).collect(),
            auto_response: get("auto_response")
                .map(AutoResponse::parse)
                .unwrap_or_default(),
            response_pace: get("response_pace")
                .map(ResponsePace::parse)
                .unwrap_or_default(),
        })
    }

    /// Load settings from a database path, without a long-lived connection.
    ///
    /// The overlay uses this on demand: it should not hold a connection open just
    /// to read one preference.
    pub fn load_from_path(path: Option<&std::path::Path>) -> Option<Self> {
        let path = path?;
        let database = Database::open(path).ok()?;
        Self::load(&database).ok()
    }

    /// Persist every setting.
    pub fn save(&self, database: &Database) -> Result<()> {
        for (key, value) in self.pairs() {
            store::set(database.connection(), key, &value)?;
        }
        Ok(())
    }

    /// Write only fields this window changed, preserving context saved elsewhere.
    pub fn save_changed(&self, previous: &Self, database: &Database) -> Result<()> {
        let previous: std::collections::BTreeMap<_, _> = previous.pairs().into_iter().collect();
        for (key, value) in self.pairs() {
            if previous.get(key) != Some(&value) {
                store::set(database.connection(), key, &value)?;
            }
        }
        Ok(())
    }

    fn pairs(&self) -> Vec<(&'static str, String)> {
        vec![
            (keys::THEME, self.theme.as_str().to_owned()),
            (keys::TRANSPARENCY, self.transparency.to_string()),
            (keys::FONT_SCALE, self.font_scale.to_string()),
            (keys::SHOW_BORDERS, self.show_borders.to_string()),
            (keys::AUTOSTART, self.autostart.to_string()),
            ("chat_provider", self.chat_provider.clone()),
            (keys::ALWAYS_ON_TOP, self.always_on_top.to_string()),
            (keys::REDUCE_MOTION, self.reduce_motion.to_string()),
            (keys::OVERLAY_SIZE, self.overlay_size.clone()),
            (keys::PRIMARY_TAB, self.primary_tab.as_str().to_owned()),
            (keys::CAPTURE_DELAY, self.capture_delay_seconds.to_string()),
            (
                keys::AUTO_SCREENSHOT,
                self.auto_screenshot.as_str().to_owned(),
            ),
            (
                keys::SCREENSHOT_INTERVAL,
                self.screenshot_interval.to_string(),
            ),
            (keys::USE_IMAGE_ON_SEND, self.use_image_on_send.to_string()),
            (keys::INPUT_DEVICE, self.input_device.clone()),
            (keys::OUTPUT_DEVICE, self.output_device.clone()),
            (keys::SPEECH_PROVIDER, self.speech_provider.clone()),
            (keys::SPEECH_MODEL, self.speech_model.clone()),
            ("response_length", self.response_length.label().into()),
            ("response_language", self.response_language.clone()),
            ("auto_scroll", self.auto_scroll.to_string()),
            ("auto_recap", self.auto_recap.to_string()),
            (
                "speech_language",
                self.speech_language.trim().chars().take(32).collect(),
            ),
            (keys::SMART_FOLLOW_UPS, self.smart_follow_ups.to_string()),
            (
                keys::SMART_FOLLOW_UPS_PROMPT,
                self.smart_follow_ups_prompt.clone(),
            ),
            (keys::CLI_CONNECTION, self.cli_connection.clone()),
            (
                "personal_context",
                self.personal_context.chars().take(16_000).collect(),
            ),
            (
                "meeting_context",
                self.meeting_context.chars().take(16_000).collect(),
            ),
            ("auto_response", self.auto_response.as_str().to_owned()),
            ("response_pace", self.response_pace.label().to_owned()),
        ]
    }

    /// Write a single setting without touching the rest.
    pub fn set(database: &Database, key: &str, value: &str) -> Result<()> {
        store::set(database.connection(), key, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn window_saves_preserve_context_updated_by_another_window() {
        let db = database();
        let previous = Settings::default();
        previous.save(&db).unwrap();
        Settings::set(&db, "personal_context", "New profile from the overlay").unwrap();
        let mut settings = previous.clone();
        settings.response_pace = ResponsePace::Fast;
        settings.auto_response = AutoResponse::Off;
        settings.save_changed(&previous, &db).unwrap();
        let loaded = Settings::load(&db).unwrap();
        assert_eq!(loaded.personal_context, "New profile from the overlay");
        assert_eq!(loaded.auto_response, AutoResponse::Off);
        assert_eq!(loaded.response_pace, ResponsePace::Fast);
        assert!(ResponsePace::Fast.delay() < ResponsePace::Balanced.delay());
        assert!(ResponsePace::Balanced.delay() < ResponsePace::Relaxed.delay());
    }

    #[test]
    fn question_prompt_migration_preserves_custom_instructions_and_bounds_context() {
        let db = database();
        Settings::set(
            &db,
            "smart_follow_ups_prompt",
            "Suggest three short follow-up questions.",
        )
        .unwrap();
        assert_eq!(
            Settings::load(&db).unwrap().smart_follow_ups_prompt,
            FOLLOW_UP_PROMPT
        );
        Settings::set(&db, "smart_follow_ups_prompt", "My custom questions").unwrap();
        Settings::set(&db, "personal_context", &"é".repeat(17_000)).unwrap();
        let loaded = Settings::load(&db).unwrap();
        assert_eq!(loaded.smart_follow_ups_prompt, "My custom questions");
        assert_eq!(loaded.personal_context.chars().count(), 16_000);
    }

    #[test]
    fn defaults_load_when_nothing_is_stored() {
        let settings = Settings::load(&database()).unwrap();
        assert_eq!(settings.theme, ThemeChoice::Dark);
        assert_eq!(settings.transparency, 50);
        assert!(settings.always_on_top);
        assert_eq!(settings.overlay_size, "sm");
    }

    #[test]
    fn round_trips_every_field() {
        let database = database();
        let settings = Settings {
            theme: ThemeChoice::Light,
            transparency: 80,
            font_scale: 120,
            show_borders: false,
            autostart: true,
            chat_provider: "gemini".into(),
            always_on_top: false,
            reduce_motion: true,
            overlay_size: "lg".into(),
            primary_tab: PrimaryTab::Listen,
            capture_delay_seconds: 3,
            auto_screenshot: AutoScreenshot::Interval,
            screenshot_interval: 45,
            use_image_on_send: true,
            input_device: "mic-1".into(),
            output_device: "spk-1".into(),
            speech_provider: "deepgram-stt".into(),
            speech_model: "nova-3".into(),
            speech_language: "en-US".into(),
            response_length: ResponseLength::Medium,
            response_language: "Hindi".into(),
            auto_scroll: false,
            auto_recap: true,
            smart_follow_ups: false,
            smart_follow_ups_prompt: "custom".into(),
            cli_connection: "".into(),
            personal_context: "I build Rust apps".into(),
            meeting_context: "Interview for a platform role".into(),
            auto_response: AutoResponse::Pauses,
            response_pace: ResponsePace::Relaxed,
        };
        settings.save(&database).unwrap();
        assert_eq!(Settings::load(&database).unwrap(), settings);
    }

    #[test]
    fn malformed_values_fall_back_to_defaults() {
        let database = database();
        Settings::set(&database, "transparency", "banana").unwrap();
        Settings::set(&database, "theme", "neon").unwrap();
        let settings = Settings::load(&database).unwrap();
        assert_eq!(settings.transparency, 50);
        assert_eq!(settings.theme, ThemeChoice::Dark);
    }

    #[test]
    fn transparency_and_interval_are_clamped() {
        let database = database();
        Settings::set(&database, "transparency", "500").unwrap();
        Settings::set(&database, "screenshot_interval", "1").unwrap();
        let settings = Settings::load(&database).unwrap();
        // 500 is clamped to the ceiling, not to the default.
        assert_eq!(settings.transparency, 100);
        assert_eq!(settings.screenshot_interval, 5);
    }

    #[test]
    fn capture_timing_rejects_out_of_range_persisted_values() {
        let database = database();
        Settings::set(&database, "capture_delay_seconds", "4294967295").unwrap();
        Settings::set(&database, "screenshot_interval", "4294967295").unwrap();
        let settings = Settings::load(&database).unwrap();
        assert_eq!(settings.capture_delay_seconds, 60);
        assert_eq!(settings.screenshot_interval, 600);
    }

    #[test]
    fn enums_round_trip_through_strings() {
        for choice in ThemeChoice::ALL {
            assert_eq!(ThemeChoice::parse(choice.as_str()), choice);
        }
        for mode in AutoScreenshot::ALL {
            assert_eq!(AutoScreenshot::parse(mode.as_str()), mode);
        }
        for tab in PrimaryTab::ALL {
            assert_eq!(PrimaryTab::parse(tab.as_str()), tab);
        }
    }

    #[test]
    fn saving_one_field_preserves_the_rest() {
        let database = database();
        let mut settings = Settings::load(&database).unwrap();
        settings.autostart = true;
        settings.save(&database).unwrap();
        Settings::set(&database, "overlay_size", "xl").unwrap();
        let reloaded = Settings::load(&database).unwrap();
        assert!(reloaded.autostart);
        assert_eq!(reloaded.overlay_size, "xl");
    }
}
