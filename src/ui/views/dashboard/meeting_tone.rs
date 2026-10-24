//! A reviewable tone timeline. Bars describe transcript sections, with verified
//! quotations underneath; the UI never presents a model guess as an outcome.
use super::*;
use crate::ai::sentiment::{Report, Tone};
use crate::db::repos::sentiment;

impl DashboardView {
    pub(super) fn load_meeting_tone(&mut self) {
        self.tone_report = None;
        self.tone_saved = None;
        self.tone_error = None;
        self.tone_segment = 0;
        let Some((db, id)) = self.database.as_ref().zip(self.open_meeting.as_deref()) else {
            return;
        };
        self.tone_revision = sentiment::revision(db.connection(), id).ok();
        match sentiment::get(db.connection(), id) {
            Ok(Some(saved)) => {
                if saved.status == "ready" {
                    self.tone_report = serde_json::from_str::<Report>(&saved.content).ok();
                }
                self.tone_saved = Some(saved);
            }
            Ok(None) => {}
            Err(error) => self.tone_error = Some(format!("Could not load meeting tone: {error}")),
        }
    }

    fn request_meeting_tone(&mut self, cx: &mut Context<Self>) {
        if self.tone_task.is_some() {
            return;
        }
        let Some((id, path)) = self.open_meeting.clone().zip(
            self.database
                .as_ref()
                .and_then(|db| db.connection().path())
                .map(std::path::PathBuf::from),
        ) else {
            return;
        };
        let config = self
            .settings
            .stored_chat
            .iter()
            .find(|c| {
                c.provider == self.settings.settings.chat_provider && c.to_config().is_usable()
            })
            .or_else(|| {
                self.settings
                    .stored_chat
                    .iter()
                    .find(|c| c.to_config().is_usable())
            })
            .map(|c| c.to_config());
        let Some(config) = config else {
            self.tone_error =
                Some("Add a chat provider in Dev Space to analyze the meeting.".into());
            return;
        };
        self.tone_error = None;
        let language = self.settings.settings.response_language.clone();
        self.tone_task = Some(cx.spawn(async move |this, cx| {
            let result = crate::ai::sentiment::generate(&path, &id, config, &language).await;
            this.update(cx, |this, cx| {
                this.tone_task = None;
                if this.open_meeting.as_deref() == Some(&id) {
                    this.load_meeting_tone();
                    if let Err(error) = result {
                        this.tone_error = Some(error);
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn tone_color(&self, tone: Tone) -> gpui::Rgba {
        match tone {
            Tone::Positive | Tone::Satisfactory => self.theme.success,
            Tone::Negative => self.theme.destructive,
            Tone::Mixed => self.theme.warning,
            _ => self.theme.muted_foreground,
        }
    }

    pub(super) fn meeting_tone_panel(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let mut panel=div().flex().flex_col().w_full().min_w(px(0.0)).gap(px(12.0)).p(px(18.0))
            .rounded(px(14.0)).border_1().border_color(theme.border).bg(self.surfaces().card())
            .child(div().flex().flex_wrap().items_center().justify_between().gap(px(10.0))
                .child(div().text_size(scaled(15.0)).font_weight(FontWeight::SEMIBOLD).child("Meeting sentiment"))
                .child(action_button("analyze-meeting-tone",&theme,if self.tone_task.is_some(){"Analyzing…"} else if self.tone_report.is_some(){"Analyze again"} else {"Analyze tone"},cx,|this,cx,_|this.request_meeting_tone(cx))))
            .child(div().text_size(scaled(11.5)).text_color(theme.muted_foreground).whitespace_normal()
                .child("Tone expressed in the transcript, not a verdict on the meeting or interview. Analyze sends transcript excerpts to your configured chat provider."));
        if let Some(error) = &self.tone_error {
            panel = panel.child(crate::ui::controls::warning(&theme, error));
        }
        if let Some(saved) = &self.tone_saved {
            if self
                .tone_revision
                .is_some_and(|revision| revision != saved.revision)
            {
                panel=panel.child(div().text_size(scaled(11.0)).text_color(theme.warning).whitespace_normal().child("The transcript has changed since this assessment. Analyze again to update it."));
            }
            if saved.status == "failed" && self.tone_error.is_none() {
                panel = panel.child(crate::ui::controls::warning(&theme, &saved.error));
            }
            if saved.status == "pending" && self.tone_task.is_none() {
                panel = panel.child(
                    div()
                        .text_size(scaled(11.0))
                        .child("The previous analysis did not finish. You can retry."),
                );
            }
        }
        let Some(report) = &self.tone_report else {
            return panel.child(div().text_size(scaled(12.0)).whitespace_normal().child(if self.tone_task.is_some(){"Reading the transcript and checking supporting quotes…"}else{"No assessment yet. The timeline will show positive, satisfactory, neutral, negative, mixed or insufficient evidence across the meeting."})).into_any_element();
        };
        panel = panel.child(
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .text_size(scaled(18.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(self.tone_color(report.overall()))
                        .child(report.overall().label()),
                )
                .child(
                    div()
                        .text_size(scaled(10.5))
                        .text_color(theme.muted_foreground)
                        .child(format!("{} sections", report.segments.len())),
                ),
        );
        let mut chart = div()
            .flex()
            .w_full()
            .min_w(px(0.0))
            .gap(px(4.0))
            .h(px(142.0));
        for (i, section) in report.segments.iter().enumerate() {
            let tone = section.assessment.tone;
            let color = self.tone_color(tone);
            let height = if tone == Tone::Unknown {
                3.0
            } else {
                (tone.score().abs() * 44.0).max(5.0)
            };
            let top = if tone.score() > 0.0 {
                54.0 - height
            } else if tone.score() < 0.0 {
                54.0
            } else {
                51.5
            };
            let stamp = crate::db::meetings::format_duration(section.start as i64);
            let tooltip = format!("{stamp} · {}", tone.label());
            chart = chart.child(
                div()
                    .id(("sentiment-section", i))
                    .flex_1()
                    .min_w(px(0.0))
                    .h_full()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .cursor_pointer()
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(px(108.0))
                            .rounded(px(6.0))
                            .bg(if self.tone_segment == i {
                                self.surfaces().muted()
                            } else {
                                gpui::transparent_black().into()
                            })
                            .child(
                                div()
                                    .absolute()
                                    .top(px(54.0))
                                    .w_full()
                                    .h(px(1.0))
                                    .bg(theme.border),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(px(6.0))
                                    .right(px(6.0))
                                    .top(px(top))
                                    .h(px(height))
                                    .rounded(px(3.0))
                                    .bg(color),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .text_center()
                            .text_size(scaled(9.0))
                            .text_color(theme.muted_foreground)
                            .child(stamp),
                    )
                    .tooltip(move |_, cx| {
                        cx.new(|_| NavTooltip {
                            text: tooltip.clone().into(),
                            theme,
                        })
                        .into()
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tone_segment = i;
                        cx.notify();
                    })),
            );
        }
        panel = panel
            .child(
                div()
                    .text_size(scaled(10.5))
                    .text_color(theme.muted_foreground)
                    .child("Positive ↑ · Neutral — · Negative ↓ · Select a section for evidence"),
            )
            .child(chart);
        let mut legend = div().flex().flex_wrap().gap(px(12.0));
        for tone in [
            Tone::Positive,
            Tone::Satisfactory,
            Tone::Neutral,
            Tone::Negative,
            Tone::Mixed,
            Tone::Unknown,
        ] {
            legend = legend.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .child(div().size(px(6.0)).rounded_full().bg(self.tone_color(tone)))
                    .child(
                        div()
                            .text_size(scaled(10.0))
                            .text_color(theme.muted_foreground)
                            .child(tone.label()),
                    ),
            );
        }
        panel = panel.child(legend);
        if let Some(section) = report.segments.get(self.tone_segment) {
            let stamp = crate::db::meetings::format_duration(section.start as i64);
            panel = panel
                .child(
                    div()
                        .text_size(scaled(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(format!("{stamp} · {}", section.assessment.tone.label())),
                )
                .child(
                    div()
                        .w_full()
                        .min_w(px(0.0))
                        .text_size(scaled(12.0))
                        .whitespace_normal()
                        .child(section.assessment.reason.clone()),
                );
            for evidence in &section.assessment.evidence {
                let quote = evidence.quote.clone();
                let line = self
                    .detail_lines
                    .iter()
                    .find(|line| line.id == evidence.line_id);
                let timestamp = line
                    .map(|line| crate::db::meetings::format_duration(line.start_time as i64))
                    .unwrap_or_default();
                let speaker = line
                    .map(|line| {
                        if line.who == "me" {
                            "You".to_owned()
                        } else {
                            line.speaker
                                .map(|s| format!("Speaker {}", s + 1))
                                .unwrap_or_else(|| "Them".into())
                        }
                    })
                    .unwrap_or_default();
                panel = panel.child(
                    div()
                        .id(gpui::ElementId::Name(
                            format!("tone-evidence-{}", evidence.line_id).into(),
                        ))
                        .w_full()
                        .min_w(px(0.0))
                        .rounded(px(8.0))
                        .p(px(12.0))
                        .bg(self.surfaces().muted())
                        .cursor_pointer()
                        .child(
                            div()
                                .text_size(scaled(10.0))
                                .text_color(theme.muted_foreground)
                                .child(format!(
                                    "{timestamp} · {speaker} · Filter transcript to this line"
                                )),
                        )
                        .child(
                            div()
                                .w_full()
                                .min_w(px(0.0))
                                .text_size(scaled(12.0))
                                .whitespace_normal()
                                .child(format!("“{}”", evidence.quote)),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            // Filter the transcript by the cited words: "00:00"
                            // alone matches every line sharing that timestamp.
                            this.filter_meeting_transcript(&quote, cx);
                            cx.notify();
                        })),
                );
            }
        }
        let coverage = if report.sampled_lines < report.total_lines || report.clipped_lines {
            format!(
                "Excerpt assessment: {} sampled lines out of {}. Sections between samples or truncated passages may have a different tone.",
                report.sampled_lines, report.total_lines
            )
        } else {
            format!(
                "Based on all {} saved lines. Text alone may miss tone, humor or context.",
                report.total_lines
            )
        };
        panel = panel.child(
            div()
                .text_size(scaled(10.5))
                .text_color(theme.muted_foreground)
                .whitespace_normal()
                .child(coverage),
        );
        if let Some(saved) = &self.tone_saved {
            panel = panel.child(
                div()
                    .text_size(scaled(10.0))
                    .text_color(theme.muted_foreground)
                    .child(format!("Assessed {} UTC", saved.updated_at)),
            );
        }
        panel.into_any_element()
    }
}
