//! Conservative, frame-level suppression of system audio leaking into the mic.

pub(crate) const FRAME: usize = 320; // 20 ms at 16 kHz
const BIN: usize = 16; // 1 ms envelope bins
const MAX_LAG: usize = 4_000; // 250 ms
const HISTORY: usize = FRAME + MAX_LAG;

/// Uses fixed storage so processing a captured frame never allocates.
pub struct EchoGate {
    system: [f32; HISTORY],
    filled: usize,
    last_lag: Option<usize>,
}

impl Default for EchoGate {
    fn default() -> Self {
        Self {
            system: [0.0; HISTORY],
            filled: 0,
            last_lag: None,
        }
    }
}

impl EchoGate {
    pub fn reset(&mut self) {
        self.filled = 0;
        self.last_lag = None;
    }

    /// `samples` is one interleaved 20 ms frame, mic left and system right.
    pub fn process(&mut self, samples: &mut [f32]) {
        if samples.len() != FRAME * 2 {
            return;
        }
        self.system.copy_within(FRAME.., 0);
        for (i, pair) in samples.as_chunks::<2>().0.iter().enumerate() {
            self.system[HISTORY - FRAME + i] = pair[1];
        }
        self.filled = (self.filled + FRAME).min(HISTORY);

        let mut mic = [0.0_f32; FRAME / BIN];
        for (i, pair) in samples.as_chunks::<2>().0.iter().enumerate() {
            mic[i / BIN] += pair[0].abs() / BIN as f32;
        }
        let mic_power: f32 = mic.iter().map(|v| v * v).sum();
        if mic_power <= 0.0 {
            self.last_lag = None;
            return;
        }

        for lag in (0..=MAX_LAG).step_by(BIN) {
            if self.filled < FRAME + lag {
                break;
            }
            let mut system_sum = 0.0;
            let mut system_power = 0.0;
            let mut mic_sum = 0.0;
            let mut cross = 0.0;
            for (bin, &y) in mic.iter().enumerate() {
                let start = HISTORY - FRAME - lag + bin * BIN;
                let x = self.system[start..start + BIN]
                    .iter()
                    .map(|v| v.abs())
                    .sum::<f32>()
                    / BIN as f32;
                system_sum += x;
                system_power += x * x;
                mic_sum += y;
                cross += x * y;
            }
            if system_power <= 0.0 || mic_power > 0.25 * system_power {
                continue;
            }
            let covariance = cross - system_sum * mic_sum / mic.len() as f32;
            let system_variance = system_power - system_sum * system_sum / mic.len() as f32;
            let mic_variance = mic_power - mic_sum * mic_sum / mic.len() as f32;
            // shortcut: conservative thresholds are provisional, tune on the owner's evaluation set before enabling.
            // Both shape and residual must agree: extra mic speech raises the residual.
            if covariance > 0.0
                && covariance * covariance > 0.9_f32.powi(2) * system_variance * mic_variance
                && (mic_power - cross * cross / system_power) < 0.08 * mic_power
            {
                let mut raw_system_power = 0.0;
                let mut raw_mic_power = 0.0;
                let mut raw_cross = 0.0;
                for (i, pair) in samples.as_chunks::<2>().0.iter().enumerate() {
                    let x = self.system[HISTORY - FRAME - lag + i];
                    raw_system_power += x * x;
                    raw_mic_power += pair[0] * pair[0];
                    raw_cross += x * pair[0];
                }
                if raw_system_power > 0.0
                    && raw_mic_power - raw_cross * raw_cross / raw_system_power
                        < 0.08 * raw_mic_power
                {
                    // Require a stable delay across frames before silencing speech.
                    if self.last_lag == Some(lag) {
                        for pair in samples.as_chunks_mut::<2>().0 {
                            pair[0] = 0.0;
                        }
                    }
                    self.last_lag = Some(lag);
                    return;
                }
            }
        }
        self.last_lag = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signal() -> Vec<f32> {
        let mut state = 1_u32;
        (0..16_000)
            .map(|i| {
                if i % BIN == 0 {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                }
                let amplitude = 0.1 + (state % 800) as f32 / 1_000.0;
                if i % 2 == 0 { amplitude } else { -amplitude }
            })
            .collect()
    }

    fn run(system: &[f32], microphone: &[f32]) -> Vec<f32> {
        let mut gate = EchoGate::default();
        let mut output = Vec::new();
        for (remote, mic) in system
            .as_chunks::<FRAME>()
            .0
            .iter()
            .zip(microphone.as_chunks::<FRAME>().0)
        {
            let mut frame: Vec<_> = mic.iter().zip(remote).flat_map(|(&m, &s)| [m, s]).collect();
            gate.process(&mut frame);
            output.extend(frame.as_chunks::<2>().0.iter().map(|pair| pair[0]));
        }
        output
    }

    #[test]
    fn pure_echo_is_removed_at_zero_middle_and_maximum_lag() {
        let system = signal();
        for lag in [0, 1_920, MAX_LAG] {
            let microphone: Vec<_> = (0..system.len())
                .map(|i| i.checked_sub(lag).map_or(0.0, |j| 0.251 * system[j]))
                .collect();
            let output = run(&system, &microphone);
            assert!(
                output[8_000..].iter().all(|&sample| sample == 0.0),
                "lag {lag}"
            );
        }
    }

    #[test]
    fn independent_microphone_speech_survives_echo() {
        let system = signal();
        let independent = signal().into_iter().rev().collect::<Vec<_>>();
        let microphone: Vec<_> = system
            .iter()
            .zip(&independent)
            .map(|(&s, &m)| 0.251 * s + 0.12 * m)
            .collect();
        assert!(
            run(&system, &microphone)
                .iter()
                .zip(&microphone)
                .all(|(a, b)| a == b)
        );
    }

    #[test]
    fn owner_speech_is_kept_when_it_starts_during_echo() {
        let system = signal();
        let independent = signal().into_iter().rev().collect::<Vec<_>>();
        let microphone: Vec<_> = system
            .iter()
            .enumerate()
            .map(|(i, &s)| {
                0.251 * s
                    + if i >= 8_000 {
                        0.12 * independent[i]
                    } else {
                        0.0
                    }
            })
            .collect();
        let output = run(&system, &microphone);
        assert!(
            output[8_000..]
                .iter()
                .zip(&microphone[8_000..])
                .all(|(a, b)| a == b)
        );
    }

    #[test]
    fn silence_is_unchanged() {
        assert_eq!(run(&[0.0; 640], &[0.0; 640]), [0.0; 640]);
    }
}
