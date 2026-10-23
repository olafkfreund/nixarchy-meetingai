//! Searchable profile cards and a focused editor for reusable conversation context.
use super::*;
use crate::db::profiles::{self as store, Profile};
use crate::ui::widgets::editor::Editor;

pub(super) struct ProfileDraft {
    id: String,
    is_new: bool,
    name: gpui::Entity<Editor>,
    personal: gpui::Entity<Editor>,
    meeting: gpui::Entity<Editor>,
}

impl ProfileDraft {
    pub(super) fn sync_theme(&self, theme: Theme, cx: &mut gpui::App) {
        for editor in [&self.name, &self.personal, &self.meeting] {
            Editor::sync_theme(editor, theme, cx);
        }
    }
}

impl DashboardView {
    pub(super) fn init_profile_search(&mut self, cx: &mut Context<Self>) {
        let search = cx.new(|cx| {
            let mut editor = Editor::new("", self.theme, cx);
            editor.compact = true;
            editor.placeholder = "Search profiles...";
            editor
        });
        self.profile_observer = Some(cx.observe(&search, |this, _, cx| {
            this.profile_delete = None;
            cx.notify();
        }));
        self.profile_search = Some(search);
    }

    fn edit_profile(
        &mut self,
        profile: Option<Profile>,
        cx: &mut Context<Self>,
        window: &mut Window,
    ) {
        let is_new = profile.is_none();
        let profile = profile.unwrap_or(Profile {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            personal_context: String::new(),
            meeting_context: String::new(),
        });
        self.profile_edit = Some(ProfileDraft {
            id: profile.id,
            is_new,
            name: cx.new(|cx| {
                let mut editor = Editor::new(&profile.name, self.theme, cx);
                editor.compact = true;
                editor.placeholder = "For example: Product interview";
                editor
            }),
            personal: cx.new(|cx| {
                let mut editor = Editor::new(&profile.personal_context, self.theme, cx);
                editor.placeholder = "Your role, experience, strengths and useful background...";
                editor
            }),
            meeting: cx.new(|cx| {
                let mut editor = Editor::new(&profile.meeting_context, self.theme, cx);
                editor.placeholder =
                    "Who you are meeting, the topic, goals and details to remember...";
                editor
            }),
        });
        if let Some(draft) = &self.profile_edit {
            draft.name.read(cx).focus(window);
        }
        self.profile_delete = None;
        self.settings.error = None;
    }

    fn save_profile(&mut self, cx: &mut Context<Self>) {
        let Some(db) = &self.database else {
            self.settings.error = Some("The local database is unavailable.".into());
            return;
        };
        let Some(draft) = &self.profile_edit else {
            return;
        };
        let profile = Profile {
            id: draft.id.clone(),
            name: draft.name.read(cx).input.text().into(),
            personal_context: draft.personal.read(cx).input.text().into(),
            meeting_context: draft.meeting.read(cx).input.text().into(),
        };
        match store::save(db.connection(), &profile) {
            Ok(()) => {
                self.profile_edit = None;
                self.settings.error = None;
                self.dashboard
                    .navigate(super::super::nav::Route::Profiles, Some(db));
                if let Some(search) = &self.profile_search {
                    search.update(cx, |editor, cx| {
                        editor.input.set_text("");
                        cx.notify();
                    });
                }
            }
            Err(error) => self.settings.error = Some(error),
        }
    }

    fn delete_profile(&mut self, id: &str) {
        let Some(db) = &self.database else {
            self.settings.error = Some("The local database is unavailable.".into());
            return;
        };
        match store::delete(db.connection(), id) {
            Ok(()) => {
                self.profile_delete = None;
                self.settings.error = None;
                self.dashboard
                    .navigate(super::super::nav::Route::Profiles, Some(db));
            }
            Err(error) => self.settings.error = Some(error.to_string()),
        }
    }

