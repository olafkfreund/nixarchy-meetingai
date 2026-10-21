//! A real text input.
//!
//! GPUI ships no input widget. Typing works only if the view implements
//! [`EntityInputHandler`] and registers itself with [`Window::handle_input`]
//! during paint; key sniffing is not enough because the platform owns the text
//! and IME events.
//!
//! [`Input`] holds the text and cursor and is pure and testable. The
//! `EntityInputHandler` implementation for the overlay lives beside it.

use std::ops::Range;

use gpui::{
    App, Bounds, Context, ElementInputHandler, Entity, EntityInputHandler, Pixels, Point,
    UTF16Selection, Window, px,
};

/// Editable text with a cursor and a selection anchor.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Input {
    text: String,
    /// Cursor position in bytes, always on a character boundary.
    cursor: usize,
    /// Selection anchor; equal to the cursor when nothing is selected.
    anchor: usize,
}

impl Input {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether the content is blank once trimmed.
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = self.text.len();
        self.anchor = self.cursor;
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.anchor = 0;
    }

    /// Take the text, leaving the input empty.
    pub fn take(&mut self) -> String {
        let text = std::mem::take(&mut self.text);
        self.cursor = 0;
        self.anchor = 0;
        text
    }

    /// Insert text at the cursor, replacing any selection.
    pub fn insert(&mut self, value: &str) {
        let range = self.selection();
        self.text.replace_range(range.clone(), value);
        self.cursor = range.start + value.len();
        self.anchor = self.cursor;
    }

    /// Move the cursor one character, optionally extending the selection.
    pub fn move_cursor(&mut self, backwards: bool, extend: bool) {
        let next = if backwards {
            self.text[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(index, _)| index)
                .unwrap_or(0)
        } else {
            self.text[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(index, _)| self.cursor + index)
                .unwrap_or(self.text.len())
        };
        self.cursor = next;
        if !extend {
            self.anchor = next;
        }
    }

    /// Delete backwards, or the selection.
    pub fn backspace(&mut self) {
        if self.cursor != self.anchor {
            self.insert("");
            return;
        }
        if self.cursor == 0 {
            return;
        }
        let previous = self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.text.replace_range(previous..self.cursor, "");
        self.cursor = previous;
        self.anchor = previous;
    }

    /// Delete forwards, or the selection.
    pub fn delete(&mut self) {
        if self.cursor != self.anchor {
            self.insert("");
            return;
        }
        if self.cursor >= self.text.len() {
            return;
        }
        let next = self.text[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(index, _)| self.cursor + index)
            .unwrap_or(self.text.len());
        self.text.replace_range(self.cursor..next, "");
    }

    /// Replace a byte range, used by the platform's editing callbacks.
    pub fn replace_range(&mut self, range: Range<usize>, value: &str) {
        let start = self.clamp_down(range.start);
        // The end rounds up, so a range that cuts a character replaces the whole
        // character rather than stopping before it.
        let end = self.clamp_up(range.end.max(start));
        self.text.replace_range(start..end, value);
        self.cursor = start + value.len();
        self.anchor = self.cursor;
    }

    /// The current selection, ordered.
    pub fn selection(&self) -> Range<usize> {
        if self.anchor <= self.cursor {
            self.anchor..self.cursor
        } else {
            self.cursor..self.anchor
        }
    }

    /// Whether the cursor sits before the anchor.
    pub fn selection_reversed(&self) -> bool {
        self.anchor > self.cursor
    }

    fn clamp_down(&self, index: usize) -> usize {
        let mut index = index.min(self.text.len());
        while index > 0 && !self.text.is_char_boundary(index) {
            index -= 1;
        }
        index
    }

    fn clamp_up(&self, index: usize) -> usize {
        let mut index = index.min(self.text.len());
        while index < self.text.len() && !self.text.is_char_boundary(index) {
            index += 1;
        }
        index
    }
}

