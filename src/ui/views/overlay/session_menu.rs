//! Menus and dialogs stay inside the capture-protected overlay window.
use super::*;

#[derive(Clone)]
pub(super) enum Dialog {
    Text(crate::core::composer::TextAttachment),
    Quit,
}

impl Overlay {
    pub(super) fn menu_surface(&self, id: &'static str) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .occlude()
            .flex()
            .flex_col()
            .w_full()
            .p(px(4.0))
            .gap(px(2.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(self.theme.border)
            .bg(self.theme.popover())
            .text_color(self.theme.foreground)
            .shadow_md()
    }
    pub(super) fn menu_row(
        &self,
        id: impl Into<gpui::ElementId>,
        active: bool,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .min_w(px(0.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(28.0))
            .px(px(8.0))
            .rounded(px(6.0))
            .text_size(scaled(10.5))
            .line_height(scaled(14.0))
            .text_color(self.theme.foreground)
            .cursor_pointer()
            .bg(if active {
                self.theme.menu_hover()
            } else {
                gpui::transparent_black().into()
            })
            .hover(|s| s.bg(self.theme.menu_hover()))
    }
    pub(super) fn close_pill_menus(&mut self) {
        if self.model_menu_open {
            self.focus_composer = true;
        }
        self.app_menu_open = false;
        self.restore_menu_open = false;
        self.model_menu_open = false;
        self.prompt_menu_open = false;
        self.profile_menu_open = false;
        self.context_menu_open = false;
    }
    pub(super) fn toggle_app_menu(&mut self) {
        self.focus_composer = true;
        let open = !self.app_menu_open;
        self.close_pill_menus();
        self.app_menu_open = open;
        if open && let Some(db) = self.services.connect() {
            self.restore_chats =
                crate::db::conversations::list(db.connection(), 40).unwrap_or_default();
            self.restore_meetings =
                crate::db::meetings::list(db.connection(), 40).unwrap_or_default();
        }
    }
    pub(super) fn show_dialog(&mut self, dialog: Dialog) {
        self.focus_composer = true;
        self.close_pill_menus();
        self.dialog = Some(dialog);
        self.dialog_open = true;
    }
    pub(super) fn save_resume_context(&mut self, db: Option<&crate::db::Database>) {
        let (Some(db), Some(id)) = (db, self.chat.conversation_id()) else {
            return;
        };
        let (length, language) = self.conversation_response.clone().unwrap_or((
            self.runtime_settings.response_length,
            self.runtime_settings.response_language.clone(),
        ));
        let context = crate::db::sessions::SavedContext {
            profile_id: self.active_profile.as_ref().map(|p| p.id.clone()),
            prompt_id: self.active_prompt,
            sources: self
                .context_sources
                .iter()
                .filter(|s| s.image.is_none())
                .cloned()
                .collect(),
            response_length: length.label().into(),
            response_language: language,
        };
        if let Err(error) = crate::db::sessions::save(
            db.connection(),
            id,
            self.meeting_id
                .as_deref()
                .or(self.resume_meeting.as_deref()),
            &context,
        ) {
            tracing::warn!(%error, "could not save restoration context");
        }
    }
    fn can_restore(&mut self) -> bool {
        if self.is_listening() || self.transcription_busy {
            self.chat_error = Some("Stop the current recording and wait for its final words before restoring a session.".into());
            false
        } else {
            true
        }
    }
    pub(super) fn restore_chat(&mut self, id: &str) -> Result<(), String> {
        if !self.can_restore() {
            return Err(self.chat_error.clone().unwrap_or_default());
        }
        let db = self
            .services
            .connect()
            .ok_or("The local database is unavailable.")?;
        crate::db::conversations::require_conversation(db.connection(), id)
            .map_err(|e| e.to_string())?;
        let context = crate::db::sessions::load(db.connection(), id)?;
        self.new_chat();
        if let Some(context) = context {
            let profile = context
                .profile_id
                .and_then(|id| self.profile_choices.iter().find(|p| p.id == id).cloned());
            crate::db::profiles::select(db.connection(), profile.as_ref().map(|p| p.id.as_str()))?;
            self.active_profile = profile;
            self.active_prompt = context
                .prompt_id
                .filter(|id| self.prompt_choices.iter().any(|p| p.0 == *id));
            for source in context.sources {
                crate::core::context::add(&mut self.context_sources, source)?;
            }
            let length = crate::core::settings::ResponseLength::ALL
                .into_iter()
                .find(|l| l.label() == context.response_length)
                .unwrap_or_default();
            self.conversation_response = Some((length, context.response_language));
        }
        let messages = self.chat.restore(&db, id)?;
        // Older exchanges remain on disk and in the request's bounded history.
        for message in messages
            .into_iter()
            .rev()
            .take(200)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            self.conversation.push(message);
        }
        self.mode = Mode::Ask;
        self.bottom_mode = BottomMode::Full;
        self.focus_composer = true;
        self.listen_notice =
            Some("Conversation restored. Your next answer continues this thread.".into());
        Ok(())
    }
    pub(super) fn restore_meeting(
        &mut self,
        record: &crate::db::meetings::MeetingRecord,
    ) -> Result<(), String> {
        if !self.can_restore() {
            return Err(self.chat_error.clone().unwrap_or_default());
        }
        let db = self
            .services
            .connect()
            .ok_or("The local database is unavailable.")?;
        let lines = crate::db::meetings::recent_lines(db.connection(), &record.id, 500)
            .map_err(|e| e.to_string())?;
        let chat = crate::db::sessions::meeting_chat(db.connection(), &record.id)
            .map_err(|e| e.to_string())?;
        if let Some(chat) = chat {
            self.restore_chat(&chat)?;
        } else {
            self.new_chat();
        }
        let (end_time, last_speaker): (f64, Option<i64>) = db.connection().query_row(
            "SELECT COALESCE(MAX(end_time), 0), MAX(speaker) FROM meeting_lines WHERE meeting_id=?1",
            [&record.id], |r| Ok((r.get(0)?, r.get(1)?)),
        ).map_err(|e| e.to_string())?;
        self.meeting_offset = end_time.max(record.duration_seconds as f64);
        self.last_elapsed = self.meeting_offset;
        self.meeting_speaker_offset = last_speaker.map_or(0, |s| s + 1);
        self.transcript = lines
            .iter()
            .rev()
            .take(500)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|l| TranscriptEntry {
                who: if l.who == "me" { "me" } else { "them" },
                speaker: l.speaker,
                text: l.text.clone(),
                start: l.start_time,
            })
            .collect();
        self.transcript_revision = self.transcript.len();
        // Keep the recap plus beginning and recent speech as bounded background.
        let recap = crate::db::meetings::recap(db.connection(), &record.id)
            .ok()
            .flatten()
            .filter(|r| r.status == "ready")
            .map(|r| r.content)
            .unwrap_or_default();
        let mut context = format!(
            "Restored meeting: {}\n{}\nSaved recap:\n{}\nEarlier speech:\n",
            record.title,
            record.context.chars().take(1_000).collect::<String>(),
            recap.chars().take(3_000).collect::<String>()
        );
        context.push_str(
            &crate::db::meetings::transcript_excerpt(db.connection(), &record.id, 10_000)
                .map_err(|e| e.to_string())?,
        );
        self.context_sources
            .retain(|s| !s.id.starts_with("restored-meeting:"));
        let source = crate::core::context::Attachment::text(
            format!("restored-meeting:{}", record.id),
            format!("Restored meeting: {}", record.title),
            context,
        );
        crate::core::context::add(&mut self.context_sources, source)?;
        self.resume_meeting = Some(record.id.clone());
        self.mode = Mode::Listen;
        self.listen_notice = Some("Meeting restored. Ask about it now, or press Play to continue recording into this meeting.".into());
        Ok(())
    }
    pub(super) fn app_menu(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !self.app_menu_open {
            return None;
        }
        let main = self
            .menu_surface("app-menu")
            .child(
                self.menu_row("app-new", false)
                    .child("New")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.can_restore() {
                            this.new_chat();
                            this.focus_composer = true;
                        } else {
                            this.close_pill_menus();
                        }
                        cx.notify();
                    })),
            )
            .child(
                self.menu_row("app-restore", self.restore_menu_open)
                    .child(div().flex_1().child("Restore"))
                    .child(icons::icon(&self.theme, Icon::ChevronRight, 12.0))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.restore_menu_open = !this.restore_menu_open;
                        cx.notify();
                    })),
            )
            .child(
                self.menu_row("app-quit", false)
                    .child("Quit Lennette…")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_dialog(Dialog::Quit);
                        cx.notify();
                    })),
            );
        let mut group = div()
            .id("app-menu-group")
            .flex()
            .items_start()
            .gap(px(8.0))
            .on_mouse_down_out(cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                if !this.app_menu_bounds.get().contains(&event.position) {
                    this.close_pill_menus();
                    cx.notify();
                }
            }))
            .child(div().w(px(180.0)).child(crate::ui::widgets::motion::reveal(
                main,
                "app-menu-open",
                self.runtime_settings.reduce_motion,
                true,
            )));
        if self.restore_menu_open {
            let mut restore = self
                .menu_surface("restore-menu")
                .max_h(px(340.0))
                .overflow_y_scroll();
            restore = restore.child(
                div()
                    .p(px(8.0))
                    .text_size(scaled(10.0))
                    .text_color(self.theme.muted_foreground)
                    .child("MEETINGS"),
            );
            for (index, record) in self.restore_meetings.iter().enumerate() {
                let record = record.clone();
                restore = restore.child(
                    self.menu_row(("restore-meeting", index), false)
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .truncate()
                                .child(record.title.clone()),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Err(error) = this.restore_meeting(&record) {
                                this.chat_error = Some(error);
                            }
                            this.close_pill_menus();
                            cx.notify();
                        })),
                );
            }
            restore = restore.child(
                div()
                    .p(px(8.0))
                    .text_size(scaled(10.0))
                    .text_color(self.theme.muted_foreground)
                    .child("AI CHATS"),
            );
            for (index, record) in self.restore_chats.iter().enumerate() {
                let id = record.id.clone();
                restore = restore.child(
                    self.menu_row(("restore-chat", index), false)
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .truncate()
                                .child(record.title.clone()),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Err(error) = this.restore_chat(&id) {
                                this.chat_error = Some(error);
                            }
                            this.close_pill_menus();
                            cx.notify();
                        })),
                );
            }
            if self.restore_chats.is_empty() && self.restore_meetings.is_empty() {
                restore = restore.child(
                    div()
                        .p(px(10.0))
                        .text_size(scaled(11.0))
                        .child("No saved sessions yet."),
                );
            }
            group = group.child(div().w(px(240.0)).child(crate::ui::widgets::motion::reveal(
                restore,
                "restore-submenu-open",
                self.runtime_settings.reduce_motion,
                true,
            )));
        }
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(gpui::point(
                        self.app_menu_bounds.get().left(),
                        self.app_menu_bounds.get().bottom() + px(6.0),
                    ))
                    .snap_to_window_with_margin(px(8.0))
                    .child(group),
            )
            .with_priority(12)
            .into_any_element(),
        )
    }
    pub(super) fn render_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let (progress, active) = self.dialog_motion.step(
            f32::from(self.dialog_open),
            self.runtime_settings.reduce_motion,
            std::time::Instant::now(),
        );
        if active {
            window.request_animation_frame();
        }
        if progress == 0.0 && !self.dialog_open {
            self.dialog = None;
            return None;
        }
        let dialog = self.dialog.clone()?;
        let mut panel = div()
            .relative()
            .occlude()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .w_full()
            .max_w(px(600.0))
            .flex_shrink_0()
            .p(px(20.0))
            .rounded(px(18.0))
            .border_1()
            .border_color(self.theme.border)
            .bg(self.theme.popover())
            .shadow_lg()
            .opacity(progress)
            .top(px(8.0 * (1.0 - progress)));
        match dialog {
            Dialog::Text(item) => {
                panel = panel
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(icons::icon(
                                &self.theme,
                                if item.code {
                                    Icon::Code
                                } else {
                                    Icon::FileText
                                },
                                16.0,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(scaled(14.0))
                                    .child(item.name.clone()),
                            )
                            .child(
                                self.menu_row("close-paste-preview", false)
                                    .child("Done")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.dialog_open = false;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .id("paste-preview-content")
                            .w_full()
                            .max_h((window.viewport_size().height - px(190.0)).max(px(80.0)))
                            .overflow_y_scroll()
                            .p(px(12.0))
                            .rounded(px(10.0))
                            .bg(self.surfaces().input())
                            .text_size(scaled(12.0))
                            .line_height(scaled(19.0))
                            .whitespace_normal()
                            .when(item.code, |s| s.font_family("monospace"))
                            .child(item.text.clone()),
                    )
                    .child(
                        self.menu_row("copy-paste-preview", false)
                            .child("Copy text")
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                    item.text.clone(),
                                ));
                            })),
                    );
            }
            Dialog::Quit => {
                panel = panel
                    .max_w(px(360.0))
                    .pb(px(28.0))
                    .child(
                        div()
                            .text_size(scaled(17.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Quit Lennette?"),
                    )
                    .child(
                        div()
                            .text_size(scaled(12.0))
                            .text_color(self.theme.muted_foreground)
                            .whitespace_normal()
                            .child(if self.quit_pending {
                                "Finishing the last transcript before quitting…"
                            } else {
                                "Your conversations and transcripts are saved on this computer."
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .pt(px(4.0))
                            .justify_end()
                            .gap(px(8.0))
                            .child(
                                self.menu_row("cancel-quit", false)
                                    .child("Cancel")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.dialog_open = false;
                                        this.quit_pending = false;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                self.menu_row("confirm-quit", false)
                                    .bg(self.theme.menu_hover())
                                    .child("Quit")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.interrupt_response();
                                        this.cancel_dictation();
                                        this.stop_listening();
                                        this.quit_pending = true;
                                        cx.notify();
                                    })),
                            ),
                    );
            }
        }
        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .p(px(24.0))
                .child(
                    div()
                        .id("overlay-dialog-scrim")
                        .absolute()
                        .inset_0()
                        .bg(Theme::with_alpha(gpui::black().into(), 0.40 * progress))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.dialog_open = false;
                            this.quit_pending = false;
                            cx.notify();
                        })),
                )
                .child(panel)
                .into_any_element(),
        )
    }
}
