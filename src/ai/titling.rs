use std::sync::Arc;
use tokio::sync::watch;

const TITLE_INSTRUCTION: &str = "Give this conversation a title of two to six plain words. Name its concrete topic using only the supplied text. Reply with the title alone: no label, quotes, or closing punctuation.";

fn title_prompt(body: &str) -> String {
    let total = body.chars().count();
    let mut prompt = String::with_capacity(TITLE_INSTRUCTION.len() + body.len().min(7_000) + 64);
    prompt.push_str(TITLE_INSTRUCTION);
    prompt.push_str("\n\n<transcript>\n");
    if total > 6_000 {
        let opening_end = body
            .char_indices()
            .nth(3_000)
            .map_or(body.len(), |(i, _)| i);
        let closing_start = body
            .char_indices()
            .nth(total - 3_000)
            .map_or(body.len(), |(i, _)| i);
        prompt.push_str(&body[..opening_end]);
        prompt.push_str("\n[transcript elided]\n");
        prompt.push_str(&body[closing_start..]);
    } else {
        prompt.push_str(body);
    }
    prompt.push_str("\n</transcript>");
    prompt
}

/// Removes markdown decoration: leading `#`/`*`/whitespace, trailing `*`.
fn strip_markdown(line: &str) -> &str {
    let mut start = line.len();
    for (at, ch) in line.char_indices() {
        if !(ch == '#' || ch == '*' || ch.is_whitespace()) {
            start = at;
            break;
        }
    }
    let mut end = start;
    for (at, ch) in line[start..].char_indices() {
        if ch != '*' {
            end = start + at + ch.len_utf8();
        }
    }
    &line[start..end]
}

fn is_placeholder(title: &str) -> bool {
    title
        .chars()
        .map(|c| c.to_ascii_lowercase())
        .eq("new conversation".chars())
}

fn clean_title(reply: &str) -> Option<String> {
    let line = reply.lines().find(|l| !l.trim().is_empty())?;
    let mut title = strip_markdown(line.trim()).trim();
    for label in ["Title:", "title:"] {
        if let Some(rest) = title.strip_prefix(label) {
            title = rest.trim();
            break;
        }
    }
    let is_quote = |c: char| matches!(c, '"' | '\'' | '`');
    title = title.trim_matches(is_quote).trim();
    title = title.trim_end_matches('.').trim();
    // a closing quote can sit behind the full stop, as in `"Plan".`
    title = title.trim_matches(is_quote).trim();
    if title.is_empty() || is_placeholder(title) {
        return None;
    }
    if title.chars().count() <= 42 {
        return Some(title.to_owned());
    }
    let cut_at = title.char_indices().nth(42).map_or(title.len(), |(i, _)| i);
    let head = &title[..cut_at];
    let kept = match head.rfind(' ') {
        Some(sp) if head[..sp].chars().count() > 21 => &head[..sp],
        _ => head,
    };
    Some(format!("{}…", kept.trim_end()))
}

fn title_context() -> crate::ai::engine::TurnContext {
    crate::ai::engine::TurnContext {
        system: Some(TITLE_INSTRUCTION.to_owned()),
        images: Vec::new(),
    }
}

pub async fn request_title(
    client: &reqwest::Client,
    config: &crate::ai::providers::ProviderConfig,
    body: &str,
) -> Option<String> {
    let (_keep_open, rx) = watch::channel(false);
    let context = title_context();
    let messages = [crate::core::domain::Message::user(title_prompt(body))];
    let call =
        crate::ai::engine::stream_chat(client, config, &messages, &context, Arc::new(rx), |_| {});
    let outcome = tokio::select! {
        r = call => r,
        () = tokio::time::sleep(std::time::Duration::from_secs(20)) => return None,
    };
    clean_title(&outcome.ok()?)
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
        let body = format!("BEGIN{}END", "x".repeat(7_500));
        let prompt = title_prompt(&body);
        assert!(
            prompt.contains("BEGIN")
                && prompt.contains("END")
                && prompt.contains("[transcript elided]")
        );
        assert!(prompt.len() < 6_800);
    }
}
