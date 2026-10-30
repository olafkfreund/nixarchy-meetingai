//! Table-level repositories.

pub mod candidates;
pub mod conversations;
pub mod exports;
pub mod items;
pub mod library;
pub mod meetings;
pub mod profiles;
pub mod screenshots;
pub mod sessions;
pub mod settings;
pub mod voice;

pub use conversations::ConversationRecord;
pub use library::FileRecord;
pub use meetings::{LineRecord, MeetingRecord};
pub use settings::PromptRecord;
