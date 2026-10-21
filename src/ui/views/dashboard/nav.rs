//! Dashboard navigation.
//!
//! One enum drives the sidebar, the active highlight and which page renders, so a
//! new screen only needs a variant and a match arm.

use lucide_icons::Icon;

/// Every dashboard destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Route {
    #[default]
    Home,
    Conversations,
    ConversationDetail,
    Meetings,
    MeetingDetail,
    Files,
    Prompts,
    Shortcuts,
    Screenshot,
    Capture,
    Settings,
    Audio,
    Responses,
    DevSpace,
}

/// A sidebar entry.
#[derive(Debug, Clone, Copy)]
pub struct NavItem {
    pub route: Route,
    pub label: &'static str,
    pub icon: Icon,
    /// Shown under the label on the home screen cards.
    pub description: &'static str,
}

/// Sidebar sections, in display order.
pub const SECTIONS: &[(&str, &[NavItem])] = &[
    (
        "Workspace",
        &[
            NavItem {
                route: Route::Home,
                label: "Home",
                icon: Icon::House,
                description: "Your workspace at a glance.",
            },
            NavItem {
                route: Route::Conversations,
                label: "All Conversations",
                icon: Icon::MessagesSquare,
                description: "Every chat and meeting in one searchable place.",
            },
            NavItem {
                route: Route::Meetings,
                label: "Meetings",
                icon: Icon::Headphones,
                description: "Recorded sessions with transcripts and recaps.",
            },
            NavItem {
                route: Route::Files,
                label: "Files",
                icon: Icon::FolderOpen,
                description: "Documents and captures kept with your notes.",
            },
            NavItem {
                route: Route::Prompts,
                label: "System Prompts",
                icon: Icon::MessageSquare,
                description: "Tune how answers are written across contexts.",
            },
        ],
    ),
    (
        "Settings",
        &[
            NavItem {
                route: Route::Settings,
                label: "App Settings",
                icon: Icon::Settings,
                description: "Overlay, appearance and startup preferences.",
            },
            NavItem {
                route: Route::Audio,
                label: "Audio",
                icon: Icon::Mic,
                description: "Microphone, system audio and speech providers.",
            },
            NavItem {
                route: Route::Screenshot,
                label: "Screenshot",
                icon: Icon::Camera,
                description: "Capture behaviour and automatic screenshots.",
            },
            NavItem {
                route: Route::Capture,
                label: "Capture",
                icon: Icon::Crop,
                description: "Region selection and timing.",
            },
            NavItem {
                route: Route::Responses,
                label: "Responses",
                icon: Icon::Sparkles,
                description: "Follow-ups and answer style.",
            },
            NavItem {
                route: Route::Shortcuts,
                label: "Cursor & Shortcuts",
                icon: Icon::Keyboard,
                description: "Global hotkeys and the on-screen cursor.",
            },
            NavItem {
                route: Route::DevSpace,
                label: "Dev Space",
                icon: Icon::Terminal,
                description: "Local CLI connections and developer options.",
            },
        ],
    ),
];

impl Route {
    /// Page title shown in the dashboard header.
    pub fn title(self) -> &'static str {
        match self {
            Route::Home => "Home",
            Route::Conversations => "All Conversations",
            Route::ConversationDetail => "Conversation",
            Route::Meetings => "Meetings",
            Route::MeetingDetail => "Meeting",
            Route::Files => "Files",
            Route::Prompts => "System Prompts",
            Route::Shortcuts => "Cursor & Shortcuts",
            Route::Screenshot => "Screenshot",
            Route::Capture => "Capture",
            Route::Settings => "App Settings",
            Route::Audio => "Audio",
            Route::Responses => "Responses",
            Route::DevSpace => "Dev Space",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Route::Home => "Your whole workspace, in one place.",
            Route::Conversations => "Search and revisit everything you have talked about.",
            Route::ConversationDetail => "The full transcript of this conversation.",
            Route::Meetings => "Recorded sessions with transcripts and recaps.",
            Route::MeetingDetail => "Transcript, notes and speaker names.",
            Route::Files => "Documents attached to conversations and prompts.",
            Route::Prompts => "Reusable instructions for how answers are written.",
            Route::Shortcuts => "Keyboard combinations that work from anywhere.",
            Route::Screenshot => "How captures are taken and sent.",
            Route::Capture => "Select a region once and reuse it.",
            Route::Settings => "Overlay, appearance and startup behaviour.",
            Route::Audio => "Devices, levels and speech-to-text providers.",
            Route::Responses => "Follow-up suggestions and answer style.",
            Route::DevSpace => "Connect local AI CLIs and developer options.",
        }
    }

    /// Whether this route is part of the settings group.
    pub fn is_setting(self) -> bool {
        matches!(
            self,
            Route::Settings
                | Route::Audio
                | Route::Screenshot
                | Route::Capture
                | Route::Responses
                | Route::Shortcuts
                | Route::DevSpace
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
        assert!(!Route::Home.is_setting());
        assert!(!Route::Conversations.is_setting());
    }

    #[test]
    fn default_route_is_home() {
        assert_eq!(Route::default(), Route::Home);
    }
}
