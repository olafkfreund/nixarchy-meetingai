//! lennette — a desktop assistant with live transcription, chat, and notes.

pub mod app;
pub mod cli;
pub mod features;

pub use app::Services;

pub use app::run;