/// Ranges to highlight, as byte offsets, for rendering.
///
/// Split out so the selection maths is testable without a window.
pub fn highlight_ranges(input: &Input) -> Vec<(Range<usize>, HighlightKind)> {
    let selection = input.selection();
    let mut ranges = Vec::new();
    if selection.start != selection.end {
        ranges.push((selection.clone(), HighlightKind::Selection));
    }
    // A caret is a zero-width position; drawn as a thin block so it is visible.
    if selection.start == selection.end {
        let caret_end = input.text[input.cursor..]
            .chars()
            .next()
            .map(|c| input.cursor + c.len_utf8())
            .unwrap_or(input.cursor);
        if caret_end > input.cursor {
            ranges.push((input.cursor..caret_end, HighlightKind::Caret));
        }
    }
    ranges
}

/// What a highlighted range means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightKind {
    Selection,
    Caret,
}

/// Render text with its selection and caret drawn.
///
/// Uses per-run background colours, so the highlight sits behind the glyphs the
/// way a text field does rather than as a block painted over them.
pub fn render(
    input: &Input,
    theme: &crate::features::theme::Theme,
    focused: bool,
    placeholder: &str,
) -> gpui::AnyElement {
    use gpui::{
        FontWeight, HighlightStyle, IntoElement, ParentElement, Styled, StyledText, hsla, px,
    };

    if input.text().is_empty() {
        return gpui::div()
            .text_size(px(13.5))
            .text_color(theme.muted_foreground)
            .child(placeholder.to_owned())
            .into_any_element();
    }

    let default_style = gpui::TextStyle {
        color: theme.foreground.into(),
        font_family: gpui::SharedString::default(),
        font_features: Default::default(),
        font_size: px(13.5).into(),
        font_weight: FontWeight::NORMAL,
        font_style: Default::default(),
        line_height: Default::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
        white_space: Default::default(),
        text_align: Default::default(),
        font_fallbacks: None,
        line_clamp: None,
        text_overflow: Default::default(),
    };

    let highlights = if focused {
        highlight_ranges(input)
            .into_iter()
            .map(|(range, kind)| {
                let style = match kind {
                    HighlightKind::Selection => HighlightStyle {
                        background_color: Some(hsla(0.6, 0.6, 0.8, 0.35)),
                        ..Default::default()
                    },
                    HighlightKind::Caret => HighlightStyle {
                        background_color: Some(hsla(0.6, 0.0, 1.0, 0.9)),
                        color: Some(hsla(0.6, 0.0, 0.0, 1.0)),
                        ..Default::default()
                    },
                };
                (range, style)
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    StyledText::new(input.text().to_owned())
        .with_default_highlights(&default_style, highlights)
        .into_any_element()
}

/// Register `view` as the platform's text input for this frame.
///
/// Must be called while painting, after the input has focus; without it the
/// composer is only a picture of text.
pub fn attach_input<V>(
    window: &mut Window,
    focus: &gpui::FocusHandle,
    view: &Entity<V>,
    bounds: Bounds<Pixels>,
    cx: &App,
) where
    V: EntityInputHandler + 'static,
{
    if focus.is_focused(window) {
        window.handle_input(focus, ElementInputHandler::new(bounds, view.clone()), cx);
    }
}

/// A caret width in logical pixels, so callers agree on the glyph.
pub const CARET_WIDTH: Pixels = px(1.0);

/// The handler for an editable buffer, kept separate from the view so it can be
/// tested and reused.
pub struct BufferHandler {
    pub buffer: Input,
    pub marked: Option<Range<usize>>,
}

impl BufferHandler {
    pub fn new() -> Self {
        Self {
            buffer: Input::new(),
            marked: None,
        }
    }
}

impl Default for BufferHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityInputHandler for BufferHandler {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let text = &self.buffer.text;
        let start = range.start.min(text.len());
        let end = range.end.min(text.len());
        *adjusted = Some(start..end);
        text.get(start..end).map(str::to_owned)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.buffer.selection(),
            reversed: self.buffer.selection_reversed(),
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked.clone()
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        match range {
            Some(range) => self.buffer.replace_range(range, text),
            None => self.buffer.insert(text),
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _selected: Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        match range {
            Some(range) => self.buffer.replace_range(range, text),
            None => self.buffer.insert(text),
        }
        self.marked = Some(self.buffer.selection());
    }

    fn bounds_for_range(
        &mut self,
        _range: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(element_bounds)
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.buffer.text.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_inserts_at_the_cursor() {
        let mut input = Input::new();
        input.insert("hel");
        input.insert("lo");
        assert_eq!(input.text(), "hello");
    }

    #[test]
    fn backspace_removes_one_character() {
        let mut input = Input::new();
        input.set_text("hello");
        input.backspace();
        assert_eq!(input.text(), "hell");
        assert_eq!(input.cursor(), 4);
    }

    #[test]
    fn backspace_handles_multibyte_characters() {
        let mut input = Input::new();
        input.set_text("héllo");
        input.backspace();
        assert_eq!(input.text(), "héll");
    }

    #[test]
    fn backspace_at_start_is_a_no_op() {
        let mut input = Input::new();
        input.set_text("abc");
        for _ in 0..5 {
            input.backspace();
        }
        assert_eq!(input.text(), "");
        assert_eq!(input.cursor(), 0);
    }

    #[test]
    fn cursor_moves_over_characters_not_bytes() {
        let mut input = Input::new();
        input.set_text("ab");
        input.cursor = 0;
        input.anchor = 0;
        input.move_cursor(false, false);
        assert_eq!(input.cursor(), 1);
        input.move_cursor(false, false);
        assert_eq!(input.cursor(), 2);
        input.move_cursor(false, false);
        assert_eq!(input.cursor(), 2, "stops at the end");
        input.move_cursor(true, false);
        assert_eq!(input.cursor(), 1);
    }

    #[test]
    fn delete_removes_forward() {
        let mut input = Input::new();
        input.set_text("abc");
        input.cursor = 0;
        input.anchor = 0;
        input.delete();
        assert_eq!(input.text(), "bc");
    }

    #[test]
    fn replace_range_edits_a_slice() {
        let mut input = Input::new();
        input.set_text("hello world");
        input.replace_range(6..11, "there");
        assert_eq!(input.text(), "hello there");
    }

    #[test]
    fn range_replacement_clamps_to_boundaries() {
        let mut input = Input::new();
        input.set_text("héllo");
        // Index 2 sits inside the é byte pair: the whole character is replaced
        // rather than a partial byte slice, which would panic.
        input.replace_range(1..2, "e");
        assert_eq!(input.text(), "hello");
    }

    #[test]
    fn selection_replaces_text() {
        let mut input = Input::new();
        input.set_text("hello world");
        input.cursor = 5;
        input.anchor = 0;
        input.insert("bye");
        assert_eq!(input.text(), "bye world");
    }

    #[test]
    fn take_empties_the_input() {
        let mut input = Input::new();
        input.set_text("question");
        assert_eq!(input.take(), "question");
        assert!(input.text().is_empty());
        assert_eq!(input.cursor(), 0);
    }

    #[test]
    fn a_selection_produces_one_highlight() {
        let mut input = Input::new();
        input.set_text("hello");
        input.anchor = 1;
        input.cursor = 4;
        assert_eq!(
            highlight_ranges(&input),
            vec![(1..4, HighlightKind::Selection)]
        );
    }

    #[test]
    fn a_reversed_selection_is_still_ordered() {
        let mut input = Input::new();
        input.set_text("hello");
        input.anchor = 4;
        input.cursor = 1;
        assert_eq!(
            highlight_ranges(&input),
            vec![(1..4, HighlightKind::Selection)]
        );
    }

    #[test]
    fn a_caret_covers_one_character_forward() {
        let mut input = Input::new();
        input.set_text("hi");
        input.cursor = 0;
        input.anchor = 0;
        assert_eq!(highlight_ranges(&input), vec![(0..1, HighlightKind::Caret)]);
    }

    #[test]
    fn a_caret_at_the_end_has_no_highlight() {
        let mut input = Input::new();
        input.set_text("hi");
        // Cursor sits after the last character, so there is nothing to shade.
        assert!(highlight_ranges(&input).is_empty());
    }

    #[test]
    fn caret_respects_multibyte_widths() {
        let mut input = Input::new();
        input.set_text("é");
        input.cursor = 0;
        input.anchor = 0;
        assert_eq!(highlight_ranges(&input), vec![(0..2, HighlightKind::Caret)]);
    }

    #[test]
    fn is_empty_ignores_whitespace() {
        let mut input = Input::new();
        input.insert("   ");
        assert!(input.is_empty());
        input.insert("x");
        assert!(!input.is_empty());
    }
}
