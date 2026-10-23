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
    pub dense: bool,
    pub secret: bool,
    pub max_chars: Option<usize>,
    layout: std::rc::Rc<std::cell::RefCell<super::text_input::TextLayout>>,
    scroll: gpui::ScrollHandle,
    selecting: bool,
    caret_dirty: std::rc::Rc<std::cell::Cell<bool>>,
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
            dense: false,
            secret: false,
            max_chars: None,
            layout: Default::default(),
            scroll: gpui::ScrollHandle::new(),
            selecting: false,
            caret_dirty: std::rc::Rc::new(std::cell::Cell::new(true)),
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
        let range = replacement_range(
            self.input.text(),
            range,
            self.marked.as_ref(),
            self.input.selection(),
        );
        // Keep the editor responsive even when a large document is pasted.
        let remaining = self
            .max_chars
            .unwrap_or(if self.compact { 80 } else { 16_000 })
            .saturating_sub(
                self.input.text().chars().count()
                    - self.input.text()[range.clone()].chars().count(),
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
        self.caret_dirty.set(true);
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

fn replacement_range(
    text: &str,
    explicit: Option<Range<usize>>,
    marked: Option<&Range<usize>>,
    selected: Range<usize>,
) -> Range<usize> {
    explicit
        .or_else(|| marked.cloned())
        .map(|range| byte_offset(text, range.start)..byte_offset(text, range.end))
        .unwrap_or(selected)
}

fn visual_index(text: &str, index: usize, secret: bool) -> usize {
    if secret {
        text[..index.min(text.len())].chars().count() * '•'.len_utf8()
    } else {
        index.min(text.len())
    }
}
fn source_index(text: &str, index: usize, secret: bool) -> usize {
    if secret {
        text.char_indices()
            .nth(index / '•'.len_utf8())
            .map(|(byte, _)| byte)
            .unwrap_or(text.len())
    } else {
        index.min(text.len())
    }
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus.clone();
        let entity = cx.entity();
        let focused = focus.is_focused(window);
        let empty = self.input.text().is_empty();
        let value = if self.secret {
            "•".repeat(self.input.text().chars().count())
        } else {
            self.input.text().to_owned()
        };
        let display = if empty {
            self.placeholder.to_owned()
        } else {
            format!("{value}\u{200b}")
        };
        let measure = format!("{value}\u{200b}");
        let visual = |index| visual_index(self.input.text(), index, self.secret);
        let selection = self.input.selection();
        let highlight = (focused && !selection.is_empty()).then_some((
            visual(selection.start)..visual(selection.end),
            gpui::HighlightStyle {
                background_color: Some(Theme::with_alpha(self.theme.foreground, 0.18).into()),
                ..Default::default()
            },
        ));
        let cursor = visual(self.input.cursor());
        let caret = focused && selection.is_empty();
        let layout = self.layout.clone();
        let layout_paint = layout.clone();
        let scroll = self.scroll.clone();
        let dirty = self.caret_dirty.clone();
        let compact = self.compact;
        let color = self.theme.foreground;
        let height = if self.dense { 28.0 } else { 38.0 };
        let padding = if self.dense {
            5.0
        } else if self.compact {
            8.0
        } else {
            12.0
        };
        let line_height = if self.dense { 16.0 } else { 19.0 };
        div()
            .id("multiline-editor")
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .min_h(crate::ui::theme::scaled(if self.compact {
                height
            } else {
                100.0
            }))
            .max_h(crate::ui::theme::scaled(if self.compact {
                height
            } else {
                240.0
            }))
            .flex_shrink_0()
            .when(self.compact, |style| {
                style.h(crate::ui::theme::scaled(height))
            })
            .overflow_hidden()
            .p(crate::ui::theme::scaled(padding))
            .rounded(px(if self.dense { 7.0 } else { 10.0 }))
            .border_1()
            .border_color(if focused {
                self.theme.focus_border()
            } else {
                self.theme.border
            })
            .bg(self.theme.muted)
            .text_color(if empty {
                self.theme.muted_foreground
            } else {
                self.theme.foreground
            })
            .text_size(crate::ui::theme::scaled(if self.dense {
                11.0
            } else {
                12.0
            }))
            .when(self.compact, |style| style.whitespace_nowrap())
            .when(!self.compact, |style| style.whitespace_normal())
            .track_focus(&focus)
            .tab_index(0)
            .cursor_text()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    window.focus(&this.focus);
                    let layout = this.layout.borrow();
                    let index = layout.index(event.position - layout.bounds.origin);
                    drop(layout);
                    if event.click_count >= 2 {
                        this.input.select_all();
                    } else {
                        this.input.set_cursor(
                            source_index(this.input.text(), index, this.secret),
                            event.modifiers.shift,
                        );
                    }
                    this.selecting = true;
                    this.caret_dirty.set(true);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                if this.selecting && event.pressed_button == Some(gpui::MouseButton::Left) {
                    let layout = this.layout.borrow();
                    let index = layout.index(event.position - layout.bounds.origin);
                    drop(layout);
                    this.input
                        .set_cursor(source_index(this.input.text(), index, this.secret), true);
                    this.caret_dirty.set(true);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| this.selecting = false),
            )
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let key = &event.keystroke;
                let command = key.modifiers.control || key.modifiers.platform;
                match key.key.as_str() {
                    "tab" => {
                        if key.modifiers.shift {
                            window.focus_prev();
                        } else {
                            window.focus_next();
                        }
                    }
                    "backspace" => this.input.backspace(),
                    "delete" => this.input.delete(),
                    "left" => this.input.move_cursor(true, key.modifiers.shift),
                    "right" => this.input.move_cursor(false, key.modifiers.shift),
                    "up" | "down" if !this.compact => {
                        let layout = this.layout.borrow();
                        let position = layout.position(visual_index(
                            this.input.text(),
                            this.input.cursor(),
                            this.secret,
                        ));
                        let index = layout.index(gpui::point(
                            position.x,
                            position.y
                                + layout.line_height * if key.key == "up" { -1.0 } else { 1.0 },
                        ));
                        drop(layout);
                        this.input.set_cursor(
                            source_index(this.input.text(), index, this.secret),
                            key.modifiers.shift,
                        );
                    }
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
                this.caret_dirty.set(true);
                cx.stop_propagation();
                cx.notify();
            }))
            .child(
                div()
                    .id("editor-text-scroll")
                    .relative()
                    .w_full()
                    .min_h(crate::ui::theme::scaled(line_height))
                    .max_h(crate::ui::theme::scaled(if compact {
                        line_height
                    } else {
                        216.0
                    }))
                    .overflow_x_scroll()
                    .when(!compact, |s| s.overflow_y_scroll())
                    .track_scroll(&self.scroll)
                    .child(
                        div()
                            .relative()
                            .min_w_full()
                            .flex_shrink_0()
                            .line_height(crate::ui::theme::scaled(line_height))
                            .when(compact, |s| s.whitespace_nowrap())
                            .when(!compact, |s| s.whitespace_normal())
                            .child(gpui::StyledText::new(display).with_highlights(highlight))
                            .child(
                                gpui::canvas(
                                    move |bounds, window, _| {
                                        let style = window.text_style();
                                        let font_size =
                                            style.font_size.to_pixels(window.rem_size());
                                        let run = style.to_run(measure.len());
                                        let lines = window
                                            .text_system()
                                            .shape_text(
                                                measure.clone().into(),
                                                font_size,
                                                &[run],
                                                if compact {
                                                    None
                                                } else {
                                                    Some(bounds.size.width)
                                                },
                                                None,
                                            )
                                            .unwrap_or_default();
                                        *layout.borrow_mut() = super::text_input::TextLayout {
                                            bounds,
                                            lines: lines.into_vec(),
                                            line_height: font_size
                                                * (line_height
                                                    / if compact && line_height == 16.0 {
                                                        11.0
                                                    } else {
                                                        12.0
                                                    }),
                                        };
                                    },
                                    move |bounds, _, window, cx| {
                                        super::text_input::attach_input(
                                            window, &focus, &entity, bounds, cx,
                                        );
                                        let layout = layout_paint.borrow();
                                        let position = bounds.origin + layout.position(cursor);
                                        if caret {
                                            window.paint_quad(gpui::fill(
                                                gpui::Bounds::new(
                                                    position,
                                                    gpui::size(px(1.5), layout.line_height),
                                                ),
                                                color,
                                            ));
                                        }
                                        if dirty.replace(false) {
                                            let visible = scroll.bounds();
                                            let mut offset = scroll.offset();
                                            if position.x < visible.left() {
                                                offset.x += visible.left() - position.x;
                                            } else if position.x + px(3.0) > visible.right() {
                                                offset.x -= position.x + px(3.0) - visible.right();
                                            }
                                            if position.y < visible.top() {
                                                offset.y += visible.top() - position.y;
                                            } else if position.y + layout.line_height
                                                > visible.bottom()
                                            {
                                                offset.y -= position.y + layout.line_height
                                                    - visible.bottom();
                                            }
                                            if offset != scroll.offset() {
                                                scroll.set_offset(offset);
                                                entity.update(cx, |_, cx| cx.notify());
                                            }
                                        }
                                    },
                                )
                                .absolute()
                                .inset_0(),
                            ),
                    ),
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
        let range = range.or_else(|| self.marked.clone());
        let start = range
            .as_ref()
            .map(|r| r.start)
            .unwrap_or_else(|| self.units(self.input.selection().start));
        self.edit(range, text, cx);
        self.marked = Some(
            start
                ..(start + text.encode_utf16().count())
                    .min(self.input.text().encode_utf16().count()),
        );
        if let Some(selected) = selected {
            let end = byte_offset(self.input.text(), start + selected.end);
            let begin = byte_offset(self.input.text(), start + selected.start);
            self.input.set_cursor(begin, false);
            self.input.set_cursor(end, true);
        }
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: gpui::Bounds<gpui::Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<gpui::Bounds<gpui::Pixels>> {
        let layout = self.layout.borrow();
        let start = layout.bounds.origin
            + layout.position(visual_index(
                self.input.text(),
                self.byte(range.start),
                self.secret,
            ));
        let end = layout.bounds.origin
            + layout.position(visual_index(
                self.input.text(),
                self.byte(range.end),
                self.secret,
            ));
        Some(gpui::Bounds::new(
            start,
            gpui::size((end.x - start.x).max(px(1.0)), layout.line_height),
        ))
    }
    fn character_index_for_point(
        &mut self,
        point: gpui::Point<gpui::Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let layout = self.layout.borrow();
        let index = layout.index(point - layout.bounds.origin);
        Some(self.units(source_index(self.input.text(), index, self.secret)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ime_commit_replaces_marked_unicode_without_appending_duplicate_text() {
        let mut input = Input::new();
        input.set_text("x\u{1f600}y");
        let range = replacement_range(input.text(), None, Some(&(1..3)), input.selection());
        input.replace_range(range, "\u{e9}");
        assert_eq!(input.text(), "x\u{e9}y");
        assert_eq!(
            replacement_range(input.text(), Some(0..1), Some(&(1..2)), input.selection()),
            0..1
        );
    }
    #[test]
    fn masked_cursor_offsets_round_trip_at_unicode_boundaries() {
        let text = "k\u{e9}\u{1f600}-secret";
        for index in text
            .char_indices()
            .map(|(index, _)| index)
            .chain(std::iter::once(text.len()))
        {
            assert_eq!(
                source_index(text, visual_index(text, index, true), true),
                index
            );
        }
        assert_eq!(source_index(text, 99999, true), text.len());
    }
    #[test]
    fn utf16_ranges_handle_emoji_and_accents() {
        assert_eq!(byte_offset("é😀x", 0), 0);
        assert_eq!(byte_offset("é😀x", 1), 2);
        assert_eq!(byte_offset("é😀x", 2), 2);
        assert_eq!(byte_offset("é😀x", 3), 6);
        assert_eq!(byte_offset("é😀x", 100), 7);
    }
}
