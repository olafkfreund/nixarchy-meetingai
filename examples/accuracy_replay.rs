//! Replays a stereo 16 kHz WAV (left = mic, right = system) through the daemon's batch path
//! (`BatchChunks` + `transcribe_with_retry` + `LineFilter`, up to 4 requests in flight, prompts
//! built before earlier results are filtered, as in the live loop). Prints one JSON line per
//! inference request: `{"channel","start","end","text","latency","kept"}`; `kept` is false for
//! lines the filter dropped and for empty text. Driven by `tools/accuracy/score.py`.
//! Usage: accuracy_replay FILE.wav (--url URL | --fake TEXT) [--model M] [--language L] [--glossary G]
use futures_util::StreamExt;
use nixarchy_meetingai::ai::providers::ProviderConfig;
use nixarchy_meetingai::speech::stt::filter::LineFilter;
use nixarchy_meetingai::speech::stt::transcription::{
    BatchChunks, TranscriptLine, transcribe_with_retry,
};
use std::time::Instant;

type Done = Result<(TranscriptLine, f64), String>;

fn report(filter: &mut LineFilter, done: Done) -> Result<(), String> {
    let (line, latency) = done?;
    let kept = !line.text.is_empty() && filter.accept(&line);
    println!(
        "{}",
        serde_json::json!({"channel": line.channel, "start": line.start, "end": line.end,
            "text": line.text, "latency": latency, "kept": kept})
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut wav, mut flags) = (None, std::collections::HashMap::new());
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg.starts_with("--") {
            let value = args.next().ok_or(format!("{arg} needs a value"))?;
            flags.insert(arg, value);
        } else if wav.is_none() {
            wav = Some(arg);
        } else {
            return Err(format!("unexpected argument {arg}").into());
        }
    }
    let value = |flag: &str| flags.get(flag).cloned();
    let (url, fake) = (value("--url"), value("--fake"));
    if url.is_none() == fake.is_none() {
        return Err("give exactly one of --url URL or --fake TEXT".into());
    }
    let glossary = value("--glossary").unwrap_or_default();
    let mut reader = hound::WavReader::open(wav.ok_or("missing WAV path")?)?;
    let spec = reader.spec();
    if spec.channels != 2 || spec.sample_rate != 16_000 || spec.bits_per_sample != 16 {
        return Err("need a 16-bit stereo 16 kHz WAV".into());
    }
    let pcm: Vec<i16> = reader.samples::<i16>().collect::<Result<_, _>>()?;
    let mut config = ProviderConfig::new("whisper-local", "", value("--model").unwrap_or_default());
    config.language = value("--language").unwrap_or_default();
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
    let mut requests = futures_util::stream::FuturesOrdered::new();
    for (channel, start, end, samples) in ready {
        if requests.len() >= 4
            && let Some(done) = requests.next().await
        {
            report(&mut filter, done)?;
        }
        let prompt = filter.prompt(channel, &glossary);
        let (config, fake) = (config.clone(), fake.clone());
        requests.push_back(async move {
            let began = Instant::now();
            let text = match fake {
                Some(text) => text,
                None => transcribe_with_retry(&config, &samples, &prompt)
                    .await
                    .map_err(|e| format!("{e} (lost audio {start:.1}s..{end:.1}s)"))?,
            };
            let line = TranscriptLine {
                channel,
                speaker: None,
                text,
                is_final: true,
                speech_final: true,
                start,
                end,
            };
            Ok((line, began.elapsed().as_secs_f64()))
        });
    }
    while let Some(done) = requests.next().await {
        report(&mut filter, done)?;
    }
    Ok(())
}
