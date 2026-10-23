//! Multiline native text editor used for meeting context and editable prompts.
use super::text_input::Input;
use crate::ui::theme::Theme;
use gpui::prelude::FluentBuilder;
use gpui::{
    Context, EntityInputHandler, FocusHandle, InteractiveElement, IntoElement, ParentElement,
    Render, StatefulInteractiveElement, Styled, UTF16Selection, Window, div, px,
};
use std::ops::Range;

pub struct Editor {
    pub compact: bool,
    pub input: Input,
    pub theme: Theme,
    pub placeholder: &'static str,
    focus: FocusHandle,
    marked: Option<Range<usize>>,
}

impl Editor {
    pub fn sync_theme(editor: &gpui::Entity<Self>, theme: Theme, cx: &mut gpui::App) {
        if editor.read(cx).theme != theme {
            editor.update(cx, |editor, cx| {
                editor.theme = theme;
                cx.notify();
            });
        }
    }
    pub fn focus(&self, window: &mut Window) {
        window.focus(&self.focus);
    }
    pub fn new(text: &str, theme: Theme, cx: &mut Context<Self>) -> Self {
        let mut input = Input::new();
        input.set_text(text);
        Self {
            compact: false,
            input,
            theme,
            placeholder: "Click to write or paste…",
            focus: cx.focus_handle(),
            marked: None,
        }
    }
    fn byte(&self, utf16: usize) -> usize {
        byte_offset(self.input.text(), utf16)
    }
    fn range(&self, range: Range<usize>) -> Range<usize> {
        self.byte(range.start)..self.byte(range.end)
    }
    fn units(&self, byte: usize) -> usize {
        self.input.text()[..byte].encode_utf16().count()
    }
    fn edit(&mut self, range: Option<Range<usize>>, text: &str, cx: &mut Context<Self>) {
        let range = range
            .map(|r| self.range(r))
            .unwrap_or_else(|| self.input.selection());
        // Keep the editor responsive even when a large document is pasted.
        let remaining = (if self.compact { 80usize } else { 16_000usize }).saturating_sub(
            self.input.text().chars().count() - self.input.text()[range.clone()].chars().count(),
        );
        let text = text
            .chars()
            .take(remaining)
            .map(|ch| {
                if self.compact && matches!(ch, '\n' | '\r' | '\t') {
                    ' '
                } else {
                    ch
                }
            })
            .collect::<String>();
        self.input.replace_range(range, &text);
        cx.notify();
    }
}

fn byte_offset(text: &str, units: usize) -> usize {
    let mut count = 0;
    for (byte, ch) in text.char_indices() {
        if count + ch.len_utf16() > units {
            return byte;
        }
        count += ch.len_utf16();
        if count == units {
            return byte + ch.len_utf8();
        }
    }
    text.len()
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus.clone();
        let entity = cx.entity();
        let focused = focus.is_focused(window);
        let mut display = self.input.text().to_owned();
        if focused {
            display.insert(self.input.cursor(), '│');
        }
        if display.is_empty() {
            display = self.placeholder.into();
        }
        div()
            .id("multiline-editor")
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .min_h(px(if self.compact { 38.0 } else { 100.0 }))
            .max_h(px(if self.compact { 38.0 } else { 240.0 }))
            .flex_shrink_0()
            .when(self.compact, |style| style.overflow_hidden().h(px(38.0)))
            .when(!self.compact, |style| style.overflow_y_scroll())
            .p(px(if self.compact { 8.0 } else { 12.0 }))
            .rounded(px(10.0))
            .border_1()
            .border_color(if focused {
                self.theme.focus_border()
            } else {
                self.theme.border
            })
            .bg(self.theme.muted)
            .text_color(self.theme.foreground)
            .text_size(crate::ui::theme::scaled(12.0))
            .when(self.compact, |style| style.whitespace_nowrap())
            .when(!self.compact, |style| style.whitespace_normal())
            .track_focus(&focus)
            .cursor_text()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.focus(&this.focus);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                let key = &event.keystroke;
                let command = key.modifiers.control || key.modifiers.platform;
                match key.key.as_str() {
                    "backspace" => this.input.backspace(),
                    "delete" => this.input.delete(),
                    "left" => this.input.move_cursor(true, key.modifiers.shift),
                    "right" => this.input.move_cursor(false, key.modifiers.shift),
                    "home" => this.input.move_to_edge(false, key.modifiers.shift),
                    "end" => this.input.move_to_edge(true, key.modifiers.shift),
                    "enter" => {
                        if !this.compact {
                            this.edit(None, "\n", cx);
                        }
                    }
                    "a" if command => this.input.select_all(),
                    "v" if command => {
                        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                            this.edit(None, &text, cx);
                        }
                    }
                    "c" | "x" if command => {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                            this.input.text()[this.input.selection()].to_owned(),
                        ));
                        if key.key == "x" {
                            this.input.insert("");
                        }
                    }
                    _ => return,
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .line_height(px(19.0))
                    .when(self.compact, |style| style.whitespace_nowrap())
                    .when(!self.compact, |style| style.whitespace_normal())
                    .child(display),
            )
            .child(
                gpui::canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        super::text_input::attach_input(window, &focus, &entity, bounds, cx);
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
    }
}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range(range);
        *adjusted = Some(self.units(range.start)..self.units(range.end));
        Some(self.input.text()[range].to_owned())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let range = self.input.selection();
        Some(UTF16Selection {
            range: self.units(range.start)..self.units(range.end),
            reversed: self.input.selection_reversed(),
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.clone()
    }
    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(range, text, cx);
        self.marked = None;
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let start = range
            .as_ref()
            .map(|r| r.start)
            .unwrap_or_else(|| self.units(self.input.selection().start));
        self.edit(range, text, cx);
        self.marked = Some(start..start + text.encode_utf16().count());
        let _ = selected;
    }
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        bounds: gpui::Bounds<gpui::Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<gpui::Bounds<gpui::Pixels>> {
        Some(bounds)
    }
    fn character_index_for_point(
        &mut self,
        _: gpui::Point<gpui::Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.units(self.input.cursor()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf16_ranges_handle_emoji_and_accents() {
        assert_eq!(byte_offset("é😀x", 0), 0);
        assert_eq!(byte_offset("é😀x", 1), 2);
        assert_eq!(byte_offset("é😀x", 2), 2);
        assert_eq!(byte_offset("é😀x", 3), 6);
        assert_eq!(byte_offset("é😀x", 100), 7);
    }
}
