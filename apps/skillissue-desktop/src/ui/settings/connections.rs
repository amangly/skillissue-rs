//! Compact role-based connection editor. Unsaved secrets stay in this window.
use super::*;
use skillissue_domain::byok::{BYOK_PROVIDERS, byok_provider, starter_image_model, starter_model};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ConnectionRole {
    Text,
    Audio,
    Images,
}
impl ConnectionRole {
    fn id(self) -> u8 {
        match self {
            Self::Text => 0,
            Self::Audio => 1,
            Self::Images => 2,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Audio => "Audio",
            Self::Images => "Images",
        }
    }
}
impl NativeView {
    pub(super) fn connection_key_label(&self, role: ConnectionRole, cx: &App) -> &'static str {
        let (endpoint, _, key) = self.connection_inputs(role);
        if !key.read(cx).value().is_empty() {
            return "API key · Unsaved";
        }
        let (saved_url, present, removing) = match role {
            ConnectionRole::Text => (
                &self.settings.api_base_url,
                self.settings_ui.has_byok_key,
                self.settings_ui.remove_key,
            ),
            ConnectionRole::Audio => (
                &self.settings.transcription_base_url,
                self.settings_ui.has_audio_key,
                self.settings_ui.remove_transcription_key,
            ),
            ConnectionRole::Images => (
                &self.settings.image_base_url,
                self.settings_ui.has_image_key,
                self.settings_ui.remove_image_key,
            ),
        };
        if removing {
            return "API key · Removal pending";
        }
        if endpoint
            .read(cx)
            .value()
            .as_str()
            .trim()
            .trim_end_matches('/')
            == saved_url.trim_end_matches('/')
            && present
        {
            "API key · •••••••• Saved"
        } else {
            "API key · Not saved"
        }
    }

    pub(in crate::ui) fn remember_connection_models(&mut self, models: &[String]) {
        let (role, url) = if self.settings_ui.byok_check_image {
            (2, &self.settings.image_base_url)
        } else if self.settings_ui.byok_check_audio {
            (1, &self.settings.transcription_base_url)
        } else {
            (0, &self.settings.api_base_url)
        };
        let url = if url.is_empty() {
            &self.settings.api_base_url
        } else {
            url
        };
        self.settings_ui
            .connection_models
            .insert((role, url.clone()), models.to_vec());
    }

    pub(super) fn select_connection_model(
        &mut self,
        role: ConnectionRole,
        model: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_ui.provider_save_pending {
            return;
        }
        let shared_image = role == ConnectionRole::Images
            && self.settings_ui.image_endpoint.read(cx).value().is_empty();
        let (input, field) = if role == ConnectionRole::Text || shared_image {
            (self.model.clone(), "chat")
        } else if role == ConnectionRole::Audio {
            (self.stt_model.clone(), "transcription")
        } else {
            (self.settings_ui.image_model.clone(), "image_model")
        };
        self.invalidate_byok_check();
        input.update(cx, |input, cx| {
            input.set_value(model.to_owned(), window, cx)
        });
        self.settings_ui.provider_dirty.insert(field);
        cx.notify();
    }

    fn connection_model_picker(&self, role: ConnectionRole, cx: &mut Context<Self>) -> AnyElement {
        let p = self.settings_palette();
        let (endpoint, model, _) = self.connection_inputs(role);
        let shared = role != ConnectionRole::Text && endpoint.read(cx).value().is_empty();
        let url = if shared {
            self.endpoint.read(cx).value().to_string()
        } else {
            endpoint.read(cx).value().to_string()
        };
        let current = if shared && role == ConnectionRole::Images {
            self.model.read(cx).value().to_string()
        } else {
            model.read(cx).value().to_string()
        };
        let mut choices = Vec::new();
        if !current.is_empty() {
            choices.push(current.clone());
        }
        if let Some(provider) = byok_provider(&url) {
            let starter = match role {
                ConnectionRole::Text => starter_model(provider.id),
                ConnectionRole::Audio => provider.transcription_model,
                ConnectionRole::Images => starter_image_model(provider.id),
            };
            if let Some(starter) = starter {
                choices.push(starter.into());
            }
            if role == ConnectionRole::Text
                && let Some(vision) = starter_image_model(provider.id)
            {
                choices.push(vision.into());
            }
        }
        let discovered = self.settings_ui.connection_models.get(&(role.id(), url));
        let unconfirmed =
            discovered.is_some_and(|models| !current.is_empty() && !models.contains(&current));
        let no_models = discovered.is_some_and(Vec::is_empty);
        if let Some(models) = discovered {
            choices = models.clone();
        }
        let mut seen = std::collections::BTreeSet::new();
        choices.retain(|model| seen.insert(model.clone()));
        let filter = self.settings_ui.byok_filter.read(cx).value().to_lowercase();
        let many = choices.len() > 128;
        let choices: Vec<_> = choices
            .into_iter()
            .filter(|id| filter.is_empty() || id.to_lowercase().contains(&filter))
            .take(128)
            .collect();
        let pending = self.settings_ui.provider_save_pending;
        let entity = cx.entity().downgrade();
        let label = if current.is_empty() {
            "Choose model".to_owned()
        } else {
            current
        };
        let picker = settings_action(
            SharedString::from(format!("connection-model-{}", role.id())),
            &label,
            p,
        )
        .disabled(pending)
        .dropdown_menu(move |mut menu, _, _| {
            for model in &choices {
                let model = model.clone();
                let entity = entity.clone();
                menu = menu.item(PopupMenuItem::new(model.clone()).on_click(
                    move |_, window, cx| {
                        let _ = entity.update(cx, |this, cx| {
                            this.select_connection_model(role, &model, window, cx)
                        });
                    },
                ));
            }
            let entity = entity.clone();
            menu.item(
                PopupMenuItem::new("Custom model…").on_click(move |_, _, cx| {
                    let _ = entity.update(cx, |this, cx| {
                        this.settings_ui.byok_advanced = true;
                        cx.notify();
                    });
                }),
            )
        });
        let disabled = pending
            || self.provider_draft_dirty(cx)
            || self.settings_ui.byok_cancel.is_some()
            || self.engine.is_none();
        let mut row =
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(div().text_size(px(11.)).child(
                    if shared && role == ConnectionRole::Images {
                        "Model (shared with Text)"
                    } else {
                        "Model"
                    },
                ))
                .child(
                    div().flex().flex_wrap().gap_2().child(picker).child(
                        settings_action(
                            SharedString::from(format!("connection-model-refresh-{}", role.id())),
                            "Test",
                            p,
                        )
                        .disabled(disabled)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.test_byok(
                                role == ConnectionRole::Audio,
                                role == ConnectionRole::Images,
                                cx,
                            )
                        })),
                    ),
                );
        if many || !filter.is_empty() {
            row = row.child(Input::new(&self.settings_ui.byok_filter).aria_label("Filter models"));
        }
        if no_models {
            row = row.child(caption(
                "No confirmed models. Custom IDs are in Advanced.",
                p,
            ));
        } else if unconfirmed {
            row = row.child(caption("Current model is not confirmed for this role.", p));
        }
        row.into_any_element()
    }

    fn connection_inputs(
        &self,
        role: ConnectionRole,
    ) -> (Entity<InputState>, Entity<InputState>, Entity<InputState>) {
        match role {
            ConnectionRole::Text => (
                self.endpoint.clone(),
                self.model.clone(),
                self.api_key.clone(),
            ),
            ConnectionRole::Audio => (
                self.settings_ui.transcription_endpoint.clone(),
                self.stt_model.clone(),
                self.settings_ui.transcription_key.clone(),
            ),
            ConnectionRole::Images => (
                self.settings_ui.image_endpoint.clone(),
                self.settings_ui.image_model.clone(),
                self.settings_ui.image_key.clone(),
            ),
        }
    }
    fn connection_enabled(&self, role: ConnectionRole) -> bool {
        match role {
            ConnectionRole::Text => true,
            ConnectionRole::Audio => self
                .settings_ui
                .audio_enabled_draft
                .unwrap_or(self.settings.byok_audio_enabled),
            ConnectionRole::Images => self
                .settings_ui
                .image_enabled_draft
                .unwrap_or(self.settings.include_screenshot),
        }
    }
    pub(super) fn choose_connection(
        &mut self,
        role: ConnectionRole,
        choice: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_ui.provider_save_pending {
            return;
        }
        self.invalidate_byok_check();
        if choice == "custom" {
            self.settings_ui.byok_advanced = true;
            match role {
                ConnectionRole::Audio => self.settings_ui.audio_enabled_draft = Some(true),
                ConnectionRole::Images => self.settings_ui.image_enabled_draft = Some(true),
                ConnectionRole::Text => {}
            }
            cx.notify();
            return;
        }
        let (endpoint, model, key) = self.connection_inputs(role);
        if choice == "off" {
            match role {
                ConnectionRole::Audio => self.settings_ui.audio_enabled_draft = Some(false),
                ConnectionRole::Images => self.settings_ui.image_enabled_draft = Some(false),
                ConnectionRole::Text => return,
            }
            cx.notify();
            return;
        }
        let old_url = endpoint.read(cx).value().to_string();
        self.settings_ui.connection_drafts.insert(
            (role.id(), old_url),
            (
                model.read(cx).value().to_string(),
                key.read(cx).value().to_string(),
            ),
        );
        let provider = BYOK_PROVIDERS.iter().find(|p| p.id == choice);
        let url = if choice == "palepale" {
            skillissue_domain::provider::PALEPALE_BASE_URL
        } else {
            provider.map_or("", |p| p.base_url)
        };
        let default = provider.and_then(|p| {
            if role == ConnectionRole::Audio {
                p.transcription_model
            } else if role == ConnectionRole::Images {
                starter_image_model(p.id)
            } else {
                starter_model(p.id)
            }
        });
        let (next_model, next_key) = self
            .settings_ui
            .connection_drafts
            .get(&(role.id(), url.into()))
            .cloned()
            .unwrap_or_else(|| (default.unwrap_or("").into(), String::new()));
        endpoint.update(cx, |input, cx| input.set_value(url, window, cx));
        // Shared audio keeps a speech model, never the text model.
        let next_model = if role == ConnectionRole::Audio && url.is_empty() {
            byok_provider(self.endpoint.read(cx).value().as_str())
                .and_then(|p| p.transcription_model)
                .unwrap_or("gpt-4o-mini-transcribe")
                .to_string()
        } else {
            next_model
        };
        model.update(cx, |input, cx| input.set_value(next_model, window, cx));
        key.update(cx, |input, cx| input.set_value(next_key, window, cx));
        match role {
            ConnectionRole::Text => {
                self.settings_ui.byok_preset = provider.map(|p| p.id);
                if self.settings_ui.image_endpoint.read(cx).value().is_empty()
                    && !provider.is_some_and(|p| {
                        starter_image_model(p.id)
                            .is_some_and(|vision| model.read(cx).value().as_str() == vision)
                    })
                {
                    self.settings_ui.image_enabled_draft = Some(false);
                }
                self.settings_ui.remove_key = false;
                self.settings_ui.provider_dirty.extend(["endpoint", "chat"]);
                if self
                    .settings_ui
                    .transcription_endpoint
                    .read(cx)
                    .value()
                    .is_empty()
                {
                    if let Some(speech) = provider.and_then(|p| p.transcription_model) {
                        self.stt_model
                            .update(cx, |input, cx| input.set_value(speech, window, cx));
                        self.settings_ui.provider_dirty.insert("transcription");
                    } else {
                        self.settings_ui.audio_enabled_draft = Some(false);
                    }
                }
            }
            ConnectionRole::Audio => {
                self.settings_ui.remove_transcription_key = false;
                self.settings_ui.audio_enabled_draft = Some(true);
                self.settings_ui
                    .provider_dirty
                    .extend(["audio_endpoint", "transcription"]);
            }
            ConnectionRole::Images => {
                self.settings_ui.remove_image_key = false;
                self.settings_ui.image_enabled_draft = Some(true);
                self.settings_ui
                    .provider_dirty
                    .extend(["image_endpoint", "image_model"]);
            }
        }
        if provider.is_some() && default.is_none() {
            self.settings_ui.byok_advanced = true;
        }
        cx.notify();
    }

    fn connection_row(&self, role: ConnectionRole, cx: &mut Context<Self>) -> AnyElement {
        let p = self.settings_palette();
        let (endpoint, _, key) = self.connection_inputs(role);
        let url = endpoint.read(cx).value().to_string();
        let enabled = self.connection_enabled(role);
        let shared = role != ConnectionRole::Text && url.is_empty();
        let provider = byok_provider(&url);
        let name = if !enabled {
            "Off"
        } else if Settings::hosted_endpoint(&url) {
            "Palepale account"
        } else if shared {
            "Use text provider"
        } else {
            provider.map_or("Custom", |p| p.name)
        };
        let pending = self.settings_ui.provider_save_pending;
        let entity = cx.entity().downgrade();
        let picker = settings_action(
            SharedString::from(format!("connection-{}", role.id())),
            name,
            p,
        )
        .disabled(pending)
        .dropdown_menu(move |mut menu, _, _| {
            let mut choices: Vec<(&'static str, &'static str)> = Vec::new();
            choices.push(("Palepale account", "palepale"));
            if role != ConnectionRole::Text {
                choices.extend([("Off", "off"), ("Use text provider", "shared")]);
            }
            for provider in BYOK_PROVIDERS {
                if role == ConnectionRole::Audio && provider.transcription_model.is_none() {
                    continue;
                }
                if role == ConnectionRole::Images && starter_image_model(provider.id).is_none() {
                    continue;
                }
                choices.push((provider.name, provider.id));
            }
            choices.push(("Custom…", "custom"));
            for (label, choice) in choices {
                let entity = entity.clone();
                menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                    let _ = entity.update(cx, |this, cx| {
                        this.choose_connection(role, choice, window, cx)
                    });
                }));
            }
            menu
        });
        let mut row = div().flex().flex_col().gap_3().child(
            gpui_kit::component::form::Form::horizontal()
                .small()
                .label_width(px(80.))
                .child(
                    gpui_kit::component::form::Field::new()
                        .label("Provider")
                        .child(picker),
                ),
        );
        if enabled && Settings::hosted_endpoint(&url) {
            row = row
                .child(caption(
                    "Uses your connected Palepale account. Save, then load models.",
                    p,
                ))
                .child(
                    settings_action(
                        SharedString::from(format!("connection-palepale-{}", role.id())),
                        "Connect Palepale",
                        p,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.select_provider_mode(ProviderMode::Palepale, cx)
                    })),
                );
        }
        if enabled && !shared && !Settings::hosted_endpoint(&url) {
            row = row
                .child(
                    div()
                        .text_size(px(11.))
                        .child(self.connection_key_label(role, cx)),
                )
                .child(
                    div()
                        .debug_selector(move || format!("connection-key-input-{}", role.id()))
                        .child(
                            Input::new(&key)
                                .disabled(pending)
                                .aria_label(format!("{} API key", role.label())),
                        ),
                );
            if let Some(provider) = provider {
                row = row.child(
                    settings_action(
                        SharedString::from(format!("connection-key-{}", role.id())),
                        "Get key",
                        p,
                    )
                    .on_click(move |_, _, cx| cx.open_url(provider.setup_url)),
                );
            }
        }
        if enabled && shared {
            row = row.child(caption(
                if self.settings_ui.has_byok_key
                    && self
                        .endpoint
                        .read(cx)
                        .value()
                        .as_str()
                        .trim()
                        .trim_end_matches('/')
                        == self.settings.api_base_url.trim_end_matches('/')
                    && !self.settings_ui.remove_key
                {
                    "•••••••• Uses saved Text key"
                } else {
                    "Uses Text key"
                },
                p,
            ));
        }
        let help = match (role, enabled, shared) {
            (_, false, _) => "Disabled.",
            (ConnectionRole::Audio, _, true) => {
                "Uses the text key. The provider needs a speech model."
            }
            (ConnectionRole::Images, _, true) => "Uses the text key and model for screenshots.",
            (ConnectionRole::Images, _, false) => {
                "Screenshots and conversation context go to this provider."
            }
            (ConnectionRole::Audio, _, false) => "Transcribes microphone and system audio.",
            _ => "Answers questions and writes notes.",
        };
        if enabled {
            row = row.child(self.connection_model_picker(role, cx));
        }
        gpui_kit::component::group_box::GroupBox::new()
            .title(role.label())
            .child(row.when(self.settings_ui.byok_advanced, |row| {
                row.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(p.muted))
                        .child(help),
                )
            }))
            .into_any_element()
    }

    pub(super) fn compact_connections(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.settings_palette();
        let pending = self.settings_ui.provider_save_pending;
        let mut view = div().mt_4().flex().flex_col().gap_4().child(caption(
            "Choose providers and save keys. Models load automatically; blank keys keep saved keys.",
            p,
        ));
        for role in [
            ConnectionRole::Text,
            ConnectionRole::Audio,
            ConnectionRole::Images,
        ] {
            view = view.child(self.connection_row(role, cx));
        }
        view = view.child(caption(&self.settings_ui.byok_status, p));
        view = view.child(caption(
            "Test sends sample data and may use API credits.",
            p,
        ));
        view = view.child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(
                    settings_action("reload-byok-models", "Refresh models", p)
                        .disabled(
                            pending
                                || self.provider_draft_dirty(cx)
                                || self.settings_ui.byok_cancel.is_some()
                                || self.engine.is_none(),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.auto_load_byok_models(cx))),
                )
                .when(self.settings_ui.byok_cancel.is_some(), |row| {
                    row.child(
                        settings_action("cancel-connection-task", "Cancel", p).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.invalidate_byok_check();
                                cx.notify();
                            }),
                        ),
                    )
                })
                .child(
                    settings_action(
                        "connection-advanced",
                        if self.settings_ui.byok_advanced {
                            "Hide advanced"
                        } else {
                            "Advanced"
                        },
                        p,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.settings_ui.byok_advanced = !this.settings_ui.byok_advanced;
                        cx.notify();
                    })),
                )
                .child(
                    settings_action(
                        "connection-reveal",
                        if self.settings_ui.byok_reveal {
                            "Hide keys"
                        } else {
                            "Show keys"
                        },
                        p,
                    )
                    .disabled(pending)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.settings_ui.byok_reveal = !this.settings_ui.byok_reveal;
                        for key in [
                            &this.api_key,
                            &this.settings_ui.transcription_key,
                            &this.settings_ui.image_key,
                        ] {
                            key.update(cx, |input, cx| {
                                input.set_masked(!this.settings_ui.byok_reveal, window, cx)
                            });
                        }
                        cx.notify();
                    })),
                ),
        );
        if self.settings_ui.byok_advanced {
            for role in [
                ConnectionRole::Text,
                ConnectionRole::Audio,
                ConnectionRole::Images,
            ] {
                let (endpoint, model, _) = self.connection_inputs(role);
                view = view
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("{} settings", role.label())),
                    )
                    .child(div().text_size(px(11.)).child("API URL"))
                    .child(
                        Input::new(&endpoint)
                            .disabled(pending)
                            .aria_label(format!("{} API URL", role.label())),
                    )
                    .child(div().text_size(px(11.)).child("Model"))
                    .child(
                        Input::new(&model)
                            .disabled(pending)
                            .aria_label(format!("{} model", role.label())),
                    );
            }
            view = view
                .child(caption(
                    "Blank Audio/Images URLs use Text. Custom endpoints need a model ID.",
                    p,
                ))
                .child(
                    Switch::new("provider-remove-key")
                        .small()
                        .label("Remove text key")
                        .checked(self.settings_ui.remove_key)
                        .disabled(pending)
                        .on_change(cx.listener(|this, value, _, cx| {
                            this.settings_ui.remove_key = *value;
                            this.invalidate_byok_check();
                            cx.notify();
                        })),
                )
                .child(
                    Switch::new("remove-transcription-key")
                        .small()
                        .label("Remove audio key")
                        .checked(self.settings_ui.remove_transcription_key)
                        .disabled(pending)
                        .on_change(cx.listener(|this, value, _, cx| {
                            this.settings_ui.remove_transcription_key = *value;
                            this.invalidate_byok_check();
                            cx.notify();
                        })),
                )
                .child(
                    Switch::new("remove-image-key")
                        .small()
                        .label("Remove image key")
                        .checked(self.settings_ui.remove_image_key)
                        .disabled(pending)
                        .on_change(cx.listener(|this, value, _, cx| {
                            this.settings_ui.remove_image_key = *value;
                            this.invalidate_byok_check();
                            cx.notify();
                        })),
                )
                .child(self.settings_row(
                    Lucide::Mic,
                    "Microphone",
                    "Your voice",
                    self.setting_switch(
                        "provider-microphone",
                        "Microphone capture",
                        self.settings.microphone,
                        |s, v| s.microphone = v,
                        cx,
                    ),
                ))
                .child(self.settings_row(
                    Lucide::Headphones,
                    "System audio",
                    "Computer sound",
                    self.setting_switch(
                        "provider-system-audio",
                        "System audio capture",
                        self.settings.system_audio,
                        |s, v| s.system_audio = v,
                        cx,
                    ),
                ))
                .child(
                    Input::new(&self.settings_ui.chunk_seconds)
                        .disabled(pending)
                        .aria_label("Audio chunk seconds"),
                )
                .child(self.byok_checks(cx));
        }
        view.into_any_element()
    }
}
