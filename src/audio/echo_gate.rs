//! Conservative, frame-level suppression of system audio leaking into the mic.

pub(crate) const FRAME: usize = 800; // 50 ms at 16 kHz
const BIN: usize = 16; // 1 ms energy bins
const BINS: usize = FRAME / BIN;
const MAX_LAG: usize = 250; // ms
const HISTORY: usize = MAX_LAG + 500 + BINS;
const HANGOVER: usize = 250; // at least 200 ms after a detected 50 ms frame

/// Fixed storage keeps frame processing allocation-free.
pub struct EchoGate {
    system: [f32; HISTORY],
    mic: [f32; HISTORY],
    system_envelope: [f32; HISTORY],
    mic_envelope: [f32; HISTORY],
    filled: usize,
    last_lag: Option<usize>,
    gain: Option<f32>,
    hangover: usize,
}

impl Default for EchoGate {
    fn default() -> Self {
        Self {
            system: [0.0; HISTORY],
            mic: [0.0; HISTORY],
            system_envelope: [0.0; HISTORY],
            mic_envelope: [0.0; HISTORY],
            filled: 0,
            last_lag: None,
            gain: None,
            hangover: 0,
        }
    }
}

impl EchoGate {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn protecting(&self) -> bool {
        self.hangover > 0
    }

    /// `samples` is one interleaved frame of at most 50 ms, mic left and system right.
    pub fn process(&mut self, samples: &mut [f32]) {
        if samples.is_empty() || samples.len() > FRAME * 2 || !samples.len().is_multiple_of(BIN * 2)
        {
            return;
        }
        let bins = samples.len() / (BIN * 2);
        self.system.copy_within(bins.., 0);
        self.mic.copy_within(bins.., 0);
        self.system_envelope.copy_within(bins.., 0);
        self.mic_envelope.copy_within(bins.., 0);
        for (bin, chunk) in samples.as_chunks::<{ BIN * 2 }>().0.iter().enumerate() {
            let (mut mic, mut system) = (0.0, 0.0);
            for pair in chunk.as_chunks::<2>().0.iter() {
                mic += pair[0] * pair[0];
                system += pair[1] * pair[1];
            }
            self.mic[HISTORY - bins + bin] = mic / BIN as f32;
            self.system[HISTORY - bins + bin] = system / BIN as f32;
        }
        for index in HISTORY - bins..HISTORY {
            self.system_envelope[index] = smooth(&self.system, index);
            self.mic_envelope[index] = smooth(&self.mic, index);
        }
        self.filled = (self.filled + bins).min(HISTORY);

        // The cached seven-bin average tolerates fractional-bin delay and room dispersion.
        let window = self.filled.saturating_sub(MAX_LAG).clamp(bins, 200);
        let mut best = None;
        for lag in 0..=MAX_LAG.min(self.filled - bins) {
            let start = HISTORY - window;
            let mut sums = [0.0; 5]; // x, y, x², y², xy
            for index in start..HISTORY {
                let x = self.system_envelope[index - lag];
                let y = self.mic_envelope[index];
                sums[0] += x;
                sums[1] += y;
                sums[2] += x * x;
                sums[3] += y * y;
                sums[4] += x * y;
            }
            let n = window as f32;
            let covariance = sums[4] - sums[0] * sums[1] / n;
            let variance = (sums[2] - sums[0] * sums[0] / n) * (sums[3] - sums[1] * sums[1] / n);
            if covariance <= 0.0 || variance <= 0.0 {
                continue;
            }
            let correlation = covariance * covariance / variance;
            if best.is_none_or(|(_, score, _)| correlation > score) {
                best = Some((lag, correlation, sums));
            }
        }
        let Some((lag, correlation, sums)) = best else {
            self.last_lag = None;
            self.hangover = self.hangover.saturating_sub(bins);
            return;
        };
        if correlation < 0.75 {
            self.last_lag = None;
            self.hangover = self.hangover.saturating_sub(bins);
            return;
        }
        if self
            .last_lag
            .is_some_and(|previous| previous.abs_diff(lag) > 2)
        {
            self.gain = None;
        }
        let recent = HISTORY - bins;
        let (mut cross, mut system_power) = (0.0, 0.0);
        for index in recent..HISTORY {
            let x = self.system_envelope[index - lag];
            let y = self.mic_envelope[index];
            cross += x * y;
            system_power += x * x;
        }
        let observed_gain = if self.filled >= lag + 100 {
            sums[4] / sums[2]
        } else {
            cross / system_power
        };
        let gain = self.gain.unwrap_or(observed_gain);
        let mut predicted = 0.0;
        let mut residual = 0.0;
        for index in recent..HISTORY {
            let x = self.system_envelope[index - lag];
            let y = self.mic_envelope[index];
            predicted += gain * x;
            residual += (y - gain * x).abs();
        }
        // Average excess over several frames; a room filter alone can vary by one bin.
        let mut history_predicted = 0.0;
        let mut history_residual = 0.0;
        for index in HISTORY - self.filled.min(100)..HISTORY {
            let x = self.system_envelope[index - lag];
            let y = self.mic_envelope[index];
            history_predicted += gain * x;
            history_residual += (y - gain * x).abs();
        }
        let mut short_predicted = 0.0;
        let mut short_actual = 0.0;
        for index in HISTORY - self.filled.min(40)..HISTORY {
            short_predicted += gain * self.system_envelope[index - lag];
            short_actual += self.mic_envelope[index];
        }
        let ratio = short_actual / short_predicted;
        let double_talk = self.gain.is_some()
            && (history_residual > history_predicted * 0.25
                || ratio > 1.12
                || residual > predicted * 0.18);
        if double_talk || (self.hangover > 0 && ratio > 1.04) {
            self.hangover = HANGOVER;
        } else {
            self.hangover = self.hangover.saturating_sub(bins);
        }
        let stable = self
            .last_lag
            .is_none_or(|previous| previous.abs_diff(lag) <= 2);
        let established = self
            .last_lag
            .is_some_and(|previous| previous.abs_diff(lag) <= 2);
        self.last_lag = Some(lag);
        if established && self.filled >= lag + 100 && !double_talk && self.hangover == 0 {
            self.gain = Some(gain * 0.5 + observed_gain * 0.5);
        }
        if stable && self.hangover == 0 && residual < predicted * 0.25 {
            for pair in samples.as_chunks_mut::<2>().0.iter_mut() {
                pair[0] = 0.0;
            }
        }
    }
}

