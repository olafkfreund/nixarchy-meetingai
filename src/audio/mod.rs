//! Audio input and signal processing.

pub mod capture;
pub mod dsp;
pub mod echo_gate;

pub use capture::devices;
pub use capture::microphone;
pub use capture::system_audio;
pub use capture::*;
pub use dsp::*;
