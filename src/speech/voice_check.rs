//! In-memory microphone history and conservative owner voice checks.

use std::path::Path;

use sherpa_onnx::{SpeakerEmbeddingExtractor, SpeakerEmbeddingExtractorConfig};

use crate::audio::dsp::{SAMPLE_RATE, rms};

const FRAME_MS: u32 = 30;
const WINDOW_SAMPLES: usize = SAMPLE_RATE as usize * 3 / 2;
const MIN_ENROLL_SPEECH: usize = SAMPLE_RATE as usize * 6;
const MIN_CHECK_SPEECH: usize = WINDOW_SAMPLES;
// Plan step 7, LibriSpeech test-clean, 40 speakers as owner (examples/voice_eval.rs):
// at 0.015 / 8 s, 22 of 40 natural 12 s samples failed enrollment; at 0.005 / 6 s, none.
const SPEECH_RMS: f32 = 0.005;
// Owner lines labelled room: clean 0/2536, far-end bleed at -12 dB 0.6%, at -6 dB 4.2%,
// all 121/7608 = 1.6% (bar 1 in 50). Other speakers caught: 78%; phone/speaker voices: 88%.
// At 0.45 (provisional) owner lines were 2.4% clean, 4.9% / 12% with bleed.
const ROOM_COSINE: f32 = 0.20;

/// Fixed-size history of microphone samples, indexed from recording start.
pub struct MicRing {
    samples: Vec<f32>,
    rate: u32,
    written: u64,
}

impl MicRing {
    pub fn new(seconds: f64, rate: u32) -> Self {
        let capacity = if seconds.is_finite() && seconds > 0.0 {
            (seconds * f64::from(rate)) as usize
        } else {
            0
        };
        Self {
            samples: vec![0.0; capacity],
            rate,
            written: 0,
        }
    }

    /// Input is stereo, with the microphone in the left channel.
    pub fn push_interleaved(&mut self, frames: &[f32]) {
        if self.samples.is_empty() {
            return;
        }
        let capacity = self.samples.len();
        for frame in frames.as_chunks::<2>().0 {
            self.samples[self.written as usize % capacity] = frame[0];
            self.written += 1;
        }
    }

    /// Return a span in seconds since recording start, if all samples remain in the ring.
    pub fn cut(&self, start: f64, end: f64) -> Option<Vec<f32>> {
        if !start.is_finite() || !end.is_finite() || start < 0.0 || end <= start || self.rate == 0 {
            return None;
        }
        let first = (start * f64::from(self.rate)).round() as u64;
        let last = (end * f64::from(self.rate)).round() as u64;
        if last <= first
            || last > self.written
            || first < self.written.saturating_sub(self.samples.len() as u64)
        {
            return None;
        }
        Some(
            (first..last)
                .map(|i| self.samples[i as usize % self.samples.len()])
                .collect(),
        )
    }
}

pub trait Embedder {
    fn embed(&self, samples: &[f32]) -> Option<Vec<f32>>;
}

pub struct SherpaEmbedder(SpeakerEmbeddingExtractor);

impl SherpaEmbedder {
    /// Load on a blocking worker, after audio is available for a check.
    pub fn load(model: &Path) -> Result<Self, String> {
        if !model.is_file() {
            return Err("speaker model file is missing".into());
        }
        let config = SpeakerEmbeddingExtractorConfig {
            model: Some(model.to_string_lossy().into_owned()),
            ..Default::default()
        };
        SpeakerEmbeddingExtractor::create(&config)
            .map(Self)
            .ok_or_else(|| "speaker model could not be loaded".into())
    }
}

impl Embedder for SherpaEmbedder {
    fn embed(&self, samples: &[f32]) -> Option<Vec<f32>> {
        let stream = self.0.create_stream()?;
        stream.accept_waveform(SAMPLE_RATE as i32, samples);
        self.0.is_ready(&stream).then(|| self.0.compute(&stream))?
    }
}

fn normalise(values: &mut [f32]) -> bool {
    let norm = values
        .iter()
        .map(|v| f64::from(*v) * f64::from(*v))
        .sum::<f64>()
        .sqrt();
    if !norm.is_finite() || norm <= 0.0 {
        return false;
    }
    for value in values {
        *value = (f64::from(*value) / norm) as f32;
    }
    true
}

/// Cosine similarity, or `None` for malformed embeddings.
pub fn cosine(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.is_empty() || a.len() != b.len() {
        return None;
    }
    let mut a = a.to_vec();
    let mut b = b.to_vec();
    if !normalise(&mut a) || !normalise(&mut b) {
        return None;
    }
    Some(a.iter().zip(b.iter()).map(|(x, y)| x * y).sum())
}

/// Collect complete 30 ms frames above the provisional speech level.
fn speech(samples: &[f32], rate: u32) -> Vec<f32> {
    let frame = (rate as usize * FRAME_MS as usize) / 1_000;
    if frame == 0 {
        return Vec::new();
    }
    samples
        .chunks_exact(frame)
        .filter(|chunk| rms(chunk) >= SPEECH_RMS)
        .flatten()
        .copied()
        .collect()
}

