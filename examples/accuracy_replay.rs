//! Replays a 16 kHz WAV through the daemon's real `LiveTranscriber::connect_config_with` at 1x
//! wall-clock (or `transcription::transcribe` with `--dictation`). Prints one JSON line per
//! inference request `{"kind":"request","latency","at"}`, one per line the filter releases
//! `{"kind":"line","channel","start","end","released_at","text"}`, and a final summary with
//! `VmHWM` peak RSS `{"kind":"summary","duration","vm_hwm_kb","vm_rss_kb","load_mode"}`.
//! Driven by `tools/accuracy/score.py`.
//! Usage: accuracy_replay FILE.wav (--engine parakeet|whisper-nb|turbo | --parakeet | --url URL | --fake TEXT)
//!        [--model-dir DIR] [--parakeet-dir DIR] [--vad PATH] [--speaker-model PATH]
//!        [--load-mode resident|unload] [--dictation] [--model M] [--language L] [--glossary G]
//!        [--no-filter | --turns FILE]
//!        accuracy_replay --filter-lines RUN_B.jsonl

use nixarchy_meetingai::ai::providers::ProviderConfig;
use nixarchy_meetingai::audio::encode_wav;
use nixarchy_meetingai::speech::stt::filter::LineFilter;
use nixarchy_meetingai::speech::stt::{
    LoadMode, parakeet, record_chunk_latencies, set_load_mode, take_chunk_latencies,
    transcription::{
        BatchChunks, LiveEvent, LiveTranscriber, SttHints, TranscriptLine, transcribe,
        transcribe_chunk,
    },
    whisper_nb,
};
use nixarchy_meetingai::speech::voice_check::SherpaEmbedder;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Engine {
    Parakeet,
    WhisperNb,
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
        return Err(
            "give exactly one of --engine parakeet|whisper-nb|turbo, --parakeet, --url URL, or --fake TEXT",
        );
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
        "whisper-nb" | "turbo" => Ok(Engine::WhisperNb),
        _ => Err("--engine must be parakeet, whisper-nb, or turbo"),
    }
}

