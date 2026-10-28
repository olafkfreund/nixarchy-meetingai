//! Audio digital signal processing: WAV encoding, RMS measurement, and linear resampling.

use std::io::Cursor;

/// Target audio sample rate in Hz (16 kHz mono).
pub const SAMPLE_RATE: u32 = 16_000;

/// Encodes normalized `f32` audio samples (`[-1.0, 1.0]`) into an in-memory 16-bit PCM WAV buffer.
pub fn encode_wav(samples: &[f32]) -> Result<Vec<u8>, hound::Error> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut buf = Vec::with_capacity(44 + samples.len() * 2);
    {
        let mut writer = hound::WavWriter::new(Cursor::new(&mut buf), spec)?;
        let scale = f32::from(i16::MAX);
        for &s in samples {
            let pcm = if s.is_finite() {
                (s.clamp(-1.0, 1.0) * scale) as i16
            } else {
                0
            };
            writer.write_sample(pcm)?;
        }
        writer.finalize()?;
    }
    Ok(buf)
}

/// Computes the root mean square (RMS) amplitude of a sample slice.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples
        .iter()
        .filter(|s| s.is_finite())
        .map(|&s| {
            let v = f64::from(s);
            v * v
        })
        .sum();
    ((sum_sq / (samples.len() as f64)).sqrt()) as f32
}

/// Stateful linear resampler that converts an arbitrary input sample rate to [`SAMPLE_RATE`].
#[derive(Debug, Clone)]
pub struct LinearResampler {
    input_rate: u32,
    step: f64,
    out_idx: u64,
    in_count: u64,
    prev_sample: f32,
}

impl LinearResampler {
    /// Creates a new resampler from `input_rate` Hz to [`SAMPLE_RATE`] Hz.
    pub fn new(input_rate: u32) -> Self {
        let effective_rate = input_rate.max(1);
        Self {
            input_rate: effective_rate,
            step: f64::from(effective_rate) / f64::from(SAMPLE_RATE),
            out_idx: 0,
            in_count: 0,
            prev_sample: 0.0,
        }
    }

    /// Resamples `input` into `output`, clearing `output` first while reusing its allocation.
    pub fn process_into(&mut self, input: &[f32], output: &mut Vec<f32>) {
        output.clear();
        if input.is_empty() {
            return;
        }
        if self.input_rate == SAMPLE_RATE {
            output.extend_from_slice(input);
            return;
        }
        // Grow once up front so the audio callback does not reallocate while pushing.
        output.reserve((input.len() as f64 / self.step) as usize + 1);

        let start_in = self.in_count;
        let end_in = start_in + input.len() as u64;
        let last_in = end_in - 1;

        let sample_at = |idx: u64, prev: f32| -> f32 {
            if idx < start_in {
                prev
            } else {
                input[(idx - start_in) as usize]
            }
        };

        loop {
            let pos = (self.out_idx as f64) * self.step;
            let base = pos.floor() as u64;
            let frac = (pos - (base as f64)) as f32;

            if base > last_in || (base == last_in && frac > 0.0) {
                break;
            }

            let x0 = sample_at(base, self.prev_sample);
            let y = if frac == 0.0 {
                x0
            } else {
                let x1 = sample_at(base + 1, self.prev_sample);
                x0 + (x1 - x0) * frac
            };

            output.push(y);
            self.out_idx += 1;
        }

        self.in_count = end_in;
        self.prev_sample = input[input.len() - 1];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn encode_wav_one_second_silence_header_and_format() {
        let silence = vec![0.0_f32; SAMPLE_RATE as usize];
        let bytes = encode_wav(&silence).expect("encode_wav failed");

        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");

        let reader = hound::WavReader::new(Cursor::new(&bytes)).expect("WavReader failed");
        let spec = reader.spec();
        assert_eq!(spec.sample_rate, 16_000);
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.bits_per_sample, 16);
        assert_eq!(spec.sample_format, hound::SampleFormat::Int);

        let decoded: Vec<i16> = reader
            .into_samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .expect("reading samples failed");
        assert_eq!(decoded.len(), 16_000);
        assert!(decoded.iter().all(|&s| s == 0));
    }

