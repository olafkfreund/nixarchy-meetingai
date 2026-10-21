//! Database schema definitions and default seeded records.

pub mod seed;

pub use seed::*;

/// The embedded schema, applied on open.
pub const SCHEMA: &str = include_str!("schema.sql");
