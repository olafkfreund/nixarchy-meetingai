//! Decodes a real clip with the real Parakeet model. Skipped unless
//! `NIXARCHY_MEETINGAI_PARAKEET_DIR` and `NIXARCHY_MEETINGAI_PARAKEET_WAV` (16 kHz mono) are set;
//! `nix flake check` sets both, so a nixpkgs bump that breaks sherpa-onnx fails there.

use nixarchy_meetingai::speech::stt::parakeet;

#[tokio::test]
async fn real_model_decodes_a_clip() {
    let Some(wav) = std::env::var_os("NIXARCHY_MEETINGAI_PARAKEET_WAV") else {
        eprintln!("skipped: no model/clip configured");
        return;
    };
    assert!(parakeet::model_ready(), "model files missing");
    let samples: Vec<f32> = hound::WavReader::open(wav)
        .unwrap()
        .into_samples::<i16>()
        .map(|s| f32::from(s.unwrap()) / f32::from(i16::MAX))
        .collect();
    let started = std::time::Instant::now();
    let text = parakeet::transcribe(samples).await.unwrap().to_lowercase();
    eprintln!("decoded in {:?}: {text}", started.elapsed());
    assert!(
        text.contains("ask what you can do for your country"),
        "{text}"
    );
}
