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
    System,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [ThemeChoice::Dark, ThemeChoice::Light, ThemeChoice::System];

    pub const fn label(self) -> &'static str {
        match self {
            ThemeChoice::Dark => "Dark",
            ThemeChoice::Light => "Light",
            ThemeChoice::System => "System",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            ThemeChoice::Dark => "dark",
            ThemeChoice::Light => "light",
            ThemeChoice::System => "system",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "light" => ThemeChoice::Light,
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

/// All persisted settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub theme: ThemeChoice,
    pub transparency: u32,
    pub autostart: bool,
    pub hide_app_icon: bool,
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
    pub smart_follow_ups: bool,
    pub smart_follow_ups_prompt: String,
    pub cli_connection: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::Dark,
            transparency: 100,
            autostart: false,
            hide_app_icon: false,
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
            speech_provider: String::new(),
            speech_model: String::new(),
            smart_follow_ups: true,
            smart_follow_ups_prompt: "Suggest three short follow-up questions.".to_owned(),
            cli_connection: String::new(),
        }
    }
}

/// Keys used in `app_settings`.
mod keys {
    pub const THEME: &str = "theme";
    pub const TRANSPARENCY: &str = "transparency";
    pub const AUTOSTART: &str = "autostart";
    pub const HIDE_APP_ICON: &str = "hide_app_icon";
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
            autostart: flag(keys::AUTOSTART, defaults.autostart),
            hide_app_icon: flag(keys::HIDE_APP_ICON, defaults.hide_app_icon),
            always_on_top: flag(keys::ALWAYS_ON_TOP, defaults.always_on_top),
            reduce_motion: flag(keys::REDUCE_MOTION, defaults.reduce_motion),
            overlay_size: text(keys::OVERLAY_SIZE, &defaults.overlay_size),
            primary_tab: get(keys::PRIMARY_TAB)
                .map(PrimaryTab::parse)
                .unwrap_or(defaults.primary_tab),
            capture_delay_seconds: number(keys::CAPTURE_DELAY, defaults.capture_delay_seconds),
            auto_screenshot: get(keys::AUTO_SCREENSHOT)
                .map(AutoScreenshot::parse)
                .unwrap_or(defaults.auto_screenshot),
            screenshot_interval: number(keys::SCREENSHOT_INTERVAL, defaults.screenshot_interval)
                .max(5),
            use_image_on_send: flag(keys::USE_IMAGE_ON_SEND, defaults.use_image_on_send),
            input_device: text(keys::INPUT_DEVICE, &defaults.input_device),
            output_device: text(keys::OUTPUT_DEVICE, &defaults.output_device),
            speech_provider: text(keys::SPEECH_PROVIDER, &defaults.speech_provider),
            speech_model: text(keys::SPEECH_MODEL, &defaults.speech_model),
            smart_follow_ups: flag(keys::SMART_FOLLOW_UPS, defaults.smart_follow_ups),
            smart_follow_ups_prompt: text(
                keys::SMART_FOLLOW_UPS_PROMPT,
                &defaults.smart_follow_ups_prompt,
            ),
            cli_connection: text(keys::CLI_CONNECTION, &defaults.cli_connection),
        })
    }

    /// Persist every setting.
    pub fn save(&self, database: &Database) -> Result<()> {
        let connection = database.connection();
        let pairs = [
            (keys::THEME, self.theme.as_str().to_owned()),
            (keys::TRANSPARENCY, self.transparency.to_string()),
            (keys::AUTOSTART, self.autostart.to_string()),
            (keys::HIDE_APP_ICON, self.hide_app_icon.to_string()),
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
            (keys::SMART_FOLLOW_UPS, self.smart_follow_ups.to_string()),
            (
                keys::SMART_FOLLOW_UPS_PROMPT,
                self.smart_follow_ups_prompt.clone(),
            ),
            (keys::CLI_CONNECTION, self.cli_connection.clone()),
        ];
        for (key, value) in pairs {
            store::set(connection, key, &value)?;
        }
        Ok(())
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
    fn defaults_load_when_nothing_is_stored() {
        let settings = Settings::load(&database()).unwrap();
        assert_eq!(settings.theme, ThemeChoice::Dark);
        assert_eq!(settings.transparency, 100);
        assert!(settings.always_on_top);
        assert_eq!(settings.overlay_size, "sm");
    }

    #[test]
    fn round_trips_every_field() {
        let database = database();
        let settings = Settings {
            theme: ThemeChoice::Light,
            transparency: 80,
            autostart: true,
            hide_app_icon: true,
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
            speech_provider: "openai-whisper".into(),
            speech_model: "whisper-1".into(),
            smart_follow_ups: false,
            smart_follow_ups_prompt: "custom".into(),
            cli_connection: "claude".into(),
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
        assert_eq!(settings.transparency, 100);
        assert_eq!(settings.theme, ThemeChoice::Dark);
    }

    #[test]
    fn transparency_and_interval_are_clamped() {
        let database = database();
        Settings::set(&database, "transparency", "500").unwrap();
        Settings::set(&database, "screenshot_interval", "1").unwrap();
        let settings = Settings::load(&database).unwrap();
        assert_eq!(settings.transparency, 100);
        assert_eq!(settings.screenshot_interval, 5);
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
