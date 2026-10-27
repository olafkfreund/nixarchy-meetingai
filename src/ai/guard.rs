//! Interviewer mode must report evidence, never a verdict: reject model output that scores,
//! ranks or recommends, and retry once with a reminder.

use std::future::Future;

/// Error shown when the retry also contained an assessment.
pub const WITHHELD: &str = "The answer contained an assessment and was withheld.";

/// Appended to the prompt for the one retry.
pub const REMINDER: &str = "Your previous answer contained an assessment of the candidate. Report only what was said and what is unverified: no score, rating, ranking, hiring recommendation, or inference about emotion, personality, confidence or honesty.";

/// Whole words and phrases (compared case-insensitively on word boundaries).
const PATTERNS: [&str; 18] = [
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
    "nervous",
    "confident",
    "honest",
    "dishonest",
];
/// The same, kept apart only to stay under clippy's line width.
const MORE: [&str; 3] = ["personality", "seemed anxious", "seemed unsure"];

/// The pattern that makes `text` an assessment, if any. False positives ("hire date")
/// only cost one retry, so the list stays strict.
pub fn assessment(text: &str) -> Option<&'static str> {
    if has_out_of_ten(text) {
        return Some("n/10");
    }
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let padded = format!(" {} ", words.join(" "));
    PATTERNS
        .into_iter()
        .chain(MORE)
        .find(|p| padded.contains(&format!(" {p} ")))
}

/// `\d+\s*/\s*10` not followed by another digit.
fn has_out_of_ten(text: &str) -> bool {
    let b = text.as_bytes();
    (0..b.len()).filter(|&i| b[i] == b'/').any(|i| {
        let before = text[..i].trim_end().as_bytes().last();
        let after = text[i + 1..].trim_start();
        before.is_some_and(u8::is_ascii_digit)
            && after.starts_with("10")
            && !after[2..].starts_with(|c: char| c.is_ascii_digit())
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
        ] {
            assert!(assessment(hit).is_some(), "{hit}");
        }
        for miss in [
            "What was your part in the migration?",
            "\"We moved 10 services\" - which ones?",
            "Tell me about the 1/100 failure rate.",
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
