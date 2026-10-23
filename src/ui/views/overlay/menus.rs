//! All pill selectors share one surface and row treatment.
use super::*;

impl Overlay {
    pub(super) fn selection_menu(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !self.model_menu_open && !self.prompt_menu_open && !self.profile_menu_open {
            return None;
        }
        let theme = self.theme;
        let mut menu = self
            .menu_surface("selection-menu")
            .on_mouse_down_out(cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                let bounds = if this.model_menu_open {
                    &this.model_bounds
                } else if this.profile_menu_open {
                    &this.profile_bounds
                } else {
                    &this.prompt_bounds
                };
                if !bounds.get().contains(&event.position) {
                    this.close_pill_menus();
                    cx.notify();
                }
            }));
        let mut rows = div()
            .id("pill-options")
            .flex()
            .flex_col()
            .gap(px(2.0))
            .max_h(px(224.0))
            .flex_shrink_0()
            .overflow_y_scroll();
        if self.model_menu_open {
            menu = menu.child(
                div()
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .gap(px(5.0))
                    .p(px(2.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .children(self.model_search.clone()),
                    )
                    .child(
                        self.menu_row("refresh-models", false)
                            .w(px(28.0))
                            .h(scaled(28.0))
                            .px(px(0.0))
                            .justify_center()
                            .opacity(if self.catalog_loading { 0.45 } else { 1.0 })
                            .tooltip(move |_, cx| {
                                cx.new(|_| super::HoverLabel {
                                    text: "Refresh models".into(),
                                    theme,
                                })
                                .into()
                            })
                            .child(icons::icon(&self.theme, Icon::RefreshCw, 12.0))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if !this.catalog_loading {
                                    this.load_model_catalog(true, cx);
                                }
                                cx.notify();
                            })),
                    ),
            );
            let query = self
                .model_search
                .as_ref()
                .map(|e| e.read(cx).input.text().trim().to_lowercase())
                .unwrap_or_default();
            let choices = if self.catalog_choices.is_empty() {
                &self.chat_choices
            } else {
                &self.catalog_choices
            };
            let mut count = 0;
            for (index, (provider, model)) in choices.iter().enumerate() {
                let label = format!(
                    "{} · {}",
                    self.provider_names.get(provider).unwrap_or(provider),
                    model
                );
                let searchable = format!("{label} {provider}").to_lowercase();
                if !query
                    .split_whitespace()
                    .all(|word| searchable.contains(word))
                {
                    continue;
                }
                count += 1;
                let active = self.model_provider.as_ref() == Some(provider)
                    && self.model.as_ref() == Some(model);
                let provider = provider.clone();
                let model = model.clone();
                rows = rows.child(
                    self.menu_row(("provider-choice", index), active)
                        .child(div().flex_1().min_w(px(0.0)).truncate().child(label))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.model_provider = Some(provider.clone());
                            this.select_model(&model);
                            this.focus_composer = true;
                            cx.notify();
                        })),
                );
            }
            if count == 0 {
                rows = rows.child(
                    div()
                        .p(px(10.0))
                        .text_size(scaled(11.0))
                        .text_color(self.theme.muted_foreground)
                        .child(if choices.is_empty() {
                            "Connect an AI in Settings."
                        } else {
                            "No matching models."
                        }),
                );
            }
            if let Some(status) = &self.catalog_status {
                menu = menu.child(
                    div()
                        .max_h(px(42.0))
                        .overflow_hidden()
                        .p(px(8.0))
                        .text_size(scaled(10.0))
                        .text_color(self.theme.muted_foreground)
                        .child(status.clone()),
                );
            }
        } else if self.profile_menu_open {
            rows = rows.child(self.profile_option(None, "No context".into(), 0, cx));
            for (index, profile) in self.profile_choices.iter().enumerate() {
                rows = rows.child(self.profile_option(
                    Some(profile.id.clone()),
                    profile.name.clone(),
                    index + 1,
                    cx,
                ));
            }
        } else {
            rows = rows.child(self.prompt_option(None, "General".into(), 0, cx));
            for (index, (id, name, _)) in self.prompt_choices.iter().enumerate() {
                rows = rows.child(self.prompt_option(Some(*id), name.clone(), index + 1, cx));
            }
        }
        menu = menu.child(rows);
        let bounds = if self.model_menu_open {
            self.model_bounds.get()
        } else if self.profile_menu_open {
            self.profile_bounds.get()
        } else {
            self.prompt_bounds.get()
        };
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(gpui::point(bounds.left(), bounds.bottom() + px(6.0)))
                    .snap_to_window_with_margin(px(8.0))
                    .child(
                        div()
                            .w(px(if self.model_menu_open { 240.0 } else { 190.0 }))
                            .child(crate::ui::widgets::motion::reveal(
                                menu,
                                (
                                    "pill-dropdown",
                                    if self.model_menu_open {
                                        0usize
                                    } else if self.profile_menu_open {
                                        1
                                    } else {
                                        2
                                    },
                                ),
                                self.runtime_settings.reduce_motion,
                                true,
                            )),
                    ),
            )
            .with_priority(10)
            .into_any_element(),
        )
    }
    fn profile_option(
        &self,
        id: Option<String>,
        name: String,
        index: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.active_profile.as_ref().map(|p| &p.id) == id.as_ref();
        self.menu_row(("profile-option", index), active)
            .child(div().flex_1().min_w(px(0.0)).truncate().child(name))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_profile(id.as_deref());
                cx.notify();
            }))
    }
    fn prompt_option(
        &self,
        id: Option<i64>,
        name: String,
        index: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.menu_row(("prompt-option", index), self.active_prompt == id)
            .child(div().flex_1().min_w(px(0.0)).truncate().child(name))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.active_prompt = id;
                this.prompt_menu_open = false;
                cx.notify();
            }))
    }
}
