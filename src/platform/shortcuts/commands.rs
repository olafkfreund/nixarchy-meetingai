use super::Action;

/// Map a forwarded command to an action.
pub fn parse_command(command: &str) -> Option<Action> {
    match command {
        "toggle-dashboard" => Some(Action::ToggleDashboard),
        "toggle-window" => Some(Action::ToggleWindow),
        "focus-input" => Some(Action::FocusInput),
        "toggle-system-audio" => Some(Action::SystemAudio),
        "toggle-voice" => Some(Action::VoiceInput),
        "screenshot" => Some(Action::Screenshot),
        _ => None,
    }
}
