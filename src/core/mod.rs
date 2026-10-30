//! Shared foundations: domain types, settings, secrets, attachments and HTTP plumbing.

pub mod context;
pub mod export;
pub mod models;
pub(crate) mod network;
pub mod security;

pub use models::domain;
pub use models::settings;
pub use models::*;
pub use security::credentials;
pub use security::vault;
pub use security::*;

/// Make rustls' `ring` provider the process default, once; an earlier install elsewhere is fine.
pub(crate) fn install_tls_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}
