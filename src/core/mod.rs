//! Core domain types, user settings, and secure credential storage.

pub mod composer;
pub mod context;
pub mod models;
pub mod security;

pub use models::domain;
pub use models::settings;
pub use models::*;
pub use security::credentials;
pub use security::vault;
pub use security::*;

/// Use one TLS provider for HTTP and speech WebSockets on every platform.
/// Explicit setup also covers tests and tools that do not launch the UI.
pub(crate) fn install_tls_provider() {
    static INSTALLED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    INSTALLED.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}
