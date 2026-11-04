//! Measures local speaker separation for plan step 4 (#61).
//!
//! Offline: `diarize_eval MODEL SET_DIR --condition clean|call --half dev|test [--sweep]`
//! - SET_DIR holds `<half>/<condition>/<k>spk/*.wav` (16 kHz mono) and `<half>/truth/<k>spk/*.tsv`
//!   (`speaker<TAB>start<TAB>end`, seconds), as built by `tools/diarize/`.
//! - Chunks come from the real `BatchChunks`; each chunk is labelled by `embed_windows` and
//!   `label_line`, then passes the real `LineFilter`. Only lines that come out are scored.
//! - Prints one TSV row per set, condition and speaker count; `--sweep` prints the
//!   error/coverage curve over a grid of JOIN, NEW and MIXED instead.
//!
//! Live: `diarize_eval MODEL --live AMI.wav OWNER.wav` replays the first 5 minutes of a call-condition
//! meeting (channel 1) and a one-speaker LibriSpeech track (channel 0, first 20 s enrol the owner)
//! in real time through Parakeet, once with speaker separation off and once on. Exits non-zero
//! when Parakeet or the model is missing.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use nixarchy_meetingai::ai::providers::ProviderConfig;
use nixarchy_meetingai::audio::dsp::rms;
use nixarchy_meetingai::speech::speakers::{
    JOIN, MIXED, NEW, SpeakerLabeller, embed_windows, label_line,
};
use nixarchy_meetingai::speech::stt::filter::LineFilter;
use nixarchy_meetingai::speech::stt::parakeet;
use nixarchy_meetingai::speech::transcription::{
    BatchChunks, LiveEvent, LiveTranscriber, SpeakerHints, SttHints, TranscriptLine,
};
use nixarchy_meetingai::speech::voice_check::{MicRing, SherpaEmbedder, Verdict, check, enroll};
use parking_lot::Mutex;

const RATE: u32 = 16_000;
const FRAME: usize = 480;
const SINGLE_SHARE: f64 = 0.9;

fn read(path: &Path) -> Vec<f32> {
    let mut r = hound::WavReader::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let spec = r.spec();
    assert!(
        spec.sample_rate == RATE && spec.channels == 1,
        "{}: need 16 kHz mono",
        path.display()
    );
    r.samples::<i16>()
        .map(|s| f32::from(s.unwrap()) / 32_768.0)
        .collect()
}

fn wavs(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "wav"))
        .collect();
    found.sort();
    found
}

struct Usage {
    cpu: f64,
    rss_kb: i64,
}

fn usage() -> Usage {
    // SAFETY: getrusage fills the zeroed struct it is given.
    let u = unsafe {
        let mut u = std::mem::zeroed::<libc::rusage>();
        libc::getrusage(libc::RUSAGE_SELF, &mut u);
        u
    };
    let secs = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
    Usage {
        cpu: secs(u.ru_utime) + secs(u.ru_stime),
        rss_kb: u.ru_maxrss,
    }
}

// ---------- offline ----------

enum Truth {
    Single(usize),
    Mixed,
}

struct Line {
    vectors: Vec<Vec<f32>>,
    kept: bool,
    truth: Truth,
    voiced: f64,
}

struct Conv {
    lines: Vec<Line>,
    speakers: usize,
}

#[derive(Default)]
struct Tally {
    kept: usize,
    labelled: usize,
    errors: usize,
    covered: usize,
    coverable: usize,
    /// Mixed lines under 3 s then 3 s or more: (count, abstained).
    mixed: [(usize, usize); 2],
}

/// The audio's voiced frames per true speaker, and the verdict on the chunk.
fn truth_of(samples: &[f32], start: f64, segments: &[(usize, f64, f64)]) -> (Truth, f64) {
    let mut share: HashMap<usize, usize> = HashMap::new();
    let mut voiced = 0;
    for (i, frame) in samples.as_chunks::<FRAME>().0.iter().enumerate() {
        if rms(frame) < 0.005 {
            continue;
        }
        voiced += 1;
        let t = start + (i as f64 + 0.5) * FRAME as f64 / f64::from(RATE);
        for (s, _, _) in segments.iter().filter(|(_, a, b)| *a <= t && t < *b) {
            *share.entry(*s).or_default() += 1;
        }
    }
    let seconds = voiced as f64 * FRAME as f64 / f64::from(RATE);
    match share.into_iter().max_by_key(|(_, n)| *n) {
        Some((s, n)) if n as f64 >= SINGLE_SHARE * voiced as f64 => (Truth::Single(s), seconds),
        _ => (Truth::Mixed, seconds),
    }
}

