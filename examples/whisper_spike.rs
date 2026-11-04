//! Decode one 16 kHz mono WAV with the sherpa-onnx Whisper turbo export.
//! Usage: cargo run --example whisper_spike -- MODEL_DIR WAV

use std::path::Path;

use serde::Deserialize;
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineWhisperModelConfig};

#[derive(Deserialize)]
struct TurnCut {
    channel: u8,
    start: f64,
    end: f64,
}

fn cut_turn(pcm: &[i16], turn: &TurnCut) -> Result<Vec<f32>, &'static str> {
    let frames = pcm.len() / 2;
    if turn.channel > 1
        || !turn.start.is_finite()
        || !turn.end.is_finite()
        || turn.start < 0.0
        || turn.end <= turn.start
        || turn.end * 16_000.0 > frames as f64 + 0.5
    {
        return Err("invalid turn bounds or channel");
    }
    let start = (turn.start * 16_000.0).round() as usize;
    let end = (turn.end * 16_000.0).round() as usize;
    if end > frames || start >= end {
        return Err("invalid turn bounds");
    }
    Ok(pcm[start * 2 + usize::from(turn.channel)..end * 2]
        .iter()
        .step_by(2)
        .map(|&s| f32::from(s) / 32768.0)
        .collect())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let model_dir = args.next().ok_or(
        "usage: whisper_spike MODEL_DIR [--stem-prefix PREFIX] [--threads N] [--turns FILE] WAV",
    )?;
    let (mut stem_prefix, mut threads, mut turns) = ("turbo-".to_owned(), 4_i32, None);
    let mut wav = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--stem-prefix" => stem_prefix = args.next().ok_or("--stem-prefix needs a value")?,
            "--threads" => {
                threads = args
                    .next()
                    .ok_or("--threads needs a value")?
                    .parse::<i32>()?
            }
            "--turns" => turns = Some(args.next().ok_or("--turns needs a value")?),
            _ if wav.is_none() => wav = Some(arg),
            _ => return Err(format!("unexpected argument {arg}").into()),
        }
    }
    if threads <= 0 {
        return Err("--threads must be positive".into());
    }
    let wav = wav.ok_or("missing WAV path")?;
    let model_dir = Path::new(&model_dir);
    let model_file = |stem: &str, ext: &str| {
        let int8 = model_dir.join(format!("{stem_prefix}{stem}.int8.{ext}"));
        let file = if int8.exists() {
            int8
        } else {
            model_dir.join(format!("{stem_prefix}{stem}.{ext}"))
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
    config.model_config.num_threads = threads;
    let recognizer =
        OfflineRecognizer::create(&config).ok_or("Whisper model could not be created")?;

    let mut reader = hound::WavReader::open(wav)?;
    let spec = reader.spec();
    if spec.channels != (if turns.is_some() { 2 } else { 1 })
        || spec.sample_rate != 16_000
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err("expected a 16 kHz 16-bit PCM WAV with the requested channel count".into());
    }
    let pcm: Vec<i16> = reader.samples::<i16>().collect::<Result<_, _>>()?;
    if let Some(path) = turns {
        let turns: Vec<TurnCut> = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        for turn in turns {
            let samples = cut_turn(&pcm, &turn)?;
            let stream = recognizer.create_stream();
            stream.accept_waveform(16_000, &samples);
            recognizer.decode(&stream);
            let text = stream
                .get_result()
                .ok_or("Whisper returned no result")?
                .text;
            if !text.trim().is_empty() {
                println!(
                    "{}",
                    serde_json::json!({"kind":"line","channel":turn.channel,"start":turn.start,"end":turn.end,"released_at":null,"text":text.trim()})
                );
            }
        }
        return Ok(());
    }
    let samples: Vec<f32> = pcm
        .into_iter()
        .map(|value| f32::from(value) / 32768.0)
        .collect();
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
