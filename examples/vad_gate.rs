//! Offline Silero check for `accuracy_replay --no-filter` records.
//! Usage: vad_gate RUN.jsonl FILE.wav --model silero_vad.onnx --threshold T --min-speech S

use serde::Deserialize;
use sherpa_onnx::{SileroVadModelConfig, VadModelConfig, VoiceActivityDetector};
use std::error::Error;
use std::time::Instant;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Record {
    #[serde(rename = "chunk")]
    Chunk {
        channel: u8,
        start: f64,
        end: f64,
        closed: f64,
    },
    #[serde(rename = "line")]
    Line {
        channel: u8,
        start: f64,
        end: f64,
        #[serde(rename = "text")]
        _text: String,
    },
    #[serde(other)]
    Other,
}

fn records(input: &str) -> Result<Vec<(&str, Record)>, Box<dyn Error>> {
    let mut rows = Vec::new();
    let mut last = None;
    let mut has_line = false;
    let mut last_closed = 0.0;
    for row in input.lines() {
        let record: Record = serde_json::from_str(row)?;
        match &record {
            Record::Chunk {
                channel,
                start,
                end,
                closed,
            } => {
                if *channel > 1
                    || !start.is_finite()
                    || !end.is_finite()
                    || !closed.is_finite()
                    || *start < 0.0
                    || start >= end
                    || closed < end
                    || *closed < last_closed
                {
                    return Err("invalid chunk record".into());
                }
                last = Some((*channel, *start, *end));
                last_closed = *closed;
                has_line = false;
            }
            Record::Line {
                channel,
                start,
                end,
                ..
            } => {
                if has_line || last != Some((*channel, *start, *end)) {
                    return Err("line does not match preceding chunk".into());
                }
                has_line = true;
            }
            Record::Other => {}
        }
        rows.push((row, record));
    }
    Ok(rows)
}

fn chunk_samples(pcm: &[i16], channel: u8, start: f64, end: f64) -> Result<Vec<f32>, &'static str> {
    let first = (start * 16_000.0).round() as usize;
    let last = (end * 16_000.0).round() as usize;
    let frames = pcm.len() / 2;
    if channel > 1 || first >= last || first >= frames || last > frames + 4 * 16_000 {
        return Err("chunk is outside WAV");
    }
    let mut samples: Vec<f32> = pcm[first * 2 + usize::from(channel)..last.min(frames) * 2]
        .iter()
        .step_by(2)
        .map(|&sample| f32::from(sample) / 32768.0)
        .collect();
    samples.resize(last - first, 0.0);
    Ok(samples)
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((sorted.len() as f64 * fraction).ceil() as usize - 1).min(sorted.len() - 1)]
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 9
        || args[3] != "--model"
        || args[5] != "--threshold"
        || args[7] != "--min-speech"
    {
        return Err(
            "usage: vad_gate RUN.jsonl FILE.wav --model PATH --threshold T --min-speech S".into(),
        );
    }
    let threshold: f32 = args[6].parse()?;
    let min_speech: f32 = args[8].parse()?;
    if !(0.0..=1.0).contains(&threshold) || !min_speech.is_finite() || min_speech <= 0.0 {
        return Err("threshold must be 0..1 and min-speech must be positive".into());
    }
    let input = std::fs::read_to_string(&args[1])?;
    let rows = records(&input)?;
    let mut reader = hound::WavReader::open(&args[2])?;
    let spec = reader.spec();
    if spec.channels != 2
        || spec.sample_rate != 16_000
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err("need a 16-bit stereo 16 kHz WAV".into());
    }
    let pcm: Vec<i16> = reader.samples::<i16>().collect::<Result<_, _>>()?;
    if !pcm.len().is_multiple_of(2) {
        return Err("WAV has an incomplete stereo frame".into());
    }
    // Check all bounds before emitting records, so a bad run cannot produce partial output.
    for (_, record) in &rows {
        if let Record::Chunk {
            channel,
            start,
            end,
            ..
        } = record
        {
            let _ = chunk_samples(&pcm, *channel, *start, *end)?;
        }
    }
    let config = VadModelConfig {
        silero_vad: SileroVadModelConfig {
            model: Some(args[4].clone()),
            threshold,
            min_silence_duration: 0.1,
            min_speech_duration: 0.0,
            window_size: 512,
            ..Default::default()
        },
        sample_rate: 16_000,
        num_threads: 1,
        ..Default::default()
    };
    let vad = VoiceActivityDetector::create(&config, 30.0).ok_or("could not load Silero VAD")?;
    let mut timings = Vec::new();
    let mut passed = false;
    for (row, record) in rows {
        match record {
            Record::Chunk {
                channel,
                start,
                end,
                ..
            } => {
                let began = Instant::now();
                let samples = chunk_samples(&pcm, channel, start, end)?;
                vad.reset();
                for window in samples.chunks(512) {
                    vad.accept_waveform(window);
                }
                vad.flush();
                let mut speech_samples = 0_usize;
                while let Some(segment) = vad.front() {
                    speech_samples += segment.n() as usize;
                    drop(segment);
                    vad.pop();
                }
                passed = speech_samples as f32 / 16_000.0 >= min_speech;
                let ms = began.elapsed().as_secs_f64() * 1_000.0;
                timings.push(ms);
                println!("{row}");
                println!(
                    "{}",
                    serde_json::json!({"kind":"gate","channel":channel,"start":start,"ms":ms,"passed":passed})
                );
            }
            Record::Line { .. } if !passed => {}
            _ => println!("{row}"),
        }
    }
    timings.sort_by(f64::total_cmp);
    let median = if timings.is_empty() {
        0.0
    } else {
        (timings[(timings.len() - 1) / 2] + timings[timings.len() / 2]) / 2.0
    };
    eprintln!(
        "{}",
        serde_json::json!({"kind":"gate_summary","chunks":timings.len(),"median_ms":median,"p95_ms":percentile(&timings, 0.95)})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_requested_channel_and_bounds() {
        let pcm = [0, 32767, -32768, 0, 16384, -16384];
        assert_eq!(
            chunk_samples(&pcm, 1, 0.0, 2.0 / 16_000.0).unwrap(),
            vec![32767.0 / 32768.0, 0.0]
        );
        assert_eq!(
            chunk_samples(&pcm, 0, 2.0 / 16_000.0, 4.0 / 16_000.0).unwrap(),
            vec![0.5, 0.0]
        );
        assert!(chunk_samples(&pcm, 0, 3.0 / 16_000.0, 4.0 / 16_000.0).is_err());
    }

    #[test]
    fn rejects_unpaired_lines_and_invalid_chunks() {
        assert!(records(r#"{"kind":"line","channel":0,"start":0,"end":1,"text":"x"}"#).is_err());
        assert!(records(r#"{"kind":"chunk","channel":0,"start":1,"end":0,"closed":1}"#).is_err());
        assert!(
            records(
                r#"{"kind":"chunk","channel":0,"start":0,"end":1,"closed":1}
{"kind":"line","channel":1,"start":0,"end":1,"text":"x"}"#
            )
            .is_err()
        );
    }
}
