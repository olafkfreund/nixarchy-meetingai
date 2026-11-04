//! Speech-to-text: transcript filtering and transcription clients.

pub mod filter;
pub mod parakeet;
pub mod transcription;
pub mod whisper_nb;

use std::sync::Mutex;

/// One lock covers model loads and all in-process decodes.
pub static DECODE: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LoadMode {
    #[default]
    Resident,
    UnloadInactive,
}

static LOAD_MODE: Mutex<LoadMode> = Mutex::new(LoadMode::Resident);

pub fn set_load_mode(mode: LoadMode) {
    let _decode = DECODE.lock().unwrap_or_else(|e| e.into_inner());
    *LOAD_MODE.lock().unwrap_or_else(|e| e.into_inner()) = mode;
}

fn unload_inactive() -> bool {
    *LOAD_MODE.lock().unwrap_or_else(|e| e.into_inner()) == LoadMode::UnloadInactive
}

pub use transcription::*;