    #[test]
    fn encode_wav_clamps_and_handles_non_finite() {
        let input = [
            2.0_f32,
            -2.0,
            1.0,
            -1.0,
            0.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ];
        let bytes = encode_wav(&input).expect("encode_wav failed");
        let reader = hound::WavReader::new(Cursor::new(&bytes)).expect("WavReader failed");
        let decoded: Vec<i16> = reader
            .into_samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .expect("reading samples failed");

        assert_eq!(decoded, vec![32767, -32767, 32767, -32767, 0, 0, 0, 0]);
    }

    #[test]
    fn encode_wav_empty_input() {
        let bytes = encode_wav(&[]).expect("encode_wav failed");
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        let reader = hound::WavReader::new(Cursor::new(&bytes)).expect("WavReader failed");
        assert_eq!(reader.len(), 0);
    }

    #[test]
    fn rms_computation_and_edge_cases() {
        assert_eq!(rms(&[]), 0.0);
        assert!((rms(&[1.0, -1.0]) - 1.0).abs() <= 1e-6);
        assert!((rms(&[0.5, -0.5]) - 0.5).abs() <= 1e-6);
        // Non-finite values contribute 0 to sum of squares but count toward total length (4)
        let mixed = [1.0, -1.0, f32::NAN, f32::INFINITY];
        let expected = (2.0_f32 / 4.0).sqrt();
        assert!((rms(&mixed) - expected).abs() <= 1e-6);
    }

    #[test]
    fn resampler_zero_rate_does_not_divide_by_zero() {
        let mut r = LinearResampler::new(0);
        let mut out = Vec::new();
        r.process_into(&[0.5, 0.5], &mut out);
        assert!(!out.is_empty());
    }

    #[test]
    fn resampler_32khz_sine_length() {
        let mut r = LinearResampler::new(32_000);
        let input: Vec<f32> = (0..3_200).map(|i| (i as f32 * 0.05).sin()).collect();
        let mut output = Vec::new();
        r.process_into(&input, &mut output);
        assert!(
            (output.len() as isize - 1_600).abs() <= 1,
            "expected ~1600 samples, got {}",
            output.len()
        );
    }

    #[test]
    fn resampler_16khz_passthrough() {
        let mut r = LinearResampler::new(16_000);
        let mut output = vec![99.0];
        r.process_into(&[0.1, 0.2, 0.3], &mut output);
        assert_eq!(output, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn resampler_48khz_constant_across_blocks() {
        let mut r = LinearResampler::new(48_000);
        let block = vec![0.4_f32; 2_400];
        let mut out1 = Vec::new();
        let mut out2 = Vec::new();
        r.process_into(&block, &mut out1);
        r.process_into(&block, &mut out2);

        assert!(!out1.is_empty());
        assert!(!out2.is_empty());
        for &s in out1.iter().chain(out2.iter()) {
            assert!((s - 0.4).abs() <= 1e-3, "sample {s} deviated from 0.4");
        }
    }

    #[test]
    fn resampler_block_invariance_and_empty_calls() {
        let full_input: Vec<f32> = (0..44_100).map(|i| (i as f32 * 0.013).sin()).collect();

        for &rate in &[11_025_u32, 22_050, 48_000] {
            let mut single = LinearResampler::new(rate);
            let mut expected = Vec::new();
            single.process_into(&full_input, &mut expected);

            for &block_size in &[3_usize, 50, 1_009] {
                let mut chunked = LinearResampler::new(rate);
                let mut actual = Vec::new();
                let mut buf = Vec::new();

                for chunk in full_input.chunks(block_size) {
                    buf.push(123.0);
                    chunked.process_into(&[], &mut buf);
                    assert!(buf.is_empty(), "empty input must produce empty output");

                    chunked.process_into(chunk, &mut buf);
                    actual.extend_from_slice(&buf);
                }

                assert_eq!(
                    actual.len(),
                    expected.len(),
                    "length mismatch for rate {rate}, block_size {block_size}"
                );
                for (idx, (&a, &e)) in actual.iter().zip(expected.iter()).enumerate() {
                    assert!(
                        (a - e).abs() <= 1e-5,
                        "sample mismatch at {idx} for rate {rate}, block_size {block_size}: {a} vs {e}"
                    );
                }
            }
        }
    }
}
