//! In-process Parakeet TDT 0.6B v3 (INT8) through sherpa-onnx.
//!
//! One recogniser per daemon, loaded on first use (about 2.5 s, 1.1 GB) on a blocking worker.
//! Calls are serialised by a mutex: the four decode threads are the cost, a second
//! simultaneous decode would only fight for them. Long audio is decoded in 30 s pieces because
//! one pass over minutes of audio fails inside onnxruntime.

use std::path::PathBuf;
use std::sync::Mutex;

use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig};

/// Pieces of at most this many samples (30 s at 16 kHz) go to the model.
pub const MAX_CHUNK: usize = 30 * 16_000;
const THREADS: i32 = 4;
const FILES: [&str; 4] = [
    "encoder.int8.onnx",
    "decoder.int8.onnx",
    "joiner.int8.onnx",
    "tokens.txt",
];

type Decoder = Box<dyn FnMut(&[f32]) -> String + Send>;

/// The loaded decoder; tests put a fake one here.
static DECODER: Mutex<Option<Decoder>> = Mutex::new(None);

/// Directory holding the model files (`NIXARCHY_MEETINGAI_PARAKEET_DIR`).
pub fn model_dir() -> Option<PathBuf> {
    std::env::var_os("NIXARCHY_MEETINGAI_PARAKEET_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Whether the model files are present.
pub fn model_ready() -> bool {
    model_dir().is_some_and(|dir| FILES.iter().all(|f| dir.join(f).is_file()))
}

fn load() -> Result<Decoder, String> {
    let dir = model_dir().ok_or("the Parakeet model directory is not set")?;
    let path = |f: &str| Some(dir.join(f).to_string_lossy().into_owned());
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.transducer = OfflineTransducerModelConfig {
        encoder: path(FILES[0]),
        decoder: path(FILES[1]),
        joiner: path(FILES[2]),
    };
    config.model_config.tokens = path(FILES[3]);
    config.model_config.model_type = Some("nemo_transducer".into());
    config.model_config.num_threads = THREADS;
    let recognizer = OfflineRecognizer::create(&config).ok_or("the Parakeet model did not load")?;
    Ok(Box::new(move |samples| {
        let stream = recognizer.create_stream();
        stream.accept_waveform(16_000, samples);
        recognizer.decode(&stream);
        stream.get_result().map(|r| r.text).unwrap_or_default()
    }))
}

/// Decode `samples` (mono, 16 kHz) piece by piece with `decode`.
fn decode_pieces(decode: &mut dyn FnMut(&[f32]) -> String, samples: &[f32]) -> String {
    let parts: Vec<String> = samples
        .chunks(MAX_CHUNK)
        .map(|piece| decode(piece).trim().to_owned())
        .filter(|text| !text.is_empty())
        .collect();
    parts.join(" ")
}

/// Transcribe mono 16 kHz samples on a blocking worker, loading the model on first use.
pub async fn transcribe(samples: Vec<f32>) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let mut slot = DECODER.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_none() {
            *slot = Some(load()?);
        }
        let decode = slot.as_mut().expect("loaded above");
        Ok(decode_pieces(decode, &samples))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Install a fake decoder (tests only).
#[cfg(test)]
pub fn set_decoder(decode: Decoder) {
    *DECODER.lock().unwrap_or_else(|e| e.into_inner()) = Some(decode);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_audio_is_split_at_30_seconds() {
        let mut sizes = Vec::new();
        let text = decode_pieces(
            &mut |piece| {
                sizes.push(piece.len());
                format!("p{}", sizes.len())
            },
            &vec![0.1; MAX_CHUNK * 2 + 100],
        );
        assert_eq!(sizes, [MAX_CHUNK, MAX_CHUNK, 100]);
        assert_eq!(text, "p1 p2 p3");
    }
}
