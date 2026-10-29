//! Filters the left (microphone) channel of a stereo 16 kHz WAV with sherpa-onnx's streaming
//! `OnlineSpeechDenoiser` and the VoxLock target-speaker model (`voice_sample` = explicit owner
//! sample WAV, never learned from the input) in 20 ms chunks, keeps the right channel unchanged,
//! writes a stereo 16 kHz WAV, and prints `{"rtf": ...}`.
//! Usage: voice_filter IN.wav SAMPLE.wav OUT.wav --model-dir DIR

use std::path::Path;

use sherpa_onnx::{
    OfflineSpeechDenoiserModelConfig, OfflineSpeechDenoiserVoxLockModelConfig,
    OnlineSpeechDenoiser, OnlineSpeechDenoiserConfig,
};

const RATE: u32 = 16_000;
const CHUNK_SAMPLES: usize = 320; // 20 ms at 16 kHz

trait SpeechDenoiser {
    fn run(&mut self, samples: &[f32], sample_rate: i32) -> Vec<f32>;
    fn flush(&mut self) -> Vec<f32>;
}

struct SherpaDenoiser(OnlineSpeechDenoiser);

impl SherpaDenoiser {
    fn load(model_dir: &Path, voice_sample: &Path) -> Result<Self, String> {
        let enroll = model_dir.join("enroll.onnx");
        let chunk = model_dir.join("chunk_int8.onnx");
        for path in [&enroll, &chunk, &voice_sample.to_path_buf()] {
            if !path.is_file() {
                return Err(format!("missing file: {}", path.display()));
            }
        }
        check_sample(voice_sample)?;
        let config = OnlineSpeechDenoiserConfig {
            model: OfflineSpeechDenoiserModelConfig {
                voxlock: OfflineSpeechDenoiserVoxLockModelConfig {
                    enroll_model: Some(enroll.to_string_lossy().into_owned()),
                    chunk_model: Some(chunk.to_string_lossy().into_owned()),
                    voice_sample: Some(voice_sample.to_string_lossy().into_owned()),
                },
                ..Default::default()
            },
        };
        OnlineSpeechDenoiser::create(&config)
            .map(Self)
            .ok_or_else(|| "failed to create OnlineSpeechDenoiser".into())
    }
}

impl SpeechDenoiser for SherpaDenoiser {
    fn run(&mut self, samples: &[f32], sample_rate: i32) -> Vec<f32> {
        self.0.run(samples, sample_rate).samples
    }

    fn flush(&mut self) -> Vec<f32> {
        self.0.flush().samples
    }
}

/// sherpa ignores an unreadable or short sample and then learns a voice from the input, which
/// would measure the wrong speaker: refuse anything but at least 3 s of 16 kHz mono.
fn check_sample(path: &Path) -> Result<(), String> {
    let reader = hound::WavReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let spec = reader.spec();
    if spec.channels != 1 || spec.sample_rate != RATE {
        return Err(format!("{}: need a mono 16 kHz WAV", path.display()));
    }
    if reader.duration() < 3 * RATE {
        return Err(format!(
            "{}: need at least 3 s of the owner's voice",
            path.display()
        ));
    }
    Ok(())
}

/// CPU time of this process, so the cost matches the Alkove script's `process_time`.
fn cpu_seconds() -> f64 {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: valid pointer to a timespec; CLOCK_PROCESS_CPUTIME_ID is always available on Linux.
    unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut t) };
    t.tv_sec as f64 + t.tv_nsec as f64 / 1e9
}

fn filter_wav(
    in_path: &Path,
    out_path: &Path,
    denoiser: &mut dyn SpeechDenoiser,
) -> Result<f64, Box<dyn std::error::Error>> {
    let mut reader = hound::WavReader::open(in_path)?;
    let spec = reader.spec();
    if spec.channels != 2
        || spec.sample_rate != RATE
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err("need a 16-bit stereo 16 kHz WAV".into());
    }
    let pcm: Vec<i16> = reader.samples::<i16>().collect::<Result<_, _>>()?;
    if !pcm.len().is_multiple_of(2) {
        return Err("stereo WAV has an odd sample count".into());
    }
    let frames = pcm.len() / 2;
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    for pair in pcm.as_chunks::<2>().0 {
        left.push(f32::from(pair[0]) / 32768.0);
        right.push(pair[1]);
    }

    let began = cpu_seconds();
    let mut filtered = Vec::with_capacity(frames);
    for chunk in left.chunks(CHUNK_SAMPLES) {
        filtered.extend(denoiser.run(chunk, RATE as i32));
    }
    filtered.extend(denoiser.flush());
    let elapsed = cpu_seconds() - began;

    filtered.resize(frames, 0.0);

    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(out_path)?
    };
    let mut writer = hound::WavWriter::new(std::io::BufWriter::new(file), spec)?;
    let scale = f32::from(i16::MAX);
    for (&l, &r) in filtered.iter().zip(right.iter()) {
        let l_pcm = if l.is_finite() {
            (l.clamp(-1.0, 1.0) * scale) as i16
        } else {
            0
        };
        writer.write_sample(l_pcm)?;
        writer.write_sample(r)?;
    }
    writer.finalize()?;

    let duration = (frames as f64) / f64::from(RATE);
    Ok(if duration > 0.0 {
        elapsed / duration
    } else {
        0.0
    })
}