/// Average normalized embeddings from 1.5 s speech windows.
pub fn enroll(e: &dyn Embedder, mic: &[f32], rate: u32) -> Result<Vec<f32>, String> {
    if rate != SAMPLE_RATE {
        return Err("speaker audio must be 16 kHz".into());
    }
    let voiced = speech(mic, rate);
    if voiced.len() < MIN_ENROLL_SPEECH {
        return Err("need at least 6 seconds of speech".into());
    }
    let mut sum: Vec<f32> = Vec::new();
    let mut count = 0;
    for window in voiced.as_chunks::<WINDOW_SAMPLES>().0 {
        let mut embedding = e.embed(window).ok_or("speaker embedding failed")?;
        if !normalise(&mut embedding) || embedding.is_empty() {
            return Err("speaker embedding failed".into());
        }
        if sum.is_empty() {
            sum = vec![0.0; embedding.len()];
        }
        if sum.len() != embedding.len() {
            return Err("speaker embedding dimensions changed".into());
        }
        for (total, value) in sum.iter_mut().zip(embedding) {
            *total += value;
        }
        count += 1;
    }
    if count == 0 || !normalise(&mut sum) {
        return Err("speaker embedding failed".into());
    }
    Ok(sum)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Owner,
    Room,
}

/// Cosine of the heard speech against the sample, or `None` when too short or unclear.
pub fn score(e: &dyn Embedder, sample: &[f32], heard: &[f32], rate: u32) -> Option<f32> {
    if rate != SAMPLE_RATE {
        return None;
    }
    let voiced = speech(heard, rate);
    if voiced.len() < MIN_CHECK_SPEECH {
        return None;
    }
    cosine(sample, &e.embed(&voiced)?)
}

/// Unclear or short audio keeps the initial owner label.
pub fn check(e: &dyn Embedder, sample: &[f32], heard: &[f32], rate: u32) -> Verdict {
    match score(e, sample, heard, rate) {
        Some(score) if score < ROOM_COSINE => Verdict::Room,
        _ => Verdict::Owner,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    struct Fake(RefCell<Vec<Vec<f32>>>);

    impl Embedder for Fake {
        fn embed(&self, _: &[f32]) -> Option<Vec<f32>> {
            Some(self.0.borrow_mut().remove(0))
        }
    }

    fn fake(vectors: Vec<Vec<f32>>) -> Fake {
        Fake(RefCell::new(vectors))
    }

    #[test]
    fn ring_cuts_across_wrap_and_rejects_lost_or_future_audio() {
        let mut ring = MicRing::new(3.0, 2);
        ring.push_interleaved(&[1.0, 90.0, 2.0, 90.0, 3.0, 90.0, 4.0, 90.0]);
        assert_eq!(ring.cut(0.0, 2.0), Some(vec![1.0, 2.0, 3.0, 4.0]));
        ring.push_interleaved(&[5.0, 90.0, 6.0, 90.0, 7.0, 90.0, 8.0, 90.0]);
        assert_eq!(ring.cut(1.0, 4.0), Some(vec![3.0, 4.0, 5.0, 6.0, 7.0, 8.0]));
        assert_eq!(ring.cut(0.0, 1.0), None);
        assert_eq!(ring.cut(3.0, 5.0), None);
        assert_eq!(ring.cut(2.0, 2.0), None);
    }

    #[test]
    fn short_or_ambiguous_audio_stays_owner() {
        let voiced = vec![0.1; WINDOW_SAMPLES];
        let e = fake(vec![vec![0.55, 0.835]]);
        assert_eq!(
            check(&e, &[1.0, 0.0], &voiced[..16_000], SAMPLE_RATE),
            Verdict::Owner
        );
        assert_eq!(check(&e, &[1.0, 0.0], &voiced, SAMPLE_RATE), Verdict::Owner);
    }

    #[test]
    fn low_similarity_is_room() {
        let e = fake(vec![vec![0.0, 1.0]]);
        assert_eq!(
            check(&e, &[1.0, 0.0], &vec![0.1; WINDOW_SAMPLES], SAMPLE_RATE),
            Verdict::Room
        );
    }

    #[test]
    fn enrollment_averages_normalized_windows_and_requires_six_seconds_of_speech() {
        let mut mic = vec![0.1; SAMPLE_RATE as usize * 10];
        mic[0..SAMPLE_RATE as usize].fill(0.0);
        let e = fake(vec![
            vec![10.0, 0.0],
            vec![0.0, 5.0],
            vec![1.0, 0.0],
            vec![0.0, 1.0],
            vec![1.0, 0.0],
            vec![1.0, 0.0],
        ]);
        let embedding = enroll(&e, &mic, SAMPLE_RATE).unwrap();
        assert!((embedding[0] - 4.0 / 20.0_f32.sqrt()).abs() < 1e-6);
        assert!((embedding[1] - 2.0 / 20.0_f32.sqrt()).abs() < 1e-6);
        mic[..SAMPLE_RATE as usize * 5].fill(0.0);
        assert!(
            enroll(&e, &mic, SAMPLE_RATE)
                .unwrap_err()
                .contains("6 seconds")
        );
    }
}
