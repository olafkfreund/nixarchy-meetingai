//! Replays a stereo 16 kHz WAV (left = mic, right = system) through the daemon's batch path
//! (`BatchChunks` + `transcribe_chunk` + `LineFilter`, up to 4 requests in flight, prompts
//! built before earlier results are filtered, as in the live loop). Prints one JSON line per
//! inference request `{"kind":"request","latency"}` and one per line the filter keeps
//! `{"kind":"line","channel","start","end","text"}`, released as the filter decides. Driven by `tools/accuracy/score.py`.
//! Usage: accuracy_replay FILE.wav (--url URL | --fake TEXT | --parakeet) [--model-dir DIR] [--model M] [--language L] [--glossary G]
use futures_util::StreamExt;
use nixarchy_meetingai::ai::providers::ProviderConfig;
use nixarchy_meetingai::speech::stt::filter::LineFilter;
use nixarchy_meetingai::speech::stt::transcription::{
    BatchChunks, TranscriptLine, transcribe_chunk,
};
use std::time::Instant;

type Done = Result<(TranscriptLine, f64), String>;

fn exactly_one_engine(url: bool, fake: bool, parakeet: bool) -> Result<(), &'static str> {
    if usize::from(url) + usize::from(fake) + usize::from(parakeet) == 1 {
        Ok(())
    } else {
        Err("give exactly one of --url URL, --fake TEXT, or --parakeet")
    }
}

fn print_line(line: &TranscriptLine) {
    println!(
        "{}",
        serde_json::json!({"kind": "line", "channel": line.channel, "start": line.start,
            "end": line.end, "text": line.text})
    );
}

/// One request finished: print its latency, then whatever the filter releases now.
fn report(filter: &mut LineFilter, done: Done) -> Result<(), String> {
    let (line, latency) = done?;
    println!(
        "{}",
        serde_json::json!({"kind": "request", "latency": latency})
    );
    if !line.text.is_empty() {
        filter.accept(&line).iter().for_each(print_line);
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut wav, mut flags) = (None, std::collections::HashMap::new());
    let mut parakeet = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--parakeet" {
            parakeet = true;
        } else if arg.starts_with("--") {
            let value = args.next().ok_or(format!("{arg} needs a value"))?;
            if value.starts_with("--") {
                return Err(format!("{arg} needs a value").into());
            }
            flags.insert(arg, value);
        } else if wav.is_none() {
            wav = Some(arg);
        } else {
            return Err(format!("unexpected argument {arg}").into());
        }
    }
    let value = |flag: &str| flags.get(flag).cloned();
    let (url, fake) = (value("--url"), value("--fake"));
    exactly_one_engine(url.is_some(), fake.is_some(), parakeet)?;
    if parakeet && let Some(dir) = value("--model-dir") {
        // Set before Tokio starts workers; the daemon's loader reads this variable.
        unsafe { std::env::set_var("NIXARCHY_MEETINGAI_PARAKEET_DIR", dir) };
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(replay(
        wav.ok_or("missing WAV path")?,
        flags,
        url,
        fake,
        parakeet,
    ))
}

async fn replay(
    wav: String,
    flags: std::collections::HashMap<String, String>,
    url: Option<String>,
    fake: Option<String>,
    parakeet: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let value = |flag: &str| flags.get(flag).cloned();
    let glossary = value("--glossary").unwrap_or_default();
    let mut reader = hound::WavReader::open(wav)?;
    let spec = reader.spec();
    if spec.channels != 2 || spec.sample_rate != 16_000 || spec.bits_per_sample != 16 {
        return Err("need a 16-bit stereo 16 kHz WAV".into());
    }
    let pcm: Vec<i16> = reader.samples::<i16>().collect::<Result<_, _>>()?;
    let mut config = ProviderConfig::new(
        if parakeet {
            "parakeet-local"
        } else {
            "openai-compatible-stt"
        },
        "",
        value("--model").unwrap_or_default(),
    );
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
                None => transcribe_chunk(&config, &samples, &prompt)
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
    // End of the recording: held mic lines are judged now, as the live loop does on stop.
    filter.flush().iter().for_each(print_line);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::exactly_one_engine;

    #[test]
    fn requires_exactly_one_engine() {
        for url in [false, true] {
            for fake in [false, true] {
                for parakeet in [false, true] {
                    assert_eq!(
                        exactly_one_engine(url, fake, parakeet).is_ok(),
                        [url, fake, parakeet]
                            .into_iter()
                            .filter(|selected| *selected)
                            .count()
                            == 1
                    );
                }
            }
        }
    }
}
