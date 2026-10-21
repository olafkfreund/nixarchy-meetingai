//! Cached, incremental code highlighting for streamed fenced blocks.
//!
//! Completed lines retain Syntect parser/highlighter state. Only newly completed
//! lines and the unfinished tail are processed on append. Unknown fence languages
//! stay readable as plain text. No HTML, JavaScript or network assets are involved.
use std::{cell::RefCell, collections::VecDeque, ops::Range, sync::Arc};
use two_face::{
    re_exports::syntect::{
        easy::HighlightLines,
        highlighting::{HighlightState, Style, Theme},
        parsing::{ParseState, SyntaxSet},
        util::LinesWithEndings,
    },
    theme::EmbeddedThemeName,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeRun {
    pub range: Range<usize>,
    pub color: [u8; 3],
    pub bold: bool,
    pub italic: bool,
}

struct Entry {
    source: String,
    language: String,
    light: bool,
    committed: usize,
    state: (HighlightState, ParseState),
    stable: Vec<CodeRun>,
    rendered: Arc<Vec<CodeRun>>,
    parsed_lines: usize,
}
struct Cache {
    syntaxes: SyntaxSet,
    dark: Theme,
    light: Theme,
    entries: VecDeque<Entry>,
}
impl Cache {
    fn new() -> Self {
        let themes = two_face::theme::extra();
        Self {
            syntaxes: two_face::syntax::extra_newlines(),
            dark: themes[EmbeddedThemeName::Base16EightiesDark].clone(),
            light: themes[EmbeddedThemeName::InspiredGithub].clone(),
            entries: VecDeque::new(),
        }
    }
    fn highlight(&mut self, language: &str, text: &str, light: bool) -> Arc<Vec<CodeRun>> {
        let language = language
            .split_whitespace()
            .next()
            .unwrap_or("text")
            .to_ascii_lowercase();
        let language = match language.as_str() {
            "js" => "javascript",
            "ts" => "typescript",
            "sh" | "shell" | "console" => "bash",
            "c++" => "cpp",
            "c#" => "cs",
            "py" => "python",
            "rs" => "rust",
            other => other,
        }
        .to_owned();
        let theme = if light { &self.light } else { &self.dark };
        let candidate = self.entries.iter().rposition(|entry| {
            entry.language == language && entry.light == light && text.starts_with(&entry.source)
        });
        let mut entry = if let Some(index) = candidate {
            self.entries.remove(index).unwrap()
        } else {
            let syntax = self
                .syntaxes
                .find_syntax_by_token(&language)
                .or_else(|| self.syntaxes.find_syntax_by_name(&language))
                .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text());
            Entry {
                source: String::new(),
                language,
                light,
                committed: 0,
                state: HighlightLines::new(syntax, theme).state(),
                stable: Vec::new(),
                rendered: Arc::new(Vec::new()),
                parsed_lines: 0,
            }
        };
        if entry.source == text && !entry.rendered.is_empty() {
            let output = entry.rendered.clone();
            self.entries.push_back(entry);
            return output;
        }
        let end = text.rfind('\n').map_or(0, |index| index + 1);
        let mut highlighter =
            HighlightLines::from_state(theme, entry.state.0.clone(), entry.state.1.clone());
        let mut offset = entry.committed;
        for line in LinesWithEndings::from(&text[entry.committed..end]) {
            append_line(
                &mut highlighter,
                &self.syntaxes,
                line,
                offset,
                &mut entry.stable,
            );
            entry.parsed_lines += 1;
            offset += line.len();
        }
        entry.committed = end;
        entry.state = highlighter.state();
        let mut rendered = entry.stable.clone();
        if end < text.len() {
            let mut tail =
                HighlightLines::from_state(theme, entry.state.0.clone(), entry.state.1.clone());
            append_line(&mut tail, &self.syntaxes, &text[end..], end, &mut rendered);
            entry.parsed_lines += 1;
        }
        entry.source = text.to_owned();
        entry.rendered = Arc::new(rendered);
        let output = entry.rendered.clone();
        self.entries.push_back(entry);
        // Bound retained source and state; do not keep every streamed version.
        while self.entries.len() > 32
            || (self.entries.len() > 1
                && self.entries.iter().map(|e| e.source.len()).sum::<usize>() > 2_000_000)
        {
            self.entries.pop_front();
        }
        output
    }
}
fn append_line(
    highlighter: &mut HighlightLines<'_>,
    syntaxes: &SyntaxSet,
    line: &str,
    offset: usize,
    output: &mut Vec<CodeRun>,
) {
    use two_face::re_exports::syntect::highlighting::FontStyle;
    let fallback = Style::default();
    let ranges = highlighter
        .highlight_line(line, syntaxes)
        .unwrap_or_else(|_| vec![(fallback, line)]);
    let mut start = offset;
    for (style, fragment) in ranges {
        let end = start + fragment.len();
        output.push(CodeRun {
            range: start..end,
            color: [style.foreground.r, style.foreground.g, style.foreground.b],
            bold: style.font_style.contains(FontStyle::BOLD),
            italic: style.font_style.contains(FontStyle::ITALIC),
        });
        start = end;
    }
}
thread_local! { static CACHE: RefCell<Cache> = RefCell::new(Cache::new()); }
pub fn warm_up() {
    CACHE.with(|cache| {
        cache.borrow();
    });
}
pub fn highlight(language: Option<&str>, text: &str, light: bool) -> Arc<Vec<CodeRun>> {
    CACHE.with(|cache| {
        cache
            .borrow_mut()
            .highlight(language.unwrap_or("text"), text, light)
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_grammars_cover_common_and_less_common_languages() {
        let cache = Cache::new();
        assert!(cache.syntaxes.syntaxes().len() > 150);
        for language in [
            "Rust",
            "Python",
            "TypeScript",
            "Go",
            "Java",
            "C",
            "C++",
            "Ruby",
            "Lua",
            "Nix",
            "Haskell",
            "TOML",
            "JSON",
            "SQL",
            "Dockerfile",
        ] {
            assert!(
                cache
                    .syntaxes
                    .find_syntax_by_token(language)
                    .or_else(|| cache.syntaxes.find_syntax_by_name(language))
                    .is_some(),
                "{language}"
            );
        }
    }
    #[test]
    fn append_highlighting_matches_a_fresh_parse_at_every_unicode_boundary() {
        let text = "/* multi\nline */\nfn main() {\n    println!(\"héllo 世界\");\n}\n";
        let mut cache = Cache::new();
        let mut fresh = Cache::new();
        for end in text
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()))
        {
            let actual = cache.highlight("rust", &text[..end], false);
            fresh.entries.clear();
            let expected = fresh.highlight("rust", &text[..end], false);
            assert_eq!(actual, expected, "prefix {end}");
            assert_eq!(actual.last().map_or(0, |r| r.range.end), end);
        }
        assert!(cache.entries.back().unwrap().parsed_lines < text.len() + 10);
    }
    #[test]
    fn unchanged_blocks_reuse_tokens_and_unknown_languages_preserve_text() {
        let mut cache = Cache::new();
        let first = cache.highlight("python", "print('hello')", false);
        let again = cache.highlight("python", "print('hello')", false);
        assert!(Arc::ptr_eq(&first, &again));
        let unknown = cache.highlight("future-language", "opaque λ text", false);
        assert_eq!(unknown.last().unwrap().range.end, "opaque λ text".len());
        assert_ne!(
            cache.highlight("rust", "fn main() {}", false),
            cache.highlight("rust", "fn main() {}", true)
        );
    }
}
