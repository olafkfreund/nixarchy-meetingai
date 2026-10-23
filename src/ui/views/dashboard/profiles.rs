//! Profile management has its own dashboard tab; the overlay selects a profile.
use super::*;
use crate::db::profiles::{self as store, Profile};
use crate::ui::widgets::editor::Editor;

pub(super) struct ProfileDraft {
    id: String,
    name: gpui::Entity<Editor>,
    personal: gpui::Entity<Editor>,
    meeting: gpui::Entity<Editor>,
}

impl DashboardView {
    fn edit_profile(&mut self, profile: Option<Profile>, cx: &mut Context<Self>) {
        let profile = profile.unwrap_or(Profile {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            personal_context: String::new(),
            meeting_context: String::new(),
        });
        self.profile_edit = Some(ProfileDraft {
            id: profile.id,
            name: cx.new(|cx| {
                let mut editor = Editor::new(&profile.name, self.theme, cx);
                editor.compact = true;
                editor
            }),
            personal: cx.new(|cx| Editor::new(&profile.personal_context, self.theme, cx)),
            meeting: cx.new(|cx| Editor::new(&profile.meeting_context, self.theme, cx)),
        });
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
            }
            Err(error) => self.settings.error = Some(error),
        }
    }

    pub fn profiles_body(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let mut body = div().flex().flex_col().items_start().w_full().gap(px(16.0))
            .child(div().text_size(scaled(12.0)).text_color(theme.muted_foreground).whitespace_normal()
                .child("Create profiles for different roles, interviews or meetings. Choose one beside the mode picker in the app, or choose No context. Files and the live transcript remain separate."));
        if let Some(error) = &self.settings.error {
            body = body.child(crate::ui::controls::warning(&theme, error));
        }
        if let Some(draft) = &self.profile_edit {
            let form = div().flex().flex_col().w_full().gap(px(8.0)).p(px(16.0)).rounded(px(14.0)).bg(self.surfaces().card())
                .child(div().text_size(scaled(12.0)).child("Profile name"))
                .child(draft.name.clone())
                .child(div().mt(px(4.0)).text_size(scaled(12.0)).child("About me"))
                .child(draft.personal.clone())
                .child(div().mt(px(4.0)).text_size(scaled(12.0)).child("Meeting / interview context"))
                .child(draft.meeting.clone())
                .child(div().text_size(scaled(11.0)).text_color(theme.muted_foreground).whitespace_normal().child("Saved on this computer. The selected profile is included with AI requests. Changes apply to the next answer using this profile."))
                .child(div().flex().gap(px(8.0))
                    .child(action_button("save-profile", &theme, "Save profile", cx, |this, cx, _| this.save_profile(cx)))
                    .child(action_button("cancel-profile", &theme, "Cancel", cx, |this, _, _| { this.profile_edit = None; this.settings.error = None; })));
            return body
                .child(crate::ui::widgets::motion::reveal(
                    form,
                    "profile-editor-open",
                    self.settings.settings.reduce_motion,
                    false,
                ))
                .into_any_element();
        }
        body = body.child(action_button(
            "create-profile",
            &theme,
            "New profile",
            cx,
            |this, cx, _| this.edit_profile(None, cx),
        ));
        let selected = self
            .database
            .as_ref()
            .and_then(|db| store::selected(db.connection()).ok().flatten())
            .map(|p| p.id);
        let PageData::Profiles(profiles) = self.dashboard.data() else {
            return body.into_any_element();
        };
        if profiles.is_empty() {
            body = body
                .child(self.empty_note("No profiles yet. Create one, then select it in the app."));
        }
        for profile in profiles {
            let active = selected.as_ref() == Some(&profile.id);
            let editing = profile.clone();
            let using = profile.id.clone();
            let deleting = profile.id.clone();
            let preview = if profile.personal_context.is_empty() {
                &profile.meeting_context
            } else {
                &profile.personal_context
            };
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap(px(10.0))
                    .p(px(16.0))
                    .rounded(px(14.0))
                    .border_1()
                    .border_color(self.surfaces().border())
                    .bg(self.surfaces().card())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(scaled(14.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(profile.name.clone())
                            .children(active.then(|| {
                                div()
                                    .text_size(scaled(10.0))
                                    .text_color(theme.muted_foreground)
                                    .child("Selected")
                            })),
                    )
                    .child(
                        div()
                            .text_size(scaled(12.0))
                            .text_color(theme.muted_foreground)
                            .whitespace_normal()
                            .child(preview.chars().take(200).collect::<String>()),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(action_button(
                                gpui::ElementId::Name(format!("use-profile-{}", profile.id).into()),
                                &theme,
                                if active {
                                    "Use no context"
                                } else {
                                    "Use profile"
                                },
                                cx,
                                move |this, _, _| {
                                    if let Some(db) = &this.database {
                                        this.settings.error = store::select(
                                            db.connection(),
                                            if active { None } else { Some(&using) },
                                        )
                                        .err();
                                    }
                                },
                            ))
                            .child(action_button(
                                gpui::ElementId::Name(
                                    format!("edit-profile-{}", profile.id).into(),
                                ),
                                &theme,
                                "Edit",
                                cx,
                                move |this, cx, _| this.edit_profile(Some(editing.clone()), cx),
                            ))
                            .child(action_button(
                                gpui::ElementId::Name(
                                    format!("delete-profile-{}", profile.id).into(),
                                ),
                                &theme,
                                "Delete",
                                cx,
                                move |this, _, _| {
                                    if let Some(db) = &this.database {
                                        match store::delete(db.connection(), &deleting) {
                                            Ok(()) => {
                                                this.settings.error = None;
                                                this.dashboard.navigate(
                                                    super::super::nav::Route::Profiles,
                                                    Some(db),
                                                );
                                            }
                                            Err(error) => {
                                                this.settings.error = Some(error.to_string())
                                            }
                                        }
                                    }
                                },
                            )),
                    ),
            );
        }
        body.into_any_element()
    }
}
