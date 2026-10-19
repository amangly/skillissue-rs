use super::*;

impl NativeView {
    pub(super) fn test_byok(&mut self, audio: bool, image: bool, cx: &mut Context<Self>) {
        if self.provider_draft_dirty(cx) || self.settings_ui.provider_save_pending {
            return;
        }
        let Some(engine) = self.engine.clone() else {
            return;
        };
        self.invalidate_byok_check();
        let generation = self.settings_ui.byok_generation;
        let cancel = tokio_util::sync::CancellationToken::new();
        self.settings_ui.byok_cancel = Some(cancel.clone());
        let role = if audio {
            "Audio"
        } else if image {
            "Images"
        } else {
            "Text"
        };
        self.settings_ui.byok_status = format!("Testing {role}…");
        let sender = self.sender.clone();
        self.runtime.spawn(async move {
            let result = engine.test_byok_connection(audio, image, cancel).await;
            let _ = sender.send(Reply::ByokTest(generation, role, result));
        });
        cx.notify();
    }

    pub(in crate::ui) fn auto_load_byok_models(&mut self, cx: &mut Context<Self>) {
        if self.settings.is_palepale() {
            return;
        }
        let Some(engine) = self.engine.clone() else {
            return;
        };
        self.invalidate_byok_check();
        let generation = self.settings_ui.byok_generation;
        let cancel = tokio_util::sync::CancellationToken::new();
        self.settings_ui.byok_cancel = Some(cancel.clone());
        self.settings_ui.byok_status = "Loading models…".into();
        let sender = self.sender.clone();
        self.runtime.spawn(async move {
            let result = engine.byok_model_lists(cancel).await;
            let _ = sender.send(Reply::ByokModelLists(generation, result));
        });
        cx.notify();
    }
    pub(in crate::ui) fn invalidate_byok_check(&mut self) {
        self.settings_ui.byok_generation = self.settings_ui.byok_generation.wrapping_add(1);
        if let Some(cancel) = self.settings_ui.byok_cancel.take() {
            cancel.cancel();
        }
        self.settings_ui.byok_models.clear();
        self.settings_ui.byok_status =
            "Not checked. Save your connection, then check model access.".into();
    }

    pub(super) fn check_byok(&mut self, audio: bool, image: bool, cx: &mut Context<Self>) {
        if self.provider_draft_dirty(cx) || self.settings_ui.provider_save_pending {
            return;
        }
        let Some(engine) = self.engine.clone() else {
            return;
        };
        self.invalidate_byok_check();
        let generation = self.settings_ui.byok_generation;
        let cancel = tokio_util::sync::CancellationToken::new();
        self.settings_ui.byok_cancel = Some(cancel.clone());
        self.settings_ui.byok_check_audio = audio;
        self.settings_ui.byok_check_image = image;
        self.settings_ui.byok_status = if image {
            "Checking image connection…"
        } else if audio {
            "Checking saved audio connection…"
        } else {
            "Checking saved chat connection…"
        }
        .into();
        let sender = self.sender.clone();
        self.runtime.spawn(async move {
            let result = if image {
                engine.byok_image_models(cancel).await
            } else {
                engine.byok_models(audio, cancel).await
            };
            let _ = sender.send(Reply::ByokModels(generation, result));
        });
        cx.notify();
    }

    pub(super) fn byok_checks(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.settings_palette();
        let pending = self.settings_ui.byok_cancel.is_some();
        let disabled = pending
            || self.provider_draft_dirty(cx)
            || self.settings_ui.provider_save_pending
            || self.engine.is_none();
        let mut view = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(caption(
                "Fetches models only. No conversation or audio is sent.",
                p,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        settings_action("check-byok-chat", "Check chat connection", p)
                            .disabled(disabled)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.check_byok(false, false, cx)),
                            ),
                    )
                    .child(
                        settings_action("check-byok-audio", "Check audio connection", p)
                            .disabled(disabled)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.check_byok(true, false, cx)),
                            ),
                    )
                    .child(
                        settings_action("check-byok-image", "Check images", p)
                            .disabled(disabled)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.check_byok(false, true, cx)),
                            ),
                    )
                    .when(pending, |view| {
                        view.child(
                            settings_action("cancel-byok-check", "Cancel check", p).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.invalidate_byok_check();
                                    cx.notify();
                                }),
                            ),
                        )
                    }),
            )
            .child(caption(&self.settings_ui.byok_status, p));
        if !self.settings_ui.byok_models.is_empty() {
            let filter = self.settings_ui.byok_filter.read(cx).value().to_lowercase();
            let models: Vec<_> = self
                .settings_ui
                .byok_models
                .iter()
                .filter(|id| id.to_lowercase().contains(&filter))
                .take(128)
                .cloned()
                .collect();
            view = view
                .child(Input::new(&self.settings_ui.byok_filter).aria_label("Filter model IDs"))
                .child(caption("Up to 128 matches. Filter or enter a model ID.", p));
            let audio = self.settings_ui.byok_check_audio;
            let image = self.settings_ui.byok_check_image;
            let entity = cx.entity().downgrade();
            view = view.child(
                settings_action("byok-model-picker", "Choose a listed model", p).dropdown_menu(
                    move |mut menu, _, _| {
                        for model in &models {
                            let id = model.clone();
                            let entity = entity.clone();
                            menu = menu.item(PopupMenuItem::new(id.clone()).on_click(
                                move |_, window, cx| {
                                    let _ = entity.update(cx, |this, cx| {
                                        let input = if image
                                            && !this
                                                .settings_ui
                                                .image_endpoint
                                                .read(cx)
                                                .value()
                                                .is_empty()
                                        {
                                            &this.settings_ui.image_model
                                        } else if audio {
                                            &this.stt_model
                                        } else {
                                            &this.model
                                        };
                                        input.update(cx, |input, cx| {
                                            input.set_value(id.clone(), window, cx)
                                        });
                                    });
                                },
                            ));
                        }
                        menu
                    },
                ),
            );
        }
        view.into_any_element()
    }
}