    pub(super) fn profiles_body(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let surfaces = self.surfaces();
        let available = (f32::from(window.viewport_size().width) - 48.0).clamp(300.0, 960.0);
        let columns = if available >= 900.0 {
            3
        } else if available >= 700.0 {
            2
        } else {
            1
        };
        let mut body = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w(px(0.0))
            .gap(px(20.0))
            .child(self.page_heading(
                "Profiles",
                "Keep useful background ready for your next conversation.",
            ));
        if let Some(error) = &self.settings.error {
            body = body.child(crate::ui::controls::warning(&theme, error));
        }
        if let Some(draft) = &self.profile_edit {
            let mut contexts = div().flex().w_full().min_w(px(0.0)).gap(px(16.0));
            if available < 820.0 {
                contexts = contexts.flex_col();
            }
            for (icon, title, subtitle, editor) in [
                (
                    Icon::UserRound,
                    "About you",
                    "Background that helps the AI understand your perspective.",
                    &draft.personal,
                ),
                (
                    Icon::MessagesSquare,
                    "Meeting context",
                    "People, objectives and details for this conversation.",
                    &draft.meeting,
                ),
            ] {
                contexts = contexts.child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w(px(0.0))
                        .gap(px(10.0))
                        .p(px(16.0))
                        .rounded(px(14.0))
                        .border_1()
                        .border_color(theme.control_border(false))
                        .bg(surfaces.card())
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(icons::icon(&theme, icon, 15.0))
                                .child(
                                    div()
                                        .text_size(scaled(13.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(title),
                                ),
                        )
                        .child(
                            div()
                                .text_size(scaled(11.0))
                                .text_color(theme.muted_foreground)
                                .child(subtitle),
                        )
                        .child(editor.clone()),
                );
            }
            let form = div().flex().flex_col().w_full().min_w(px(0.0)).gap(px(18.0))
                .child(div().flex().items_center().justify_between().gap(px(12.0))
                    .child(div().text_size(scaled(15.0)).font_weight(FontWeight::SEMIBOLD)
                        .child(if draft.is_new { "New profile" } else { "Edit profile" }))
                    .child(div().flex().items_center().gap(px(8.0))
                        .child(action_button("cancel-profile", &theme, "Cancel", cx, |this, _, _| {
                            this.profile_edit = None;
                            this.settings.error = None;
                        }))
                        .child(primary("save-profile", "Save profile", Icon::Check, theme, cx,
                            |this, cx, _| this.save_profile(cx)))))
                .child(div().flex().flex_col().gap(px(8.0)).w_full().max_w(px(420.0))
                    .child(div().text_size(scaled(12.0)).font_weight(FontWeight::MEDIUM).child("Profile name"))
                    .child(draft.name.clone()))
                .child(contexts)
                .child(div().flex().items_start().gap(px(8.0)).pb(px(12.0))
                    .child(icons::icon(&theme, Icon::Info, 13.0))
                    .child(div().text_size(scaled(11.0)).text_color(theme.muted_foreground).whitespace_normal()
                        .child("Saved on this computer. Select a profile to include it in AI requests. Edits apply to the next answer; files and transcripts remain separate.")));
            return body
                .child(crate::ui::widgets::motion::reveal(
                    form,
                    "profile-editor-open",
                    self.settings.settings.reduce_motion,
                    false,
                ))
                .into_any_element();
        }
        let selected = self
            .database
            .as_ref()
            .and_then(|db| store::selected(db.connection()).ok().flatten());
        body = body
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .gap(px(12.0))
                    .child(
                        div()
                            .w(px(360.0))
                            .max_w_full()
                            .min_w(px(0.0))
                            .children(self.profile_search.clone()),
                    )
                    .child(primary(
                        "create-profile",
                        "New profile",
                        Icon::Plus,
                        theme,
                        cx,
                        |this, cx, window| this.edit_profile(None, cx, window),
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .p(px(14.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(theme.control_border(false))
                    .bg(surfaces.input())
                    .child(icons::icon(&theme, Icon::UserRound, 18.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                div()
                                    .text_size(scaled(10.5))
                                    .text_color(theme.muted_foreground)
                                    .child("Selected context"),
                            )
                            .child(
                                div().text_size(scaled(12.0)).truncate().child(
                                    selected
                                        .as_ref()
                                        .map(|p| p.name.clone())
                                        .unwrap_or_else(|| "No context".into()),
                                ),
                            ),
                    )
                    .children(selected.as_ref().map(|_| {
                        action_button(
                            "clear-profile-selection",
                            &theme,
                            "Use no context",
                            cx,
                            |this, _, _| {
                                if let Some(db) = &this.database {
                                    this.settings.error =
                                        store::select(db.connection(), None).err();
                                }
                            },
                        )
                    })),
            );
        let query = self
            .profile_search
            .as_ref()
            .map(|search| search.read(cx).input.text().trim().to_lowercase())
            .unwrap_or_default();
        let PageData::Profiles(profiles) = self.dashboard.data() else {
            return body
                .child(self.collection_empty(
                    Icon::UserRound,
                    "Your profiles will appear here",
                    "Add background once and reuse it in interviews, meetings and chats.",
                ))
                .into_any_element();
        };
        let matching = profiles
            .iter()
            .filter(|profile| {
                let text = format!(
                    "{} {} {}",
                    profile.name, profile.personal_context, profile.meeting_context
                )
                .to_lowercase();
                query.split_whitespace().all(|word| text.contains(word))
            })
            .collect::<Vec<_>>();
        if matching.is_empty() {
            return body
                .child(if profiles.is_empty() {
                    self.collection_empty(
                        Icon::UserRound,
                        "Your profiles will appear here",
                        "Create your first profile, then choose it in the overlay.",
                    )
                } else {
                    self.collection_empty(
                        Icon::Search,
                        "No matching profiles",
                        "Try a name, role or topic from your saved context.",
                    )
                })
                .into_any_element();
        }
        body = body.child(
            div()
                .text_size(scaled(11.0))
                .text_color(theme.muted_foreground)
                .child(format!(
                    "{} {}",
                    matching.len(),
                    if matching.len() == 1 {
                        "profile"
                    } else {
                        "profiles"
                    }
                )),
        );
        let width = (available - (columns - 1) as f32 * 14.0) / columns as f32;
        let mut grid = div().flex().flex_wrap().w_full().gap(px(14.0));
        for profile in matching {
            let active = selected
                .as_ref()
                .is_some_and(|selected| selected.id == profile.id);
            let editing = profile.clone();
            let using = profile.id.clone();
            let deleting = profile.id.clone();
            let confirming = self.profile_delete.as_ref() == Some(&profile.id);
            let mut card = div()
                .flex()
                .flex_col()
                .w(px(width))
                .h(scaled(300.0))
                .min_h(px(300.0))
                .min_w(px(0.0))
                .gap(px(12.0))
                .p(px(16.0))
                .rounded(px(14.0))
                .border_1()
                .border_color(theme.control_border(active))
                .bg(surfaces.card())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(30.0))
                                .rounded(px(9.0))
                                .bg(surfaces.muted())
                                .child(icons::icon(&theme, Icon::UserRound, 15.0)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .text_size(scaled(13.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .truncate()
                                .child(profile.name.clone()),
                        )
                        .children(active.then(|| {
                            div()
                                .flex_shrink_0()
                                .text_size(scaled(10.0))
                                .text_color(theme.muted_foreground)
                                .child("Selected")
                        })),
                )
                .child(context_preview(
                    theme,
                    "About you",
                    &profile.personal_context,
                ))
                .child(context_preview(
                    theme,
                    "Meeting context",
                    &profile.meeting_context,
                ))
                .child(div().flex_1().min_h(px(0.0)));
            if confirming {
                card = card.child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .gap(px(7.0))
                        .child(div().text_size(scaled(11.0)).child("Delete this profile?"))
                        .child(
                            div()
                                .flex()
                                .gap(px(6.0))
                                .child(action_button(
                                    gpui::ElementId::Name(
                                        format!("confirm-profile-{deleting}").into(),
                                    ),
                                    &theme,
                                    "Delete",
                                    cx,
                                    move |this, _, _| this.delete_profile(&deleting),
                                ))
                                .child(action_button(
                                    "cancel-profile-delete",
                                    &theme,
                                    "Cancel",
                                    cx,
                                    |this, _, _| this.profile_delete = None,
                                )),
                        ),
                );
            } else {
                card = card.child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .justify_between()
                        .gap(px(6.0))
                        .child(action_button(
                            gpui::ElementId::Name(format!("use-profile-{using}").into()),
                            &theme,
                            if active { "Deselect" } else { "Use profile" },
                            cx,
                            move |this, _, _| {
                                this.profile_delete = None;
                                if let Some(db) = &this.database {
                                    this.settings.error = store::select(
                                        db.connection(),
                                        if active { None } else { Some(&using) },
                                    )
                                    .err();
                                }
                            },
                        ))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(action_button(
                                    gpui::ElementId::Name(
                                        format!("edit-profile-{}", profile.id).into(),
                                    ),
                                    &theme,
                                    "Edit",
                                    cx,
                                    move |this, cx, window| {
                                        this.edit_profile(Some(editing.clone()), cx, window)
                                    },
                                ))
                                .child(
                                    div()
                                        .id(gpui::ElementId::Name(
                                            format!("delete-profile-{deleting}").into(),
                                        ))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .size(px(26.0))
                                        .flex_shrink_0()
                                        .rounded(px(7.0))
                                        .cursor_pointer()
                                        .hover(|style| style.bg(theme.menu_hover()))
                                        .child(icons::icon(&theme, Icon::Trash2, 13.0))
                                        .tooltip(move |_, cx| {
                                            cx.new(|_| super::NavTooltip {
                                                text: "Delete profile".into(),
                                                theme,
                                            })
                                            .into()
                                        })
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.profile_delete = Some(deleting.clone());
                                            cx.notify();
                                        })),
                                ),
                        ),
                );
            }
            grid = grid.child(card);
        }
        body.child(grid).into_any_element()
    }
}

