//! Local speaker labels for batch speech chunks.

use std::collections::VecDeque;

use parking_lot::Mutex;

use super::voice_check::{self, Embedder, WINDOW_SAMPLES};
use crate::audio::dsp::SAMPLE_RATE;

pub const JOIN: f32 = 0.55;
pub const NEW: f32 = 0.35;
pub const MIXED: f32 = 0.45;
const MAX_SPEAKERS: usize = 8;
const HISTORY: usize = 20;

pub fn embed_windows(embedder: &dyn Embedder, samples: &[f32]) -> Vec<Vec<f32>> {
    let voiced = voice_check::speech(samples, SAMPLE_RATE);
    let length = voiced.len();
    if length < WINDOW_SAMPLES {
        return Vec::new();
    }
    if length < 2 * WINDOW_SAMPLES {
        return embedder.embed(&voiced).into_iter().collect();
    }
    let count = (length / WINDOW_SAMPLES).min(4);
    (0..count)
        .filter_map(|i| {
            let offset = i * (length - WINDOW_SAMPLES) / (count - 1);
            embedder.embed(&voiced[offset..offset + WINDOW_SAMPLES])
        })
        .collect()
}

struct Speaker {
    number: i64,
    vectors: VecDeque<Vec<f32>>,
}

pub struct SpeakerLabeller {
    speakers: Vec<Speaker>,
    next: i64,
}

impl SpeakerLabeller {
    pub fn new(first: i64) -> Self {
        Self {
            speakers: Vec::new(),
            next: first,
        }
    }

    pub fn raise_next(&mut self, n: i64) {
        self.next = self.next.max(n);
    }

    pub fn decide(&mut self, vectors: &[Vec<f32>]) -> Option<i64> {
        let mut vector = vectors.first()?.clone();
        if !voice_check::normalise(&mut vector) {
            return None;
        }
        if vectors.len() > 1 {
            for (i, a) in vectors.iter().enumerate() {
                for b in &vectors[i + 1..] {
                    if voice_check::cosine(a, b)? < MIXED {
                        return None;
                    }
                }
            }
            for other in &vectors[1..] {
                let mut other = other.clone();
                if !voice_check::normalise(&mut other) || other.len() != vector.len() {
                    return None;
                }
                for (sum, value) in vector.iter_mut().zip(other) {
                    *sum += value;
                }
            }
            if !voice_check::normalise(&mut vector) {
                return None;
            }
        }
        let best = self
            .speakers
            .iter()
            .enumerate()
            .filter_map(|(i, speaker)| {
                let mut centroid = vec![0.0; vector.len()];
                for sample in &speaker.vectors {
                    if sample.len() != centroid.len() {
                        return None;
                    }
                    for (sum, value) in centroid.iter_mut().zip(sample) {
                        *sum += value;
                    }
                }
                voice_check::cosine(&vector, &centroid).map(|score| (i, score))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, score)) = best {
            if score >= JOIN {
                let speaker = &mut self.speakers[i];
                if speaker.vectors.len() == HISTORY {
                    speaker.vectors.pop_front();
                }
                speaker.vectors.push_back(vector);
                return Some(speaker.number);
            }
            if score >= NEW {
                return None;
            }
        }
        if self.speakers.len() >= MAX_SPEAKERS {
            return None;
        }
        let number = self.next;
        self.next = self.next.checked_add(1)?;
        self.speakers.push(Speaker {
            number,
            vectors: VecDeque::from([vector]),
        });
        Some(number)
    }
}

