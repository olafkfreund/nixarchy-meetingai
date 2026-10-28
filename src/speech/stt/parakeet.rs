//! In-process Parakeet TDT 0.6B v3 (INT8) through sherpa-onnx.
//!
//! One recogniser per daemon, loaded on first use (about 2.5 s, 1.1 GB) on a blocking worker.
//! Calls are serialised by a mutex: the four decode threads are the cost, a second
//! simultaneous decode would only fight for them. Long audio is decoded in 30 s pieces because
//! one pass over minutes of audio fails inside onnxruntime.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Mutex;

use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig};

/// Seconds of audio per model pass.
const MAX_SECONDS: usize = 30;
const THREADS: i32 = 4;
const FILES: [&str; 4] = [
    "encoder.int8.onnx",
    "decoder.int8.onnx",
    "joiner.int8.onnx",
    "tokens.txt",
];

/// Decodes samples at the given sample rate (the recogniser resamples).
type Decoder = Box<dyn FnMut(i32, &[f32]) -> String + Send>;

/// The loaded decoder; tests put a fake one here.
static DECODER: Mutex<Option<Decoder>> = Mutex::new(None);
/// The model directory and reason of the last failed load; readiness stays false until the path changes.
static FAILED: Mutex<Option<(Option<PathBuf>, String)>> = Mutex::new(None);

