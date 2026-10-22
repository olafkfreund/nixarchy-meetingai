//! Short, model-written titles for finished meetings and conversations.
//!
//! The subtitle in a list is far more useful when it says what the conversation
//! was about than when it repeats the opening question. Titles are generated
//! once, on the user's own chat model, and stored like any other title.

/// Longest title we keep, in characters. Shorter than this reads better in the
/// sidebar and still fits on one line.
pub const MAX_TITLE_CHARS: usize = 42;

/// Instruction sent to the model. Kept terse so the answer is cheap.
pub const TITLE_INSTRUCTION: &str = "Write a title for the conversation below. Reply with the title only, no quotes, no punctuation at the end, and no prefix like \"Title:\". Use 2 to 6 plain words that say what the conversation was actually about. Prefer the concrete topic over a generic label, and never invent facts that are not in the text.";

/// Build the prompt for a title request.
pub fn title_prompt(body: &str) -> String {
    format!(
        "{TITLE_INSTRUCTION}\n\n<conversation>\n{}\n</conversation>",
        clip(body, 6000)
    )
}

/// Clean a model reply into a title, or `None` when nothing usable came back.
pub fn clean_title(reply: &str) -> Option<String> {
    // Models like to wrap the answer in quotes or a "Title:" label anyway.
    let line = reply.lines().map(str::trim).find(|line| !line.is_empty())?;
    let line = line
        .trim_start_matches(|c: char| c == '#' || c == '*' || c.is_whitespace())
        .trim_end_matches('*');
    let line = line
        .strip_prefix("Title:")
        .or_else(|| line.strip_prefix("title:"))
        .unwrap_or(line)
        .trim();
    let line = line
        .trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
        .trim()
        .trim_end_matches('.')
        .trim();
    if line.is_empty() || line.eq_ignore_ascii_case("new conversation") {
        return None;
    }
    let mut title: String = line.chars().take(MAX_TITLE_CHARS).collect();
    if line.chars().count() > MAX_TITLE_CHARS {
        // Cut on a word boundary so the title does not end mid-word.
        if let Some(space) = title.rfind(' ')
            && space > MAX_TITLE_CHARS / 2
        {
            title.truncate(space);
        }
        title.push('…');
    }
    Some(title.trim().to_owned())
}

/// Keep the transcript small enough to stay cheap, around its middle.
fn clip(text: &str, limit: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= limit {
        return text.to_owned();
    }
    let half = limit / 2;
    let head: String = chars[..half].iter().collect();
    let tail: String = chars[chars.len() - half..].iter().collect();
    format!("{head}\n...\n{tail}")
}

/// Ask `config` for one title.
pub async fn request_title(
    client: &reqwest::Client,
    config: &crate::ai::providers::ProviderConfig,
    body: &str,
) -> Option<String> {
    let context = crate::ai::engine::TurnContext {
        system: Some(TITLE_INSTRUCTION.to_owned()),
        images: Vec::new(),
    };
    let history = vec![crate::core::domain::Message::user(title_prompt(body))];
    let cancel = std::sync::Arc::new(tokio::sync::watch::channel(false).1);
    let reply = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        crate::ai::engine::stream_chat(client, config, &history, &context, cancel, |_| {}),
    )
    .await
    .ok()?
    .ok()?;
    clean_title(&reply)
}

/// Name every finished meeting that still carries its timestamp title.
///
/// Runs in the background when the meetings list opens, so sessions recorded
/// before titles were generated end up readable too. Returns how many changed.
pub async fn name_untitled_meetings(
    database: &std::sync::Arc<crate::db::Database>,
    config: crate::ai::providers::ProviderConfig,
) -> usize {
    let client = crate::ai::engine::http_client();
    let pending = {
        let connection = database.connection();
        crate::db::meetings::untitled(connection, 8).unwrap_or_default()
    };
    let mut renamed = 0;
    for (id, body) in pending {
        if let Some(title) = request_title(&client, &config, &body).await {
            let connection = database.connection();
            if crate::db::meetings::rename(connection, &id, &title).is_ok() {
                renamed += 1;
            }
        }
    }
    renamed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_lose_labels_quotes_and_trailing_periods() {
        assert_eq!(
            clean_title("Title: \"Migrating to Postgres.\"").as_deref(),
            Some("Migrating to Postgres")
        );
        assert_eq!(
            clean_title("**Kubernetes rollout**").as_deref(),
            Some("Kubernetes rollout")
        );
        assert_eq!(
            clean_title("  Budget planning  ").as_deref(),
            Some("Budget planning")
        );
    }

    #[test]
    fn unusable_replies_yield_no_title() {
        assert_eq!(clean_title(""), None);
        assert_eq!(clean_title("   \n \n"), None);
        assert_eq!(clean_title("New conversation"), None);
        assert_eq!(clean_title("\"\""), None);
    }

    #[test]
    fn long_titles_cut_on_a_word_boundary() {
        let reply = "Designing a resilient payment retry pipeline for the checkout flow";
        let title = clean_title(reply).unwrap();
        assert!(title.chars().count() <= MAX_TITLE_CHARS + 1);
        assert!(title.ends_with('…'));
        assert!(!title.ends_with(" …"));
        assert!(title.starts_with("Designing a resilient"));
    }

    #[test]
    fn prompt_clips_a_long_transcript_around_its_middle() {
        let body = format!("{}{}", "a".repeat(5000), "z".repeat(5000));
        let prompt = title_prompt(&body);
        assert!(prompt.contains("..."));
        assert!(prompt.contains(&"a".repeat(100)));
        assert!(prompt.contains(&"z".repeat(100)));
        assert!(prompt.chars().count() < 7000);
    }
}
