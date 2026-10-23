//! Search all preferences from any Settings tab, without indexing secrets.
use super::*;

const INDEX: &[(Route, &str, &str)] = &[
    (
        Route::Settings,
        "Theme",
        "appearance palette light dark glass",
    ),
    (
        Route::Settings,
        "Transparency",
        "opacity transparent window background",
    ),
    (Route::Settings, "Text size", "font scale readability"),
    (Route::Settings, "Show borders", "outline frame corners"),
    (
        Route::Settings,
        "Reduce motion",
        "animations accessibility movement",
    ),
    (
        Route::Audio,
        "Always on top",
        "window above stacking meeting",
    ),
    (
        Route::Settings,
        "Start at login",
        "startup autostart launch windows linux",
    ),
    (Route::Settings, "Hide app icon", "tray taskbar privacy"),
    (
        Route::Settings,
        "Screen capture",
        "screenshots context image monitor automatic interval delay on send",
    ),
    (
        Route::Responses,
        "Response length",
        "short medium auto detailed answers",
    ),
    (
        Route::Responses,
        "Response language",
        "language translation global",
    ),
    (
        Route::Responses,
        "Smart follow-ups",
        "suggestions follow up questions",
    ),
    (
        Route::Responses,
        "Meeting recap",
        "automatic summary decisions action items transcription",
    ),
    (
        Route::Responses,
        "Auto scroll",
        "scrolling latest response bottom",
    ),
    (
        Route::Profiles,
        "Context profiles",
        "personal about me interview meeting role background create edit",
    ),
    (
        Route::Audio,
        "Microphone",
        "input device mic dropdown refresh audio permissions push to talk voice dictation",
    ),
    (
        Route::Audio,
        "System Audio",
        "speakers headphones output device loopback meeting other participants refresh",
    ),
    (
        Route::Audio,
        "Speech provider",
        "stt speech to text transcribe live streaming",
    ),
    (
        Route::Audio,
        "Speech language",
        "spoken language custom locale auto",
    ),
    (
        Route::Audio,
        "Capture checks",
        "test microphone system audio screenshots permissions",
    ),
    (
        Route::DevSpace,
        "AI providers",
        "gemini merge gateway llm model api key credentials",
    ),
    (
        Route::DevSpace,
        "Custom AI providers",
        "endpoint auth authentication method post response json streaming",
    ),
    (
        Route::DevSpace,
        "Custom speech providers",
        "stt endpoint upload wav response format websocket",
    ),
    (
        Route::DevSpace,
        "Credential vault",
        "encrypted storage keys database",
    ),
    (
        Route::Shortcuts,
        "Keyboard shortcuts",
        "hotkeys push to talk hide show screenshot move resize",
    ),
    (
        Route::Prompts,
        "System prompts",
        "instructions search interview meeting create edit delete",
    ),
    (
        Route::About,
        "Check for updates",
        "version github release website lennette.itssingularity.com",
    ),
];

fn matches(query: &str, title: &str, keywords: &str) -> bool {
    let haystack = format!("{title} {keywords}").to_lowercase();
    query.split_whitespace().all(|word| haystack.contains(word))
}

pub fn results(
    query: &str,
    theme: &Theme,
    reduced: bool,
    cx: &mut Context<crate::ui::dashboard::view::DashboardView>,
) -> gpui::AnyElement {
    let query = query.trim().to_lowercase();
    let mut list = div().flex().flex_col().w_full().gap(px(8.0));
    let mut count = 0;
    for (index, (route, title, keywords)) in INDEX.iter().enumerate() {
        if !matches(&query, title, keywords) {
            continue;
        }
        count += 1;
        let route = *route;
        list = list.child(
            div()
                .id(("settings-search-result", index))
                .w_full()
                .p(px(14.0))
                .rounded(px(12.0))
                .border_1()
                .border_color(theme.control_border(false))
                .bg(theme.input)
                .cursor_pointer()
                .hover(|s| s.bg(theme.menu_hover()))
                .child(div().text_size(scaled(13.0)).child((*title).to_owned()))
                .child(
                    div()
                        .mt(px(4.0))
                        .text_size(scaled(11.0))
                        .text_color(theme.muted_foreground)
                        .child(route.title()),
                )
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.clear_settings_search(cx);
                    view.navigate_to(route);
                    cx.notify();
                })),
        );
    }
    if count == 0 {
        list = list.child(help_text(
            theme,
            "No settings match. Try microphone, transparency, language or providers.",
        ));
    }
    crate::ui::widgets::motion::reveal(list, "settings-search-results", reduced, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn searches_multiple_words_across_labels_and_synonyms() {
        assert!(matches("voice input", "Microphone", INDEX[15].2));
        assert!(matches(
            "stt endpoint",
            "Custom speech providers",
            INDEX[22].2
        ));
        assert!(!matches("speaker password", "System Audio", INDEX[16].2));
    }
}
