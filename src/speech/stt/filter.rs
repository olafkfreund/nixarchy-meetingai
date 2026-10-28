//! Quality filters for final batch-transcript lines: whisper hallucinations (T3),
//! microphone echo of the system channel (T4), and the whisper prompt (T2).

use super::transcription::TranscriptLine;

/// Whole-line phrases whisper invents on silence, normalized (see [`tokens`]).
const SILENCE_PHRASES: &[&str] = &[
    "thank you",
    "thanks for watching",
    "thank you for watching",
    "you",
    "blank audio",
    "silence",
    "music",
    "subtitles by the amara org community",
    "amen",
];
/// Exact repeats of short lines ("yes", "okay") are dropped within this many seconds.
const REPEAT_WINDOW: f64 = 10.0;
/// Longer lines that closely match one already said on the same channel are dropped within
/// this many seconds: whisper, fed its own last words as the prompt, keeps "hearing" them in
/// quiet audio ("...in that word. Amen." every few seconds).
const LOOP_WINDOW: f64 = 60.0;
const LOOP_SIMILARITY: f64 = 0.8;
const ECHO_WINDOW: f64 = 2.0;
const ECHO_SIMILARITY: f64 = 0.8;
/// shortcut: shorter mic lines ("yes") are kept even if the call said it too, upgrade with real AEC.
const ECHO_MIN_TOKENS: usize = 3;
const PROMPT_PART: usize = 200;

/// Lowercase alphanumeric words; punctuation, brackets and underscores split.
pub(crate) fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// A line that is one phrase said several times ("a b a b") counts as the phrase once.
fn collapse(words: &[String]) -> &[String] {
    (1..=words.len() / 2)
        .filter(|unit| words.len().is_multiple_of(*unit))
        .find(|&unit| words.chunks(unit).all(|c| c == &words[..unit]))
        .map_or(words, |unit| &words[..unit])
}

/// Share of the longer line's tokens that also occur in the other (multiset).
pub(crate) fn similarity(a: &[String], b: &[String]) -> f64 {
    let mut rest: Vec<&String> = b.iter().collect();
    let mut shared = 0usize;
    for word in a {
        if let Some(i) = rest.iter().position(|w| *w == word) {
            rest.swap_remove(i);
            shared += 1;
        }
    }
    shared as f64 / a.len().max(b.len()).max(1) as f64
}

/// Stateful filter over final lines of both channels, in arrival order.
#[derive(Default)]
pub struct LineFilter {
    /// Last accepted line per channel: text, end time.
    last: [Option<(String, f64)>; 2],
    /// Accepted lines of the last minute per channel: collapsed tokens, end time.
    said: [Vec<(Vec<String>, f64)>; 2],
    /// Recent system lines: start, end, tokens.
    system: Vec<(f64, f64, Vec<String>)>,
    /// Recent accepted text per channel, the whisper prompt context.
    recent: [String; 2],
}

impl LineFilter {
    /// True when the line should be kept.
    pub fn accept(&mut self, line: &TranscriptLine) -> bool {
        let words = tokens(&line.text);
        let text = line.text.trim();
        if words.is_empty() || SILENCE_PHRASES.contains(&words.join(" ").as_str()) {
            return false;
        }
        let channel = usize::from(line.channel.min(1));
        if let Some((previous, end)) = &self.last[channel]
            && previous == text
            && line.start - end <= REPEAT_WINDOW
        {
            return false;
        }
        // "Amen" is whisper's sign-off on silence; compare what is left.
        let spoken: Vec<String> = words.iter().filter(|w| *w != "amen").cloned().collect();
        let phrase = collapse(&spoken).to_vec();
        let said = &mut self.said[channel];
        said.retain(|(_, end)| line.start - end <= LOOP_WINDOW);
        if phrase.len() >= ECHO_MIN_TOKENS
            && said
                .iter()
                .any(|(other, _)| similarity(&phrase, other) >= LOOP_SIMILARITY)
        {
            return false;
        }
        if channel == 1 {
            self.system.retain(|s| s.1 > line.end - 30.0);
            self.system.push((line.start, line.end, words));
        } else if words.len() >= ECHO_MIN_TOKENS
            && self.system.iter().any(|(start, end, other)| {
                line.start - ECHO_WINDOW <= *end
                    && *start <= line.end + ECHO_WINDOW
                    && similarity(&words, other) >= ECHO_SIMILARITY
            })
        {
            return false;
        }
        self.said[channel].push((phrase, line.end));
        self.last[channel] = Some((text.to_owned(), line.end));
        let recent = &mut self.recent[channel];
        recent.push(' ');
        recent.push_str(text);
        let cut = recent.len().saturating_sub(4 * PROMPT_PART);
        let cut = (cut..=recent.len())
            .find(|i| recent.is_char_boundary(*i))
            .unwrap_or(0);
        recent.drain(..cut);
        true
    }

    /// Prompt for the next request on `channel`.
    pub fn prompt(&self, channel: u8, glossary: &str) -> String {
        whisper_prompt(glossary, &self.recent[usize::from(channel.min(1))])
    }
}

fn tail(text: &str, max: usize) -> &str {
    let skip = text.chars().count().saturating_sub(max);
    let text = &text[text.char_indices().nth(skip).map_or(text.len(), |(i, _)| i)..];
    // Start on a word boundary unless the text already started there.
    if skip > 0 {
        text.split_once(' ').map_or(text, |(_, rest)| rest)
    } else {
        text
    }
}

