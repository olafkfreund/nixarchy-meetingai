//! Replays a 16 kHz WAV through the daemon's real `LiveTranscriber::connect_config_with` at 1x
//! wall-clock (or `transcription::transcribe` with `--dictation`). Prints one JSON line per
//! inference request `{"kind":"request","latency","at"}`, one per line the filter releases
//! `{"kind":"line","channel","start","end","released_at","text"}`, and a final summary with
//! `VmHWM` peak RSS `{"kind":"summary","duration","vm_hwm_kb","vm_rss_kb"}`.
//! Driven by `tools/accuracy/score.py`.
//! Usage: accuracy_replay FILE.wav (--engine parakeet | --parakeet | --url URL | --fake TEXT)
//!        [--parakeet-dir DIR] [--speaker-model PATH]
//!        [--dictation] [--model M] [--language L] [--glossary G]

use nixarchy_meetingai::ai::providers::ProviderConfig;
use nixarchy_meetingai::audio::encode_wav;
use nixarchy_meetingai::speech::stt::{
    parakeet, record_chunk_latencies, take_chunk_latencies,
    transcription::{
        BatchChunks, LiveEvent, LiveTranscriber, SttHints, TranscriptLine, transcribe,
    },
};
use nixarchy_meetingai::speech::voice_check::SherpaEmbedder;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Engine {
    Parakeet,
    Url(String),
    Fake(String),
}

fn resolve_engine(
    engine: Option<&str>,
    parakeet_flag: bool,
    url: Option<&str>,
    fake: Option<&str>,
) -> Result<Engine, &'static str> {
    let count = usize::from(engine.is_some())
        + usize::from(parakeet_flag)
        + usize::from(url.is_some())
        + usize::from(fake.is_some());
    if count != 1 {
        return Err("give exactly one of --engine parakeet, --parakeet, --url URL, or --fake TEXT");
    }
    if parakeet_flag {
        return Ok(Engine::Parakeet);
    }
    if let Some(u) = url {
        return Ok(Engine::Url(u.to_owned()));
    }
    if let Some(t) = fake {
        return Ok(Engine::Fake(t.to_owned()));
    }
    match engine.unwrap_or_default() {
        "parakeet" => Ok(Engine::Parakeet),
        _ => Err("--engine must be parakeet"),
    }
}

fn proc_status_kb(field: &str) -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines().find_map(|line| {
                line.strip_prefix(field)?
                    .trim()
                    .strip_suffix("kB")?
                    .trim()
                    .parse::<u64>()
                    .ok()
            })
        })
        .unwrap_or(0)
}

fn default_eval_models_dir() -> Option<PathBuf> {
    if let Some(cache) = std::env::var_os("MEETINGAI_EVAL_CACHE") {
        return Some(PathBuf::from(cache).join("no/models"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache/meetingai-eval/no/models"))
}

fn print_line(line: &TranscriptLine, released_at: f64) {
    println!(
        "{}",
        serde_json::json!({
            "kind": "line",
            "channel": line.channel,
            "start": line.start,
            "end": line.end,
            "released_at": released_at,
            "text": line.text,
        })
    );
}

fn drain_requests(
    started: tokio::time::Instant,
    seen: &mut usize,
    ticks_since_request: &mut usize,
) {
    let now = started.elapsed().as_secs_f64();
    for latency in take_chunk_latencies() {
        *seen += 1;
        *ticks_since_request = 0;
        println!(
            "{}",
            serde_json::json!({"kind": "request", "latency": latency, "at": now})
        );
    }
}

fn spawn_fake_server(reply_text: String) -> Result<String, Box<dyn std::error::Error>> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let body = serde_json::json!({"text": reply_text}).to_string();
    std::thread::spawn(move || {
        while let Ok((mut socket, _)) = listener.accept() {
            let _ = socket.set_read_timeout(Some(Duration::from_secs(5)));
            let mut request = Vec::new();
            let mut buf = [0u8; 4096];
            while let Ok(n) = socket.read(&mut buf) {
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..n]);
                if let Some(split) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..split]);
                    let len: usize = headers
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|s| s.trim().parse().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= split + 4 + len {
                        break;
                    }
                }
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes());
        }
    });
    Ok(format!("http://127.0.0.1:{port}"))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    record_chunk_latencies(true);
    let (mut wav, mut flags) = (None, HashMap::new());
    let (mut parakeet_flag, mut dictation) = (false, false);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--parakeet" {
            parakeet_flag = true;
        } else if arg == "--dictation" {
            dictation = true;
        } else if arg.starts_with("--") {
            let value = args.next().ok_or_else(|| format!("{arg} needs a value"))?;
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
    let value = |flag: &str| flags.get(flag).map(String::as_str);
    let engine = resolve_engine(
        value("--engine"),
        parakeet_flag,
        value("--url"),
        value("--fake"),
    )?;

    if let Some(dir) = value("--parakeet-dir") {
        // Set before Tokio starts workers; the daemon's loader reads this variable.
        unsafe { std::env::set_var("NIXARCHY_MEETINGAI_PARAKEET_DIR", dir) };
    }
    if let Some(speaker) = value("--speaker-model") {
        unsafe { std::env::set_var("MEETINGAI_SPEAKER_MODEL", speaker) };
    }

    let _speaker = std::env::var_os("MEETINGAI_SPEAKER_MODEL")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            default_eval_models_dir()
                .map(|m| m.join("3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx"))
                .filter(|p| p.is_file())
        })
        .map(|path| SherpaEmbedder::load(&path))
        .transpose()?;

    if parakeet::model_dir().is_some() {
        parakeet::preload()?;
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(replay(
        wav.ok_or("missing WAV path")?,
        flags,
        engine,
        dictation,
    ))
}

