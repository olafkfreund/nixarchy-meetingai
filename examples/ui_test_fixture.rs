//! Seed an isolated UI smoke-test data directory with a local fake provider.
use lennette::{
    core::{
        credentials::{self, Credential, Kind},
        vault::Vault,
    },
    db::Database,
};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let directory = std::path::PathBuf::from(args.next().expect("test data directory"));
    let endpoint = args.next().expect("local fake provider endpoint");
    anyhow::ensure!(
        endpoint.starts_with("http://127.0.0.1:"),
        "only local test endpoints are supported"
    );
    std::fs::create_dir_all(&directory)?;
    let vault = Vault::open(&directory)?;
    let mut credential = Credential::chat("gemini", "ui-test-placeholder");
    credential.model = "ui-test-model".into();
    credential.base_url = endpoint;
    credentials::save(&vault, &credential).map_err(anyhow::Error::msg)?;
    match args.next().as_deref() {
        Some("--live-speech") => {
            let source = Vault::open_default()?;
            let speech = credentials::load(&source, Kind::Speech, "deepgram-stt")
                .map_err(anyhow::Error::msg)?
                .ok_or_else(|| anyhow::anyhow!("no saved speech credential"))?;
            credentials::save(&vault, &speech).map_err(anyhow::Error::msg)?;
        }
        Some("--clear-speech") => {
            credentials::remove(&vault, Kind::Speech, "deepgram-stt")
                .map_err(anyhow::Error::msg)?;
        }
        _ => {}
    }
    Database::open(directory.join("lennette.db"))?;
    Ok(())
}
