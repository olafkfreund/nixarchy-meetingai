//! Live manual smoke, using the encrypted vault. No plaintext key file or logging.
use lennette::{
    core::{
        credentials::{self, Kind},
        vault::Vault,
    },
    speech::transcription::{LiveEvent, LiveTranscriber},
};
use std::time::{Duration, Instant};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let vault = Vault::open_default()?;
    let credential = credentials::load(&vault, Kind::Speech, "deepgram-stt")
        .map_err(anyhow::Error::msg)?
        .ok_or_else(|| anyhow::anyhow!("save a Deepgram credential in Settings first"))?;
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("provide a 16 kHz mono WAV fixture"))?;
    let mut wav = hound::WavReader::open(path)?;
    let input_rate = wav.spec().sample_rate;
    assert_eq!(wav.spec().channels, 1);
    let audio: Vec<f32> = wav
        .samples::<i16>()
        .map(|s| s.unwrap() as f32 / 32768.0)
        .collect();
    let audio = if input_rate == 16000 {
        audio
    } else {
        lennette::audio::LinearResampler::new(input_rate).process(&audio)
    };
    let mut live = LiveTranscriber::connect(credential.api_key, credential.model, None).await?;
    let began = Instant::now();
    let mut interim = [0; 2];
    let mut finals = [0; 2];
    let mut vad = [0; 2];
    let mut first_interim = None;
    let mut first_final = None;
    // Play different phrases on each side at different times to prove channel identity.
    let mut both = Vec::new();
    both.extend(audio.iter().flat_map(|s| [*s, 0.0]));
    both.extend(vec![0.0; 32000]);
    both.extend(audio.iter().flat_map(|s| [0.0, *s]));
    both.extend(vec![0.0; 64000]);
    for chunk in both.chunks(1600) {
        live.push(chunk).await?;
        tokio::time::sleep(Duration::from_millis(50)).await;
        while let Ok(event) = live.try_event() {
            record(
                event,
                &mut interim,
                &mut finals,
                &mut vad,
                &mut first_interim,
                &mut first_final,
                began,
            )?;
        }
    }
    live.finish();
    while let Some(event) = live.next_event().await {
        record(
            event,
            &mut interim,
            &mut finals,
            &mut vad,
            &mut first_interim,
            &mut first_final,
            began,
        )?;
    }
    assert!(
        interim.iter().all(|n| *n > 0),
        "both sources must receive interim text"
    );
    assert!(
        finals.iter().all(|n| *n > 0),
        "both sources must receive finalized text"
    );
    assert!(
        vad.iter().all(|n| *n > 0),
        "both sources must receive speech-start events"
    );
    println!(
        "PASS: live stereo, interim={interim:?}, final={finals:?}, VAD={vad:?}, first interim={first_interim:?}, first final={first_final:?}"
    );
    Ok(())
}
fn record(
    event: LiveEvent,
    interim: &mut [u32; 2],
    finals: &mut [u32; 2],
    vad: &mut [u32; 2],
    first_interim: &mut Option<Duration>,
    first_final: &mut Option<Duration>,
    began: Instant,
) -> anyhow::Result<()> {
    match event {
        LiveEvent::Transcript(line) => {
            if line.is_final {
                finals[line.channel as usize] += 1;
                first_final.get_or_insert(began.elapsed());
            } else {
                interim[line.channel as usize] += 1;
                first_interim.get_or_insert(began.elapsed());
            }
            println!(
                "{} final={} endpoint={} speaker={:?}: {}",
                if line.channel == 0 { "You" } else { "Them" },
                line.is_final,
                line.speech_final,
                line.speaker,
                line.text
            );
        }
        LiveEvent::SpeechStarted { channel } => vad[channel as usize] += 1,
        LiveEvent::Failed(error) => anyhow::bail!(error),
        _ => {}
    }
    Ok(())
}
