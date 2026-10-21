//! Reproducible render-preparation benchmark; excludes model/network latency.
use lennette::ui::{markdown, widgets::syntax};
use std::time::Instant;
fn main() {
    syntax::warm_up();
    let code = (0..250)
        .map(|i| format!("let value_{i} = format!(\"héllo 世界: {{}}\", {i});\n"))
        .collect::<String>();
    let mut boundaries = code
        .char_indices()
        .map(|(i, _)| i)
        .step_by(12)
        .collect::<Vec<_>>();
    boundaries.push(code.len());
    let mut times = Vec::new();
    for end in boundaries {
        let source = format!("## Live code\n\n```rust\n{}", &code[..end]);
        let start = Instant::now();
        for block in markdown::cached_parse(&source).iter() {
            if let markdown::Block::Code { language, text } = block {
                syntax::highlight(language.as_deref(), text, false);
            }
        }
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    println!(
        "{} append updates, {} bytes: render preparation p50={:.3}ms p95={:.3}ms max={:.3}ms",
        times.len(),
        code.len(),
        times[times.len() / 2],
        times[times.len() * 95 / 100],
        times.last().unwrap()
    );
}