fn context_preview(theme: Theme, title: &'static str, text: &str) -> impl IntoElement {
    let bounded = text.chars().take(240).collect::<String>();
    let excerpt = bounded
        .split_whitespace()
        .take(36)
        .collect::<Vec<_>>()
        .join(" ");
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap(px(5.0))
        .h(scaled(65.0))
        .overflow_hidden()
        .child(
            div()
                .text_size(scaled(10.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.muted_foreground)
                .child(title),
        )
        .child(
            div()
                .text_size(scaled(11.0))
                .line_height(scaled(16.0))
                .whitespace_normal()
                .text_color(if excerpt.is_empty() {
                    theme.muted_foreground
                } else {
                    theme.foreground
                })
                .child(if excerpt.is_empty() {
                    "No context added yet".to_owned()
                } else {
                    excerpt
                }),
        )
}

fn primary(
    id: &'static str,
    label: &'static str,
    icon: Icon,
    theme: Theme,
    cx: &mut Context<DashboardView>,
    handler: impl Fn(&mut DashboardView, &mut Context<DashboardView>, &mut Window) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .h(scaled(38.0))
        .px(px(12.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(theme.control_border(true))
        .bg(Theme::with_alpha(theme.primary, 0.12))
        .text_color(theme.primary)
        .text_size(scaled(12.0))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(|style| style.bg(Theme::with_alpha(theme.primary, 0.20)))
        .child(icons::icon_colored(theme.primary, icon, 14.0))
        .child(label)
        .on_click(cx.listener(move |this, _, window, cx| {
            handler(this, cx, window);
            cx.notify();
        }))
}
