//! Embeds audio with the real speaker model. Skipped unless `MEETINGAI_SPEAKER_MODEL` is set;
//! the `voice-check` flake check sets it, so a sherpa-onnx or model change that breaks
//! embeddings fails there.

use nixarchy_meetingai::speech::voice_check::{Embedder, SherpaEmbedder, cosine};
use std::path::Path;

fn tone(hz: f32, seconds: usize) -> Vec<f32> {
    (0..16_000 * seconds)
        .map(|i| (i as f32 * hz * std::f32::consts::TAU / 16_000.0).sin() * 0.1)
        .collect()
}

#[test]
fn real_model_gives_a_stable_finite_embedding() {
    let Some(model) = std::env::var_os("MEETINGAI_SPEAKER_MODEL") else {
        eprintln!("skipped: no speaker model configured");
        return;
    };
    let embedder = SherpaEmbedder::load(Path::new(&model)).unwrap();
    let first = embedder
        .embed(&tone(220.0, 2))
        .expect("two seconds are enough");
    let again = embedder.embed(&tone(220.0, 2)).unwrap();
    assert!(!first.is_empty() && first.iter().all(|x| x.is_finite()));
    // The same audio twice is the same voice.
    assert!(cosine(&first, &again).unwrap() > 0.99);
}
