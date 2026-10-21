//! Default listen modes.
//!
//! Seeded once on first open. The texts are the same prompts the reference
//! shipped, and users can edit, reorder or remove them afterwards, so seeding
//! must never overwrite edits.

use rusqlite::params;

use super::Result;

/// One seeded prompt.
pub struct SeedPrompt {
    pub name: &'static str,
    pub prompt: &'static str,
    pub slot: i64,
}

/// System prompt used when the user has not chosen a mode.
pub const DEFAULT_SYSTEM_PROMPT: &str =
    "You are a helpful AI assistant. Be concise, accurate, and friendly in your responses.";

/// The five default listen modes, in tab order.
pub const DEFAULT_LISTEN_MODES: &[SeedPrompt] = &[
    SeedPrompt {
        name: "General",
        prompt: "You are a helpful AI assistant. Be concise, accurate, and friendly in your responses.",
        slot: 0,
    },
    SeedPrompt {
        name: "Interview",
        prompt: "You are a discreet real-time interview copilot. The user is in a live job interview. Given the conversation transcript and/or the user's question, suggest a strong, natural first-person answer the user can say out loud. Ground the answer in concrete experience, keep it under 150 words, structure it clearly (situation → action → result when relevant), and never mention that you are an AI or that the user is being assisted.",
        slot: 1,
    },
    SeedPrompt {
        name: "Coding",
        prompt: "You are a real-time technical interview copilot. The user is in a live coding or system-design interview. Analyze the problem from the transcript or question, then give: (1) the key insight, (2) a clean, idiomatic solution in the most likely expected language with the time/space complexity, and (3) one clarifying question worth asking. Be precise and compact — the user is reading while speaking.",
        slot: 2,
    },
    SeedPrompt {
        name: "Translate",
        prompt: "You are a real-time conversation translator. Detect the language of the latest transcript lines, translate them into the user's language (default English), and, when the other person asked something, suggest a short reply the user can say, written in both languages. Format: the original line as a quote, then **Translated:** and, when useful, **Suggested reply:**.",
        slot: 3,
    },
    SeedPrompt {
        name: "Meeting",
        prompt: "You are a real-time meeting copilot. From the live transcript, keep the user on top of the discussion: summarize the key points so far, call out decisions, owners and action items, and suggest what the user should say or ask next when helpful. Be brief and skimmable — short bullets over prose.",
        slot: 4,
    },
];

/// Insert any missing default listen modes.
///
/// Uses `INSERT OR IGNORE` against the unique slot index, so running this on an
/// existing database is a no-op and never clobbers a user's edits.
pub fn seed_listen_modes(connection: &rusqlite::Connection) -> Result<()> {
    for mode in DEFAULT_LISTEN_MODES {
        connection.execute(
            "INSERT OR IGNORE INTO system_prompts (name, prompt, listen_slot)
             VALUES (?1, ?2, ?3)",
            params![mode.name, mode.prompt, mode.slot],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::db::{Database, settings};

    #[test]
    fn seeding_is_idempotent() {
        let database = Database::open_in_memory().unwrap();
        let connection = database.connection();
        seed_listen_modes(connection).unwrap();
        seed_listen_modes(connection).unwrap();
        assert_eq!(settings::listen_modes(connection).unwrap().len(), 5);
    }

    #[test]
    fn user_edits_survive_reseeding() {
        let database = Database::open_in_memory().unwrap();
        let connection = database.connection();
        seed_listen_modes(connection).unwrap();
        connection
            .execute(
                "UPDATE system_prompts SET prompt = 'edited' WHERE listen_slot = 0",
                [],
            )
            .unwrap();
        seed_listen_modes(connection).unwrap();
        let general = settings::listen_modes(connection)
            .unwrap()
            .into_iter()
            .find(|mode| mode.listen_slot == Some(0))
            .unwrap();
        assert_eq!(general.prompt, "edited");
    }

    #[test]
    fn slots_are_ordered_zero_to_four() {
        let slots: Vec<i64> = DEFAULT_LISTEN_MODES.iter().map(|mode| mode.slot).collect();
        assert_eq!(slots, [0, 1, 2, 3, 4]);
    }
}
