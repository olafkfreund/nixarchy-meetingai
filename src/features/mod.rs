//! Feature module facades and backwards-compatibility re-exports.
//!
//! The codebase is organized into domain-specific modules:
//! - [`crate::core`]: Domain models, settings, and encrypted vault
//! - [`crate::db`]: SQLite database persistence
//! - [`crate::audio`]: Sound processing, microphone, and system audio capture
//! - [`crate::speech`]: Speech-to-text and live listening sessions
//! - [`crate::ai`]: Chat engines, providers, models, and local CLI tools
//! - [`crate::platform`]: Display server, compositors, window management, and shortcuts
//! - [`crate::ui`]: GPUI views, design system tokens, and widgets

pub use crate::core::credentials;
pub use crate::core::domain;
pub use crate::core::settings;
pub use crate::core::vault;

pub use crate::db;

pub use crate::audio;
pub use crate::audio::devices;
pub use crate::audio::microphone;
pub use crate::audio::system_audio;

pub use crate::speech::listen;
pub use crate::speech::transcription;

pub use crate::ai::conversation as chat;
pub use crate::ai::engine as chat_engine;
pub use crate::ai::local_cli as cli;
pub use crate::ai::models;
pub use crate::ai::providers;
pub use crate::ai::session as chat_session;

pub use crate::platform::capture;
pub use crate::platform::control;
pub use crate::platform::hyprland;
pub use crate::platform::screen;
pub use crate::platform::shortcuts;
pub use crate::platform::tray;
pub use crate::platform::wayland;
pub use crate::platform::window;

pub use crate::ui::components;
pub use crate::ui::controls;
pub use crate::ui::dashboard;
pub use crate::ui::icons;
pub use crate::ui::markdown;
pub use crate::ui::overlay;
pub use crate::ui::settings as settings_pages;
pub use crate::ui::text_input;
pub use crate::ui::theme;
