//! Growing composer, paste references and native text/IME input.
use super::*;
use gpui::{EntityInputHandler, UTF16Selection};
use std::ops::Range;

#[derive(Default)]
pub(super) struct Layout {
    pub bounds: gpui::Bounds<gpui::Pixels>,
    pub lines: Vec<gpui::WrappedLine>,
    pub line_height: gpui::Pixels,
}
impl Layout {
    pub fn position(&self, index: usize) -> gpui::Point<gpui::Pixels> {
        let mut start = 0;
        let mut y = px(0.0);
        for line in &self.lines {
            if index <= start + line.len() {
                let mut p = line
                    .position_for_index(index.saturating_sub(start), self.line_height)
                    .unwrap_or_default();
                p.y += y;
                return p;
            }
            start += line.len() + 1;
            y += line.size(self.line_height).height;
        }
        gpui::point(px(0.0), y)
    }
    pub fn index(&self, position: gpui::Point<gpui::Pixels>) -> usize {
        let mut start = 0;
        let mut y = px(0.0);
        for line in &self.lines {
            let height = line.size(self.line_height).height;
            if position.y < y + height {
                return start
                    + line
                        .closest_index_for_position(
                            gpui::point(position.x, (position.y - y).max(px(0.0))),
                            self.line_height,
                        )
                        .unwrap_or_else(|i| i);
            }
            start += line.len() + 1;
            y += height;
        }
        start.saturating_sub(1)
    }
}

pub(super) fn byte(text: &str, units: usize) -> usize {
    let mut count = 0;
    for (offset, ch) in text.char_indices() {
        if count + ch.len_utf16() > units {
            return offset;
        }
        count += ch.len_utf16();
        if count == units {
            return offset + ch.len_utf8();
        }
    }
    text.len()
}

