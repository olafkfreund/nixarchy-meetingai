//! Quality filters for final batch-transcript lines: whisper hallucinations (T3),
//! microphone echo of the system channel (T4), and the speech prompt (T2).

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
const HOLD_CAP: f64 = 15.0;
/// Minimum content words and consecutive phrase length required for echo drops.
const ECHO_MIN_TOKENS: usize = 3;
const PROMPT_PART: usize = 200;

/// Common English function and conversational words ignored when counting content words.
#[rustfmt::skip]
const STOP_WORDS: &[&str] = &[
    // Pronouns
    "i", "me", "my", "mine", "myself", "you", "your", "yours", "yourself", "he", "him", "his",
    "himself", "she", "her", "hers", "herself", "it", "its", "itself", "we", "us", "our", "ours",
    "ourselves", "they", "them", "their", "theirs", "themselves", "this", "that", "these",
    "those", "who", "whom", "whose", "what", "which",
    // Articles, determiners, conjunctions
    "a", "an", "the", "and", "or", "but", "if", "because", "as", "while", "not",
    // Auxiliaries and contraction fragments
    "am", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had", "having", "do",
    "does", "did", "will", "would", "shall", "should", "can", "could", "may", "might", "must",
    "don", "doesn", "didn", "won", "wouldn", "shouldn", "couldn", "isn", "aren", "wasn", "weren",
    "haven", "hasn", "hadn", "t", "s", "m", "re", "ve", "ll", "d",
    // Prepositions
    "in", "on", "at", "to", "for", "of", "with", "by", "from", "up", "down", "out", "off", "over",
    "under", "into", "onto", "about", "after", "before", "between", "through", "during",
    "without", "within", "against", "around",
    // Short conversational words
    "yes", "no", "ok", "okay", "so", "think", "yeah", "well", "there", "here", "then", "than",
    "too", "very", "just",
];

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

fn has_consecutive_run(mic: &[String], system: &[&String], run_len: usize) -> bool {
    mic.len() >= run_len
        && system.len() >= run_len
        && mic.windows(run_len).any(|run| {
            system
                .windows(run_len)
                .any(|sys| run.iter().zip(sys).all(|(a, b)| a == *b))
        })
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
    /// Candidate mic lines waiting for overlapping system lines: line, tokens, arrived_at.
    held: Vec<(TranscriptLine, Vec<String>, f64)>,
    /// Audio clock observed so far.
    clock: f64,
    /// Earliest start time of any open or in-flight system chunk.
    pending_sys_start: Option<f64>,
    /// Recent accepted text on the system channel, the prompt context.
    recent_system: String,
}

impl LineFilter {
    /// Accept a candidate line and return all lines released now.
    pub fn accept(&mut self, line: &TranscriptLine) -> Vec<TranscriptLine> {
        let now = self.clock.max(line.end);
        self.clock = now;
        let words = tokens(&line.text);
        let text = line.text.trim();
        // "Amen. Amen. Amen." is the silence phrase said three times.
        if words.is_empty() || SILENCE_PHRASES.contains(&collapse(&words).join(" ").as_str()) {
            return self.release_ready(now, false);
        }
        let channel = usize::from(line.channel.min(1));
        if channel == 1 {
            if !self.keep_for_channel(1, line, &words) {
                return self.release_ready(now, false);
            }
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
            self.held.push((line.clone(), words, now));
            self.release_ready(now, false)
        }
    }

    /// Release held mic lines whose hold window has expired at audio clock `now`
    /// and that have no overlapping open-or-in-flight system chunk (`pending_sys_start`).
    pub fn tick(&mut self, now: f64, pending_sys_start: Option<f64>) -> Vec<TranscriptLine> {
        self.clock = self.clock.max(now);
        self.pending_sys_start = pending_sys_start;
        self.release_ready(self.clock, false)
    }

