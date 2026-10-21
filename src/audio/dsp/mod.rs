//! Audio primitives: WAV encoding, resampling and voice-activity segmentation.
//!
//! Live capture feeds 16 kHz mono f32 samples. Everything that decides *what* to
//! send is pure and unit-tested here; device I/O lives in [`capture`].

use std::io::Cursor;

/// Sample rate used for every encoded chunk and for live streaming.
pub const SAMPLE_RATE: u32 = 16_000;

/// Encode mono f32 samples as a 16-bit PCM WAV file.
///
/// Values outside `[-1, 1]` are clamped and NaN is treated as silence, so a
/// misbehaving device can never produce an invalid file.
pub fn encode_wav(samples: &[f32]) -> Result<Vec<u8>, hound::Error> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)?;
        for sample in samples {
            let clamped = if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            writer.write_sample((clamped * f32::from(i16::MAX)) as i16)?;
        }
        writer.finalize()?;
    }
    Ok(cursor.into_inner())
}

/// Root-mean-square level of a frame, the value VAD compares against.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples
        .iter()
        .filter(|sample| sample.is_finite())
        .map(|sample| sample * sample)
        .sum();
    (sum / samples.len() as f32).sqrt()
}

/// Downmix interleaved frames to mono.
pub fn downmix(channels: &[Vec<f32>]) -> Vec<f32> {
    let frames = channels.iter().map(Vec::len).min().unwrap_or(0);
    if channels.is_empty() {
        return Vec::new();
    }
    (0..frames)
        .map(|i| channels.iter().map(|channel| channel[i]).sum::<f32>() / channels.len() as f32)
        .collect()
}

/// Linear resampler that keeps phase across calls.
///
/// Streaming needs continuity: resampling each block independently would insert
/// clicks at every boundary, so the fractional position and the last sample are
/// carried between calls.
#[derive(Debug, Clone)]
pub struct LinearResampler {
    input_rate: f32,
    position: f32,
    previous: f32,
    started: bool,
}

impl LinearResampler {
    pub fn new(input_rate: u32) -> Self {
        Self {
            input_rate: input_rate as f32,
            position: 0.0,
            previous: 0.0,
            started: false,
        }
    }

    pub fn is_passthrough(&self) -> bool {
        (self.input_rate - SAMPLE_RATE as f32).abs() < f32::EPSILON
    }

    /// Resample one block. Returns `SAMPLE_RATE`-family samples.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if self.is_passthrough() {
            return input.to_vec();
        }
        if !self.started {
            self.previous = input.first().copied().unwrap_or(0.0);
            self.started = true;
        }
        let step = self.input_rate / SAMPLE_RATE as f32;
        let mut output = Vec::with_capacity((input.len() as f32 / step) as usize + 1);
        let mut position = self.position;
        while position < input.len() as f32 {
            let index = position.floor() as usize;
            let fraction = position - index as f32;
            let current = input.get(index).copied().unwrap_or(self.previous);
            let next = input.get(index + 1).copied().unwrap_or(current);
            output.push(current + (next - current) * fraction);
            position += step;
        }
        self.position = position - input.len() as f32;
        self.previous = input.last().copied().unwrap_or(self.previous);
        output
    }
}

/// Why a buffer was flushed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlushReason {
    /// Speech followed by enough silence.
    Utterance,
    /// The buffer hit its maximum length.
    Maximum,
    /// Capture stopped while speech was present.
    Stopped,
}

/// A finished utterance ready to send for transcription.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub samples: Vec<f32>,
    pub reason: FlushReason,
}

/// Tunables for speech segmentation.
#[derive(Debug, Clone, Copy)]
pub struct VadConfig {
    /// RMS above which a frame counts as speech.
    pub speech_threshold: f32,
    /// Consecutive quiet ticks needed to close an utterance.
    pub silence_ticks: usize,
    /// Minimum samples in a normal utterance.
    pub min_samples: usize,
    /// Hard cap on a single utterance.
    pub max_samples: usize,
    /// Buffered audio with no speech at all is dropped at this length.
    pub silence_reset: usize,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            // Values match the reference: RMS 0.01, eight 100 ms quiet ticks,
            // 2 s minimum, 20 s maximum and a 5 s silent reset at 16 kHz.
            speech_threshold: 0.01,
            silence_ticks: 8,
            min_samples: 32_000,
            max_samples: 320_000,
            silence_reset: 80_000,
        }
    }
}

/// Streaming speech segmenter.
#[derive(Debug, Clone)]
pub struct Segmenter {
    config: VadConfig,
    samples: Vec<f32>,
    silence_ticks: usize,
    has_speech: bool,
}

impl Segmenter {
    pub fn new(config: VadConfig) -> Self {
        Self {
            config,
            samples: Vec::new(),
            silence_ticks: 0,
            has_speech: false,
        }
    }

    pub fn buffered(&self) -> usize {
        self.samples.len()
    }

    /// Feed one 100 ms frame, returning a finished segment when one closes.
    pub fn push(&mut self, frame: &[f32]) -> Option<Segment> {
        let level = rms(frame);
        if level >= self.config.speech_threshold {
            self.silence_ticks = 0;
            self.has_speech = true;
        } else {
            self.silence_ticks += 1;
        }
        self.samples.extend_from_slice(frame);

        if (self.samples.len() >= self.config.min_samples
            && self.silence_ticks >= self.config.silence_ticks)
            || self.samples.len() >= self.config.max_samples
        {
            let reason = if self.samples.len() >= self.config.max_samples {
                FlushReason::Maximum
            } else {
                FlushReason::Utterance
            };
            return self.take(reason);
        }
        if !self.has_speech && self.samples.len() >= self.config.silence_reset {
            self.reset();
        }
        None
    }

