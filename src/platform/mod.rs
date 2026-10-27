//! Platform, compositor, display server, and OS integration.

pub mod display;
pub mod shortcuts;
pub mod system;

// Direct re-exports
pub use display::capture;
pub use display::screen;
pub use display::window;

pub use shortcuts::control;
pub use shortcuts::hyprland;
pub use shortcuts::wayland;
