//! Audio input devices, microphone streams, and system audio capture.

pub mod devices;
pub mod microphone;
pub mod system_audio;

pub use devices::*;
pub use microphone::*;
pub use system_audio::*;
