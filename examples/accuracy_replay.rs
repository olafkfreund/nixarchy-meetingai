//! Replays a stereo 16 kHz WAV (left = mic, right = system) through the daemon's batch path
//! (`BatchChunks` + provider + `LineFilter`) and prints one JSON line per kept transcript line:
//! `{"channel","start","end","text","latency"}`. Driven by `tools/accuracy/score.py`.
//! Usage: accuracy_replay FILE.wav (--url URL | --fake TEXT)
use nixarchy_meetingai::ai::providers::ProviderConfig;
use nixarchy_meetingai::speech::stt::filter::LineFilter;
use nixarchy_meetingai::speech::stt::transcription::{
    BatchChunks, TranscriptLine, to_wav, transcribe_with_prompt,
};
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let (url, fake) = (value("--url"), value("--fake"));
    if url.is_none() == fake.is_none() {
        return Err("give exactly one of --url URL or --fake TEXT".into());
    }
    let mut reader = hound::WavReader::open(args.first().ok_or("missing WAV path")?)?;
    let spec = reader.spec();
    if spec.channels != 2 || spec.sample_rate != 16_000 || spec.bits_per_sample != 16 {
        return Err("need a 16-bit stereo 16 kHz WAV".into());
    }
    let pcm: Vec<i16> = reader.samples::<i16>().collect::<Result<_, _>>()?;
    let mut config = ProviderConfig::new("whisper-local", "", "");
    if let Some(url) = &url {
        config.base_url = url.clone();
    }
    let mut chunks = BatchChunks::default();
    let mut ready = Vec::new();
    for block in pcm.chunks(3_200) {
        ready.extend(chunks.push(block));
    }
    ready.extend(chunks.finish());
    let mut filter = LineFilter::default();
    for (channel, start, end, samples) in ready {
        let began = Instant::now();
        let text = match &fake {
            Some(text) => text.clone(),
            None => {
                let prompt = filter.prompt(channel, "");
                transcribe_with_prompt(&config, to_wav(&samples)?, None, &prompt).await?
            }
        };
        let latency = began.elapsed().as_secs_f64();
        let line = TranscriptLine {
            channel,
            speaker: None,
            text,
            is_final: true,
            speech_final: true,
            start,
            end,
        };
        if !line.text.is_empty() && filter.accept(&line) {
            println!(
                "{}",
                serde_json::json!({"channel": channel, "start": start, "end": end, "text": line.text, "latency": latency})
            );
        }
    }
    Ok(())
}
