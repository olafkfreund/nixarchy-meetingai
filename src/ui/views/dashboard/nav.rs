//! Dashboard navigation.
//!
//! One enum drives the sidebar/topbar, the active highlight and which page renders,
//! consolidating all features into a cohesive, modern glass interface.

use lucide_icons::Icon;

/// Every dashboard destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Route {
    #[default]
    Conversations,
    ConversationDetail,
    Meetings,
    MeetingDetail,
    Prompts,
    Settings,
    Audio,
    Shortcuts,
    DevSpace,
    About,
    Profiles,
    Responses,
}

/// A navigation entry.
#[derive(Debug, Clone, Copy)]
pub struct NavItem {
    pub route: Route,
    pub label: &'static str,
    pub icon: Icon,
    pub description: &'static str,
}

/// Dashboard destinations in display order.
pub const NAV_ITEMS: &[NavItem] = &[
    NavItem {
        route: Route::Conversations,
        label: "Conversations",
        icon: Icon::MessagesSquare,
        description: "Search and revisit chat conversations and transcripts.",
    },
    NavItem {
        route: Route::Meetings,
        label: "Meetings",
        icon: Icon::Headphones,
        description: "Recorded sessions with transcripts and recaps.",
    },
    NavItem {
        route: Route::Prompts,
        label: "Prompts",
        icon: Icon::Sparkles,
        description: "Tune how answers and instructions are written.",
    },
    NavItem {
        route: Route::Profiles,
        label: "Profiles",
        icon: Icon::UserRound,
        description: "Reusable background for meetings and interviews.",
    },
    NavItem {
        route: Route::Settings,
        label: "Settings",
        icon: Icon::Settings,
        description: "App preferences, audio devices, AI models and shortcuts.",
    },
    NavItem {
        route: Route::About,
        label: "About",
        icon: Icon::Info,
        description: "What nixarchy-meetingai does and where to find the project.",
    },
];

/// Top-level sections in display order.
pub const SECTIONS: &[(&str, &[NavItem])] = &[("Workspace", NAV_ITEMS)];

impl Route {
    /// Page title shown in the dashboard header.
    pub fn title(self) -> &'static str {
        match self {
            Route::Conversations => "All Conversations",
            Route::ConversationDetail => "Conversation",
            Route::Meetings => "Meetings",
            Route::MeetingDetail => "Meeting",
            Route::Prompts => "System Prompts",
            Route::Settings => "App Settings",
            Route::Audio => "Audio & Speech",
            Route::Shortcuts => "Cursor & Shortcuts",
            Route::DevSpace => "AI Models & Keys",
            Route::About => "About nixarchy-meetingai",
            Route::Profiles => "Context profiles",
            Route::Responses => "Response settings",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Route::Conversations => "Search and revisit everything you have talked about.",
            Route::ConversationDetail => "The full transcript of this conversation.",
            Route::Meetings => "Recorded sessions with transcripts and recaps.",
            Route::MeetingDetail => "Transcript, notes and speaker names.",
            Route::Prompts => "Reusable instructions for how answers are written.",
            Route::Settings => "Overlay, appearance, capture timing and startup preferences.",
            Route::Audio => "Devices, levels and speech-to-text providers.",
            Route::Shortcuts => "Keyboard combinations that work from anywhere.",
            Route::DevSpace => "Gemini and Deepgram credentials.",
            Route::About => "A little help with what to say next.",
            Route::Profiles => "Reusable background for meetings and interviews.",
            Route::Responses => {
                "Answer length, language, follow-ups, scrolling and meeting recaps."
            }
        }
    }

    /// Whether this route is part of the settings group.
    pub fn is_setting(self) -> bool {
        matches!(
            self,
            Route::Settings | Route::Audio | Route::Shortcuts | Route::DevSpace | Route::Responses
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sidebar_route_has_metadata() {
        for (_, items) in SECTIONS {
            for item in *items {
                assert!(!item.label.is_empty());
                assert!(!item.description.is_empty());
                assert!(!item.route.title().is_empty());
                assert!(!item.route.description().is_empty());
            }
        }
    }

    #[test]
    fn sidebar_has_no_duplicate_routes() {
        let mut seen = std::collections::HashSet::new();
        for (_, items) in SECTIONS {
            for item in *items {
                assert!(seen.insert(item.route), "duplicate {:?}", item.route);
            }
        }
    }

    #[test]
    fn settings_group_is_flagged() {
        assert!(Route::Settings.is_setting());
        assert!(Route::Audio.is_setting());
        assert!(Route::DevSpace.is_setting());
        assert!(Route::Shortcuts.is_setting());
        assert!(!Route::Conversations.is_setting());
        assert!(!Route::Meetings.is_setting());
        assert!(!Route::Prompts.is_setting());
        assert!(!Route::Profiles.is_setting());
    }

    #[test]
    fn default_route_is_conversations() {
        assert_eq!(Route::default(), Route::Conversations);
    }
}