fn parse_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(String, String, String, String), String> {
    let mut positional = Vec::new();
    let mut model_dir = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if arg == "--model-dir" {
            model_dir = Some(iter.next().ok_or("--model-dir needs a value")?);
        } else if arg.starts_with("--") {
            return Err(format!("unexpected flag {arg}"));
        } else {
            positional.push(arg);
        }
    }
    let [in_wav, sample_wav, out_wav]: [String; 3] = positional
        .try_into()
        .map_err(|_| "usage: voice_filter IN.wav SAMPLE.wav OUT.wav --model-dir DIR".to_owned())?;
    let model_dir = model_dir.ok_or_else(|| {
        "usage: voice_filter IN.wav SAMPLE.wav OUT.wav --model-dir DIR".to_owned()
    })?;
    Ok((in_wav, sample_wav, out_wav, model_dir))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (in_wav, sample_wav, out_wav, model_dir) = parse_args(std::env::args().skip(1))?;
    let mut denoiser = SherpaDenoiser::load(Path::new(&model_dir), Path::new(&sample_wav))?;
    let rtf = filter_wav(Path::new(&in_wav), Path::new(&out_wav), &mut denoiser)?;
    println!("{}", serde_json::json!({"rtf": rtf}));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct HalfDenoiser {
        chunk_sizes: Vec<usize>,
        tail: Vec<f32>,
    }

    impl SpeechDenoiser for HalfDenoiser {
        fn run(&mut self, samples: &[f32], sample_rate: i32) -> Vec<f32> {
            assert_eq!(sample_rate, RATE as i32);
            self.chunk_sizes.push(samples.len());
            // Hold the last two samples until flush, as a streaming denoiser with lookahead does.
            let mut all = std::mem::take(&mut self.tail);
            all.extend(samples.iter().map(|s| s * 0.5));
            let split = all.len().saturating_sub(2);
            self.tail = all.split_off(split);
            all
        }

        fn flush(&mut self) -> Vec<f32> {
            std::mem::take(&mut self.tail)
        }
    }

    #[test]
    fn owner_sample_must_be_mono_16k_and_long_enough() {
        let dir = std::env::temp_dir().join(format!("voice-sample-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, channels: u16, frames: u32| {
            let path = dir.join(name);
            let spec = hound::WavSpec {
                channels,
                sample_rate: RATE,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut w = hound::WavWriter::create(&path, spec).unwrap();
            for _ in 0..frames * u32::from(channels) {
                w.write_sample(1_234_i16).unwrap();
            }
            w.finalize().unwrap();
            path
        };
        assert!(check_sample(&write("short.wav", 1, RATE * 2)).is_err());
        assert!(check_sample(&write("stereo.wav", 2, RATE * 4)).is_err());
        assert!(check_sample(&dir.join("absent.wav")).is_err());
        assert!(check_sample(&write("ok.wav", 1, RATE * 3)).is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn right_channel_is_unchanged_and_output_length_matches() {
        let dir = std::env::temp_dir().join(format!("voice-filter-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let in_path = dir.join("in.wav");
        let out_path = dir.join("out.wav");

        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let frames = 700; // two 320-sample (20 ms) chunks plus a 60-sample tail
        let mut expected_right = Vec::with_capacity(frames);
        {
            let mut writer = hound::WavWriter::create(&in_path, spec).unwrap();
            for i in 0..frames {
                let l = 10_000_i16;
                let r = (i as i16 * 17).wrapping_sub(3_000);
                writer.write_sample(l).unwrap();
                writer.write_sample(r).unwrap();
                expected_right.push(r);
            }
            writer.finalize().unwrap();
        }

        let mut denoiser = HalfDenoiser {
            chunk_sizes: Vec::new(),
            tail: Vec::new(),
        };
        let rtf = filter_wav(&in_path, &out_path, &mut denoiser).unwrap();
        assert!(rtf >= 0.0);
        assert_eq!(denoiser.chunk_sizes, vec![320, 320, 60]);

        let mut reader = hound::WavReader::open(&out_path).unwrap();
        assert_eq!(reader.spec(), spec);
        let out_samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
        assert_eq!(out_samples.len(), frames * 2);

        let actual_left: Vec<i16> = out_samples
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| p[0])
            .collect();
        let actual_right: Vec<i16> = out_samples
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| p[1])
            .collect();
        assert_eq!(actual_right, expected_right);
        assert!(actual_left.iter().all(|&s| (s - 5_000).abs() <= 1));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn argument_parsing_accepts_model_dir_anywhere_and_rejects_bad_calls() {
        assert_eq!(
            parse_args(
                ["in.wav", "sample.wav", "out.wav", "--model-dir", "/models"].map(String::from)
            )
            .unwrap(),
            (
                "in.wav".into(),
                "sample.wav".into(),
                "out.wav".into(),
                "/models".into()
            )
        );
        assert!(parse_args(["in.wav", "sample.wav", "out.wav"].map(String::from)).is_err());
        assert!(
            parse_args(["in.wav", "sample.wav", "--model-dir", "/m"].map(String::from)).is_err()
        );
    }
}