struct Loaded {
    convs: Vec<Conv>,
    chunks: usize,
    embed: Duration,
    minutes: f64,
}

fn load(model: &Path, wav_dir: &Path, truth_dir: &Path) -> Loaded {
    let e = SherpaEmbedder::load(model).expect("speaker model");
    let mut out = Loaded {
        convs: Vec::new(),
        chunks: 0,
        embed: Duration::ZERO,
        minutes: 0.0,
    };
    for wav in wavs(wav_dir) {
        let audio = read(&wav);
        out.minutes += audio.len() as f64 / f64::from(RATE) / 60.0;
        let tsv = truth_dir
            .join(wav.file_stem().unwrap())
            .with_extension("tsv");
        let mut names: Vec<String> = Vec::new();
        let segments: Vec<(usize, f64, f64)> = std::fs::read_to_string(&tsv)
            .unwrap_or_else(|e| panic!("{}: {e}", tsv.display()))
            .lines()
            .map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                let who = names.iter().position(|n| n == f[0]).unwrap_or_else(|| {
                    names.push(f[0].to_string());
                    names.len() - 1
                });
                (who, f[1].parse().unwrap(), f[2].parse().unwrap())
            })
            .collect();
        let mut chunker = BatchChunks::default();
        let mut chunks = Vec::new();
        for block in audio.chunks(1_600) {
            let pcm: Vec<i16> = block
                .iter()
                .flat_map(|s| [0, (s * 32_767.0) as i16])
                .collect();
            chunks.extend(chunker.push(&pcm));
        }
        chunks.extend(chunker.finish());
        let mut filter = LineFilter::default();
        let mut lines = Vec::new();
        for (n, (channel, start, end, samples)) in chunks.into_iter().enumerate() {
            let began = Instant::now();
            let vectors = embed_windows(&e, &samples);
            out.embed += began.elapsed();
            out.chunks += 1;
            let (truth, voiced) = truth_of(&samples, start, &segments);
            // Distinct words per line, so the repeat and loop filters see real speech.
            let text = format!("alpha{n} bravo{n} charlie{n} delta{n}");
            let line = TranscriptLine {
                channel,
                speaker: None,
                text,
                is_final: true,
                speech_final: true,
                start,
                end,
            };
            let kept = filter.accept(&line).iter().any(|l| l.start == start);
            lines.push(Line {
                vectors,
                kept,
                truth,
                voiced,
            });
        }
        out.convs.push(Conv {
            lines,
            speakers: names.len(),
        });
    }
    out
}

/// Most lines matched by a one-to-one mapping of labels to true speakers.
fn best_mapping(counts: &HashMap<(i64, usize), usize>, labels: &[i64], speakers: usize) -> usize {
    fn go(counts: &HashMap<(i64, usize), usize>, labels: &[i64], used: &mut Vec<bool>) -> usize {
        let Some((label, rest)) = labels.split_first() else {
            return 0;
        };
        let mut best = go(counts, rest, used);
        for s in 0..used.len() {
            if !used[s] {
                used[s] = true;
                let hit = counts.get(&(*label, s)).copied().unwrap_or(0);
                best = best.max(hit + go(counts, rest, used));
                used[s] = false;
            }
        }
        best
    }
    go(counts, labels, &mut vec![false; speakers])
}

fn score(convs: &[Conv], limits: (f32, f32, f32)) -> Tally {
    let mut t = Tally::default();
    for conv in convs {
        let labeller = Mutex::new(SpeakerLabeller::with_limits(
            0, limits.0, limits.1, limits.2,
        ));
        let mut counts: HashMap<(i64, usize), usize> = HashMap::new();
        let mut labels = BTreeSet::new();
        let mut single_labelled = 0;
        for line in &conv.lines {
            // The labeller sees every chunk, as the batch path does; the filter decides what is shown.
            let label = label_line(&labeller, &line.vectors);
            if !line.kept {
                continue;
            }
            t.kept += 1;
            match (&line.truth, label) {
                (Truth::Single(s), label) => {
                    if line.voiced >= 1.5 {
                        t.coverable += 1;
                        t.covered += usize::from(label.is_some());
                    }
                    if let Some(l) = label {
                        *counts.entry((l, *s)).or_default() += 1;
                        labels.insert(l);
                        single_labelled += 1;
                    }
                }
                (Truth::Mixed, label) => {
                    let slot = &mut t.mixed[usize::from(line.voiced >= 3.0)];
                    slot.0 += 1;
                    slot.1 += usize::from(label.is_none());
                    if label.is_some() {
                        t.errors += 1;
                        t.labelled += 1;
                    }
                }
            }
        }
        let labels: Vec<i64> = labels.into_iter().collect();
        t.labelled += single_labelled;
        t.errors += single_labelled - best_mapping(&counts, &labels, conv.speakers);
    }
    t
}

