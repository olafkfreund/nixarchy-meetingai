//! GPUI views, widgets, and design system.

pub mod theme;
pub mod views;
pub mod widgets;

// Convenient re-exports for easy access:
pub use theme::{Theme, icons, radius, space, tokens};
pub use views::{dashboard, overlay, settings};
pub use widgets::{components, controls, markdown, panels, text_input};
