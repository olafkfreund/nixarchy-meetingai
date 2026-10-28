use crate::{core::settings::MeetingMode, db::Result};
pub const DEFAULT_SYSTEM_PROMPT: &str = "You are nixarchy-meetingai, helping the user take part in a conversation. Respond to the latest question or request in the context of what has just been said. When the user needs words to say aloud, give a direct, connected reply in their voice, as part of that exchange. Do not explain to them how to answer. For a direct writing, research or coding request, produce the requested work instead of pretending it is a spoken reply. If a crucial fact is missing, give a brief clarification or a clearly conditional answer.";
/// Default text of the `live_answer` prompt kind, used for automatic answers in every mode.
pub const DEFAULT_LIVE_ANSWER: &str = "Write a reply the user can take in at a glance. Start with one bold sentence they can say first, then give 3 to 4 short bullet talking points. Use about 80 words or fewer, with no preamble and no closing remark. Ground every point in the supplied profile, documents and recent speech; never invent experience, numbers or commitments, and say so in a bullet when a needed fact is missing.";

/// Default `live_answer` prompt for interviewer mode: probes grounded in the candidate's words.
pub const INTERVIEWER_LIVE_ANSWER: &str = "Suggest 1 to 3 short follow-up probes the interviewer could ask about the candidate's last answer. Start each with a quote from that answer in double quotes, then the probe as a question. Use about 80 words or fewer, with no preamble. Never assess the candidate: give no score, rating, ranking, hiring recommendation or inference about emotion, personality, confidence or honesty.";

/// The built-in `live_answer` text for a mode.
pub const fn default_live_answer(mode: MeetingMode) -> &'static str {
    match mode {
        MeetingMode::Interviewer => INTERVIEWER_LIVE_ANSWER,
        _ => DEFAULT_LIVE_ANSWER,
    }
}

/// The setting that holds the editable `live_answer` prompt for a mode.
pub fn live_answer_key(mode: MeetingMode) -> String {
    format!("live_answer_prompt_{}", mode.as_str())
}

/// The current `live_answer` prompt; an empty stored text means the default.
pub fn live_answer(connection: &rusqlite::Connection, mode: MeetingMode) -> String {
    crate::db::settings::get(connection, &live_answer_key(mode))
        .ok()
        .flatten()
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| default_live_answer(mode).to_owned())
}

/// The default listen modes, in tab order.
    SeedPrompt {
        name: "Interviewer",
        prompt: "Help the user run an interview, asking the questions and listening to the candidate. Suggest what the interviewer could ask next, in their voice, grounded in the planned question bank and in what the candidate has actually said. Quote the candidate when probing. Never assess the candidate: give no score, rating, ranking, hiring recommendation or inference about emotion, personality, confidence or honesty. Report only what was said and what is still unverified.",
        slot: 5,
    },
        "You are nixarchy-meetingai, a real-time conversation copilot. Understand the latest question in the context of the discussion and any supplied screen. Help the user respond to that question directly. For a spoken question, lead with a natural first-person reply they can say aloud. Add supporting details only when useful. For a direct coding or writing request, produce the requested artifact.",
    #[test]
    fn live_answer_defaults_per_mode_and_edits_survive() {
        let database = Database::open_in_memory().unwrap();
        let c = database.connection();
        for mode in [
            MeetingMode::General,
            MeetingMode::Interview,
            MeetingMode::TeamMeeting,
        ] {
            assert_eq!(live_answer(c, mode), DEFAULT_LIVE_ANSWER);
        }
        assert_eq!(
            live_answer(c, MeetingMode::Interviewer),
            INTERVIEWER_LIVE_ANSWER
        );
        settings::set(c, &live_answer_key(MeetingMode::Interview), "mine").unwrap();
        assert_eq!(live_answer(c, MeetingMode::Interview), "mine");
        assert_eq!(live_answer(c, MeetingMode::General), DEFAULT_LIVE_ANSWER);
        assert!(DEFAULT_LIVE_ANSWER.contains("bold") && DEFAULT_LIVE_ANSWER.contains("80 words"));
    }

        assert_eq!(settings::listen_modes(connection).unwrap().len(), 6);
    fn interviewer_prompts_forbid_assessment() {
        let answer = DEFAULT_LISTEN_MODES
            .iter()
            .find(|mode| mode.name == MeetingMode::Interviewer.listen_name())
            .unwrap()
            .prompt;
        for text in [
            answer,
            INTERVIEWER_LIVE_ANSWER,
            crate::core::settings::RECAP_PROMPT_INTERVIEWER,
        ] {
            assert!(text.contains("Give no score") || text.contains("give no score"));
            assert!(text.contains("hiring recommendation"));
        }
    }

    #[test]
    fn slots_are_ordered_zero_to_five() {
        assert_eq!(slots, [0, 1, 2, 3, 4, 5]);