fn ratio(n: usize, d: usize) -> String {
    if d == 0 {
        "NA".into()
    } else {
        format!("{:.4}", n as f64 / d as f64)
    }
}

fn offline(model: &Path, set: &Path, condition: &str, half: &str, sweep: bool) {
    let name = set.file_name().unwrap().to_string_lossy().into_owned();
    let mut counts: Vec<PathBuf> = std::fs::read_dir(set.join(half).join(condition))
        .unwrap_or_else(|e| panic!("{condition}/{half}: {e}"))
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    counts.sort();
    let mut all = Vec::new();
    if !sweep {
        println!(
            "set\tcondition\tspeakers\tconvs\tlines\tlabelled\terrors\terror_rate\tcoverage\tmixed_lt3_n\tmixed_lt3_abstain\tmixed_ge3_n\tmixed_ge3_abstain\tembed_ms_per_line\tcpu_s_per_min\trss_delta_mb"
        );
    }
    for dir in counts {
        let k = dir.file_name().unwrap().to_string_lossy().into_owned();
        let before = usage();
        let loaded = load(model, &dir, &set.join(half).join("truth").join(&k));
        if sweep {
            all.extend(loaded.convs);
            continue;
        }
        let t = score(&loaded.convs, (JOIN, NEW, MIXED));
        let after = usage();
        println!(
            "{name}\t{condition}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.1}\t{:.2}\t{:.0}",
            k.trim_end_matches("spk"),
            loaded.convs.len(),
            t.kept,
            t.labelled,
            t.errors,
            ratio(t.errors, t.labelled),
            ratio(t.covered, t.coverable),
            t.mixed[0].0,
            ratio(t.mixed[0].1, t.mixed[0].0),
            t.mixed[1].0,
            ratio(t.mixed[1].1, t.mixed[1].0),
            loaded.embed.as_secs_f64() * 1000.0 / loaded.chunks.max(1) as f64,
            (after.cpu - before.cpu) / loaded.minutes.max(1e-9),
            (after.rss_kb - before.rss_kb) as f64 / 1024.0,
        );
    }
    if sweep {
        println!("join\tnew\tmixed\terror_rate\tcoverage\tmixed_ge3_abstain");
        let steps = |lo: f32, hi: f32| {
            (0..)
                .map(move |i| lo + 0.05 * i as f32)
                .take_while(move |v| *v <= hi + 1e-4)
        };
        for join in steps(0.40, 0.70) {
            for new in steps(0.20, 0.45).filter(|n| *n <= join) {
                for mixed in steps(0.25, 0.60) {
                    let t = score(&all, (join, new, mixed));
                    println!(
                        "{join:.2}\t{new:.2}\t{mixed:.2}\t{}\t{}\t{}",
                        ratio(t.errors, t.labelled),
                        ratio(t.covered, t.coverable),
                        ratio(t.mixed[1].1, t.mixed[1].0),
                    );
                }
            }
        }
    }
}

// ---------- live ----------

fn fail(reason: &str) -> ! {
    eprintln!("{reason}");
    std::process::exit(1);
}

fn median(v: &mut [f64]) -> String {
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2)
        .map_or("NA".into(), |m| format!("{m:.0}"))
}