/// Whisper `prompt`: glossary terms, then the channel's recent final text.
pub fn whisper_prompt(glossary: &str, previous: &str) -> String {
    let glossary: String = glossary.trim().chars().take(PROMPT_PART).collect();
    let previous = tail(previous.trim(), PROMPT_PART);
    [glossary.as_str(), previous]
        .iter()
        .filter(|p| !p.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(channel: u8, text: &str, start: f64, end: f64) -> TranscriptLine {
        TranscriptLine {
            channel,
            speaker: None,
            text: text.into(),
            is_final: true,
            speech_final: true,
            start,
            end,
        }
    }

    #[test]
    fn empty_and_punctuation_only_are_dropped() {
        let mut f = LineFilter::default();
        for t in ["", "  ", ".", "...", "- ?!"] {
            assert!(!f.accept(&line(0, t, 0.0, 1.0)), "{t:?}");
        }
    }

    #[test]
    fn silence_phrases_are_dropped_case_insensitively_but_only_as_whole_lines() {
        let mut f = LineFilter::default();
        for t in [
            "Thank you.",
            "THANKS FOR WATCHING!",
            "You",
            "[BLANK_AUDIO]",
            "(silence)",
        ] {
            assert!(!f.accept(&line(0, t, 0.0, 1.0)), "{t:?}");
        }
        assert!(f.accept(&line(0, "Thank you for the update on billing", 2.0, 4.0)));
        assert!(f.accept(&line(0, "Are you ready", 5.0, 6.0)));
    }

    #[test]
    fn exact_repeat_on_same_channel_within_ten_seconds_is_dropped() {
        let mut f = LineFilter::default();
        assert!(f.accept(&line(0, "Let us begin", 0.0, 2.0)));
        assert!(!f.accept(&line(0, "Let us begin", 5.0, 7.0)));
        assert!(f.accept(&line(0, "Let us begin", 63.0, 65.0)));
        // Another channel is independent.
        assert!(f.accept(&line(1, "Let us begin", 63.5, 65.0)));
        // Short lines only repeat-drop as exact text within ten seconds.
        assert!(f.accept(&line(0, "Yes", 70.0, 71.0)));
        assert!(f.accept(&line(0, "Yes", 82.0, 83.0)));
    }

    #[test]
    fn a_hallucination_loop_on_quiet_audio_is_dropped() {
        // Seen on p620: the mic channel produced these every few seconds for minutes.
        let mut f = LineFilter::default();
        assert!(f.accept(&line(0, "...in that word.", 48.0, 49.0)));
        for (i, t) in [
            "...in that word. Amen.",
            "...in that word. Amen. ...in that word. Amen.",
            "Amen. ...in that word.",
            "Amen. ...in that word. Amen.",
            "Amen.",
        ]
        .iter()
        .enumerate()
        {
            let start = 60.0 + 15.0 * i as f64;
            assert!(!f.accept(&line(0, t, start, start + 1.0)), "{t:?}");
        }
        // Real speech in between still gets through, and the phrase may return after a minute.
        assert!(f.accept(&line(0, "Can we move the review to Thursday", 100.0, 102.0)));
        assert!(f.accept(&line(0, "...in that word.", 200.0, 201.0)));
    }

    #[test]
    fn repeated_phrase_collapses_to_one() {
        let w = |t: &str| tokens(t);
        assert_eq!(collapse(&w("a b a b")), &w("a b")[..]);
        assert_eq!(collapse(&w("a a a")), &w("a")[..]);
        assert_eq!(collapse(&w("a b c")), &w("a b c")[..]);
        assert_eq!(collapse(&w("a b a")), &w("a b a")[..]);
        assert!(collapse(&[]).is_empty());
    }

    #[test]
    fn mic_echo_of_system_line_is_dropped() {
        let mut f = LineFilter::default();
        assert!(f.accept(&line(1, "we should ship the release on friday", 10.0, 13.0)));
        assert!(!f.accept(&line(
            0,
            "we should ship the release on Friday.",
            12.0,
            14.0
        )));
        // 4 of 5 shared tokens is exactly 80%.
        assert!(!f.accept(&line(0, "we should ship release on friday", 10.5, 13.5)));
    }

    #[test]
    fn mic_line_is_kept_when_different_late_or_short() {
        let mut f = LineFilter::default();
        assert!(f.accept(&line(1, "we should ship the release on friday", 10.0, 13.0)));
        assert!(f.accept(&line(0, "I think monday is safer for us", 12.0, 14.0)));
        assert!(f.accept(&line(0, "we should ship the release on friday", 20.0, 22.0)));
        assert!(f.accept(&line(0, "friday", 12.0, 13.0)));
    }

    #[test]
    fn accepted_lines_feed_the_channels_own_prompt() {
        let mut f = LineFilter::default();
        f.accept(&line(0, "mic words here", 0.0, 1.0));
        f.accept(&line(1, "system words here", 0.0, 1.0));
        f.accept(&line(1, "[BLANK_AUDIO]", 2.0, 3.0));
        assert_eq!(f.prompt(0, "Acme"), "Acme mic words here");
        assert_eq!(f.prompt(1, ""), "system words here");
    }

    #[test]
    fn prompt_has_glossary_then_a_bounded_word_aligned_tail() {
        assert_eq!(whisper_prompt("", ""), "");
        assert_eq!(whisper_prompt("Acme, Kubernetes", ""), "Acme, Kubernetes");
        assert_eq!(whisper_prompt("", "short text"), "short text");
        assert_eq!(whisper_prompt("Acme", "we met"), "Acme we met");
        let long = "word ".repeat(100);
        let p = whisper_prompt(&"g".repeat(500), &long);
        assert!(p.len() <= 2 * PROMPT_PART + 1);
        assert!(p.ends_with("word"));
        assert!(p.split(' ').skip(1).all(|w| w == "word"));
        // Multibyte text never splits a character.
        let _ = whisper_prompt("", &"é".repeat(500));
    }
}