async fn replay(
    wav: String,
    flags: HashMap<String, String>,
    engine: Engine,
    dictation: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let value = |flag: &str| flags.get(flag).cloned();
    let glossary = value("--glossary").unwrap_or_default();
    let mut config = match &engine {
        Engine::Parakeet => ProviderConfig::new("parakeet-local", "", ""),
        Engine::Url(url) => {
            let mut c = ProviderConfig::new(
                "openai-compatible-stt",
                "",
                value("--model").unwrap_or_default(),
            );
            c.base_url = url.clone();
            c
        }
        Engine::Fake(text) => {
            let mut c = ProviderConfig::new("openai-compatible-stt", "", "fake");
            c.base_url = spawn_fake_server(text.clone())?;
            c
        }
    };
    config.language = value("--language").unwrap_or_default();

    let _ = take_chunk_latencies();

    if dictation {
        let mut reader = hound::WavReader::open(&wav)?;
        let spec = reader.spec();
        if spec.sample_rate != 16_000
            || spec.bits_per_sample != 16
            || spec.sample_format != hound::SampleFormat::Int
        {
            return Err("need a 16-bit 16 kHz PCM WAV".into());
        }
        let samples: Vec<i16> = reader.samples::<i16>().collect::<Result<_, _>>()?;
        let mono_f32: Vec<f32> = samples
            .iter()
            .step_by(usize::from(spec.channels.max(1)))
            .map(|&s| f32::from(s) / 32768.0)
            .collect();
        let duration = mono_f32.len() as f64 / 16_000.0;
        let wav_bytes = if spec.channels == 1 {
            std::fs::read(&wav)?
        } else {
            encode_wav(&mono_f32)?
        };
        let began = Instant::now();
        let text = transcribe(&config, wav_bytes, None).await?;
        let latency = began.elapsed().as_secs_f64();
        println!(
            "{}",
            serde_json::json!({"kind": "request", "latency": latency, "at": latency})
        );
        if !text.trim().is_empty() {
            print_line(
                &TranscriptLine {
                    channel: 0,
                    speaker: None,
                    text: text.trim().to_owned(),
                    is_final: true,
                    speech_final: true,
                    start: 0.0,
                    end: duration,
                },
                latency,
            );
        }
        println!(
            "{}",
            serde_json::json!({
                "kind": "summary",
                "duration": duration,
                "vm_hwm_kb": proc_status_kb("VmHWM:"),
                "vm_rss_kb": proc_status_kb("VmRSS:"),
            })
        );
        return Ok(());
    }

    let mut reader = hound::WavReader::open(&wav)?;
    let spec = reader.spec();
    if spec.channels != 2
        || spec.sample_rate != 16_000
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err("need a 16-bit stereo 16 kHz WAV".into());
    }
    let pcm: Vec<i16> = reader.samples::<i16>().collect::<Result<_, _>>()?;
    let duration = (pcm.len() / 2) as f64 / 16_000.0;

    let mut live = LiveTranscriber::connect_config_with(&config, SttHints { glossary }).await?;
    let block_samples = 3_200usize; // 100 ms of stereo 16 kHz (1,600 frames)
    let silence_f32 = vec![0.0_f32; block_samples];
    let silence_i16 = vec![0_i16; block_samples];
    let mut shadow = BatchChunks::default();
    let (mut expected_requests, mut seen_requests) = (0usize, 0usize);
    let mut ticks_since_request = 0usize;
    let mut tail_ticks = 0usize;
    let (mut at, mut finished) = (0usize, false);

    let started = tokio::time::Instant::now();
    // `--speed N` feeds N times faster than real time (default 1). The bleed hold counts audio
    // time (`chunks.now()`), so it still holds 3 s of audio; only wall-clock release times scale.
    let speed: u64 = flags
        .get("--speed")
        .and_then(|v| v.parse().ok())
        .filter(|&n| n >= 1)
        .unwrap_or(1);
    let mut tick = tokio::time::interval(Duration::from_micros(100_000 / speed));

    loop {
        tokio::select! {
            biased;
            event = live.next_event() => match event {
                None => {
                    drain_requests(started, &mut seen_requests, &mut ticks_since_request);
                    break;
                }
                Some(LiveEvent::Failed(why)) => return Err(why.into()),
                Some(LiveEvent::Transcript(line)) => {
                    drain_requests(started, &mut seen_requests, &mut ticks_since_request);
                    let released_at = started.elapsed().as_secs_f64();
                    print_line(&line, released_at);
                }
                Some(_) => {
                    drain_requests(started, &mut seen_requests, &mut ticks_since_request);
                }
            },
            _ = tick.tick(), if !finished => {
                drain_requests(started, &mut seen_requests, &mut ticks_since_request);
                if at < pcm.len() {
                    let end = (at + block_samples).min(pcm.len());
                    let slice = &pcm[at..end];
                    expected_requests += shadow.push(slice).len();
                    let stereo: Vec<f32> = slice
                        .iter()
                        .map(|&s| f32::from(s) / 32767.0)
                        .collect();
                    live.push(&stereo).await?;
                    at = end;
                    ticks_since_request += 1;
                } else {
                    // Push silent ticks so any open utterance closes (0.6 s) and any held mic
                    // line is released by filter.tick after its 3 s bleed hold.
                    tail_ticks += 1;
                    expected_requests += shadow.push(&silence_i16).len();
                    let all_decoded = seen_requests >= expected_requests;
                    let hold_elapsed = expected_requests == 0 || ticks_since_request >= 32;
                    if (tail_ticks >= 8 && all_decoded && hold_elapsed) || tail_ticks >= 180 {
                        live.finish();
                        finished = true;
                    } else {
                        live.push(&silence_f32).await?;
                        if all_decoded {
                            ticks_since_request += 1;
                        }
                    }
                }
            }
        }
    }

    println!(
        "{}",
        serde_json::json!({
            "kind": "summary",
            "duration": duration,
            "vm_hwm_kb": proc_status_kb("VmHWM:"),
            "vm_rss_kb": proc_status_kb("VmRSS:"),
        })
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Engine, resolve_engine};

    #[test]
    fn requires_exactly_one_engine() {
        for engine in [None, Some("parakeet")] {
            for parakeet in [false, true] {
                for url in [None, Some("http://127.0.0.1:8000")] {
                    for fake in [None, Some("ok")] {
                        let count = usize::from(engine.is_some())
                            + usize::from(parakeet)
                            + usize::from(url.is_some())
                            + usize::from(fake.is_some());
                        assert_eq!(
                            resolve_engine(engine, parakeet, url, fake).is_ok(),
                            count == 1
                        );
                    }
                }
            }
        }
        assert_eq!(
            resolve_engine(Some("parakeet"), false, None, None),
            Ok(Engine::Parakeet)
        );
        assert!(resolve_engine(Some("unknown"), false, None, None).is_err());
    }
}