fn parse_load_mode(value: Option<&str>) -> Result<(LoadMode, &'static str), &'static str> {
    match value.unwrap_or("resident") {
        "resident" => Ok((LoadMode::Resident, "resident")),
        "unload" => Ok((LoadMode::UnloadInactive, "unload")),
        _ => Err("--load-mode must be resident or unload"),
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

fn find_vad_path(explicit: Option<&str>, model_dir: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return Some(PathBuf::from(p));
    }
    if let Some(env) = std::env::var_os("NIXARCHY_MEETINGAI_SILERO_VAD").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(env));
    }
    let mut candidates = Vec::new();
    if let Some(dir) = model_dir {
        let d = PathBuf::from(dir);
        candidates.push(d.join("silero_vad.onnx"));
        if let Some(parent) = d.parent() {
            candidates.push(parent.join("silero_vad.onnx"));
            candidates.push(parent.join("silero-vad/silero_vad.onnx"));
        }
    }
    if let Some(models) = default_eval_models_dir() {
        candidates.push(models.join("silero_vad.onnx"));
        candidates.push(models.join("silero-vad/silero_vad.onnx"));
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn print_line(line: &TranscriptLine, released_at: Option<f64>) {
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

#[derive(Deserialize)]
struct TurnCut {
    channel: u8,
    start: f64,
    end: f64,
}

fn cut_turn(pcm: &[i16], turn: &TurnCut) -> Result<Vec<f32>, &'static str> {
    let frames = pcm.len() / 2;
    if turn.channel > 1
        || !turn.start.is_finite()
        || !turn.end.is_finite()
        || turn.start < 0.0
        || turn.end <= turn.start
        || turn.end * 16_000.0 > frames as f64 + 0.5
    {
        return Err("invalid turn bounds or channel");
    }
    let start = (turn.start * 16_000.0).round() as usize;
    let end = (turn.end * 16_000.0).round() as usize;
    if end > frames || start >= end {
        return Err("invalid turn bounds");
    }
    Ok(pcm[start * 2 + usize::from(turn.channel)..end * 2]
        .iter()
        .step_by(2)
        .map(|&s| f32::from(s) / 32768.0)
        .collect())
}

fn line(channel: u8, start: f64, end: f64, text: String) -> TranscriptLine {
    TranscriptLine {
        channel,
        speaker: None,
        text,
        is_final: true,
        speech_final: true,
        start,
        end,
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum SavedRecord {
    #[serde(rename = "chunk")]
    Chunk {
        channel: u8,
        start: f64,
        end: f64,
        closed: f64,
    },
    #[serde(rename = "line")]
    Line {
        channel: u8,
        start: f64,
        end: f64,
        text: String,
    },
    #[serde(other)]
    Other,
}

struct SavedChunk {
    channel: u8,
    start: f64,
    end: f64,
    closed: f64,
    line: Option<TranscriptLine>,
}

fn filter_lines(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut chunks: Vec<SavedChunk> = Vec::new();
    for row in std::fs::read_to_string(path)?.lines() {
        match serde_json::from_str::<SavedRecord>(row)? {
            SavedRecord::Chunk {
                channel,
                start,
                end,
                closed,
            } => {
                if channel > 1
                    || !start.is_finite()
                    || !end.is_finite()
                    || !closed.is_finite()
                    || start < 0.0
                    || start >= end
                    || closed < end
                    || chunks.last().is_some_and(|c| c.closed > closed)
                {
                    return Err("invalid chunk record".into());
                }
                chunks.push(SavedChunk {
                    channel,
                    start,
                    end,
                    closed,
                    line: None,
                });
            }
            SavedRecord::Line {
                channel,
                start,
                end,
                text,
            } => {
                let chunk = chunks.last_mut().ok_or("line without chunk")?;
                if chunk.line.is_some()
                    || (channel, start, end) != (chunk.channel, chunk.start, chunk.end)
                {
                    return Err("line does not match preceding chunk".into());
                }
                chunk.line = Some(line(channel, start, end, text));
            }
            SavedRecord::Other => {}
        }
    }
    for (_, released) in filtered(&chunks) {
        print_line(&released, None);
    }
    Ok(())
}

fn filtered(chunks: &[SavedChunk]) -> Vec<(f64, TranscriptLine)> {
    let mut filter = LineFilter::default();
    let mut output = Vec::new();
    let mut next = 0;
    let end = chunks.iter().map(|c| c.closed).fold(0.0_f64, f64::max) + 4.0;
    for tick in 0..=(end * 10.0).ceil() as usize {
        let now = tick as f64 / 10.0;
        let earliest = chunks
            .iter()
            .filter(|c| c.channel == 1 && c.start <= now && now < c.end)
            .map(|c| c.start)
            .reduce(f64::min);
        let open_end = earliest.map(|_| now);
        output.extend(
            filter
                .tick(now, earliest, open_end)
                .into_iter()
                .map(|l| (now, l)),
        );
        while next < chunks.len() && chunks[next].closed <= now {
            if let Some(line) = &chunks[next].line {
                output.extend(filter.accept(line).into_iter().map(|l| (now, l)));
            }
            next += 1;
        }
    }
    output.extend(filter.flush().into_iter().map(|l| (end, l)));
    output
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
    let (mut parakeet_flag, mut dictation, mut no_filter) = (false, false, false);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--parakeet" {
            parakeet_flag = true;
        } else if arg == "--dictation" {
            dictation = true;
        } else if arg == "--no-filter" {
            no_filter = true;
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
    if let Some(path) = value("--filter-lines") {
        if wav.is_some() || no_filter || dictation {
            return Err("--filter-lines takes only a log path".into());
        }
        return filter_lines(path);
    }
    if usize::from(no_filter) + usize::from(value("--turns").is_some()) + usize::from(dictation) > 1
    {
        return Err("choose only one replay mode".into());
    }
    let engine = resolve_engine(
        value("--engine"),
        parakeet_flag,
        value("--url"),
        value("--fake"),
    )?;
    let (load_mode, load_mode_name) = parse_load_mode(value("--load-mode"))?;

    if let Some(dir) = value("--parakeet-dir") {
        // Set before Tokio starts workers; the daemon's loader reads this variable.
        unsafe { std::env::set_var("NIXARCHY_MEETINGAI_PARAKEET_DIR", dir) };
    }
    match &engine {
        Engine::Parakeet => {
            if let Some(dir) = value("--model-dir") {
                unsafe { std::env::set_var("NIXARCHY_MEETINGAI_PARAKEET_DIR", dir) };
            }
        }
        Engine::WhisperNb => {
            if let Some(dir) = value("--model-dir") {
                unsafe { std::env::set_var("NIXARCHY_MEETINGAI_WHISPER_NB_DIR", dir) };
            }
            if let Some(vad) = find_vad_path(value("--vad"), value("--model-dir")) {
                unsafe { std::env::set_var("NIXARCHY_MEETINGAI_SILERO_VAD", vad) };
            }
        }
        Engine::Url(_) | Engine::Fake(_) => {}
    }
    if let Some(speaker) = value("--speaker-model") {
        unsafe { std::env::set_var("MEETINGAI_SPEAKER_MODEL", speaker) };
    }

    set_load_mode(load_mode);

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
    if engine == Engine::WhisperNb {
        whisper_nb::preload()?;
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(replay(
        wav.ok_or("missing WAV path")?,
        flags,
        engine,
        dictation,
        no_filter,
        load_mode_name,
    ))
}

async fn replay(
    wav: String,
    flags: HashMap<String, String>,
    engine: Engine,
    dictation: bool,
    no_filter: bool,
    load_mode_name: &'static str,
) -> Result<(), Box<dyn std::error::Error>> {
    let value = |flag: &str| flags.get(flag).cloned();
    let glossary = value("--glossary").unwrap_or_default();
    let mut config = match &engine {
        Engine::Parakeet => ProviderConfig::new("parakeet-local", "", ""),
        Engine::WhisperNb => ProviderConfig::new("whisper-nb-local", "", ""),
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

    if let Some(turns_path) = value("--turns") {
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
        let turns: Vec<TurnCut> = serde_json::from_str(&std::fs::read_to_string(turns_path)?)?;
        for turn in turns {
            let samples = cut_turn(&pcm, &turn)?;
            let text = transcribe(&config, encode_wav(&samples)?, None).await?;
            if !text.trim().is_empty() {
                print_line(
                    &line(turn.channel, turn.start, turn.end, text.trim().to_owned()),
                    None,
                );
            }
        }
        return Ok(());
    }

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
                Some(latency),
            );
        }
        println!(
            "{}",
            serde_json::json!({
                "kind": "summary",
                "duration": duration,
                "vm_hwm_kb": proc_status_kb("VmHWM:"),
                "vm_rss_kb": proc_status_kb("VmRSS:"),
                "load_mode": load_mode_name,
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

    if no_filter {
        let mut chunks = BatchChunks::default();
        let silence = [0_i16; 3_200];
        let mut frames = 0;
        for block in pcm
            .chunks(3_200)
            .chain(std::iter::repeat_n(silence.as_slice(), 40))
        {
            frames += block.len() / 2;
            let closed = frames as f64 / 16_000.0;
            for (channel, start, end, samples) in chunks.push(block) {
                println!(
                    "{}",
                    serde_json::json!({"kind":"chunk","channel":channel,"start":start,"end":end,"closed":closed})
                );
                let text = transcribe_chunk(&config, &samples, "").await?;
                if !text.is_empty() {
                    print_line(&line(channel, start, end, text), None);
                }
            }
        }
        for (channel, start, end, samples) in chunks.finish() {
            let closed = duration + 4.0;
            println!(
                "{}",
                serde_json::json!({"kind":"chunk","channel":channel,"start":start,"end":end,"closed":closed})
            );
            let text = transcribe_chunk(&config, &samples, "").await?;
            if !text.is_empty() {
                print_line(&line(channel, start, end, text), None);
            }
        }
        return Ok(());
    }

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
                    print_line(&line, Some(released_at));
                }
                Some(LiveEvent::Notice(text)) => {
                    drain_requests(started, &mut seen_requests, &mut ticks_since_request);
                    println!("{}", serde_json::json!({"kind":"notice","text":text}));
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
            "load_mode": load_mode_name,
        })
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        Engine, LoadMode, SavedChunk, TurnCut, cut_turn, filtered, line, parse_load_mode,
        resolve_engine,
    };

    #[test]
    fn requires_exactly_one_engine() {
        for engine in [None, Some("parakeet"), Some("whisper-nb"), Some("turbo")] {
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
            resolve_engine(Some("turbo"), false, None, None),
            Ok(Engine::WhisperNb)
        );
        assert!(resolve_engine(Some("unknown"), false, None, None).is_err());
    }

    #[test]
    fn parses_load_modes() {
        assert_eq!(parse_load_mode(None), Ok((LoadMode::Resident, "resident")));
        assert_eq!(
            parse_load_mode(Some("resident")),
            Ok((LoadMode::Resident, "resident"))
        );
        assert_eq!(
            parse_load_mode(Some("unload")),
            Ok((LoadMode::UnloadInactive, "unload"))
        );
        assert!(parse_load_mode(Some("other")).is_err());
    }

    #[test]
    fn offline_clock_holds_mic_for_three_seconds() {
        let chunks = [SavedChunk {
            channel: 0,
            start: 0.0,
            end: 1.0,
            closed: 1.0,
            line: Some(line(0, 0.0, 1.0, "hello world".into())),
        }];
        let output = filtered(&chunks);
        assert_eq!(output.len(), 1);
        assert!((output[0].0 - 4.0).abs() < 0.0001);
    }

    #[test]
    fn offline_clock_removes_system_echo() {
        let text = "the project meeting starts tomorrow";
        let chunks = [
            SavedChunk {
                channel: 0,
                start: 0.0,
                end: 1.0,
                closed: 1.0,
                line: Some(line(0, 0.0, 1.0, text.into())),
            },
            SavedChunk {
                channel: 1,
                start: 0.0,
                end: 1.5,
                closed: 1.5,
                line: Some(line(1, 0.0, 1.5, text.into())),
            },
        ];
        let output = filtered(&chunks);
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].1.channel, 1);
    }

    #[test]
    fn same_turn_cut_is_identical() {
        let pcm = [0, 10, 1, 11, 2, 12, 3, 13];
        let turn = TurnCut {
            channel: 1,
            start: 1.0 / 16_000.0,
            end: 3.0 / 16_000.0,
        };
        assert_eq!(
            cut_turn(&pcm, &turn).unwrap(),
            cut_turn(&pcm, &turn).unwrap()
        );
        assert_eq!(
            cut_turn(&pcm, &turn).unwrap(),
            vec![11.0 / 32768.0, 12.0 / 32768.0]
        );
    }
}