fn model_dir_from(value: Option<OsString>) -> Option<PathBuf> {
    value.filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Directory holding the model files (`NIXARCHY_MEETINGAI_PARAKEET_DIR`).
pub fn model_dir() -> Option<PathBuf> {
    model_dir_from(std::env::var_os("NIXARCHY_MEETINGAI_PARAKEET_DIR"))
}

/// Why the model in `dir` cannot be used, or `None` when it can.
pub fn not_ready_reason_in(dir: Option<PathBuf>) -> Option<String> {
    let Some(path) = &dir else {
        return Some("the Parakeet model directory is not set".into());
    };
    if !FILES.iter().all(|f| path.join(f).is_file()) {
        return Some(format!(
            "Parakeet model files are missing in {}",
            path.display()
        ));
    }
    let failed = FAILED.lock().unwrap_or_else(|e| e.into_inner());
    failed
        .as_ref()
        .filter(|(failed_dir, _)| *failed_dir == dir)
        .map(|(_, reason)| format!("Parakeet model failed to load: {reason}"))
}

/// [`not_ready_reason_in`] for the configured directory.
pub fn not_ready_reason() -> Option<String> {
    not_ready_reason_in(model_dir())
}

/// Whether the model files are present and the last load did not fail.
pub fn model_ready_in(dir: Option<PathBuf>) -> bool {
    not_ready_reason_in(dir).is_none()
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
    let recognizer = OfflineRecognizer::create(&config).ok_or("the model could not be created")?;
    Ok(Box::new(move |rate, samples| {
        let stream = recognizer.create_stream();
        stream.accept_waveform(rate, samples);
        recognizer.decode(&stream);
        stream.get_result().map(|r| r.text).unwrap_or_default()
    }))
}

/// Fill `slot` with `loader`'s decoder if empty; a failure is remembered for readiness.
fn ensure(
    slot: &mut Option<Decoder>,
    loader: impl FnOnce() -> Result<Decoder, String>,
) -> Result<&mut Decoder, String> {
    if slot.is_none() {
        match loader() {
            Ok(decoder) => {
                *slot = Some(decoder);
                *FAILED.lock().unwrap_or_else(|e| e.into_inner()) = None;
            }
            Err(reason) => {
                *FAILED.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some((model_dir(), reason.clone()));
                return Err(format!("Parakeet model failed to load: {reason}"));
            }
        }
    }
    Ok(slot.as_mut().expect("loaded above"))
}

/// Load the model now (blocking, about 2.5 s) so a recording does not start with a slow first decode.
pub fn preload() -> Result<(), String> {
    let mut slot = DECODER.lock().unwrap_or_else(|e| e.into_inner());
    ensure(&mut slot, load).map(|_| ())
}

/// Piece boundaries: at most 30 s each, cut at the quietest 100 ms of the last 2 s before a
/// boundary rather than blindly through a word.
fn split_points(samples: &[f32], rate: usize) -> Vec<usize> {
    let (max, window) = (rate * MAX_SECONDS, rate / 10);
    let mut cuts = vec![0];
    let mut start = 0;
    while samples.len() - start > max {
        let end = start + max;
        let from = end.saturating_sub(rate * 2).max(start);
        let mut best = (f32::MAX, end);
        let mut at = from;
        while at + window <= end {
            let energy: f32 = samples[at..at + window].iter().map(|s| s * s).sum();
            if energy < best.0 {
                best = (energy, at + window / 2);
            }
            at += window;
        }
        start = best.1;
        cuts.push(start);
    }
    cuts.push(samples.len());
    cuts
}

/// Decode `samples` (mono) piece by piece with `decode`.
fn decode_pieces(decode: &mut Decoder, rate: i32, samples: &[f32]) -> String {
    let cuts = split_points(samples, rate.max(1) as usize);
    let parts: Vec<String> = cuts
        .windows(2)
        .map(|w| decode(rate, &samples[w[0]..w[1]]).trim().to_owned())
        .filter(|text| !text.is_empty())
        .collect();
    parts.join(" ")
}

/// Transcribe mono samples at `rate` on a blocking worker, loading the model on first use.
pub async fn transcribe(samples: Vec<f32>, rate: i32) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let mut slot = DECODER.lock().unwrap_or_else(|e| e.into_inner());
        let decode = ensure(&mut slot, load)?;
        Ok(decode_pieces(decode, rate, &samples))
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
    fn long_audio_is_cut_at_the_quietest_moment_before_30_seconds() {
        let rate = 16_000;
        let mut signal = vec![0.1_f32; rate * 70];
        // A pause at 28.0 s..28.1 s, inside the last two seconds before the first boundary.
        signal[28 * rate..28 * rate + rate / 10].fill(0.0);
        let cuts = split_points(&signal, rate);
        assert_eq!(cuts[1], 28 * rate + rate / 20);
        assert!(cuts.windows(2).all(|w| w[1] - w[0] <= 30 * rate));
        assert_eq!(*cuts.last().unwrap(), signal.len());
        let mut sizes = Vec::new();
        let mut fake: Decoder = Box::new(move |_, piece| {
            sizes.push(piece.len());
            format!("p{}", sizes.len())
        });
        assert_eq!(decode_pieces(&mut fake, rate as i32, &signal), "p1 p2 p3");
    }

    #[test]
    fn short_audio_is_one_piece() {
        assert_eq!(split_points(&vec![0.1; 16_000 * 30], 16_000), [0, 480_000]);
    }

    #[test]
    fn a_failed_load_is_an_error_and_reported_until_the_path_changes() {
        let mut slot: Option<Decoder> = None;
        let error = ensure(&mut slot, || Err("no memory".into())).err().unwrap();
        assert_eq!(error, "Parakeet model failed to load: no memory");
        assert!(slot.is_none());
        // The test process has no model directory configured, so the failure is keyed to `None`.
        assert!(not_ready_reason_in(None).is_some());
        let other = std::env::temp_dir();
        assert!(
            not_ready_reason_in(Some(other))
                .unwrap()
                .contains("missing")
        );
        let ok: Decoder = Box::new(|_, _| String::new());
        assert!(ensure(&mut slot, || Ok(ok)).is_ok());
    }

    #[test]
    fn model_dir_ignores_empty_values() {
        assert_eq!(model_dir_from(Some("".into())), None);
        assert_eq!(model_dir_from(Some("/m".into())), Some(PathBuf::from("/m")));
    }
}
