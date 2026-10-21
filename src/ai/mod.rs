//! AI assistants, chat completions, model catalogs, and provider configurations.

pub mod catalog;
pub mod chat;

// Direct re-exports
pub use catalog::local_cli;
pub use catalog::models;
pub use catalog::providers;
pub use catalog::*;

pub use chat::conversation;
pub use chat::engine;
pub use chat::session;
pub use chat::*;
