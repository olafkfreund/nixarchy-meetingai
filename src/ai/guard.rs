//! Interviewer mode must report evidence, never a verdict: reject model output that scores,
//! ranks or recommends, and retry once with a reminder.

use std::future::Future;

/// Error shown when the retry also contained an assessment.
pub const WITHHELD: &str = "The answer contained an assessment and was withheld.";

/// Appended to the prompt for the one retry.
pub const REMINDER: &str = "Your previous answer contained an assessment of the candidate. Report only what was said and what is unverified: no score, rating, ranking, hiring recommendation, or inference about emotion, personality, confidence or honesty.";

/// Whole words and phrases (compared case-insensitively on word boundaries).
const PATTERNS: &[&str] = &[
    "score",
    "scores",
    "rating",
    "ratings",
    "rank",
    "ranking",
    "hire",
    "no hire",
    "recommend hiring",
    "recommend against",
    "strong candidate",
    "weak candidate",
    "red flag",
    "culture fit",
    "good fit",
    "fit for",
    "nervous",
    "anxious",
    "confident",
    "confidence",
    "honest",
    "dishonest",
    "trustworthy",
    "personality",
    "seemed anxious",
    "seemed unsure",
];

/// The pattern that makes `text` an assessment, if any. False positives ("hire date")
/// only cost one retry, so the list stays strict. Quoted speech is the candidate's own
/// words, not the model's judgement, so it is ignored.
pub fn assessment(text: &str) -> Option<&'static str> {
    let text = unquoted(text);
    if has_ratio(&text) {
        return Some("n/m");
    }
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let padded = format!(" {} ", words.join(" "));
    PATTERNS
        .iter()
        .find(|p| padded.contains(&format!(" {p} ")))
        .copied()
}

/// `text` without anything inside "…", “…” or '…'. An apostrophe inside a word is not a quote,
/// and a quote that never closes is kept (it must not hide what follows it).
fn unquoted(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let close = match chars[i] {
            '"' => Some('"'),
            '“' => Some('”'),
            '\'' if i == 0 || !chars[i - 1].is_alphanumeric() => Some('\''),
            _ => None,
        };
        let end = close.and_then(|q| {
            (i + 1..chars.len()).find(|&j| {
                chars[j] == q
                    && (q != '\'' || !chars.get(j + 1).is_some_and(|c| c.is_alphanumeric()))
            })
        });
        match end {
            Some(j) => {
                out.push(' ');
                i = j + 1;
            }
            None => {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    out
}

/// `\d+\s*(/|out of)\s*\d+`
fn has_ratio(text: &str) -> bool {
    let t = text.to_lowercase();
    ["/", "out of"].into_iter().any(|sep| {
        t.match_indices(sep).any(|(i, _)| {
            t[..i].trim_end().ends_with(|c: char| c.is_ascii_digit())
                && t[i + sep.len()..]
                    .trim_start()
                    .starts_with(|c: char| c.is_ascii_digit())
        })
    })
}

/// Run `call` (given extra prompt text, empty on the first try); on an assessment run it
/// once more with [`REMINDER`], and withhold the answer if that one also fails.
pub async fn with_retry<F, Fut>(mut call: F) -> Result<String, String>
where
    F: FnMut(&'static str) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    for extra in ["", REMINDER] {
        let text = call(extra).await?;
        if assessment(&text).is_none() {
            return Ok(text);
        }
    }
    Err(WITHHELD.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_verdicts_and_passes_plain_probes() {
        for hit in [
            "7/10",
            "She gets 8 / 10 overall",
            "I'd hire her.",
            "They seemed nervous.",
            "A Strong Candidate",
            "Recommend against.",
            "Clear red flag here",
            "4 out of 5 on rust",
            "She lacks confidence",
            "Seemed anxious about it",
            "A good fit for the team",
            "Not trustworthy",
            "He said \"fine\" but she is a strong candidate",
            "It's a 'maybe' and I'd hire her",
            "An unclosed \"quote then I'd hire her",
        ] {
            assert!(assessment(hit).is_some(), "{hit}");
        }
        for miss in [
            "What was your part in the migration?",
            "\"We moved 10 services\" - which ones?",
            "Tell me about the failure rate.",
            "She said \"I got a 9/10 rating\" in the story, so ask about that",
            "She said \u{201c}I was nervous and 7/10 confident\u{201d}, which part?",
            "Quoted: 'I was anxious that day' - what changed?",
            "Don't assume; it's the candidate's call",
            "Their hire date was in May",
        ] {
            // "hire date" is a known cheap false positive, asserted separately below.
            if !miss.contains("hire") {
                assert!(assessment(miss).is_none(), "{miss}");
            }
        }
        assert_eq!(assessment("Their hire date"), Some("hire"));
    }

    #[tokio::test]
    async fn retries_once_then_withholds() {
        let mut calls = Vec::new();
        let ok = with_retry(|extra| {
            calls.push(extra);
            let reply = if extra.is_empty() {
                "She is a strong candidate"
            } else {
                "Ask about the migration"
            };
            async move { Ok(reply.to_owned()) }
        })
        .await;
        assert_eq!(ok.unwrap(), "Ask about the migration");
        assert_eq!(calls, ["", REMINDER]);

        let n = std::cell::Cell::new(0);
        let err = with_retry(|_| {
            n.set(n.get() + 1);
            async { Ok("7/10".to_owned()) }
        })
        .await;
        assert_eq!((err.unwrap_err().as_str(), n.get()), (WITHHELD, 2));
    }
}
