//! nixarchy-meetingai — a desktop assistant with live transcription, chat, and notes.

pub mod ai;
pub mod app;
pub mod audio;
pub mod cli;
pub mod core;
pub mod db;
pub mod features;
pub mod platform;
pub mod speech;
pub mod ui;

pub use app::Services;
pub use app::run;
