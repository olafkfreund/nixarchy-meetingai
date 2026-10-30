//! Scores the owner voice check on a labelled set, for plan step 7 (#31).
//!
//! Usage: `voice_eval MODEL ENROLL.tsv TRIALS.tsv`
//! - ENROLL.tsv: `owner<TAB>wav` (16 kHz mono, the owner's enrollment audio)
//! - TRIALS.tsv: `owner<TAB>label<TAB>condition<TAB>wav[<TAB>bleed.wav<TAB>bleed_db]`; the bleed
//!   (far-end speech from the speakers) is mixed in 120 ms late at `bleed_db`.
//!
//! Prints `owner<TAB>label<TAB>condition<TAB>score` per trial; `NA` when the check would keep
//! the line as the owner's for being too short or unclear. Audio stays where it is.

use std::collections::HashMap;
use std::path::Path;

use nixarchy_meetingai::speech::voice_check::{SherpaEmbedder, enroll, score};

const RATE: u32 = 16_000;

fn read(path: &str) -> Vec<f32> {
    let mut r = hound::WavReader::open(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let spec = r.spec();
    assert!(
        spec.sample_rate == RATE && spec.channels == 1,
        "{path}: need 16 kHz mono"
    );
    r.samples::<i16>()
        .map(|s| f32::from(s.unwrap()) / 32_768.0)
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [model, enroll_list, trials] = args.as_slice() else {
        eprintln!("usage: voice_eval MODEL ENROLL.tsv TRIALS.tsv");
        std::process::exit(2);
    };
    let e = SherpaEmbedder::load(Path::new(model)).expect("speaker model");
    let mut samples = HashMap::new();
    for line in std::fs::read_to_string(enroll_list).unwrap().lines() {
        let (owner, wav) = line.split_once('\t').expect("owner<TAB>wav");
        match enroll(&e, &read(wav), RATE) {
            Ok(sample) => drop(samples.insert(owner.to_string(), sample)),
            Err(err) => eprintln!("enroll {owner}: {err}"),
        }
    }
    for line in std::fs::read_to_string(trials).unwrap().lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let mut heard = read(f[3]);
        if let (Some(bleed), Some(db)) = (f.get(4), f.get(5)) {
            let gain = 10f32.powf(db.parse::<f32>().unwrap() / 20.0);
            let delay = RATE as usize * 120 / 1_000;
            for (h, b) in heard.iter_mut().skip(delay).zip(read(bleed)) {
                *h = (*h + gain * b).clamp(-1.0, 1.0);
            }
        }
        let Some(sample) = samples.get(f[0]) else {
            continue;
        };
        let s = score(&e, sample, &heard, RATE);
        let s = s.map_or("NA".into(), |s| format!("{s:.4}"));
        println!("{}\t{}\t{}\t{s}", f[0], f[1], f[2]);
    }
}
