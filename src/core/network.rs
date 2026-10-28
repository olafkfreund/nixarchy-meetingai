fn client(total: u64, read: u64) -> reqwest::Client {
        .read_timeout(Duration::from_secs(read))
    CLIENT.get_or_init(|| client(180, 30)).clone()
}
/// Loopback Ollama may spend minutes loading a cold model before its first byte.
pub(crate) fn loading_chat_client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| client(600, 300)).clone()
    CLIENT.get_or_init(|| client(120, 30)).clone()
