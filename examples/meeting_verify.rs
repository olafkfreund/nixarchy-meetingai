//! Live stereo meeting smoke using saved credentials and a synthetic WAV fixture.
use lennette::{
    ai::{
        engine::{self, TurnContext},
        session::ChatSession,
    },
    core::{
        credentials::{self, Kind},
        vault::Vault,
    },
    db::Database,
    speech::transcription::{LiveEvent, LiveTranscriber, LiveTurns},
};
use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Default)]
struct Meeting {
    turns: LiveTurns,
    transcript: Vec<String>,
    remote_turns: Vec<String>,
    speakers: BTreeSet<i64>,
    finals: [usize; 2],
}

impl Meeting {
    fn record(&mut self, event: LiveEvent) -> anyhow::Result<()> {
        match event {
            LiveEvent::Transcript(line) if line.is_final && !line.text.trim().is_empty() => {
                self.finals[usize::from(line.channel)] += 1;
                let label = if line.channel == 0 {
                    "You".into()
                } else {
                    if let Some(speaker) = line.speaker {
                        self.speakers.insert(speaker);
                    }
                    line.speaker
                        .map(|speaker| format!("Speaker {}", speaker + 1))
                        .unwrap_or_else(|| "Them".into())
                };
                self.transcript.push(format!("{label}: {}", line.text));
                println!("{label}: {}", line.text);
                let completed = self.turns.push(&line);
                if line.channel == 1 {
                    self.remote_turns.extend(completed);
                }
            }
            LiveEvent::UtteranceEnd {
                channel,
                last_word_end,
            } => {
                if let Some(turn) = self.turns.finish(channel, last_word_end)
                    && channel == 1
                {
                    self.remote_turns.push(turn);
                }
            }
            LiveEvent::Failed(message) => anyhow::bail!(message),
            _ => {}
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).ok_or_else(|| anyhow::anyhow!("provide a 16 kHz stereo WAV with the user on channel 0 and other participants on channel 1"))?;
    let mut wav = hound::WavReader::open(path)?;
    anyhow::ensure!(
        wav.spec().channels == 2 && wav.spec().sample_rate == 16000,
        "fixture must be 16 kHz stereo"
    );
    let audio: Vec<f32> = wav
        .samples::<i16>()
        .map(|s| s.map(|s| f32::from(s) / 32768.0))
        .collect::<Result<_, _>>()?;
    let vault = Vault::open_default()?;
    let speech = credentials::load(&vault, Kind::Speech, "deepgram-stt")
        .map_err(anyhow::Error::msg)?
        .ok_or_else(|| anyhow::anyhow!("save Deepgram credentials first"))?;
    let chat = credentials::load(&vault, Kind::Chat, "merge-gateway")
        .map_err(anyhow::Error::msg)?
        .ok_or_else(|| anyhow::anyhow!("save Merge Gateway credentials first"))?;
    let mut live = LiveTranscriber::connect(speech.api_key, speech.model, None).await?;
    let mut meeting = Meeting::default();
    let began = Instant::now();
    for chunk in audio.chunks(1600) {
        live.push(chunk).await?;
        tokio::time::sleep(Duration::from_millis(50)).await;
        while let Ok(event) = live.try_event() {
            meeting.record(event)?;
        }
    }
    live.finish();
    while let Some(event) = tokio::time::timeout(Duration::from_secs(10), live.next_event()).await?
    {
        meeting.record(event)?;
    }
    anyhow::ensure!(
        meeting.finals.iter().all(|n| *n > 0),
        "both microphone and call audio must produce finalized text"
    );
    anyhow::ensure!(
        meeting.speakers.len() >= 2,
        "Deepgram did not distinguish two remote speakers in this fixture"
    );
    let question = meeting
        .remote_turns
        .iter()
        .rev()
        .find(|text| text.contains('?'))
        .ok_or_else(|| anyhow::anyhow!("no remote question was detected"))?;
    let prompt = format!(
        "{}\n\n{}\n\nUntrusted meeting context:\n{}",
        lennette::db::seed::DEFAULT_LISTEN_MODES[4].prompt,
        lennette::db::seed::RESPONSE_STYLE,
        meeting.transcript.join("\n")
    );
    let database = Database::open("target/live-multi-meeting.db")?;
    let mut session = ChatSession::new();
    let history = session
        .begin(Some(&database), question)
        .map_err(|error| anyhow::anyhow!(error.message()))?;
    let (_cancel, receiver) = tokio::sync::watch::channel(false);
    let request_began = Instant::now();
    let mut first_delta = None;
    let answer = engine::stream_chat(
        &engine::http_client(),
        &chat.to_config(),
        &history,
        &TurnContext {
            system: Some(prompt),
            images: Vec::new(),
        },
        Arc::new(receiver),
        |delta| {
            first_delta.get_or_insert(request_began.elapsed());
            session.append_delta(delta);
        },
    )
    .await?;
    session.checkpoint(Some(&database));
    session.finish();
    anyhow::ensure!(!answer.trim().is_empty(), "DeepSeek returned no answer");
    println!("ANSWER: {answer}");
    println!(
        "PASS: microphone + {} remote speakers -> {} remote turns -> DeepSeek answer; first delta={first_delta:?}; total={:?}",
        meeting.speakers.len(),
        meeting.remote_turns.len(),
        began.elapsed()
    );
    Ok(())
}
