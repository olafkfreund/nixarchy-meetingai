//! SQLite schema and the built-in prompts.
pub mod seed;
pub use seed::*;

pub const SCHEMA: &str = include_str!("schema.sql");
