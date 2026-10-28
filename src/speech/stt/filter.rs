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
const ECHO_WINDOW: f64 = 8.0;
const ECHO_CONTAINMENT: f64 = 0.6;
const HOLD_WINDOW: f64 = 3.0;
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

/// Number of tokens in `a` that also occur in `b` (multiset).
fn shared_tokens<'a, I>(a: &[String], b: I) -> usize
where
    I: IntoIterator<Item = &'a String>,
{
    let mut rest: Vec<&String> = b.into_iter().collect();
    let mut shared = 0usize;
    for word in a {
        if let Some(i) = rest.iter().position(|w| *w == word) {
            rest.swap_remove(i);
            shared += 1;
        }
    }
    shared
}

/// Share of the longer line's tokens that also occur in the other (multiset).
pub(crate) fn similarity(a: &[String], b: &[String]) -> f64 {
    shared_tokens(a, b) as f64 / a.len().max(b.len()).max(1) as f64
}

/// Share of `mic` tokens that occur in `system` (multiset).
fn containment<'a, I>(mic: &[String], system: I) -> f64
where
    I: IntoIterator<Item = &'a String>,
{
    shared_tokens(mic, system) as f64 / mic.len().max(1) as f64
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
    /// Candidate mic lines waiting for overlapping system lines: line, tokens.
    held: Vec<(TranscriptLine, Vec<String>)>,
    /// Recent accepted text on the system channel, the whisper prompt context.
    recent_system: String,
}

impl LineFilter {
    /// Accept a candidate line and return all lines released now.
    pub fn accept(&mut self, line: &TranscriptLine) -> Vec<TranscriptLine> {
        let now = line.end;
        let words = tokens(&line.text);
        let text = line.text.trim();
        // "Amen. Amen. Amen." is the silence phrase said three times.
        if words.is_empty() || SILENCE_PHRASES.contains(&collapse(&words).join(" ").as_str()) {
            return self.release_ready(now, false);
        }
        let channel = usize::from(line.channel.min(1));
        if let Some((previous, end)) = &self.last[channel]
            && previous == text
            && line.start - end <= REPEAT_WINDOW
        {
            return self.release_ready(now, false);
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
            return self.release_ready(now, false);
        }
        self.said[channel].push((phrase, line.end));
        self.last[channel] = Some((text.to_owned(), line.end));
        if channel == 1 {
            self.system.retain(|s| s.1 > line.end - 30.0);
            self.system.push((line.start, line.end, words));
            self.recent_system.push(' ');
            self.recent_system.push_str(text);
            let cut = self.recent_system.len().saturating_sub(4 * PROMPT_PART);
            let cut = (cut..=self.recent_system.len())
                .find(|i| self.recent_system.is_char_boundary(*i))
                .unwrap_or(0);
            self.recent_system.drain(..cut);
            let mut out = self.release_ready(now, false);
            out.push(line.clone());
            out
        } else {
            self.held.push((line.clone(), words));
            self.release_ready(now, false)
        }
    }

    /// Release held mic lines whose hold window has expired at audio clock `now`.
    pub fn tick(&mut self, now: f64) -> Vec<TranscriptLine> {
        self.release_ready(now, false)
    }

    /// Judge and release all held mic lines at end of stream.
    pub fn flush(&mut self) -> Vec<TranscriptLine> {
        self.release_ready(f64::INFINITY, true)
    }

    fn release_ready(&mut self, now: f64, all: bool) -> Vec<TranscriptLine> {
        let max_sys_end = self
            .system
            .iter()
            .map(|(_, end, _)| *end)
            .fold(f64::NEG_INFINITY, f64::max);
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.held.len() {
            let ready = {
                let (line, _) = &self.held[i];
                all || max_sys_end >= line.end || now - line.end >= HOLD_WINDOW
            };
            if ready {
                let (line, words) = self.held.remove(i);
                if !self.is_echo(&line, &words) {
                    out.push(line);
                }
            } else {
                i += 1;
            }
        }
        out
    }

    fn is_echo(&self, line: &TranscriptLine, words: &[String]) -> bool {
        if words.len() < ECHO_MIN_TOKENS {
            return false;
        }
        let combined = self
            .system
            .iter()
            .filter(|(start, end, _)| {
                line.start - ECHO_WINDOW <= *end && *start <= line.end + ECHO_WINDOW
            })
            .flat_map(|(_, _, tokens)| tokens.iter());
        containment(words, combined) >= ECHO_CONTAINMENT
    }