impl Overlay {
    pub(super) fn paste_composer(&mut self, text: &str) {
        if crate::core::composer::should_attach(text) {
            match crate::core::composer::attach(&mut self.draft_attachments, text) {
                Ok(()) => {
                    self.composer.insert("");
                    self.chat_error = None;
                }
                Err(error) => self.chat_error = Some(error),
            }
        } else if self.composer.text().chars().count() + text.chars().count()
            <= crate::core::composer::MAX_PASTE_CHARS
        {
            self.composer.insert(text);
        } else {
            self.chat_error =
                Some("The input is full. Attach the text or remove some content first.".into());
        }
        self.composer_changed();
    }
    pub(super) fn composer_changed(&mut self) {
        if (self.composer.text().lines().count() >= 30
            || self.composer.text().chars().count() >= 4_000)
            && !self.composer.text().is_empty()
        {
            match crate::core::composer::attach(&mut self.draft_attachments, self.composer.text()) {
                Ok(()) => self.composer.clear(),
                Err(error) => self.chat_error = Some(error),
            }
        }
        self.composer_caret_dirty.set(true);
        self.caret_visible = true;
        self.conversation.set_draft(self.composer.text());
    }
    pub(super) fn composer_field(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let text = self.composer.text().to_owned();
        let display = if text.is_empty() {
            "Session guidance, notes or a question…".to_owned()
        } else {
            format!("{text}\u{200b}")
        };
        let selection = self.composer.selection();
        let highlight = (!selection.is_empty()).then_some((
            selection,
            gpui::HighlightStyle {
                background_color: Some(Theme::with_alpha(self.theme.foreground, 0.16).into()),
                ..Default::default()
            },
        ));
        let layout = self.composer_layout.clone();
        let layout_paint = layout.clone();
        let scroll = self.composer_scroll.clone();
        let dirty = self.composer_caret_dirty.clone();
        let focus = self.focus_handle(cx);
        let entity = cx.entity();
        let caret =
            self.composer_focused && self.caret_visible && self.composer.selection().is_empty();
        let cursor = self.composer.cursor();
        let color = self.theme.foreground;
        let measure = format!("{text}\u{200b}");
        div()
            .flex_1()
            .min_w(px(0.0))
            .py(px(5.0))
            .child(
                div()
                    .id("composer-text-scroll")
                    .min_h(scaled(22.0))
                    .max_h(scaled(110.0))
                    .overflow_y_scroll()
                    .track_scroll(&self.composer_scroll)
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                            window.focus(&this.focus_handle(cx));
                            let layout = this.composer_layout.borrow();
                            let index = layout.index(event.position - layout.bounds.origin);
                            drop(layout);
                            this.composer.set_cursor(index, event.modifiers.shift);
                            this.composer_selecting = true;
                            this.composer_changed();
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                        if this.composer_selecting
                            && event.pressed_button == Some(gpui::MouseButton::Left)
                        {
                            let l = this.composer_layout.borrow();
                            let index = l.index(event.position - l.bounds.origin);
                            drop(l);
                            this.composer.set_cursor(index, true);
                            this.composer_changed();
                            cx.notify();
                        }
                    }))
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .min_h(scaled(22.0))
                            .text_size(scaled(13.5))
                            .line_height(scaled(22.0))
                            .whitespace_normal()
                            .text_color(if text.is_empty() {
                                self.theme.muted_foreground
                            } else {
                                self.theme.foreground
                            })
                            .child(gpui::StyledText::new(display).with_highlights(highlight))
                            .child(
                                gpui::canvas(
                                    move |bounds, window, _| {
                                        let style = window.text_style();
                                        let font_size =
                                            style.font_size.to_pixels(window.rem_size());
                                        let line_height = font_size * (22.0 / 13.5);
                                        let run = style.to_run(measure.len());
                                        let lines = window
                                            .text_system()
                                            .shape_text(
                                                measure.clone().into(),
                                                font_size,
                                                &[run],
                                                Some(bounds.size.width),
                                                None,
                                            )
                                            .unwrap_or_default();
                                        *layout.borrow_mut() = Layout {
                                            bounds,
                                            lines: lines.into_vec(),
                                            line_height,
                                        };
                                    },
                                    move |bounds, _, window, cx| {
                                        crate::ui::widgets::text_input::attach_input(
                                            window, &focus, &entity, bounds, cx,
                                        );
                                        let layout = layout_paint.borrow();
                                        let position = bounds.origin + layout.position(cursor);
                                        if caret {
                                            window.paint_quad(gpui::fill(
                                                gpui::Bounds::new(
                                                    position + gpui::point(px(0.0), px(2.0)),
                                                    gpui::size(
                                                        px(1.5),
                                                        layout.line_height - px(4.0),
                                                    ),
                                                ),
                                                color,
                                            ));
                                        }
                                        if dirty.replace(false) {
                                            let visible = scroll.bounds();
                                            let mut offset = scroll.offset();
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
            .into_any_element()
    }

    pub(super) fn draft_attachment_tray(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if self.draft_attachments.is_empty() {
            return None;
        }
        let mut tray = div().flex().flex_wrap().gap(px(8.0));
        for item in &self.draft_attachments {
            let preview = item.clone();
            let id = item.id.clone();
            let card = div()
                .id(gpui::ElementId::Name(format!("paste-{}", item.id).into()))
                .relative()
                .size(px(74.0))
                .flex_shrink_0()
                .rounded(px(13.0))
                .border_1()
                .border_color(self.theme.control_border(false))
                .bg(self.surfaces().card())
                .p(px(8.0))
                .flex()
                .flex_col()
                .gap(px(5.0))
                .cursor_pointer()
                .hover(|s| s.bg(self.theme.menu_hover()))
                .child(icons::icon(
                    &self.theme,
                    if item.code {
                        Icon::Code
                    } else {
                        Icon::FileText
                    },
                    18.0,
                ))
                .child(div().text_size(scaled(9.0)).truncate().child(if item.code {
                    "Code"
                } else {
                    "Text"
                }))
                .child(
                    div()
                        .text_size(scaled(9.0))
                        .text_color(self.theme.muted_foreground)
                        .child(format!("{} lines", item.text.lines().count().max(1))),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.show_dialog(super::session_menu::Dialog::Text(preview.clone()));
                    cx.notify();
                }))
                .child(
                    div()
                        .id(gpui::ElementId::Name(
                            format!("remove-paste-{}", item.id).into(),
                        ))
                        .absolute()
                        .top(px(4.0))
                        .right(px(4.0))
                        .size(px(18.0))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(self.surfaces().muted())
                        .cursor_pointer()
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(icons::icon(&self.theme, Icon::X, 10.0))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.draft_attachments.retain(|i| i.id != id);
                            cx.stop_propagation();
                            cx.notify();
                        })),
                );
            tray = tray.child(div().w(px(74.0)).flex_shrink_0().child(
                crate::ui::widgets::motion::reveal(
                    card,
                    gpui::ElementId::Name(format!("paste-enter-{}", item.id).into()),
                    self.runtime_settings.reduce_motion,
                    true,
                ),
            ));
        }
        // Each animation wrapper has its own fixed square measure.
        Some(tray.into_any_element())
    }
}

