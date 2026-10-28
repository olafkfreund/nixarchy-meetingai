// Spike: Parakeet TDT 0.6B v3 INT8 via sherpa-onnx.
// Usage: parakeet <model-dir> <16k-mono.wav> [threads] [chunk-seconds (0 = whole file)]
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig, Wave};
use std::time::Instant;

fn peak_rss_mb() -> f64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    s.lines()
        .find_map(|l| l.strip_prefix("VmHWM:"))
        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<f64>().ok())
        .map_or(0.0, |kb| kb / 1024.0)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (dir, wav) = (&a[1], &a[2]);
    let wave = Wave::read(wav).expect("read wav");
    let secs = wave.samples().len() as f64 / wave.sample_rate() as f64;

    let mut c = OfflineRecognizerConfig::default();
    c.model_config.transducer = OfflineTransducerModelConfig {
        encoder: Some(format!("{dir}/encoder.int8.onnx")),
        decoder: Some(format!("{dir}/decoder.int8.onnx")),
        joiner: Some(format!("{dir}/joiner.int8.onnx")),
    };
    c.model_config.tokens = Some(format!("{dir}/tokens.txt"));
    c.model_config.model_type = Some("nemo_transducer".into());
    c.model_config.num_threads = a.get(3).and_then(|t| t.parse().ok()).unwrap_or(4);

    let t = Instant::now();
    let r = OfflineRecognizer::create(&c).expect("create recognizer");
    let load = t.elapsed().as_secs_f64();
    let rss_loaded = peak_rss_mb();

    let chunk = a.get(4).and_then(|t| t.parse::<usize>().ok()).unwrap_or(0) * wave.sample_rate() as usize;
    let samples = wave.samples();
    let chunk = if chunk == 0 { samples.len() } else { chunk };
    let (mut text, mut ntok, mut nts, mut dec) = (String::new(), 0, 0, 0.0);
    let mut first = vec![];
    for part in samples.chunks(chunk) {
        let t = Instant::now();
        let st = r.create_stream();
        st.accept_waveform(wave.sample_rate(), part);
        r.decode(&st);
        dec += t.elapsed().as_secs_f64();
        let res = st.get_result().expect("result");
        let ts = res.timestamps.as_deref().unwrap_or(&[]);
        if first.is_empty() {
            first = ts.iter().take(5).copied().collect();
        }
        (ntok, nts) = (ntok + res.tokens.len(), nts + ts.len());
        text.push_str(&res.text);
        text.push(' ');
    }
    println!("{}", &text[..text.len().min(400)]);
    println!("words={} tokens={ntok} timestamps={nts} first={first:?}", text.split_whitespace().count());
    println!("audio={secs:.1}s load={load:.2}s decode={dec:.2}s RTF={:.4} rss_after_load={rss_loaded:.0}MB peak_rss={:.0}MB", dec / secs, peak_rss_mb());
}
