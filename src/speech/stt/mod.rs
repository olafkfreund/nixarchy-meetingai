//! Speech-to-text: transcript filtering and transcription clients.

pub mod filter;
pub mod parakeet;
pub mod transcription;

use std::sync::Mutex;

/// One lock covers model loads and all in-process decodes.
pub static DECODE: Mutex<()> = Mutex::new(());

static CHUNK_LATENCIES: Mutex<Vec<f64>> = Mutex::new(Vec::new());

/// Off in the daemon: only the accuracy replay turns it on, so the list never grows unread.
static RECORD_LATENCIES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Measurement only (`examples/accuracy_replay.rs`): record each chunk's decode time.
pub fn record_chunk_latencies(on: bool) {
    RECORD_LATENCIES.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn record_chunk_latency(seconds: f64) {
    if !RECORD_LATENCIES.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    CHUNK_LATENCIES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(seconds);
}

pub fn take_chunk_latencies() -> Vec<f64> {
    std::mem::take(&mut *CHUNK_LATENCIES.lock().unwrap_or_else(|e| e.into_inner()))
}

pub use transcription::*;

#[cfg(test)]
mod latency_tests {
    use super::*;

    #[test]
    fn chunk_latencies_are_only_kept_when_measuring() {
        record_chunk_latencies(false);
        let _ = take_chunk_latencies();
        record_chunk_latency(0.5);
        assert!(
            take_chunk_latencies().is_empty(),
            "the daemon must never accumulate these"
        );
        record_chunk_latencies(true);
        record_chunk_latency(0.5);
        assert_eq!(take_chunk_latencies(), vec![0.5]);
        record_chunk_latencies(false);
    }
}