impl EntityInputHandler for Overlay {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = byte(self.composer.text(), range.start)..byte(self.composer.text(), range.end);
        *adjusted = Some(
            self.composer.text()[..range.start].encode_utf16().count()
                ..self.composer.text()[..range.end].encode_utf16().count(),
        );
        Some(self.composer.text()[range].to_owned())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let r = self.composer.selection();
        Some(UTF16Selection {
            range: self.composer.text()[..r.start].encode_utf16().count()
                ..self.composer.text()[..r.end].encode_utf16().count(),
            reversed: self.composer.selection_reversed(),
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.composer_marked.clone()
    }
    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.composer_marked = None;
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dialog_open || self.model_menu_open {
            return;
        }
        if crate::core::composer::should_attach(text) {
            self.paste_composer(text);
        } else {
            let range = range
                .or_else(|| self.composer_marked.take())
                .map(|r| byte(self.composer.text(), r.start)..byte(self.composer.text(), r.end))
                .unwrap_or_else(|| self.composer.selection());
            if self.composer.text().chars().count() + text.chars().count()
                <= crate::core::composer::MAX_PASTE_CHARS
            {
                self.composer.replace_range(range, text);
                self.composer_changed();
            }
        }
        self.composer_marked = None;
        cx.notify();
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .or_else(|| self.composer_marked.clone())
            .unwrap_or_else(|| {
                let r = self.composer.selection();
                self.composer.text()[..r.start].encode_utf16().count()
                    ..self.composer.text()[..r.end].encode_utf16().count()
            });
        self.composer.replace_range(
            byte(self.composer.text(), range.start)..byte(self.composer.text(), range.end),
            text,
        );
        self.composer_marked = Some(range.start..range.start + text.encode_utf16().count());
        if let Some(r) = selected {
            self.composer
                .set_cursor(byte(self.composer.text(), range.start + r.start), false);
            self.composer
                .set_cursor(byte(self.composer.text(), range.start + r.end), true);
        }
        self.composer_changed();
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: gpui::Bounds<gpui::Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<gpui::Bounds<gpui::Pixels>> {
        let l = self.composer_layout.borrow();
        Some(gpui::Bounds::new(
            l.bounds.origin + l.position(byte(self.composer.text(), range.start)),
            gpui::size(px(1.5), l.line_height),
        ))
    }
    fn character_index_for_point(
        &mut self,
        point: gpui::Point<gpui::Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let l = self.composer_layout.borrow();
        let index = l
            .index(point - l.bounds.origin)
            .min(self.composer.text().len());
        Some(self.composer.text()[..index].encode_utf16().count())
    }
}
