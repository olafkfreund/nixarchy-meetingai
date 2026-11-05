---
status: approved
issue: 71
author: olafkfreund
---

# Intent: Live answers work on calls that are not in English

## Problem

Automatic answers and quick actions assume the call is in English. #70 made
Norwegian calls transcribe as Norwegian, and Parakeet already handles 25
languages, Polish among them, so non-English transcripts now reach code that
only understands English.

- **Questions can go unanswered.** With the default setting (`auto_response` is
  `Questions`), an answer is triggered only by `is_question`
  (`src/daemon/session.rs:458`). For text of two or more words, it accepts a `?`
  in any language, or an English opener such as "what ", "can you " or "tell me ".
  A Norwegian "hva mener du" with no `?` from the speech engine gets no answer.
- **Answers the app asks for itself can come back in the wrong language.** Some
  actions send fixed English text as the user's question:
  - the "What should I say?" button (`shell/Overlay.qml:417`);
  - asking about a quoted line with nothing typed (`shell/Overlay.qml:231`);
  - a screenshot or attachment sent with nothing typed
    (`src/daemon/session.rs:1732`);
  - the follow-up suggestions generated after a typed or quick-action answer
    (`src/daemon/session.rs:2155`).

  With `response_language` on Auto, the instruction is to reply in the language
  of the user's latest question (`src/core/models/settings.rs:167`), which here is
  that English text. The same prompt also carries the Norwegian transcript and
  says to continue the exchange (`src/daemon/session.rs:1645`), so the model gets
  two competing cues. An English answer on a Norwegian call is possible, not
  established; how often it happens, and on which models, has not been measured.
  Automatic answers do not have this conflict: their question is the remote
  speaker's own line.

How large the first problem is has not been measured. If the speech engines
usually punctuate spoken questions, the `?` rule already catches most of them.
Neither engine's question-mark rate on spoken questions is known, in any
language.

**Corrections to the issue as filed:**

- `meaningful_remark` (`src/daemon/session.rs:513`) is not English-only in
  practice. It requires at least 5 words, and every entry in its filler list is
  shorter, so the list never changes the result. "Takk" and "tusen takk" are
  already filtered out by the word count. Longer filler passes in every
  language, including English.
- Only "What should I say?" is a real button. "Follow-up questions" and "Fact
  check" appear only in `FIXED_FOLLOW_UPS` (`src/daemon/session.rs:131`), which
  keeps generated suggestions from repeating them; the shell does not offer them.
- `is_question` matters only in **Assist** and **Interview me**, and only with
  `auto_response` on `Questions`. **Team** gives no automatic answers, and
  **Interview someone** uses `meaningful_remark`.

## Proposed outcome

On a call in a language the transcript supports:

- **Detection:** a finished remote line that is a spoken question is recognised
  about as often as in English, without ordinary statements being taken for
  questions. The spec sets the test set and the thresholds. This covers detection
  only: pace, expiry and provider availability work as they do today.
- **Language:** answers to the fixed-text actions above come back in the language
  of the call, or in the language set in `response_language` if one is set.
- English calls behave as they do today.

## Affected users and systems

- Anyone on a non-English call in **Assist** or **Interview me**; in practice
  today, the owner's Norwegian and possibly Polish calls.
- `src/daemon/session.rs` (`is_question`, the fixed texts, follow-ups and the
  language instruction), `src/core/models/settings.rs` and `shell/Overlay.qml`.
- All hosts through the daemon; no database or Nix option is expected to change.

## Constraints

- **No extra cost on the live path.** `is_question` runs on every finished remote
  line. It must not add a network or model call there without an explicit
  decision, since it would add delay and could send call text to a provider.
  Any local work there must be too small to notice.
- **Local-only stays intact.** Nothing here may send text anywhere a Local-only
  interview does not already allow.
- **False answers are worse than missed ones.** An answer popping up on a
  statement is distracting during a call. Openers are less reliable than they
  look: in the Norwegian test sentences "når" often starts a statement
  ("når representanten ...") rather than a question.
- **English behaviour must not regress.** Existing `is_question` tests keep
  passing.
- **Smallest change that works.** No language detection or settings beyond what
  the outcome needs.

## Open questions

1. **Which languages?** Recommended: decide from measurement, with Norwegian
   first, then whichever of Parakeet's languages the owner uses, Polish being
   the known one. A word list for all 25 is a lot of unverified grammar.
2. **Is a word list needed at all?** Recommended: first measure how often NB-Whisper
   and Parakeet produce a `?` on spoken questions. If the rate is high, the
   trigger may need little or no change and this becomes mainly the quick action
   fix.
3. **What should the quick action button show?** Recommended: keep the label in
   English and make only the answer follow the call's language. Translating the
   interface is a separate job.
4. **Delete the dead filler list in `meaningful_remark`?** Recommended: not here.
   It has no effect on the outcome, so it is a separate small cleanup.
