//! Audio processing, hardware capture, and digital signal processing.

pub mod capture;
pub mod dsp;

pub use capture::devices;
pub use capture::microphone;
pub use capture::system_audio;
pub use capture::*;
pub use dsp::*;
