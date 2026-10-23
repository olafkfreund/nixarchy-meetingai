//! Manual live smoke: keys are read from a private file, never printed.
use lennette::{
    ai::{
        engine::{self, TurnContext},
        models,
        session::ChatSession,
    },
    core::credentials::{self, Credential, Kind},
    core::vault::Vault,
    db::{Database, conversations, meetings},
    speech::transcription,
};
use std::{path::Path, sync::Arc};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    // Read-only catalog verification: reuse existing encrypted credentials and
    // print counts only. No key replacement, chat generation, or audio upload.
    if args.get(1).map(String::as_str) == Some("--models") {
        let vault = Vault::open_default()?;
        let credentials = credentials::list(&vault, Kind::Chat).map_err(anyhow::Error::msg)?;
        anyhow::ensure!(
            !credentials.is_empty(),
            "No saved chat credentials are available"
        );
        for credential in credentials {
            let config = credential.to_config();
            let models = models::discover(None, config, true)
                .await
                .map_err(anyhow::Error::msg)?;
            println!(
                "{}: fetched {} chat models using its saved key",
                credential.provider,
                models.len()
            );
        }
        return Ok(());
    }
    let secrets: serde_json::Value = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    let vault = Vault::open_default()?;
    let mut chat = Credential::chat("merge-gateway", secrets["merge"].as_str().unwrap());
    chat.model = models::MERGE_MODEL.into();
    let mut speech = Credential::speech("deepgram-stt", secrets["deepgram"].as_str().unwrap());
    speech.model = "nova-3".into();
    credentials::save(&vault, &chat).map_err(anyhow::Error::msg)?;
    credentials::save(&vault, &speech).map_err(anyhow::Error::msg)?;
    println!("Credentials saved encrypted; model={}", models::MERGE_MODEL);
    if args.get(2).is_none() {
        return Ok(());
    }
    let wav = std::fs::read(&args[2])?;
    let text = match transcription::transcribe(&speech.to_config(), wav, Some("en")).await {
        Ok(text) => text,
        Err(error) => {
            eprintln!("Deepgram live test failed: {error}");
            return Err(anyhow::anyhow!("Deepgram request failed"));
        }
    };
    println!("Deepgram transcript: {text}");
    let path = Path::new("target/live-provider-flow.db");
    let db = Database::open(path)?;
    let meeting = meetings::start(db.connection(), "Live provider smoke", "meeting", "en")?;
    meetings::add_line(db.connection(), &meeting, "them", None, &text, 0.0, 6.0)?;
    let mut session = ChatSession::new();
    let history = session
        .begin(Some(&db), &text)
        .map_err(|e| anyhow::anyhow!("{}", e.message()))?;
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let started = std::time::Instant::now();
    let answer = engine::stream_chat(&engine::http_client(), &chat.to_config(), &history,
        &TurnContext { system: Some("You are a meeting copilot. Suggest one concise answer the user can say aloud. Do not invent experience.".into()), images: vec![] },
        Arc::new(rx), |delta| session.push_delta(Some(&db), delta)).await?;
    session.finish();
    meetings::finish(db.connection(), &meeting, 6)?;
    let stored = conversations::messages(db.connection(), session.conversation_id().unwrap())?;
    assert_eq!(stored.last().unwrap().content, answer);
    assert!(meetings::list(db.connection(), 1)?[0].ended_at.is_some());
    println!(
        "Merge streamed answer ({:.1}s): {}",
        started.elapsed().as_secs_f32(),
        answer
    );
    println!(
        "PASS: WAV -> Deepgram -> DeepSeek streamed answer -> saved conversation and finished meeting"
    );
    // Readiness check reports IDs only, never credential contents.
    println!(
        "Active chat services: {:?}",
        credentials::list(&vault, Kind::Chat)
            .map_err(anyhow::Error::msg)?
            .iter()
            .map(|c| &c.provider)
            .collect::<Vec<_>>()
    );
    Ok(())
}
