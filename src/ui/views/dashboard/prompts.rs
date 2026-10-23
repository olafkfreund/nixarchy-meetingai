//! Searchable square prompt cards and an editor for built-in and user prompts.
use super::*;
use crate::{
    db::settings::{self as store, PromptRecord},
    ui::widgets::editor::Editor,
};

pub(super) struct PromptDraft {
    id: Option<i64>,
    name: gpui::Entity<Editor>,
    body: gpui::Entity<Editor>,
}

impl DashboardView {
    pub(super) fn init_prompt_search(&mut self, cx: &mut Context<Self>) {
        let search = cx.new(|cx| {
            let mut editor = Editor::new("", self.theme, cx);
            editor.compact = true;
            editor
        });
        self.prompt_observer = Some(cx.observe(&search, |_, _, cx| cx.notify()));
        self.prompt_search = Some(search);
    }

    fn edit_prompt(&mut self, prompt: Option<PromptRecord>, cx: &mut Context<Self>) {
        self.settings.key_field = None;
        self.prompt_menu = None;
        self.settings.error = None;
        let (id, name, body) = prompt.map(|p| (Some(p.id), p.name, p.prompt)).unwrap_or((
            None,
            String::new(),
            String::new(),
        ));
        self.prompt_edit = Some(PromptDraft {
            id,
            name: cx.new(|cx| {
                let mut editor = Editor::new(&name, self.theme, cx);
                editor.compact = true;
                editor
            }),
            body: cx.new(|cx| Editor::new(&body, self.theme, cx)),
        });
    }

    fn save_prompt(&mut self, cx: &mut Context<Self>) {
        let (Some(db), Some(draft)) = (&self.database, &self.prompt_edit) else {
            return;
        };
        let name = draft.name.read(cx).input.text().trim();
        let body = draft.body.read(cx).input.text().trim();
        if name.is_empty() || body.is_empty() {
            self.settings.error = Some("Enter a name and instructions for this prompt.".into());
            return;
        }
        let result = match draft.id {
            Some(id) => store::update_prompt(db.connection(), id, name, body),
            None => store::create_prompt(db.connection(), name, body).map(|_| ()),
        };
        match result {
            Ok(()) => {
                self.prompt_edit = None;
                self.settings.error = None;
                self.dashboard
                    .navigate(super::super::nav::Route::Prompts, Some(db));
            }
            Err(error) => self.settings.error = Some(error.to_string()),
        }
    }

    pub(super) fn prompts_body(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let mut body = div().flex().flex_col().gap(px(16.0)).w_full().child(self.page_heading("System prompts", "Reusable instructions for interviews, meetings and other conversations. Select a prompt in the overlay."));
        if let Some(error) = &self.settings.error {
            body = body.child(crate::ui::controls::warning(&theme, error));
        }
        if let Some(draft) = &self.prompt_edit {
            return body
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .w_full()
                        .gap(px(10.0))
                        .p(px(18.0))
                        .rounded(px(14.0))
                        .bg(self.surfaces().card())
                        .child(div().text_size(scaled(12.0)).child("Prompt name"))
                        .child(draft.name.clone())
                        .child(div().text_size(scaled(12.0)).child("Instructions"))
                        .child(draft.body.clone())
                        .child(
                            div()
                                .flex()
                                .gap(px(8.0))
                                .child(action_button(
                                    "save-prompt",
                                    &theme,
                                    "Save prompt",
                                    cx,
                                    |this, cx, _| this.save_prompt(cx),
                                ))
                                .child(action_button(
                                    "cancel-prompt",
                                    &theme,
                                    "Cancel",
                                    cx,
                                    |this, _, _| {
                                        this.prompt_edit = None;
                                        this.settings.error = None;
                                    },
                                )),
                        ),
                )
                .into_any_element();
        }
        body = body.child(
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(
                            div()
                                .text_size(scaled(11.0))
                                .text_color(theme.muted_foreground)
                                .child("Search prompts by name or instructions"),
                        )
                        .children(self.prompt_search.clone()),
                )
                .child(action_button(
                    "new-prompt",
                    &theme,
                    "New prompt",
                    cx,
                    |this, cx, _| this.edit_prompt(None, cx),
                )),
        );
        let query = self
            .prompt_search
            .as_ref()
            .map(|search| search.read(cx).input.text().trim().to_lowercase())
            .unwrap_or_default();
        let PageData::Prompts(prompts) = self.dashboard.data() else {
            return body
                .child(self.empty_note("No prompts loaded."))
                .into_any_element();
        };
        let mut grid = div().flex().flex_wrap().gap(px(14.0));
        let mut count = 0;
        for prompt in prompts.iter().filter(|prompt| {
            query.is_empty()
                || prompt.name.to_lowercase().contains(&query)
                || prompt.prompt.to_lowercase().contains(&query)
        }) {
            count += 1;
            let id = prompt.id;
            let mut card = div()
                .relative()
                .w(px(220.0))
                .h(px(220.0))
                .flex()
                .flex_col()
                .gap(px(12.0))
                .p(px(16.0))
                .rounded(px(14.0))
                .border_1()
                .border_color(theme.control_border(false))
                .bg(self.surfaces().card())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .text_size(scaled(14.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(prompt.name.clone()),
                        )
                        .child(action_button(
                            gpui::ElementId::Name(format!("more-prompt-{id}").into()),
                            &theme,
                            "More ⋯",
                            cx,
                            move |this, _, _| {
                                this.prompt_menu = if this.prompt_menu == Some(id) {
                                    None
                                } else {
                                    Some(id)
                                };
                            },
                        )),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.0))
                        .overflow_hidden()
                        .text_size(scaled(12.0))
                        .line_height(scaled(17.0))
                        .text_color(theme.muted_foreground)
                        .whitespace_normal()
                        .child(prompt.prompt.chars().take(190).collect::<String>()),
                );
            if self.prompt_menu == Some(id) {
                let editing = prompt.clone();
                card = card.child(
                    div()
                        .absolute()
                        .top(px(48.0))
                        .right(px(10.0))
                        .p(px(6.0))
                        .flex()
                        .flex_col()
                        .gap(px(5.0))
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.popover())
                        .occlude()
                        .child(action_button(
                            gpui::ElementId::Name(format!("edit-prompt-{id}").into()),
                            &theme,
                            "Edit",
                            cx,
                            move |this, cx, _| this.edit_prompt(Some(editing.clone()), cx),
                        ))
                        .child(action_button(
                            gpui::ElementId::Name(format!("delete-prompt-{id}").into()),
                            &theme,
                            "Delete",
                            cx,
                            move |this, _, _| {
                                if let Some(db) = &this.database {
                                    match store::delete_prompt(db.connection(), id) {
                                        Ok(()) => {
                                            this.prompt_menu = None;
                                            this.dashboard.navigate(
                                                super::super::nav::Route::Prompts,
                                                Some(db),
                                            );
                                        }
                                        Err(error) => this.settings.error = Some(error.to_string()),
                                    }
                                }
                            },
                        )),
                );
            }
            grid = grid.child(card);
        }
        if count == 0 {
            body = body.child(
                self.empty_note("No prompts match. Create a new prompt or change the search."),
            );
        }
        body.child(grid).into_any_element()
    }
}
