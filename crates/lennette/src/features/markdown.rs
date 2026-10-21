//! Markdown rendering.
//!
//! Models stream markdown, so answers are parsed and drawn as real formatting
//! rather than raw text. The parse step is pure and testable; the render step maps
//! the resulting blocks onto GPUI elements.
//!
//! Deliberately incremental-friendly: partial input (an unterminated code fence, a
//! half-written list) still parses into sensible blocks instead of erroring.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// A block of formatted content.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading {
        level: u8,
        spans: Vec<Span>,
    },
    Paragraph {
        spans: Vec<Span>,
    },
    Code {
        language: Option<String>,
        text: String,
    },
    ListItem {
        ordered: bool,
        marker: String,
        spans: Vec<Span>,
    },
    Quote {
        spans: Vec<Span>,
    },
    Rule,
}

/// Inline formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strikethrough: bool,
}

/// A run of text with one style.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

impl Span {
    fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: Style::default(),
        }
    }
}

/// Parser options: tables, strikethrough, task lists.
fn options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options
}

/// Parse markdown into renderable blocks.
pub fn parse(markdown: &str) -> Vec<Block> {
    let parser = Parser::new_ext(markdown, options());
    let mut blocks = Vec::new();

    let mut spans: Vec<Span> = Vec::new();
    let mut style = Style::default();
    let mut current: Option<Open> = None;
    let mut list_stack: Vec<Option<u64>> = Vec::new();
    let mut in_code = false;
    let mut code_language = None;
    let mut code_text = String::new();

    fn flush(blocks: &mut Vec<Block>, current: &Option<Open>, spans: &mut Vec<Span>) {
        if spans.is_empty() {
            return;
        }
        let taken = std::mem::take(spans);
        match current {
            Some(Open::Paragraph) | None => blocks.push(Block::Paragraph { spans: taken }),
            Some(Open::Heading(level)) => blocks.push(Block::Heading {
                level: *level,
                spans: taken,
            }),
            Some(Open::Quote) => blocks.push(Block::Quote { spans: taken }),
            Some(Open::Item { ordered, marker }) => blocks.push(Block::ListItem {
                ordered: *ordered,
                marker: marker.clone(),
                spans: taken,
            }),
        }
    }

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                // A quote contains paragraphs; keep the enclosing block so the
                // quote is not flattened into plain paragraphs.
                Tag::Paragraph => {
                    if !matches!(current, Some(Open::Quote)) {
                        current = Some(Open::Paragraph);
                    }
                }
                Tag::Heading { level, .. } => {
                    current = Some(Open::Heading(match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    }))
                }
                Tag::BlockQuote(_) => {
                    flush(&mut blocks, &current, &mut spans);
                    current = Some(Open::Quote);
                }
                Tag::CodeBlock(kind) => {
                    in_code = true;
                    code_text.clear();
                    code_language = match kind {
                        CodeBlockKind::Fenced(info) => {
                            let lang = info.trim().to_owned();
                            (!lang.is_empty()).then_some(lang)
                        }
                        CodeBlockKind::Indented => None,
                    };
                }
                Tag::List(start) => list_stack.push(start),
                Tag::Item => {
                    let marker = match list_stack.last_mut() {
                        Some(Some(next)) => {
                            let marker = format!("{next}.");
                            *next += 1;
                            marker
                        }
                        _ => "•".to_owned(),
                    };
                    current = Some(Open::Item {
                        ordered: matches!(list_stack.last(), Some(Some(_))),
                        marker,
                    });
                }
                Tag::Strong => style.bold = true,
                Tag::Emphasis => style.italic = true,
                Tag::Strikethrough => style.strikethrough = true,
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => {
                    if !matches!(current, Some(Open::Quote)) {
                        flush(&mut blocks, &current, &mut spans);
                        current = None;
                    }
                }
                TagEnd::Heading(_) | TagEnd::Item | TagEnd::BlockQuote(_) => {
                    flush(&mut blocks, &current, &mut spans);
                    current = None;
                }
                TagEnd::List(_) => {
                    list_stack.pop();
                }
                TagEnd::CodeBlock => {
                    in_code = false;
                    blocks.push(Block::Code {
                        language: code_language.take(),
                        text: code_text.trim_end().to_owned(),
                    });
                    code_text.clear();
                }
                TagEnd::Strong => style.bold = false,
                TagEnd::Emphasis => style.italic = false,
                TagEnd::Strikethrough => style.strikethrough = false,
                _ => {}
            },
            Event::Text(text) => {
                if in_code {
                    code_text.push_str(&text);
                } else {
                    spans.push(Span {
                        text: text.into_string(),
                        style,
                    });
                }
            }
            Event::Code(code) => spans.push(Span {
                text: code.into_string(),
                style: Style {
                    code: true,
                    ..style
                },
            }),
            Event::SoftBreak | Event::HardBreak => {
                if in_code {
                    code_text.push('\n');
                } else {
                    spans.push(Span::plain(" "));
                }
            }
            Event::Rule => {
                flush(&mut blocks, &current, &mut spans);
                blocks.push(Block::Rule);
            }
            _ => {}
        }
    }

    flush(&mut blocks, &current, &mut spans);
    blocks
}

