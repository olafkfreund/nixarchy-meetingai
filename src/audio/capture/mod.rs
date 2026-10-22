//! Audio input devices, microphone streams, and system audio capture.

pub mod devices;
pub mod microphone;
#[cfg(not(windows))]
pub mod system_audio;
#[cfg(windows)]
#[path = "system_audio_windows.rs"]
pub mod system_audio;

pub use devices::*;
pub use microphone::*;
pub use system_audio::*;