    /// Flush on stop, but only if something worth transcribing is buffered.
    ///
    /// A short answer ("yes", "ok") can legitimately be under a second, so the
    /// stop flush uses a much lower bar than a normal utterance: a quarter
    /// second of actual speech.
    pub fn finish(&mut self) -> Option<Segment> {
        let minimum = (SAMPLE_RATE as usize) / 4;
        if self.has_speech && self.samples.len() >= minimum {
            return self.take(FlushReason::Stopped);
        }
        self.reset();
        None
    }

    /// Drop buffered audio without producing a segment.
    pub fn reset(&mut self) {
        self.samples.clear();
        self.silence_ticks = 0;
        self.has_speech = false;
    }

    fn take(&mut self, reason: FlushReason) -> Option<Segment> {
        if !self.has_speech || self.samples.is_empty() {
            self.reset();
            return None;
        }
        let samples = std::mem::take(&mut self.samples);
        self.silence_ticks = 0;
        self.has_speech = false;
        Some(Segment { samples, reason })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(amplitude: f32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| {
                amplitude
                    * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / SAMPLE_RATE as f32).sin()
            })
            .collect()
    }

    fn silence(len: usize) -> Vec<f32> {
        vec![0.0; len]
    }

    #[test]
    fn wav_has_expected_header_and_duration() {
        let bytes = encode_wav(&silence(SAMPLE_RATE as usize)).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        let reader = hound::WavReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(reader.spec().sample_rate, SAMPLE_RATE);
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.spec().bits_per_sample, 16);
        assert_eq!(reader.len(), SAMPLE_RATE);
    }

    #[test]
    fn wav_clamps_and_sanitises() {
        let bytes = encode_wav(&[2.0, -2.0, f32::NAN, f32::INFINITY]).unwrap();
        let mut reader = hound::WavReader::new(Cursor::new(bytes)).unwrap();
        let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
        assert_eq!(samples[0], i16::MAX);
        assert_eq!(samples[1], -i16::MAX);
        // NaN and infinity are not finite, so both become silence.
        assert_eq!(samples[2], 0);
        assert_eq!(samples[3], 0);
    }

    #[test]
    fn rms_matches_expected() {
        assert_eq!(rms(&[]), 0.0);
        assert!((rms(&[1.0, -1.0]) - 1.0).abs() < 1e-6);
        assert!((rms(&[0.5, -0.5]) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn downmix_averages_channels() {
        let mono = downmix(&[vec![1.0, 0.0], vec![0.0, 1.0]]);
        assert_eq!(mono, vec![0.5, 0.5]);
        assert!(downmix(&[]).is_empty());
    }

    #[test]
    fn resampler_halves_for_32k() {
        let mut resampler = LinearResampler::new(32_000);
        let input = tone(0.5, 3200);
        let output = resampler.process(&input);
        assert!((output.len() as i32 - 1600).abs() <= 1);
    }

    #[test]
    fn resampler_is_passthrough_at_native_rate() {
        let mut resampler = LinearResampler::new(SAMPLE_RATE);
        let input = vec![0.1, 0.2, 0.3];
        assert_eq!(resampler.process(&input), input);
    }

    #[test]
    fn resampler_stays_continuous_across_blocks() {
        // A constant signal must resample to the same constant: any discontinuity
        // at a block boundary would show up as a different value.
        let mut resampler = LinearResampler::new(48_000);
        let first = resampler.process(&vec![0.25; 4800]);
        let second = resampler.process(&vec![0.25; 4800]);
        for sample in first.iter().chain(second.iter()) {
            assert!((sample - 0.25).abs() < 1e-3, "discontinuity: {sample}");
        }
    }

    #[test]
    fn segmenter_emits_after_speech_then_silence() {
        let mut segmenter = Segmenter::new(VadConfig::default());
        let mut emitted = None;
        // 2 s of speech in 100 ms frames.
        for _ in 0..20 {
            if let Some(segment) = segmenter.push(&tone(0.3, 1600)) {
                emitted = Some(segment);
            }
        }
        assert!(emitted.is_none(), "must not cut mid-speech");
        // 800 ms of silence closes it.
        for _ in 0..8 {
            if let Some(segment) = segmenter.push(&silence(1600)) {
                emitted = Some(segment);
            }
        }
        let segment = emitted.expect("utterance should close");
        assert_eq!(segment.reason, FlushReason::Utterance);
        assert!(segment.samples.len() >= 32_000);
    }

    #[test]
    fn silence_is_never_sent() {
        let mut segmenter = Segmenter::new(VadConfig::default());
        for _ in 0..100 {
            assert!(segmenter.push(&silence(1600)).is_none());
        }
        assert!(segmenter.finish().is_none());
    }

    #[test]
    fn long_speech_hits_maximum() {
        let mut segmenter = Segmenter::new(VadConfig::default());
        let mut reason = None;
        for _ in 0..300 {
            if let Some(segment) = segmenter.push(&tone(0.3, 1600)) {
                reason = Some(segment.reason);
                break;
            }
        }
        assert_eq!(reason, Some(FlushReason::Maximum));
    }

    #[test]
    fn finish_flushes_pending_speech_only() {
        let mut segmenter = Segmenter::new(VadConfig::default());
        for _ in 0..4 {
            segmenter.push(&tone(0.3, 1600));
        }
        let segment = segmenter.finish().expect("pending speech flushes");
        assert_eq!(segment.reason, FlushReason::Stopped);
        assert!(segmenter.finish().is_none());
    }
}
