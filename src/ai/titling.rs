use std::sync::Arc;
use tokio::sync::watch;

const TITLE_INSTRUCTION: &str = "Give this conversation a title of two to six plain words. Name its concrete topic using only the supplied text. Reply with the title alone: no label, quotes, or closing punctuation.";

fn title_prompt(body: &str) -> String {
    let shortened = if body.chars().count() > 6_000 {
        let start: String = body.chars().take(3_000).collect();
        let end: String = body
            .chars()
            .rev()
            .take(3_000)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("{start}\n[transcript elided]\n{end}")
    } else {
        body.to_owned()
    };
    format!(
        "Follow the title instruction for the transcript below.\n<transcript>\n{shortened}\n</transcript>"
    )
}

fn clean_title(reply: &str) -> Option<String> {
    let first = reply.lines().find(|line| !line.trim().is_empty())?;
    let mut title = first
        .trim()
        .trim_start_matches(|c: char| c == '#' || c == '*' || c.is_whitespace())
        .trim_end_matches('*')
        .trim();
    if title
        .get(..6)
        .is_some_and(|label| label.eq_ignore_ascii_case("title:"))
    {
        title = title[6..].trim();
    }
    title = title.trim_start_matches(['"', '\'', '`']).trim();
    title = title.trim_end_matches(['"', '\'', '`', '.', '*']).trim();
    if title.is_empty() || title.eq_ignore_ascii_case("new conversation") {
        return None;
    }
    if title.chars().count() <= 42 {
        return Some(title.into());
    }
    let first: String = title.chars().take(42).collect();
    let cut = first
        .rfind(' ')
        .filter(|at| first[..*at].chars().count() > 21);
    Some(format!(
        "{}…",
        cut.map_or(first.as_str(), |at| &first[..at]).trim_end()
    ))
}

pub async fn request_title(
    client: &reqwest::Client,
    config: &crate::ai::providers::ProviderConfig,
    body: &str,
) -> Option<String> {
    let (_tx, rx) = watch::channel(false);
    let context = crate::ai::engine::TurnContext {
        system: Some(TITLE_INSTRUCTION.into()),
        images: Vec::new(),
    };
    let messages = [crate::core::domain::Message::user(title_prompt(body))];
    let reply = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        crate::ai::engine::stream_chat(client, config, &messages, &context, Arc::new(rx), |_| {}),
    )
    .await
    .ok()?
    .ok()?;
    clean_title(&reply)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_cleanup() {
        assert_eq!(
            clean_title("  ## *Title: `Quarterly planning`.*  \nignored"),
            Some("Quarterly planning".into())
        );
        for reply in ["", "   ", "NEW CONVERSATION", "\"\"", "`'"] {
            assert!(clean_title(reply).is_none());
        }
        let result = clean_title(
            "An extended conversation concerning database migrations and deployment plans",
        )
        .unwrap();
        assert!(result.chars().count() <= 43 && result.ends_with('…') && !result.ends_with(" …"));
    }

    #[test]
    fn transcript_keeps_both_ends() {
        let body = format!("BEGIN{}END", "a".repeat(8_000));
        let prompt = title_prompt(&body);
        assert!(
            prompt.contains("BEGIN")
                && prompt.contains("END")
                && prompt.contains("[transcript elided]")
        );
        assert!(prompt.len() < 7_000);
    }
}