    /// Judge and release all held mic lines at end of stream.
    pub fn flush(&mut self) -> Vec<TranscriptLine> {
        self.pending_sys_start = None;
        self.release_ready(f64::INFINITY, true)
    }

    fn keep_for_channel(
        &mut self,
        channel: usize,
        line: &TranscriptLine,
        words: &[String],
    ) -> bool {
        let text = line.text.trim();
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
        self.said[channel].push((phrase, line.end));
        self.last[channel] = Some((text.to_owned(), line.end));
        true
    }

    fn release_ready(&mut self, now: f64, all: bool) -> Vec<TranscriptLine> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.held.len() {
            let ready = {
                let (line, _, arrived_at) = &self.held[i];
                let waited = now - *arrived_at;
                all || waited >= HOLD_CAP
                    || (waited >= HOLD_WINDOW
                        && self.pending_sys_start.is_none_or(|s| s > line.end))
            };
            if ready {
                let (line, words, _) = self.held.remove(i);
                if !self.is_echo(&line, &words) && self.keep_for_channel(0, &line, &words) {
                    out.push(line);
                }
            } else {
                i += 1;
            }
        }
        out
    }

    fn is_echo(&self, line: &TranscriptLine, words: &[String]) -> bool {
        let content: Vec<String> = words
            .iter()
            .filter(|w| !STOP_WORDS.contains(&w.as_str()))
            .cloned()
            .collect();
        if content.len() < ECHO_MIN_TOKENS {
            return false;
        }
        let sys_tokens: Vec<&String> = self
            .system
            .iter()
            .filter(|(start, end, _)| {
                line.start - ECHO_WINDOW <= *end && *start <= line.end + ECHO_WINDOW
            })
            .flat_map(|(_, _, tokens)| tokens.iter())
            .collect();
        has_consecutive_run(words, &sys_tokens, ECHO_MIN_TOKENS)
            && containment(&content, sys_tokens) >= ECHO_CONTAINMENT
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

/// The `prompt` for OpenAI-compatible servers: glossary terms, then the channel's recent final text.
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
            f.accept(&line(
                1,
                "we should ship the new release on friday afternoon",
                10.0,
                15.0
            ))
            .len(),
            1
        );
        // Exact echo is dropped.
        assert!(
            f.accept(&line(
                0,
                "we should ship the new release on Friday afternoon.",
                12.0,
                14.0
            ))
            .is_empty()
        );
        // Real echo repeating the remote sentence with a word or two misheard is dropped
        // (3/5 content words = 60%, plus consecutive 3-word phrase runs).
        assert!(
            f.accept(&line(
                0,
                "we should ship the next release on friday morning",
                10.5,
                14.5
            ))
            .is_empty()
        );
        assert!(f.flush().is_empty());
    }

    #[test]
    fn short_owner_replies_survive_ordinary_remote_speech() {
        let mut f = LineFilter::default();
        assert_eq!(
            f.accept(&line(
                1,
                "I think we can send it to you if you will do the draft ok so yes we should ship the release on friday",
                10.0,
                16.0
            ))
            .len(),
            1
        );
        for (i, reply) in [
            "I think so",
            "Can you send it to me",
            "OK I will send the draft",
            "we should ship tomorrow morning",
        ]
        .iter()
        .enumerate()
        {
            let start = 11.0 + i as f64;
            assert!(
                f.accept(&line(0, reply, start, start + 1.0)).is_empty(),
                "{reply:?}"
            );
        }
        let kept: Vec<String> = f.flush().into_iter().map(|l| l.text).collect();
        assert_eq!(
            kept,
            vec![
                "I think so",
                "Can you send it to me",
                "OK I will send the draft",
                "we should ship tomorrow morning",
            ]
        );
    }

    #[test]
    fn dropped_echo_does_not_block_owner_later_saying_same_sentence() {
        let mut f = LineFilter::default();
        assert_eq!(
            f.accept(&line(1, "we should ship the release on friday", 0.0, 2.0))
                .len(),
            1
        );
        // Echo on the mic channel is dropped when released at t=5.0.
        assert!(
            f.accept(&line(0, "we should ship the release on friday", 0.2, 2.0))
                .is_empty()
        );
        assert!(f.tick(5.0, None).is_empty());
        // Owner says the same sentence later (within the 60 s loop window, outside the 8 s echo window).
        assert!(
            f.accept(&line(0, "we should ship the release on friday", 11.0, 13.0))
                .is_empty()
        );
        let kept = f.flush();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].text, "we should ship the release on friday");
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
        // Split across two system chunks, neither alone has >= 60% of the content words,
        // but combined within +/-8 s they have 3/3 = 100% and a 3-word consecutive run.
        assert!(
            f.accept(&line(0, "we should ship the release friday", 10.5, 14.5))
                .is_empty()
        );
        assert!(f.flush().is_empty());
    }

    #[test]
    fn mic_line_arriving_before_its_system_line_is_dropped_once_system_arrives() {
        let mut f = LineFilter::default();
        // Mic chunk finishes first and is held.
        assert!(
            f.accept(&line(0, "we should ship the release on friday", 10.0, 13.0))
                .is_empty()
        );
        // System chunk covering the utterance arrives later; once held window expires, echo is dropped.
        let out = f.accept(&line(1, "we should ship the release on friday", 9.8, 13.5));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].channel, 1);
        assert!(f.tick(16.0, None).is_empty());
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
        // Owner speaks overlapping system speech.
        assert!(
            f.accept(&line(0, "I think monday is safer for us", 12.0, 14.0))
                .is_empty()
        );
        // Beyond +/-8 s window -> releases the earlier line (now=24.0 >= 14.0 + 3.0) and holds this one.
        let released = f.accept(&line(0, "we should ship the release on friday", 22.0, 24.0));
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].text, "I think monday is safer for us");
        // Short (< 3 words) -> held and kept on flush.
        assert!(f.accept(&line(0, "friday", 24.0, 25.0)).is_empty());
        let flushed = f.flush();
        assert_eq!(flushed.len(), 2);
        assert_eq!(flushed[0].text, "we should ship the release on friday");
        assert_eq!(flushed[1].text, "friday");
    }

    #[test]
    fn tick_holds_for_three_seconds_after_arrival_and_waits_for_overlapping_system_chunk() {
        let mut f = LineFilter::default();
        // Audio clock is already at 14.5 s when the 10.0..12.0 mic chunk finishes transcribing.
        assert!(f.tick(14.5, Some(10.0)).is_empty());
        assert!(
            f.accept(&line(0, "I will follow up with the notes", 10.0, 12.0))
                .is_empty()
        );
        // 3 s after line.end (15.0) is only 0.5 s after arrival -> still held even without pending system.
        assert!(f.tick(15.0, None).is_empty());
        assert!(f.tick(17.49, None).is_empty());
        // At 17.5 s (3 s after arrival), if an overlapping system chunk (start 11.0 <= 12.0) is still
        // open or in flight, the line remains held.
        assert!(f.tick(17.5, Some(11.0)).is_empty());
        assert!(f.tick(20.0, Some(11.0)).is_empty());
        // A system chunk that started after the mic line ended (12.5 > 12.0) does not block release.
        let released = f.tick(20.0, Some(12.5));
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].text, "I will follow up with the notes");
        assert!(f.flush().is_empty());
    }

    #[test]
    fn tick_enforces_fifteen_second_hard_cap_when_system_chunk_stays_open() {
        let mut f = LineFilter::default();
        assert!(f.tick(14.0, Some(10.0)).is_empty());
        assert!(
            f.accept(&line(0, "I will follow up with the notes", 10.0, 12.0))
                .is_empty()
        );
        assert!(f.tick(28.99, Some(10.0)).is_empty());
        let released = f.tick(29.0, Some(10.0));
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].text, "I will follow up with the notes");
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