/// Where a block sits in the document while it opens.
enum Open {
    Paragraph,
    Heading(u8),
    Quote,
    Item { ordered: bool, marker: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_one_paragraph() {
        let blocks = parse("Hello there");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            Block::Paragraph { spans } => assert_eq!(spans[0].text, "Hello there"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn bold_and_italic_are_styled() {
        let blocks = parse("a **b** and *c*");
        let Block::Paragraph { spans } = &blocks[0] else {
            panic!("expected paragraph");
        };
        assert!(spans.iter().any(|s| s.text == "b" && s.style.bold));
        assert!(spans.iter().any(|s| s.text == "c" && s.style.italic));
    }

    #[test]
    fn inline_code_is_marked() {
        let blocks = parse("use `std::io`");
        let Block::Paragraph { spans } = &blocks[0] else {
            panic!("expected paragraph");
        };
        assert!(spans.iter().any(|s| s.text == "std::io" && s.style.code));
    }

    #[test]
    fn headings_carry_their_level() {
        let blocks = parse("## Setup");
        assert!(matches!(blocks[0], Block::Heading { level: 2, .. }));
    }

    #[test]
    fn fenced_code_keeps_language_and_body() {
        let blocks = parse("```rust\nlet x = 1;\n```");
        match &blocks[0] {
            Block::Code { language, text } => {
                assert_eq!(language.as_deref(), Some("rust"));
                assert_eq!(text, "let x = 1;");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn unterminated_fence_still_renders() {
        // Streaming often stops mid-fence; the text must still be shown.
        let blocks = parse("```rust\nlet x = 1;");
        assert!(matches!(&blocks[0], Block::Code { text, .. } if text.contains("let x = 1;")));
    }

    #[test]
    fn lists_number_and_bullet() {
        let blocks = parse("1. one\n2. two");
        let markers: Vec<&str> = blocks
            .iter()
            .filter_map(|block| match block {
                Block::ListItem {
                    marker, ordered, ..
                } => {
                    assert!(ordered);
                    Some(marker.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(markers, ["1.", "2."]);

        let bullets = parse("- a\n- b");
        let count = bullets
            .iter()
            .filter(|block| matches!(block, Block::ListItem { ordered: false, marker, .. } if marker == "•"))
            .count();
        assert_eq!(count, 2);
    }

    #[test]
    fn quotes_and_rules_parse() {
        assert!(matches!(parse("> quoted")[0], Block::Quote { .. }));
        assert!(parse("---").contains(&Block::Rule));
    }

    #[test]
    fn strikethrough_is_styled() {
        let blocks = parse("~~gone~~");
        let Block::Paragraph { spans } = &blocks[0] else {
            panic!("expected paragraph");
        };
        assert!(
            spans
                .iter()
                .any(|s| s.text == "gone" && s.style.strikethrough)
        );
    }

    #[test]
    fn empty_input_produces_nothing() {
        assert!(parse("").is_empty());
        assert!(parse("   ").is_empty());
    }

    #[test]
    fn partial_inline_markdown_does_not_panic() {
        // Half-typed emphasis mid-stream.
        let blocks = parse("some **bold and then");
        assert!(!blocks.is_empty());
    }
}