    /// Prompt for the next request on `channel`.
    pub fn prompt(&self, channel: u8, glossary: &str) -> String {
        let previous = if channel == 0 {
            ""
        } else {
            &self.recent_system
        };
        whisper_prompt(glossary, previous)
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
            assert!(f.accept(&line(0, t, 0.0, 1.0)).is_empty(), "{t:?}");
        }
        assert!(f.flush().is_empty());
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
            "- Amen. - Amen. - Amen. - Amen. - Amen. - Amen.",
            "- Amen. - Amen.",
            "Thank you. Thank you.",
        ] {
            assert!(f.accept(&line(0, t, 0.0, 1.0)).is_empty(), "{t:?}");
        }
        assert!(f.flush().is_empty());
        assert!(
            f.accept(&line(0, "Thank you for the update on billing", 2.0, 4.0))
                .is_empty()
        );
        assert_eq!(f.flush().len(), 1);
        assert!(f.accept(&line(0, "Are you ready", 5.0, 6.0)).is_empty());
        assert_eq!(f.flush().len(), 1);
    }

    #[test]
    fn exact_repeat_on_same_channel_within_ten_seconds_is_dropped() {
        let mut f = LineFilter::default();
        assert!(f.accept(&line(0, "Let us begin", 0.0, 2.0)).is_empty());
        assert_eq!(f.accept(&line(0, "Let us begin", 5.0, 7.0)).len(), 1);
        assert!(f.flush().is_empty());
        assert!(f.accept(&line(0, "Let us begin", 63.0, 65.0)).is_empty());
        assert_eq!(f.flush().len(), 1);
        // Another channel is independent.
        assert_eq!(f.accept(&line(1, "Let us begin", 63.5, 65.0)).len(), 1);
        // Short lines only repeat-drop as exact text within ten seconds.
        assert!(f.accept(&line(0, "Yes", 70.0, 71.0)).is_empty());
        assert_eq!(f.accept(&line(0, "Yes", 82.0, 83.0)).len(), 1);
        assert_eq!(f.flush().len(), 1);
    }

    #[test]
    fn a_hallucination_loop_on_quiet_audio_is_dropped() {
        // Seen on p620: the mic channel produced these every few seconds for minutes.
        let mut f = LineFilter::default();
        assert!(
            f.accept(&line(0, "...in that word.", 48.0, 49.0))
                .is_empty()
        );
        assert_eq!(f.flush().len(), 1);
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
            assert!(
                f.accept(&line(0, t, start, start + 1.0)).is_empty(),
                "{t:?}"
            );
        }
        assert!(f.flush().is_empty());
        // Real speech in between still gets through, and the phrase may return after a minute.
        assert!(
            f.accept(&line(0, "Can we move the review to Thursday", 100.0, 102.0))
                .is_empty()
        );
        assert_eq!(f.flush().len(), 1);
        assert!(
            f.accept(&line(0, "...in that word.", 200.0, 201.0))
                .is_empty()
        );
        assert_eq!(f.flush().len(), 1);
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
        assert_eq!(
            f.accept(&line(1, "we should ship the release on friday", 10.0, 15.0))
                .len(),
            1
        );
        assert!(
            f.accept(&line(
                0,
                "we should ship the release on Friday.",
                12.0,
                14.0
            ))
            .is_empty()
        );
        // 3 of 5 shared tokens is 60% containment.
        assert!(
            f.accept(&line(0, "we should ship tomorrow morning", 10.5, 13.5))
                .is_empty()
        );
        assert!(f.flush().is_empty());
    }

    #[test]
    fn containment_over_two_system_lines_combined_drops_mic_echo() {
        let mut f = LineFilter::default();
        assert_eq!(f.accept(&line(1, "we should ship", 10.0, 12.0)).len(), 1);
        assert_eq!(
            f.accept(&line(1, "the release on friday", 12.2, 15.0))
                .len(),
            1
        );
        // Split across two system chunks, neither alone has >= 60% of the 6-word mic line (3/6 = 50%),
        // but combined within +/-8 s they have 6/6 = 100%.
        assert!(
            f.accept(&line(0, "we should ship the release friday", 10.5, 14.5))
                .is_empty()
        );
        assert!(f.flush().is_empty());
    }

    #[test]
    fn mic_line_arriving_before_its_system_line_is_dropped_once_system_arrives() {
        let mut f = LineFilter::default();
        // Mic chunk finishes first and is held because no system line >= 13.0 has arrived.
        assert!(
            f.accept(&line(0, "we should ship the release on friday", 10.0, 13.0))
                .is_empty()
        );
        // System chunk covering the utterance arrives later and triggers judgment -> dropped.
        let out = f.accept(&line(1, "we should ship the release on friday", 9.8, 13.5));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].channel, 1);
        assert!(f.flush().is_empty());
    }

    #[test]
    fn genuine_owner_line_during_system_speech_is_kept_and_flush_releases_held_lines() {
        let mut f = LineFilter::default();
        assert_eq!(
            f.accept(&line(1, "we should ship the release on friday", 10.0, 13.0))
                .len(),
            1
        );
        // Owner speaks overlapping system speech (ends at 14.0 > 13.0, so held initially).
        assert!(
            f.accept(&line(0, "I think monday is safer for us", 12.0, 14.0))
                .is_empty()
        );
        // Beyond +/-8 s window -> held until flush, then kept.
        assert!(
            f.accept(&line(0, "we should ship the release on friday", 22.0, 24.0))
                .len()
                == 1
        );
        // Short (< 3 words) -> kept.
        assert!(f.accept(&line(0, "friday", 12.0, 13.0)).len() == 1);
        let flushed = f.flush();
        assert_eq!(flushed.len(), 1);
        assert_eq!(flushed[0].text, "we should ship the release on friday");
    }

    #[test]
    fn tick_releases_held_mic_line_after_hold_window_without_later_lines() {
        let mut f = LineFilter::default();
        assert!(
            f.accept(&line(0, "I will follow up with the notes", 10.0, 12.0))
                .is_empty()
        );
        assert!(f.tick(12.0).is_empty());
        assert!(f.tick(14.99).is_empty());
        let released = f.tick(15.0);
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].text, "I will follow up with the notes");
        assert!(f.tick(20.0).is_empty());
        assert!(f.flush().is_empty());
    }

    #[test]
    fn prompt_for_channel_zero_has_no_own_text() {
        let mut f = LineFilter::default();
        f.accept(&line(0, "mic words here", 0.0, 1.0));
        f.accept(&line(1, "system words here", 0.0, 1.0));
        f.accept(&line(1, "[BLANK_AUDIO]", 2.0, 3.0));
        assert_eq!(f.prompt(0, "Acme"), "Acme");
        assert_eq!(f.prompt(0, ""), "");
        assert_eq!(f.prompt(1, ""), "system words here");
        assert_eq!(f.prompt(1, "Acme"), "Acme system words here");
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
