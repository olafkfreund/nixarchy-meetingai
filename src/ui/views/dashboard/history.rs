//! Search state for the archive and the selected record. Editors retain queries
//! independently, so searching a transcript never filters its parent collection.
use super::super::nav::Route;
use super::*;
use crate::ui::widgets::editor::Editor;

pub(super) fn search_slot(route: Route) -> Option<usize> {
    match route {
        Route::Conversations => Some(0),
        Route::Meetings => Some(1),
        Route::ConversationDetail => Some(2),
        Route::MeetingDetail => Some(3),
        _ => None,
    }
}

impl DashboardView {
    pub(super) fn init_history_search(&mut self, cx: &mut Context<Self>) {
        let Some(slot) = search_slot(self.dashboard.route()) else {
            return;
        };
        if self.history_search[slot].is_some() {
            return;
        }
        let editor = cx.new(|cx| self.history_editor(slot, "", cx));
        self.history_observers[slot] = Some(cx.observe(&editor, move |this, editor, cx| {
            let query = editor.read(cx).input.text().to_owned();
            if this.history_queries[slot] == query {
                return;
            }
            this.history_queries[slot] = query;
            if slot < 2 {
                // Debounce disk work during typing; each slot has its own task.
                this.history_tasks[slot] = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(160))
                        .await;
                    let pending = this
                        .update(cx, |this, _| {
                            let path = this
                                .database
                                .as_ref()
                                .and_then(|db| db.connection().path())
                                .map(std::path::PathBuf::from);
                            (path, this.history_queries[slot].clone())
                        })
                        .ok();
                    let Some((Some(path), query)) = pending else {
                        return;
                    };
                    let requested = query.clone();
                    let result = cx
                        .background_executor()
                        .spawn(async move {
                            let db = crate::db::Database::open(path)
                                .map_err(|error| error.to_string())?;
                            if slot == 0 {
                                crate::db::conversations::search(db.connection(), &requested, 200)
                                    .map(PageData::Conversations)
                                    .map_err(|error| error.to_string())
                            } else {
                                crate::db::meetings::search(db.connection(), &requested, 200)
                                    .map(PageData::Meetings)
                                    .map_err(|error| error.to_string())
                            }
                        })
                        .await;
                    this.update(cx, |this, cx| {
                        if search_slot(this.dashboard.route()) == Some(slot)
                            && this.history_queries[slot] == query
                        {
                            this.dashboard.apply_search_result(&query, result);
                            cx.notify();
                        }
                    })
                    .ok();
                }));
            }
            cx.notify();
        }));
        self.history_queries[slot] = editor.read(cx).input.text().to_owned();
        self.history_search[slot] = Some(editor);
    }

    fn history_editor(&self, slot: usize, initial: &str, cx: &mut Context<Editor>) -> Editor {
        let mut editor = Editor::new(initial, self.theme, cx);
        editor.compact = true;
        editor.max_chars = Some(200);
        editor.placeholder = [
            "Search conversations and messages…",
            "Search meetings and transcripts…",
            "Search this conversation…",
            "Search transcript or speaker…",
        ][slot];
        editor
    }

    pub(super) fn history_search_bar(
        &self,
        slot: usize,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .w(self.content_width)
            .min_w(px(0.0))
            .flex_shrink_0()
            .gap(px(10.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .children(self.history_search[slot].clone()),
            )
            .children((!self.history_queries[slot].is_empty()).then(|| {
                action_button(
                    "clear-history-search",
                    &theme,
                    "Clear",
                    cx,
                    move |this, cx, _| {
                        if let Some(editor) = &this.history_search[slot] {
                            editor.update(cx, |editor, cx| {
                                editor.input.clear();
                                cx.notify();
                            });
                        }
                    },
                )
            }))
            .into_any_element()
    }

    pub(super) fn matching_text(&self, slot: usize, fields: &[&str]) -> bool {
        let query = self.history_queries[slot].trim().to_lowercase();
        query.is_empty()
            || fields
                .iter()
                .any(|text| text.to_lowercase().contains(&query))
    }
}
