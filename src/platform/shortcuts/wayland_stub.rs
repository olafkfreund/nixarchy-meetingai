//! Wayland portals do not exist on Windows; native hotkeys are used instead.
use super::Action;

#[derive(Debug, thiserror::Error)]
pub enum PortalError {
    #[error("{0}")]
    Bus(String),
    #[error("Wayland portals are only available on Linux")]
    Unsupported,
}

pub struct PortalShortcuts;
impl PortalShortcuts {
    pub async fn bind(_: &[Action]) -> Result<Self, PortalError> {
        Err(PortalError::Unsupported)
    }
    pub fn drain(&self) -> Vec<Action> {
        Vec::new()
    }
}