fn smooth(values: &[f32; HISTORY], index: usize) -> f32 {
    (0..7).map(|offset| values[index - offset]).sum::<f32>() / 7.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signal(seed: u32) -> Vec<f32> {
        let mut state = seed;
        let mut level = 0.5;
        (0..24_000)
            .map(|i| {
                if i % 160 == 0 {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    level = 0.2 + (state % 800) as f32 / 1_000.0;
                }
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                level * (state as f32 / u32::MAX as f32 * 2.0 - 1.0)
            })
            .collect()
    }

    fn echo(system: &[f32], lag: usize) -> Vec<f32> {
        (0..system.len())
            .map(|i| {
                [(0, 0.55), (3, 0.25), (9, 0.12)]
                    .into_iter()
                    .map(|(tap, gain)| i.checked_sub(lag + tap).map_or(0.0, |j| gain * system[j]))
                    .sum()
            })
            .collect()
    }

    fn run(system: &[f32], mic: &[f32]) -> Vec<f32> {
        let mut gate = EchoGate::default();
        let mut output = Vec::new();
        let mut pending: Option<(Vec<f32>, Vec<f32>)> = None;
        for (remote, microphone) in system
            .as_chunks::<FRAME>()
            .0
            .iter()
            .zip(mic.as_chunks::<FRAME>().0.iter())
        {
            let mut frame: Vec<_> = microphone
                .iter()
                .zip(remote)
                .flat_map(|(&m, &s)| [m, s])
                .collect();
            let original = frame.clone();
            gate.process(&mut frame);
            if let Some((mut previous, raw)) = pending.replace((frame, original)) {
                if gate.protecting() {
                    for (pair, original) in previous
                        .as_chunks_mut::<2>()
                        .0
                        .iter_mut()
                        .zip(raw.as_chunks::<2>().0.iter())
                    {
                        pair[0] = original[0];
                    }
                }
                output.extend(previous.as_chunks::<2>().0.iter().map(|pair| pair[0]));
            }
        }
        if let Some((_, original)) = pending {
            output.extend(original.as_chunks::<2>().0.iter().map(|pair| pair[0]));
        }
        output
    }

    fn zeroed_frames(output: &[f32], reference: &[f32], start: usize, end: usize) -> f32 {
        let (mut zeroed, mut total) = (0, 0);
        for (actual, expected) in output[start..end]
            .as_chunks::<FRAME>()
            .0
            .iter()
            .zip(reference[start..end].as_chunks::<FRAME>().0.iter())
        {
            if expected.iter().any(|&x| x != 0.0) {
                total += 1;
                zeroed += usize::from(actual.iter().all(|&x| x == 0.0));
            }
        }
        zeroed as f32 / total as f32
    }

    #[test]
    fn filtered_off_grid_echo_is_suppressed() {
        let system = signal(1);
        for lag in [0, 1, 7, 1_921, 3_999, 4_000] {
            let mic = echo(&system, lag);
            let output = run(&system, &mic);
            if lag == 0 {
                assert!(output[..FRAME].iter().all(|&sample| sample == 0.0));
            }
            let fraction = zeroed_frames(&output, &mic, 8_000, 24_000);
            assert!(fraction >= 0.9, "lag {lag}: {fraction}");
        }
    }

    #[test]
    fn quiet_owner_under_echo_survives_with_hangover() {
        let system = signal(1);
        let owner = signal(39);
        let mut mic = echo(&system, 1_921);
        for i in 8_000..16_000 {
            mic[i] += owner[i] * 0.25;
        }
        let output = run(&system, &mic);
        let kept = output[8_000..16_000]
            .as_chunks::<FRAME>()
            .0
            .iter()
            .zip(mic[8_000..16_000].as_chunks::<FRAME>().0.iter())
            .filter(|(actual, expected)| actual == expected)
            .count();
        assert!(kept >= 10, "kept {kept}/10 owner frames");
        let after = output[16_000..19_200]
            .iter()
            .zip(&mic[16_000..19_200])
            .filter(|(a, b)| a == b)
            .count();
        assert!(after >= 3_000, "after {after}/3200");
    }

    #[test]
    fn owner_after_remote_stops_and_silence_stay_untouched() {
        let mut system = signal(1);
        system[12_000..].fill(0.0);
        let owner = signal(39);
        let mut mic = echo(&system, 0);
        mic[12_000..16_000].copy_from_slice(&owner[12_000..16_000]);
        let output = run(&system, &mic);
        assert_eq!(&output[12_000..16_000], &mic[12_000..16_000]);
        assert_eq!(&output[16_000..], &mic[16_000..]);
    }
}
