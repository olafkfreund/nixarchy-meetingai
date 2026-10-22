//! Global shortcuts, Wayland portal bindings, Hyprland configuration, and control socket.

pub mod bindings;
mod commands;
pub use commands::parse_command;
#[cfg(unix)]
pub mod control;
#[cfg(windows)]
#[path = "control_windows.rs"]
pub mod control;
pub mod hyprland;
#[cfg(target_os = "linux")]
pub mod wayland;
#[cfg(not(target_os = "linux"))]
#[path = "wayland_stub.rs"]
pub mod wayland;

pub use bindings::*;
pub use control::*;
