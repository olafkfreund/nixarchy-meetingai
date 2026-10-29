use crate::audio::echo_gate::EchoGate;
impl ListenSession { // ours stub
    fn queue_audio(
        gate: &mut Option<EchoGate>,
        pending: &mut Option<(Vec<f32>, Vec<f32>)>,
        mut samples: Vec<f32>,
    ) -> Option<Vec<f32>> {
        let Some(gate) = gate else {
            return Some(samples);
        };
        let original = samples.clone();
        gate.process(&mut samples);
        let previous = pending.replace((samples, original));
        previous.map(|(mut gated, raw)| {
            if gate.protecting() {
                for (pair, original) in gated
                    .as_chunks_mut::<2>()
                    .0
                    .iter_mut()
                    .zip(raw.as_chunks::<2>().0.iter())
                {
                    pair[0] = original[0];
                }
            }
            gated
        })
    }

    fn flush_audio(&mut self) -> Option<SessionEvent> {
        self.pending_audio
            .take()
            .map(|(_, original)| SessionEvent::Audio(original))
    } }