pub fn label_line(labeller: &Mutex<SpeakerLabeller>, vectors: &[Vec<f32>]) -> Option<i64> {
    labeller.lock().decide(vectors)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::{audio::dsp::SAMPLE_RATE, speech::voice_check::WINDOW_SAMPLES};

    struct Fake(RefCell<Vec<(usize, f32, f32)>>);

    impl Embedder for Fake {
        fn embed(&self, samples: &[f32]) -> Option<Vec<f32>> {
            self.0
                .borrow_mut()
                .push((samples.len(), samples[0], samples[samples.len() - 1]));
            Some(vec![1.0, 0.0])
        }
    }

    #[test]
    fn voiced_length_bands() {
        let fake = Fake(RefCell::new(Vec::new()));
        let short = vec![0.1; WINDOW_SAMPLES - 480];
        assert!(embed_windows(&fake, &short).is_empty());
        assert!(fake.0.borrow().is_empty());
        let mut raw = vec![0.0; SAMPLE_RATE as usize];
        raw.extend(vec![0.1; WINDOW_SAMPLES]);
        assert_eq!(embed_windows(&fake, &raw).len(), 1);
        assert_eq!(fake.0.borrow()[0].0, WINDOW_SAMPLES);
        fake.0.borrow_mut().clear();
        assert_eq!(
            embed_windows(&fake, &vec![0.1; WINDOW_SAMPLES * 2 - 480]).len(),
            1
        );
        assert_eq!(fake.0.borrow()[0].0, WINDOW_SAMPLES * 2 - 480);
    }

    #[test]
    fn windows_have_exact_spread_offsets() {
        for (seconds, count) in [(3.0, 2), (4.5, 3), (6.0, 4), (10.0, 4)] {
            let length = (seconds * SAMPLE_RATE as f64) as usize;
            let raw: Vec<f32> = (0..length).map(|i| 0.1 + i as f32 / 1_000_000.0).collect();
            let fake = Fake(RefCell::new(Vec::new()));
            assert_eq!(embed_windows(&fake, &raw).len(), count);
            let calls = fake.0.borrow();
            let voiced_length = length / 480 * 480;
            for (i, (size, first, last)) in calls.iter().enumerate() {
                let offset = i * (voiced_length - WINDOW_SAMPLES) / (count - 1);
                assert_eq!(*size, WINDOW_SAMPLES);
                assert_eq!(*first, raw[offset]);
                assert_eq!(*last, raw[offset + WINDOW_SAMPLES - 1]);
            }
        }
    }

    #[test]
    fn rejected_windows_are_dropped() {
        struct Reject;
        impl Embedder for Reject {
            fn embed(&self, samples: &[f32]) -> Option<Vec<f32>> {
                (samples[0] > 0.2).then(|| vec![1.0, 0.0])
            }
        }
        let mut raw = vec![0.1; WINDOW_SAMPLES];
        raw.extend(vec![0.3; WINDOW_SAMPLES]);
        assert_eq!(embed_windows(&Reject, &raw), vec![vec![1.0, 0.0]]);
    }

    #[test]
    fn assigns_joins_abstains_and_raises_numbers() {
        let mut l = SpeakerLabeller::new(7);
        assert_eq!(l.decide(&[]), None);
        assert_eq!(l.decide(&[vec![1.0, 0.0]]), Some(7));
        assert_eq!(l.decide(&[vec![0.9, 0.1]]), Some(7));
        assert_eq!(l.decide(&[vec![0.45, 0.893]]), None);
        assert_eq!(l.decide(&[vec![0.0, 1.0]]), Some(8));
        assert_eq!(l.decide(&[vec![1.0, 0.0], vec![-1.0, 0.0]]), None);
        l.raise_next(20);
        assert_eq!(l.decide(&[vec![-1.0, 0.0]]), Some(20));
        let locked = Mutex::new(l);
        assert_eq!(label_line(&locked, &[vec![-1.0, 0.0]]), Some(20));
    }

    #[test]
    fn caps_speakers_and_keeps_twenty_recent_vectors() {
        let mut l = SpeakerLabeller::new(0);
        for i in 0..8 {
            let mut v = vec![0.0; 8];
            v[i] = 1.0;
            assert_eq!(l.decide(&[v]), Some(i as i64));
        }
        assert_eq!(
            l.decide(&[vec![-1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]]),
            None
        );
        let mut l = SpeakerLabeller::new(0);
        assert_eq!(l.decide(&[vec![1.0, 0.0]]), Some(0));
        for _ in 0..20 {
            assert_eq!(l.decide(&[vec![0.8, 0.6]]), Some(0));
        }
        assert_eq!(l.speakers[0].vectors.len(), 20);
        assert_eq!(l.speakers[0].vectors[0], vec![0.8, 0.6]);
    }
}
