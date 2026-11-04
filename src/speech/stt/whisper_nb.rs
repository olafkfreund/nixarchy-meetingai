//! In-process Norwegian Whisper with Silero speech gating.

use sherpa_onnx::{
    OfflineRecognizer, OfflineRecognizerConfig, OfflineWhisperModelConfig, SileroVadModelConfig,
    VadModelConfig, VoiceActivityDetector,
};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

type Decoder = Box<dyn FnMut(i32, &[f32]) -> String + Send>;
type FailedLoad = Option<(Option<PathBuf>, Option<PathBuf>, String)>;
static DECODER: Mutex<Option<Decoder>> = Mutex::new(None);
static FAILED: Mutex<FailedLoad> = Mutex::new(None);

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}
fn model_dir() -> Option<PathBuf> {
    env_path("NIXARCHY_MEETINGAI_WHISPER_NB_DIR")
}
fn vad_path() -> Option<PathBuf> {
    env_path("NIXARCHY_MEETINGAI_SILERO_VAD")
}
fn default_threads() -> i32 {
    std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).clamp(1, 16) as i32)
}
fn model_file(dir: &Path, stem: &str, ext: &str) -> Option<PathBuf> {
    [
        format!("{stem}.int8.{ext}"),
        format!("turbo-{stem}.int8.{ext}"),
        format!("{stem}.{ext}"),
        format!("turbo-{stem}.{ext}"),
    ]
    .into_iter()
    .map(|name| dir.join(name))
    .find(|path| path.is_file())
}
fn files() -> Result<(PathBuf, PathBuf, PathBuf, PathBuf), String> {
    let dir = model_dir().ok_or("the Norwegian Whisper model directory is not set")?;
    let encoder = model_file(&dir, "encoder", "onnx");
    let decoder = model_file(&dir, "decoder", "onnx");
    let tokens = model_file(&dir, "tokens", "txt");
    let vad = vad_path().ok_or("the Silero VAD path is not set")?;
    let vad = if vad.is_dir() {
        vad.join("silero_vad.onnx")
    } else {
        vad
    };
    match (encoder, decoder, tokens, vad.is_file()) {
        (Some(e), Some(d), Some(t), true) => Ok((e, d, t, vad)),
        (_, _, _, false) => Err(format!("Silero VAD model is missing at {}", vad.display())),
        _ => Err(format!(
            "Norwegian Whisper model files are missing in {}",
            dir.display()
        )),
    }
}
pub fn not_ready_reason() -> Option<String> {
    if let Err(reason) = files() {
        return Some(reason);
    }
    FAILED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .filter(|(dir, vad, _)| *dir == model_dir() && *vad == vad_path())
        .map(|(_, _, reason)| format!("Norwegian Whisper model failed to load: {reason}"))
}
fn load() -> Result<Decoder, String> {
    let (encoder, decoder, tokens, vad) = files()?;
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.whisper = OfflineWhisperModelConfig {
        encoder: Some(encoder.to_string_lossy().into_owned()),
        decoder: Some(decoder.to_string_lossy().into_owned()),
        language: Some("no".into()),
        task: Some("transcribe".into()),
        ..Default::default()
    };
    config.model_config.tokens = Some(tokens.to_string_lossy().into_owned());
    // Whisper's 32-layer encoder runs a 30 s window per chunk, so threads matter.
    config.model_config.num_threads = std::env::var("NIXARCHY_MEETINGAI_WHISPER_NB_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n: &i32| (1..=64).contains(&n))
        .unwrap_or_else(default_threads);
    let recognizer = OfflineRecognizer::create(&config).ok_or("the model could not be created")?;
    let vad_config = VadModelConfig {
        silero_vad: SileroVadModelConfig {
            model: Some(vad.to_string_lossy().into_owned()),
            threshold: 0.5,
            min_silence_duration: 0.25,
            min_speech_duration: 0.0,
            window_size: 512,
            ..Default::default()
        },
        sample_rate: 16_000,
        num_threads: 1,
        ..Default::default()
    };
    let detector = VoiceActivityDetector::create(&vad_config, 60.0)
        .ok_or("the Silero VAD could not be created")?;
    Ok(Box::new(move |rate, samples| {
        detector.reset();
        let mut segments = Vec::new();
        for window in samples.chunks(512) {
            if window.len() == 512 {
                detector.accept_waveform(window);
            } else {
                let mut padded = [0.0; 512];
                padded[..window.len()].copy_from_slice(window);
                detector.accept_waveform(&padded);
            }
            drain_segments(&detector, &mut segments);
        }
        detector.flush();
        drain_segments(&detector, &mut segments);
        let mut decode = |rate, piece: &[f32]| {
            let stream = recognizer.create_stream();
            stream.accept_waveform(rate, piece);
            recognizer.decode(&stream);
            stream
                .get_result()
                .map(|result| result.text)
                .unwrap_or_default()
        };
        decode_segments(&segments, rate, &mut decode)
    }))
}
fn drain_segments(detector: &VoiceActivityDetector, segments: &mut Vec<Vec<f32>>) {
    while let Some(segment) = detector.front() {
        segments.push(segment.samples().to_vec());
        detector.pop();
    }
}
fn decode_segments(
    segments: &[Vec<f32>],
    rate: i32,
    decode: &mut dyn FnMut(i32, &[f32]) -> String,
) -> String {
    let text = segments
        .iter()
        .map(|segment| super::parakeet::decode_pieces(decode, rate, segment))
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    filter_phantoms(&text)
}
const PHANTOMS: &[&str] = &[
    "takk for at du så på",
    "teksting av ai-media",
    "undertekster av ai-media",
];
/// Subtitle credits Whisper learned from TV: the opener is followed by a varying name, so
/// everything from the opener to the end of its sentence goes.
const CREDIT_OPENERS: &[&str] = &["teksting av", "undertekster av", "tekst:", "oversatt av"];
fn filter_phantoms(text: &str) -> String {
    let mut clean = text.to_owned();
    for phrase in PHANTOMS {
        while let Some((start, mut end)) = clean.char_indices().find_map(|(start, _)| {
            let end = clean[start..]
                .char_indices()
                .nth(phrase.chars().count())
                .map_or(clean.len(), |(at, _)| start + at);
            (clean[start..end].to_lowercase() == *phrase).then_some((start, end))
        }) {
            if matches!(clean.as_bytes().get(end), Some(b'.' | b',' | b'!')) {
                end += 1;
            }
            clean.replace_range(start..end, "");
        }
    }
    for opener in CREDIT_OPENERS {
        // ASCII openers matched case-insensitively on the original bytes, so offsets stay valid.
        while let Some(start) = clean
            .as_bytes()
            .windows(opener.len())
            .position(|w| w.eq_ignore_ascii_case(opener.as_bytes()))
        {
            let rest = &clean[start..];
            let end = rest
                .find(['.', '!', '?'])
                .map_or(clean.len(), |at| start + at + 1);
            clean.replace_range(start..end, "");
        }
    }
    clean
        .trim_matches(|c: char| c.is_whitespace() || c == '.' || c == ',' || c == '!')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn ensure(slot: &mut Option<Decoder>) -> Result<&mut Decoder, String> {
    if slot.is_none() {
        match load() {
            Ok(decoder) => {
                *slot = Some(decoder);
                *FAILED.lock().unwrap_or_else(|e| e.into_inner()) = None;
            }
            Err(reason) => {
                *FAILED.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some((model_dir(), vad_path(), reason.clone()));
                return Err(format!("Norwegian Whisper model failed to load: {reason}"));
            }
        }
    }
    Ok(slot.as_mut().expect("loaded above"))
}
pub fn preload() -> Result<(), String> {
    let _decode = super::DECODE.lock().unwrap_or_else(|e| e.into_inner());
    if super::unload_inactive() {
        super::parakeet::unload_locked();
    }
    let mut slot = DECODER.lock().unwrap_or_else(|e| e.into_inner());
    ensure(&mut slot)?;
    Ok(())
}
pub(crate) fn unload_locked() {
    if DECODER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
        .is_some()
    {
        #[cfg(target_os = "linux")]
        unsafe {
            libc::malloc_trim(0);
        }
    }
}
pub fn unload() {
    let _decode = super::DECODE.lock().unwrap_or_else(|e| e.into_inner());
    unload_locked();
}
pub async fn transcribe(samples: Vec<f32>, rate: i32) -> Result<String, String> {
    if rate != 16_000 {
        return Err("Norwegian Whisper needs 16 kHz audio".into());
    }
    tokio::task::spawn_blocking(move || {
        let _decode = super::DECODE.lock().unwrap_or_else(|e| e.into_inner());
        if super::unload_inactive() {
            super::parakeet::unload_locked();
        }
        let mut slot = DECODER.lock().unwrap_or_else(|e| e.into_inner());
        let decode = ensure(&mut slot)?;
        Ok(decode(rate, &samples))
    })
    .await
    .map_err(|error| error.to_string())?
}
#[cfg(test)]
pub(crate) fn set_decoder(decode: Decoder) {
    *DECODER.lock().unwrap_or_else(|e| e.into_inner()) = Some(decode);
}
#[cfg(test)]
pub(crate) fn loaded() -> bool {
    DECODER.lock().unwrap_or_else(|e| e.into_inner()).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_thread_count_is_half_available_with_bounds() {
        let expected =
            std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).clamp(1, 16) as i32);
        assert_eq!(default_threads(), expected);
        assert!((1..=16).contains(&default_threads()));
    }
    #[test]
    fn phantom_phrases_are_removed_anywhere() {
        assert_eq!(filter_phantoms("Takk for at du så på. Hei"), "Hei");
        assert_eq!(filter_phantoms("Hei Takk for at du så på. der"), "Hei der");
        assert_eq!(filter_phantoms("Hei Takk for at du så på."), "Hei");
        assert_eq!(filter_phantoms("Takk for at du så på."), "");
        // Subtitle credits with any name, anywhere in a line.
        assert_eq!(filter_phantoms("Teksting av Nicolai Winther"), "");
        assert_eq!(
            filter_phantoms("Vi går videre. Teksting av Nicolai Winther. Neste sak"),
            "Vi går videre. Neste sak"
        );
        assert_eq!(
            filter_phantoms("Ærlig talt, UNDERTEKSTER AV Øyvind Ås!"),
            "Ærlig talt"
        );
    }
    #[test]
    fn speech_segments_skip_silence_and_keep_short_words() {
        let calls = std::cell::Cell::new(0);
        let mut fake = |_: i32, _: &[f32]| {
            calls.set(calls.get() + 1);
            "ja".to_owned()
        };
        assert_eq!(decode_segments(&[], 16_000, &mut fake), "");
        assert_eq!(calls.get(), 0);
        assert_eq!(
            decode_segments(&[vec![0.1; 4_800]], 16_000, &mut fake),
            "ja"
        );
        assert_eq!(calls.get(), 1);
    }
    #[test]
    fn long_speech_is_split_below_thirty_seconds() {
        let mut sizes = Vec::new();
        let mut fake = |_: i32, part: &[f32]| {
            sizes.push(part.len());
            "ok".to_owned()
        };
        assert_eq!(
            decode_segments(&[vec![0.1; 45 * 16_000]], 16_000, &mut fake),
            "ok ok"
        );
        assert!(sizes.iter().all(|size| *size <= 30 * 16_000));
    }
}