async fn live_run(
    label: &str,
    embedder: &Arc<SherpaEmbedder>,
    audio: &[f32],
    owner: &[f32],
    sample: &[f32],
    separation: bool,
) {
    let speakers = separation.then(|| SpeakerHints {
        labeller: Arc::new(Mutex::new(SpeakerLabeller::new(0))),
        embedder: embedder.clone(),
    });
    let config = ProviderConfig::new("parakeet-local", "", "");
    let mut live = LiveTranscriber::connect_config_with(
        &config,
        SttHints {
            glossary: String::new(),
            speakers,
        },
    )
    .await
    .unwrap_or_else(|e| fail(&format!("speech connect failed: {e}")));
    let block = 1_600;
    let mut ring = MicRing::new(60.0, RATE);
    let started = tokio::time::Instant::now();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let (mut at, mut finished) = (0, false);
    let mut latency: [Vec<f64>; 2] = Default::default();
    let mut labelled = 0;
    let mut checks = Vec::new();
    loop {
        tokio::select! {
            event = live.next_event() => match event {
                None => break,
                Some(LiveEvent::Failed(why)) => fail(&format!("speech failed: {why}")),
                Some(LiveEvent::Transcript(line)) => {
                    let late = started.elapsed().as_secs_f64() - line.end;
                    latency[usize::from(line.channel != 0)].push(late * 1000.0);
                    labelled += usize::from(line.speaker.is_some());
                    if line.channel == 0
                        && let Some(heard) = ring.cut(line.start, line.end)
                    {
                        let (e, s) = (embedder.clone(), sample.to_vec());
                        checks.push(tokio::task::spawn_blocking(move || {
                            let t = Instant::now();
                            let v = check(e.as_ref(), &s, &heard, RATE);
                            (t.elapsed().as_secs_f64() * 1000.0, v == Verdict::Owner)
                        }));
                    }
                }
                Some(_) => {}
            },
            _ = tick.tick(), if !finished => {
                if at >= audio.len() {
                    live.finish();
                    finished = true;
                    continue;
                }
                let end = (at + block).min(audio.len());
                let stereo: Vec<f32> = (at..end)
                    .flat_map(|i| [owner.get(i).copied().unwrap_or(0.0), audio[i]])
                    .collect();
                ring.push_interleaved(&stereo);
                live.push(&stereo).await.unwrap_or_else(|e| fail(&format!("push failed: {e}")));
                at = end;
            }
        }
    }
    let (mut times, mut owners) = (Vec::new(), 0);
    for c in checks {
        let (ms, owner) = c.await.unwrap();
        times.push(ms);
        owners += usize::from(owner);
    }
    println!(
        "{label}\tmic_n={}\tmic_median_ms={}\tsystem_n={}\tsystem_median_ms={}\tsystem_labelled={labelled}\towner_checks={}\towner_kept={owners}\towner_check_median_ms={}",
        latency[0].len(),
        median(&mut latency[0]),
        latency[1].len(),
        median(&mut latency[1]),
        times.len(),
        median(&mut times),
    );
}

fn live(model: &Path, ami: &Path, owner: &Path) {
    if let Some(why) = parakeet::not_ready_reason() {
        fail(&why);
    }
    parakeet::preload().unwrap_or_else(|e| fail(&format!("Parakeet failed to load: {e}")));
    let embedder = Arc::new(SherpaEmbedder::load(model).unwrap_or_else(|e| fail(&e)));
    let mut audio = read(ami);
    audio.truncate(RATE as usize * 300);
    let mut owner = read(owner);
    let rest = owner.split_off((RATE as usize * 20).min(owner.len()));
    let sample = enroll(embedder.as_ref(), &owner, RATE).unwrap_or_else(|e| fail(&e));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    println!("run\tchannel and owner-check timings (chunk end to line, ms)");
    for separation in [false, true] {
        let name = if separation {
            "separation_on"
        } else {
            "separation_off"
        };
        runtime.block_on(live_run(
            name, &embedder, &audio, &rest, &sample, separation,
        ));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    };
    match args.as_slice() {
        [model, mode, ami, owner] if mode == "--live" => {
            live(Path::new(model), Path::new(ami), Path::new(owner));
        }
        [model, set, ..] => {
            let (Some(condition), Some(half)) = (flag("--condition"), flag("--half")) else {
                eprintln!(
                    "usage: diarize_eval MODEL SET_DIR --condition clean|call --half dev|test [--sweep]"
                );
                std::process::exit(2);
            };
            offline(
                Path::new(model),
                Path::new(set),
                &condition,
                &half,
                args.iter().any(|a| a == "--sweep"),
            );
        }
        _ => {
            eprintln!(
                "usage: diarize_eval MODEL SET_DIR --condition clean|call --half dev|test [--sweep]\n       diarize_eval MODEL --live AMI.wav OWNER.wav"
            );
            std::process::exit(2);
        }
    }
}
