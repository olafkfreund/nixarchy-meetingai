//! Runs a stereo 16 kHz WAV (left = mic, right = system) through the daemon's echo gate in
//! 50 ms frames, as the live session does with `echo_gate` on, and writes the result.
//! Usage: `echo_gate_file IN.wav OUT.wav` (#14's pending echo_gate decision, measured in #32).

use nixarchy_meetingai::audio::echo_gate::EchoGate;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output] = args.as_slice() else {
        eprintln!("usage: echo_gate_file IN.wav OUT.wav");
        std::process::exit(2);
    };
    let mut reader = hound::WavReader::open(input).expect("input wav");
    let spec = reader.spec();
    assert!(spec.channels == 2 && spec.sample_rate == 16_000, "need stereo 16 kHz");
    let mut samples: Vec<f32> = reader
        .samples::<i16>()
        .map(|s| f32::from(s.unwrap()) / 32_768.0)
        .collect();
    let mut gate = EchoGate::default();
    for frame in samples.chunks_mut(800 * 2) {
        gate.process(frame);
    }
    let mut writer = hound::WavWriter::create(output, spec).expect("output wav");
    for s in samples {
        writer.write_sample((s.clamp(-1.0, 1.0) * 32_767.0) as i16).unwrap();
    }
    writer.finalize().unwrap();
}
