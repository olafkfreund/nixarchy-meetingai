//! Decode one 16 kHz mono WAV with the sherpa-onnx Whisper turbo export.
//! Usage: cargo run --example whisper_spike -- MODEL_DIR WAV

use std::path::Path;

use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineWhisperModelConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let [model_dir, wav]: [String; 2] = std::env::args()
        .skip(1)
        .collect::<Vec<_>>()
        .try_into()
        .map_err(|_| "usage: whisper_spike MODEL_DIR WAV")?;
    let model_dir = Path::new(&model_dir);
    let model_file = |stem: &str, ext: &str| {
        let int8 = model_dir.join(format!("turbo-{stem}.int8.{ext}"));
        let file = if int8.exists() {
            int8
        } else {
            model_dir.join(format!("turbo-{stem}.{ext}"))
        };
        Some(file.to_string_lossy().into_owned())
    };

    let mut config = OfflineRecognizerConfig::default();
    config.model_config.whisper = OfflineWhisperModelConfig {
        encoder: model_file("encoder", "onnx"),
        decoder: model_file("decoder", "onnx"),
        language: Some("no".into()),
        task: Some("transcribe".into()),
        ..Default::default()
    };
    config.model_config.tokens = model_file("tokens", "txt");
    config.model_config.num_threads = 4;
    let recognizer =
        OfflineRecognizer::create(&config).ok_or("Whisper model could not be created")?;

    let mut reader = hound::WavReader::open(wav)?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.sample_rate != 16_000
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err("expected a 16 kHz mono 16-bit PCM WAV".into());
    }
    let samples = reader
        .samples::<i16>()
        .map(|sample| sample.map(|value| f32::from(value) / 32768.0))
        .collect::<Result<Vec<_>, _>>()?;
    let stream = recognizer.create_stream();
    stream.accept_waveform(16_000, &samples);
    recognizer.decode(&stream);
    println!(
        "{}",
        stream
            .get_result()
            .ok_or("Whisper returned no result")?
            .text
    );
    Ok(())
}
