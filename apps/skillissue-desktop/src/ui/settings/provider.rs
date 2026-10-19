use super::*;

impl NativeView {
    pub(in crate::ui) fn selected_hosted_model(&self) -> Option<&ProviderModel> {
        if self.settings_ui.catalog_loading || self.settings_ui.catalog_error.is_some() {
            return None;
        }
        self.settings_ui.catalog_models.iter().find(|model| {
            model.id == self.settings.palepale.chat_model && model.capabilities.streaming
        })
    }
    pub(in crate::ui) fn supports_audio_input(&self) -> bool {
        !self.settings.is_palepale() || self.selected_hosted_audio_model().is_some()
    }
    pub(in crate::ui) fn selected_hosted_audio_model(&self) -> Option<&ProviderModel> {
        if self.settings_ui.catalog_loading || self.settings_ui.catalog_error.is_some() {
            return None;
        }
        self.settings_ui.catalog_models.iter().find(|model| {
            model.id == self.settings.provider_transcription_model()
                && model.capabilities.streaming
                && model.capabilities.audio_input
        })
    }
    pub(in crate::ui) fn supports_image_input(&self) -> bool {
        !self.settings.is_palepale()
            || self
                .selected_hosted_model()
                .is_some_and(|model| model.capabilities.image_input)
    }
    pub(in crate::ui) fn provider_label(&self) -> &'static str {
        if self.settings.is_palepale() {
            "Palepale"
        } else {
            "BYOK"
        }
    }
    pub(in crate::ui) fn hosted_audio_unavailable_message(&self) -> &'static str {
        if self.settings_ui.catalog_loading {
            "Checking Palepale audio…"
        } else if self.settings_ui.catalog_error.is_some()
            || self.settings_ui.catalog_models.is_empty()
        {
            "Audio check failed. Refresh Palepale in AI providers."
        } else {
            "Palepale audio is unavailable. Set up BYOK Audio to record."
        }
    }
    pub(in crate::ui) fn hosted_image_unavailable_message(&self) -> &'static str {
        if self.settings_ui.catalog_loading {
            "Checking Palepale image support…"
        } else if self.selected_hosted_model().is_none() {
            "Image check failed. Refresh Palepale in AI providers."
        } else {
            "Images are unavailable. Type a question or set up BYOK Images."
        }
    }
    pub(in crate::ui) fn sync_settings_fields(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.settings_ui.provider_dirty.contains("prompt") {
            self.settings_ui.provider_prompt.update(cx, |input, cx| {
                input.set_value(self.settings.system_prompt.clone(), window, cx)
            });
        }
        if !self.settings_ui.provider_dirty.contains("seconds") {
            self.settings_ui.chunk_seconds.update(cx, |input, cx| {
                input.set_value(self.settings.audio_chunk_seconds.to_string(), window, cx)
            });
        }
    }
    pub(in crate::ui) fn sync_provider_config(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.settings_ui.provider_mode_dirty {
            self.settings_ui.provider_mode = self.settings.provider_mode;
        }
        if !self.settings_ui.palepale_model_dirty {
            self.settings_ui.palepale_model = self.settings.palepale.chat_model.clone();
        }
        if !self.settings_ui.palepale_reasoning_dirty {
            self.settings_ui.palepale_reasoning_effort =
                self.settings.palepale.reasoning_effort.clone();
        }
        if self.settings_ui.provider_bound {
            self.settings_ui.saved_until = Some(std::time::Instant::now() + Duration::from_secs(2));
        }
        if !self.settings_ui.provider_bound {
            for (name, input) in [
                ("endpoint", &self.endpoint),
                ("audio_endpoint", &self.settings_ui.transcription_endpoint),
                ("image_endpoint", &self.settings_ui.image_endpoint),
                ("image_model", &self.settings_ui.image_model),
                ("chat", &self.model),
                ("transcription", &self.stt_model),
                ("seconds", &self.settings_ui.chunk_seconds),
            ] {
                self.settings_ui.subscriptions.push(cx.subscribe_in(
                    input,
                    window,
                    move |this, input, event, _, cx| {
                        if matches!(event, EditorEvent::Change) {
                            let current = input.read(cx).value().to_string();
                            let saved = match name {
                                "endpoint" => this.settings.api_base_url.clone(),
                                "audio_endpoint" => this.settings.transcription_base_url.clone(),
                                "image_endpoint" => this.settings.image_base_url.clone(),
                                "image_model" => this.settings.image_model.clone(),
                                "chat" => this.settings.chat_model.clone(),
                                "transcription" => this.settings.transcription_model.clone(),
                                _ => this.settings.audio_chunk_seconds.to_string(),
                            };
                            if current != saved || this.settings_ui.provider_dirty.contains(name) {
                                this.invalidate_byok_check();
                            }
                            if current == saved {
                                this.settings_ui.provider_dirty.remove(name);
                            } else {
                                this.settings_ui.provider_dirty.insert(name);
                            }
                            cx.notify();
                        }
                    },
                ));
            }
            self.settings_ui.subscriptions.push(cx.subscribe_in(
                &self.settings_ui.provider_prompt,
                window,
                |this, input, event, _, cx| {
                    if matches!(event, EditorEvent::Change) {
                        if input.read(cx).value().as_str() == this.settings.system_prompt {
                            this.settings_ui.provider_dirty.remove("prompt");
                        } else {
                            this.settings_ui.provider_dirty.insert("prompt");
                        }
                        cx.notify();
                    }
                },
            ));
            for input in [
                &self.api_key,
                &self.settings_ui.palepale_key,
                &self.settings_ui.transcription_key,
                &self.settings_ui.image_key,
            ] {
                self.settings_ui.subscriptions.push(cx.subscribe_in(
                    input,
                    window,
                    |this, input, event, _, cx| {
                        if matches!(event, EditorEvent::Change) {
                            if !input.read(cx).value().is_empty() {
                                this.invalidate_byok_check();
                            }
                            cx.notify();
                        }
                    },
                ));
            }
            self.settings_ui.subscriptions.push(cx.subscribe_in(
                &self.settings_ui.byok_filter,
                window,
                |_, _, event, _, cx| {
                    if matches!(event, EditorEvent::Change) {
                        cx.notify();
                    }
                },
            ));
            self.settings_ui.provider_bound = true;
        }
        for (name, input, value) in [
            (
                "endpoint",
                &self.endpoint,
                self.settings.api_base_url.clone(),
            ),
            (
                "audio_endpoint",
                &self.settings_ui.transcription_endpoint,
                self.settings.transcription_base_url.clone(),
            ),
            (
                "image_endpoint",
                &self.settings_ui.image_endpoint,
                self.settings.image_base_url.clone(),
            ),
            (
                "image_model",
                &self.settings_ui.image_model,
                self.settings.image_model.clone(),
            ),
            ("chat", &self.model, self.settings.chat_model.clone()),
            (
                "transcription",
                &self.stt_model,
                self.settings.transcription_model.clone(),
            ),
        ] {
            if !self.settings_ui.provider_dirty.contains(name) {
                input.update(cx, |input, cx| input.set_value(value, window, cx));
            }
        }
        self.sync_settings_fields(window, cx);
    }
    pub(in crate::ui) fn provider_draft_dirty(&self, cx: &App) -> bool {
        (self.settings_ui.provider_mode == ProviderMode::Byok
            && self.settings_ui.provider_mode_dirty)
            || self.settings_ui.provider_dirty.contains("prompt")
            || if self.settings_ui.provider_mode == ProviderMode::Palepale {
                false
            } else {
                self.settings_ui
                    .provider_dirty
                    .iter()
                    .any(|field| *field != "prompt")
                    || self.settings_ui.audio_enabled_draft.is_some()
                    || self.settings_ui.image_enabled_draft.is_some()
                    || self.settings_ui.remove_image_key
                    || !self.settings_ui.image_key.read(cx).value().is_empty()
                    || self.settings_ui.remove_transcription_key
                    || !self
                        .settings_ui
                        .transcription_key
                        .read(cx)
                        .value()
                        .is_empty()
                    || self.settings_ui.remove_key
                    || !self.api_key.read(cx).value().is_empty()
            }
    }

    #[cfg(test)]
    fn byok_key_status(&self, cx: &App) -> &'static str {
        let Ok(endpoint) =
            skillissue_core::provider::validate_base_url(self.endpoint.read(cx).value().as_str())
        else {
            return "Enter a valid provider endpoint before saving its API key.";
        };
        if endpoint != self.settings.api_base_url {
            "Save the endpoint to check its key. Keys are separate per endpoint."
        } else if self.settings_ui.has_byok_key {
            "A BYOK key is saved for this provider endpoint."
        } else {
            "BYOK and Palepale keys are separate."
        }
    }

    pub(in crate::ui) fn select_provider_mode(
        &mut self,
        mode: ProviderMode,
        cx: &mut Context<Self>,
    ) {
        if self.settings_ui.provider_save_pending {
            return;
        }
        if mode != ProviderMode::Palepale {
            self.cancel_palepale_pairing(cx);
        }
        self.cancel_palepale_checks();
        self.invalidate_byok_check();
        self.settings_ui.provider_mode = mode;
        self.settings_ui.provider_mode_dirty = mode != self.settings.provider_mode;
        if mode == ProviderMode::Palepale
            && self.settings_ui.has_palepale_key
            && self.settings.provider_mode != ProviderMode::Palepale
            && self.engine.is_some()
            && !self.snapshot.active
            && !self.busy
            && !self.notes_busy
            && !self.saved_question_busy
        {
            self.settings_ui.provider_save_pending = true;
            self.background(|engine| Reply::ProviderConnection(engine.activate_saved_palepale()));
        }
        if mode == ProviderMode::Palepale
            && self.settings.is_palepale()
            && self.settings_ui.has_palepale_key
        {
            self.refresh_palepale_connection(cx);
        }
        cx.notify();
    }

    /// Starts the hosted browser pairing flow. The browser receives only the public six digit
    /// approval code; the private device code remains inside the core transport.
    pub(in crate::ui) fn start_palepale_pairing(&mut self, cx: &mut Context<Self>) {
        use settings::PalepalePairingState;

        if self.engine.is_none()
            || self.gallery
            || self.smoke
            || self.smoke_gallery
            || self.settings_ui.provider_save_pending
            || matches!(
                self.settings_ui.palepale_pairing,
                PalepalePairingState::Starting | PalepalePairingState::Waiting { .. }
            )
        {
            return;
        }
        if let Some(cancel) = self.settings_ui.palepale_pairing_cancel.take() {
            if let Some(engine) = &self.engine {
                engine.cancel_palepale_pairing(cancel);
            } else {
                cancel.cancel();
            }
        }
        self.settings_ui.palepale_pairing_generation =
            self.settings_ui.palepale_pairing_generation.wrapping_add(1);
        let generation = self.settings_ui.palepale_pairing_generation;
        let cancel = tokio_util::sync::CancellationToken::new();
        self.settings_ui.palepale_pairing_cancel = Some(cancel.clone());
        self.settings_ui.palepale_pairing = PalepalePairingState::Starting;
        self.error = None;
        let sender = self.sender.clone();
        self.runtime.spawn({
            let engine = self.engine.clone().expect("engine checked above");
            let cancel = cancel.clone();
            async move {
                let result = engine.start_palepale_pairing(cancel.clone()).await;
                match result {
                    Ok(pairing) => {
                        let public = Ok((
                            pairing.user_code().to_owned(),
                            pairing.browser_url(),
                            pairing.ttl().as_secs().max(1),
                        ));
                        let _ = sender.send(Reply::PairingStarted(generation, public));
                        let result =
                            match engine.wait_palepale_pairing(pairing, cancel.clone()).await {
                                Ok(key) => engine.finish_palepale_pairing(key, cancel).await,
                                Err(error) => Err(error),
                            };
                        let _ = sender.send(Reply::PairingReady(generation, result));
                    }
                    Err(error) => {
                        let _ = sender.send(Reply::PairingStarted(generation, Err(error)));
                    }
                }
            }
        });
        cx.notify();
    }

    pub(in crate::ui) fn cancel_palepale_pairing(&mut self, cx: &mut Context<Self>) {
        use settings::PalepalePairingState;

        if let Some(cancel) = self.settings_ui.palepale_pairing_cancel.take() {
            if let Some(engine) = &self.engine {
                engine.cancel_palepale_pairing(cancel);
            } else {
                cancel.cancel();
            }
        }
        self.settings_ui.palepale_pairing_generation =
            self.settings_ui.palepale_pairing_generation.wrapping_add(1);
        if matches!(
            self.settings_ui.palepale_pairing,
            PalepalePairingState::Starting | PalepalePairingState::Waiting { .. }
        ) {
            self.settings_ui.palepale_pairing = PalepalePairingState::Idle;
        }
        cx.notify();
    }

    pub(in crate::ui) fn receive_palepale_pairing_started(
        &mut self,
        generation: u64,
        result: Result<(String, String, u64), String>,
        cx: &mut Context<Self>,
    ) {
        use settings::PalepalePairingState;

        if generation != self.settings_ui.palepale_pairing_generation
            || !matches!(
                self.settings_ui.palepale_pairing,
                PalepalePairingState::Starting
            )
        {
            return;
        }
        match result {
            Ok((code, url, ttl_seconds)) => {
                self.settings_ui.palepale_pairing = PalepalePairingState::Waiting {
                    code,
                    url: url.clone(),
                    ttl_seconds,
                };
                cx.open_url(&url);
            }
            Err(error) => {
                self.settings_ui.palepale_pairing = PalepalePairingState::Error(error);
                self.settings_ui.palepale_pairing_cancel = None;
            }
        }
        cx.notify();
    }

    pub(in crate::ui) fn receive_palepale_pairing_ready(
        &mut self,
        generation: u64,
        result: Result<ConfigView, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use settings::PalepalePairingState;

        if generation != self.settings_ui.palepale_pairing_generation
            || !matches!(
                self.settings_ui.palepale_pairing,
                PalepalePairingState::Waiting { .. }
            )
        {
            return;
        }
        self.settings_ui.palepale_pairing_cancel = None;
        match result {
            Ok(config) => {
                self.cancel_palepale_checks();
                self.settings_ui.catalog_models.clear();
                self.settings_ui.catalog_error = None;
                self.settings_ui.palepale_pairing = PalepalePairingState::Ready;
                self.settings_ui.provider_mode = ProviderMode::Palepale;
                self.settings_ui.provider_mode_dirty = false;
                self.settings_ui.provider_save_pending = false;
                self.status = "Palepale connection saved · checking availability".into();
                let config = self
                    .engine
                    .as_ref()
                    .and_then(|engine| engine.config().ok())
                    .unwrap_or(config);
                self.set_config(config, window, cx);
            }
            Err(error) => {
                self.settings_ui.palepale_pairing = PalepalePairingState::Error(error);
                if let Some(engine) = &self.engine
                    && let Ok(config) = engine.config()
                {
                    self.set_config(config, window, cx);
                }
                self.error = Some("Palepale pairing did not complete. Try again.".into());
            }
        }
        cx.notify();
    }

    pub(in crate::ui) fn disconnect_palepale(&mut self, cx: &mut Context<Self>) {
        use settings::PalepalePairingState;

        self.cancel_palepale_pairing(cx);
        self.cancel_palepale_checks();
        if self.engine.is_none() || self.settings_ui.provider_save_pending {
            return;
        }
        self.settings_ui.provider_save_pending = true;
        self.background(move |engine| Reply::ProviderConnection(engine.disconnect_palepale()));
        self.settings_ui.palepale_pairing = PalepalePairingState::Idle;
        cx.notify();
    }

    pub(in crate::ui) fn cancel_palepale_checks(&mut self) {
        if let Some(cancel) = self.settings_ui.catalog_cancel.take() {
            cancel.cancel();
        }
        self.settings_ui.catalog_generation = self.settings_ui.catalog_generation.wrapping_add(1);
        self.settings_ui.catalog_loading = false;
        self.cancel_palepale_account();
    }

    pub(in crate::ui) fn cancel_palepale_account(&mut self) {
        if let Some(cancel) = self.settings_ui.palepale_account_cancel.take() {
            cancel.cancel();
        }
        self.settings_ui.palepale_account_generation =
            self.settings_ui.palepale_account_generation.wrapping_add(1);
        self.settings_ui.palepale_account_loading = false;
        self.settings_ui.palepale_account_result = None;
    }

    pub(in crate::ui) fn refresh_palepale_connection(&mut self, cx: &mut Context<Self>) {
        if self.gallery
            || self.smoke
            || self.smoke_gallery
            || self.settings_ui.provider_save_pending
            || self.engine.is_none()
        {
            return;
        }
        self.cancel_palepale_checks();
        self.refresh_palepale_catalog(cx);
    }

    pub(in crate::ui) fn reconnect_palepale(&mut self, cx: &mut Context<Self>) {
        if self.engine.is_none()
            || self.gallery
            || self.smoke
            || self.smoke_gallery
            || self.settings_ui.provider_save_pending
            || self.snapshot.active
            || self.busy
            || self.notes_busy
            || self.saved_question_busy
        {
            return;
        }
        self.cancel_palepale_checks();
        self.settings_ui.catalog_models.clear();
        self.settings_ui.catalog_error = None;
        self.settings_ui.provider_save_pending = true;
        self.background(|engine| Reply::ProviderConnection(engine.activate_saved_palepale()));
        cx.notify();
    }

    pub(in crate::ui) fn refresh_palepale_account(&mut self, cx: &mut Context<Self>) {
        if self.gallery
            || self.smoke
            || self.smoke_gallery
            || self.settings_ui.provider_save_pending
            || !self.settings_ui.has_palepale_key
            || !self.settings.is_palepale()
        {
            return;
        }
        let Some(engine) = self.engine.clone() else {
            return;
        };
        if let Some(cancel) = self.settings_ui.palepale_account_cancel.take() {
            cancel.cancel();
        }
        self.settings_ui.palepale_account_generation =
            self.settings_ui.palepale_account_generation.wrapping_add(1);
        let generation = self.settings_ui.palepale_account_generation;
        let cancel = tokio_util::sync::CancellationToken::new();
        self.settings_ui.palepale_account_cancel = Some(cancel.clone());
        self.settings_ui.palepale_account_loading = true;
        self.settings_ui.palepale_account_result = None;
        let sender = self.sender.clone();
        self.runtime.spawn(async move {
            let result = engine.palepale_account(cancel).await;
            let _ = sender.send(Reply::PalepaleAccount(generation, result));
        });
        self.sync_provider_status_to_overlay(cx);
        cx.notify();
    }

    pub(in crate::ui) fn receive_palepale_account(
        &mut self,
        generation: u64,
        result: Result<skillissue_core::PalepaleAccountStatus, String>,
    ) {
        if generation != self.settings_ui.palepale_account_generation
            || !self.settings_ui.palepale_account_loading
        {
            return;
        }
        self.settings_ui.palepale_account_loading = false;
        self.settings_ui.palepale_account_cancel = None;
        self.settings_ui.palepale_account_result = Some(result);
    }

    pub(in crate::ui) fn palepale_connection_status(&self) -> (&'static str, String) {
        if !self.settings_ui.has_palepale_key {
            return (
                "Not connected",
                "Approve this device in your browser to connect.".into(),
            );
        }
        if self.settings_ui.provider_save_pending {
            return (
                "Updating connection",
                "Saving the connection change…".into(),
            );
        }
        if !self.settings.is_palepale() {
            return (
                "Connection saved",
                "Reconnect to use the saved Palepale connection.".into(),
            );
        }
        if self.settings_ui.palepale_account_loading || self.settings_ui.catalog_loading {
            return (
                "Checking Palepale",
                "Checking the saved connection and available model…".into(),
            );
        }
        if let Some(error) = &self.settings_ui.catalog_error {
            return ("Palepale unavailable", error.clone());
        }
        match &self.settings_ui.palepale_account_result {
            Some(Err(error)) => return ("Palepale unavailable", error.clone()),
            Some(Ok(status)) if !status.can_spend => {
                return (
                    "Account cannot make requests",
                    "Open your Palepale account to check availability, then refresh.".into(),
                );
            }
            Some(Ok(_)) => {}
            None => {
                return (
                    "Connection saved",
                    "Refresh to check this device's connection.".into(),
                );
            }
        }
        if self.selected_hosted_model().is_none() {
            return (
                "Model unavailable",
                "Refresh to get an available model from Palepale.".into(),
            );
        }
        (
            "Palepale",
            format!(
                "Chat: {}. Screenshots: {}. Audio transcription: {}.",
                self.selected_hosted_model()
                    .map(|model| model.display_name.as_str())
                    .unwrap_or("Unavailable"),
                self.selected_hosted_model()
                    .filter(|model| model.capabilities.image_input)
                    .map(|model| model.display_name.as_str())
                    .unwrap_or("Unavailable"),
                self.selected_hosted_audio_model()
                    .map(|model| model.display_name.as_str())
                    .unwrap_or("Unavailable")
            ),
        )
    }

    pub(in crate::ui) fn refresh_palepale_catalog(&mut self, cx: &mut Context<Self>) {
        if self.gallery
            || self.smoke
            || self.smoke_gallery
            || self.settings_ui.provider_save_pending
        {
            return;
        }
        let Some(engine) = self.engine.clone() else {
            return;
        };
        if let Some(cancel) = self.settings_ui.catalog_cancel.take() {
            cancel.cancel();
        }
        self.settings_ui.catalog_generation = self.settings_ui.catalog_generation.wrapping_add(1);
        let generation = self.settings_ui.catalog_generation;
        let cancel = tokio_util::sync::CancellationToken::new();
        self.settings_ui.catalog_cancel = Some(cancel.clone());
        self.settings_ui.catalog_loading = true;
        self.settings_ui.catalog_error = None;
        let sender = self.sender.clone();
        self.runtime.spawn(async move {
            let result = engine.refresh_palepale_models(cancel).await;
            let _ = sender.send(Reply::Catalog(generation, result));
        });
        self.sync_provider_status_to_overlay(cx);
        cx.notify();
    }

    pub(in crate::ui) fn receive_provider_catalog(
        &mut self,
        generation: u64,
        result: Result<Vec<ProviderModel>, String>,
    ) {
        if generation != self.settings_ui.catalog_generation || !self.settings_ui.catalog_loading {
            return;
        }
        self.settings_ui.catalog_loading = false;
        self.settings_ui.catalog_cancel = None;
        match result {
            Ok(models) => {
                self.settings_ui.catalog_models = models;
                self.settings_ui.catalog_error = None;
            }
            Err(error) => self.settings_ui.catalog_error = Some(error),
        }
    }

    #[allow(dead_code)]
    pub(in crate::ui) fn select_palepale_model(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.settings_ui.provider_save_pending {
            return;
        }
        let Some(model) = self
            .settings_ui
            .catalog_models
            .iter()
            .find(|model| model.id == id)
        else {
            return;
        };
        if id != self.settings_ui.palepale_model
            && self
                .settings_ui
                .palepale_reasoning_effort
                .as_deref()
                .is_some_and(|effort| !model.supports_reasoning_effort(effort))
        {
            self.settings_ui.palepale_reasoning_effort = None;
            self.settings_ui.palepale_reasoning_dirty =
                self.settings.palepale.reasoning_effort.is_some();
            self.error = Some("The previous reasoning effort is unavailable for this model. Reasoning is set to Server default; choose an option and save settings.".into());
        }
        self.settings_ui.palepale_model = id.into();
        self.settings_ui.palepale_model_dirty = id != self.settings.palepale.chat_model;
        cx.notify();
    }

    #[allow(dead_code)]
    pub(in crate::ui) fn palepale_connection_saved(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_ui.provider_mode_dirty = false;
        self.settings_ui.provider_save_pending = false;
        self.cancel_palepale_checks();
        self.settings_ui.catalog_models.clear();
        self.settings_ui.catalog_error = None;
        self.settings_ui.palepale_model_dirty = false;
        self.settings_ui.palepale_reasoning_dirty = false;
        self.settings_ui.remove_palepale_key = false;
        self.settings_ui
            .palepale_key
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    #[allow(dead_code)]
    pub(in crate::ui) fn provider_saved(
        &mut self,
        mode: ProviderMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_ui.provider_dirty.remove("prompt");
        self.settings_ui.provider_mode_dirty = false;
        self.settings_ui.provider_save_pending = false;
        if mode == ProviderMode::Palepale {
            self.palepale_connection_saved(window, cx);
        } else {
            self.settings_ui.provider_dirty.clear();
            self.settings_ui.remove_key = false;
            self.settings_ui.remove_transcription_key = false;
            self.settings_ui.byok_reveal = false;
            self.settings_ui.byok_preset = None;
            self.settings_ui.audio_enabled_draft = None;
            self.settings_ui.image_enabled_draft = None;
            self.settings_ui.remove_image_key = false;
            self.settings_ui.connection_drafts.clear();
            self.settings_ui.image_key.update(cx, |input, cx| {
                input.set_value("", window, cx);
                input.set_masked(true, window, cx);
            });
            for input in [&self.api_key, &self.settings_ui.transcription_key] {
                input.update(cx, |input, cx| input.set_masked(true, window, cx));
            }
            self.settings_ui
                .transcription_key
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.api_key
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
    }

    pub(in crate::ui) fn settings_save_provider(&mut self, cx: &mut Context<Self>) {
        if self.settings_ui.provider_save_pending || self.engine.is_none() {
            return;
        }
        let mode = self.settings_ui.provider_mode;
        let seconds = if mode == ProviderMode::Byok {
            match self
                .settings_ui
                .chunk_seconds
                .read(cx)
                .value()
                .parse::<u32>()
            {
                Ok(seconds) => seconds,
                Err(_) => {
                    self.error = Some("Use a whole number from 3 to 20 seconds.".into());
                    cx.notify();
                    return;
                }
            }
        } else {
            self.settings.audio_chunk_seconds
        };
        let api_base_url = self.endpoint.read(cx).value().to_string();
        let image_base_url = self.settings_ui.image_endpoint.read(cx).value().to_string();
        let image_model = self.settings_ui.image_model.read(cx).value().to_string();
        let image_key =
            if mode == ProviderMode::Palepale || Settings::hosted_endpoint(&image_base_url) {
                None
            } else if self.settings_ui.remove_image_key {
                Some(String::new())
            } else {
                let key = self.settings_ui.image_key.read(cx).value().to_string();
                if key.trim().is_empty() {
                    None
                } else {
                    Some(key)
                }
            };
        let audio_enabled = self.settings_ui.audio_enabled_draft;
        let image_enabled = self.settings_ui.image_enabled_draft;
        let transcription_base_url = self
            .settings_ui
            .transcription_endpoint
            .read(cx)
            .value()
            .to_string();
        let transcription_key = if mode == ProviderMode::Palepale
            || Settings::hosted_endpoint(&transcription_base_url)
        {
            None
        } else if self.settings_ui.remove_transcription_key {
            Some(String::new())
        } else {
            let key = self
                .settings_ui
                .transcription_key
                .read(cx)
                .value()
                .to_string();
            if key.trim().is_empty() {
                None
            } else {
                Some(key)
            }
        };
        let chat_model = self.model.read(cx).value().to_string();
        let transcription_model = self.stt_model.read(cx).value().to_string();
        let system_prompt = self
            .settings_ui
            .provider_prompt
            .read(cx)
            .value()
            .to_string();
        let (key, remove) =
            if mode == ProviderMode::Palepale || Settings::hosted_endpoint(&api_base_url) {
                (String::new(), false)
            } else {
                (
                    self.api_key.read(cx).value().to_string(),
                    self.settings_ui.remove_key,
                )
            };
        self.invalidate_byok_check();
        self.settings_ui.provider_save_pending = true;
        self.error = None;
        self.background(move |engine| {
            Reply::ProviderConfig(
                mode,
                engine.update_media_configuration(
                    move |settings| {
                        settings.provider_mode = mode;
                        if mode == ProviderMode::Palepale {
                            settings.palepale.reasoning_effort = None;
                        } else {
                            settings.api_base_url = api_base_url;
                            settings.transcription_base_url = transcription_base_url;
                            settings.image_base_url = image_base_url;
                            settings.image_model = image_model;
                            if let Some(enabled) = audio_enabled {
                                settings.byok_audio_enabled = enabled;
                            }
                            if let Some(enabled) = image_enabled {
                                settings.include_screenshot = enabled;
                            }
                            settings.chat_model = chat_model;
                            settings.transcription_model = transcription_model;
                            settings.audio_chunk_seconds = seconds;
                        }
                        settings.system_prompt = system_prompt;
                        Ok(())
                    },
                    if remove {
                        Some(String::new())
                    } else if key.trim().is_empty() {
                        None
                    } else {
                        Some(key)
                    },
                    transcription_key,
                    image_key,
                ),
            )
        });
        cx.notify();
    }
    pub(in crate::ui) fn settings_provider(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.settings_palette();
        let label = |text: &str, control: AnyElement| {
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_size(px(12.))
                        .line_height(px(18.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(text.to_owned()),
                )
                .child(control)
        };
        let pending = self.settings_ui.provider_save_pending;
        let hosted = self.settings_ui.provider_mode == ProviderMode::Palepale;
        let mut mode_selector = div().flex().gap_2();
        for (mode, name) in [
            (ProviderMode::Byok, "Bring your own key"),
            (ProviderMode::Palepale, "Palepale account"),
        ] {
            mode_selector = mode_selector.child(
                settings_action(SharedString::from(format!("provider-mode-{name}")), name, p)
                    .selected(self.settings_ui.provider_mode == mode)
                    .disabled(pending)
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.select_provider_mode(mode, cx)),
                    ),
            );
        }
        let mut content = div()
            .child(section_title(
                "AI provider",
                "Choose how skillissue connects to AI.",
                p,
            ))
            .child(div().mt_4().child(mode_selector));
        if hosted {
            use settings::PalepalePairingState;
            let pairing = self.settings_ui.palepale_pairing.clone();
            let saved = self.settings_ui.has_palepale_key;
            let waiting = matches!(
                &pairing,
                PalepalePairingState::Starting | PalepalePairingState::Waiting { .. }
            );
            let provider_busy =
                self.snapshot.active || self.busy || self.notes_busy || self.saved_question_busy;
            let (connection_title, connection_body) = self.palepale_connection_status();
            let pairing_body = match &pairing {
                PalepalePairingState::Idle if saved => connection_body.clone(),
                PalepalePairingState::Idle => "Connect Palepale in your browser.".to_owned(),
                PalepalePairingState::Starting => "Requesting a secure pairing code…".to_owned(),
                PalepalePairingState::Waiting {
                    code, ttl_seconds, ..
                } => format!(
                    "Approve this device in the browser. Code {code} · expires in about {} min.",
                    (*ttl_seconds / 60).max(1)
                ),
                PalepalePairingState::Ready => connection_body,
                PalepalePairingState::Error(error) => {
                    format!("{error} Try again to start a fresh pairing.")
                }
            };
            let pair_action = if saved {
                settings_action("palepale-disconnect", "Disconnect", p)
                    .disabled(pending || waiting || provider_busy || self.engine.is_none())
                    .on_click(cx.listener(|this, _, _, cx| this.disconnect_palepale(cx)))
            } else {
                settings_action(
                    "palepale-connect",
                    match &pairing {
                        PalepalePairingState::Starting => "Starting…",
                        PalepalePairingState::Waiting { .. } => "Waiting for approval…",
                        PalepalePairingState::Error(_) => "Try again",
                        _ => "Connect Palepale",
                    },
                    p,
                )
                .disabled(pending || waiting || provider_busy || self.engine.is_none())
                .on_click(cx.listener(|this, _, _, cx| this.start_palepale_pairing(cx)))
            };
            let waiting_actions = if let PalepalePairingState::Waiting { url, .. } = &pairing {
                let url = url.clone();
                div()
                    .flex()
                    .gap_2()
                    .child(
                        settings_action("palepale-open-pairing", "Open approval page", p)
                            .on_click(move |_, _, cx| cx.open_url(&url)),
                    )
                    .child(
                        settings_action("palepale-cancel-pairing", "Cancel", p).on_click(
                            cx.listener(|this, _, _, cx| this.cancel_palepale_pairing(cx)),
                        ),
                    )
            } else if matches!(&pairing, PalepalePairingState::Starting) {
                div().child(
                    settings_action("palepale-cancel-pairing", "Cancel", p)
                        .on_click(cx.listener(|this, _, _, cx| this.cancel_palepale_pairing(cx))),
                )
            } else if saved {
                let reconnect = !self.settings.is_palepale() || !self.has_key;
                div()
                    .flex()
                    .gap_2()
                    .child(
                        settings_action(
                            "palepale-refresh",
                            if reconnect {
                                "Reconnect"
                            } else {
                                "Refresh status"
                            },
                            p,
                        )
                        .disabled(
                            pending
                                || provider_busy
                                || self.engine.is_none()
                                || self.settings_ui.catalog_loading
                                || self.settings_ui.palepale_account_loading,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if reconnect {
                                this.reconnect_palepale(cx);
                            } else {
                                this.refresh_palepale_connection(cx);
                            }
                        })),
                    )
                    .when(
                        self.settings_ui
                            .palepale_account_result
                            .as_ref()
                            .is_some_and(Result::is_err),
                        |view| {
                            view.child(
                                settings_action("palepale-pair-again", "Pair again", p)
                                    .disabled(pending || provider_busy || self.engine.is_none())
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.start_palepale_pairing(cx)
                                    })),
                            )
                        },
                    )
            } else {
                div()
            };
            content = content.child(
                div()
                    .mt_6()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        div()
                            .p_4()
                            .rounded(px(8.))
                            .border_1()
                            .border_color(rgb(p.line))
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_3()
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(if waiting {
                                                "Connecting Palepale"
                                            } else if saved {
                                                connection_title
                                            } else {
                                                "Connect Palepale"
                                            }),
                                    )
                                    .child(pair_action),
                            )
                            .child(caption(&pairing_body, p))
                            .child(waiting_actions),
                    )
                    .child(self.palepale_usage_panel(cx))
                    .child(
                        settings_action("palepale-account", "Open Palepale account", p)
                            .on_click(|_, _, cx| cx.open_url("https://palepale.tech")),
                    ),
            );
        } else {
            content = content.child(self.compact_connections(cx));
        }

        content
            .when(hosted || self.settings_ui.byok_advanced, |view| {
                view.child(
                    div().mt_5().child(label(
                        "System prompt",
                        Textarea::new(&self.settings_ui.provider_prompt)
                            .h(px(108.))
                            .font_family("JetBrains Mono")
                            .text_size(px(12.))
                            .line_height(px(18.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(p.foreground))
                            .rounded(px(6.))
                            .border_1()
                            .border_color(rgb(p.line))
                            .disabled(pending)
                            .aria_label("System prompt")
                            .into_any_element(),
                    )),
                )
            })
            .into_any_element()
    }
}

#[cfg(all(test, feature = "native-effects"))]
mod tests {
    use super::{NativeView, Page, Reply, SettingsTab};
    use gpui_kit::{
        AnyWindowHandle, AppContext, Context, Entity, TestAppContext, Window, WindowOptions,
        component::Theme,
    };
    use skillissue_domain::models::{ConfigView, ProviderMode, Settings};
    use std::{path::PathBuf, sync::Arc};

    fn fixture(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<NativeView>, PathBuf) {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("inert provider test runtime"),
        );
        let directory = std::env::temp_dir().join(format!(
            "skillissue-provider-drafts-{}",
            uuid::Uuid::new_v4()
        ));
        let (handle, view) = cx.update(|cx| {
            if !cx.has_global::<Theme>() {
                gpui_kit::init(cx);
            }
            gpui_kit::open_window(
                WindowOptions {
                    focus: false,
                    titlebar: None,
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    cx.new(|cx| {
                        let mut view =
                            NativeView::new_for_test(runtime, directory.clone(), window, cx);
                        view.gallery = true;
                        view.page = Page::Settings;
                        view.settings_ui.tab = SettingsTab::Provider;
                        view
                    })
                },
            )
            .expect("inert provider test window")
        });
        (handle, view, directory)
    }

    fn edit_form(view: &mut NativeView, window: &mut Window, cx: &mut Context<NativeView>) {
        for (input, value) in [
            (&view.endpoint, "http://127.0.0.1:4567/v1"),
            (&view.model, "draft-chat"),
            (&view.stt_model, "draft-transcription"),
            (&view.api_key, "synthetic-unsaved-key"),
            (&view.settings_ui.chunk_seconds, "11"),
        ] {
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
        view.settings_ui.provider_prompt.update(cx, |input, cx| {
            input.set_value("Unsaved system prompt", window, cx)
        });
        view.settings_ui.provider_dirty.extend([
            "endpoint",
            "chat",
            "transcription",
            "seconds",
            "prompt",
        ]);
        view.settings_ui.remove_key = true;
    }

    fn config(settings: Settings) -> ConfigView {
        let hosted = settings.is_palepale();
        ConfigView {
            settings,
            has_api_key: hosted,
            has_byok_key: false,
            has_audio_key: false,
            has_image_key: false,
            has_palepale_key: hosted,
            platform: "test".into(),
            shortcuts: vec![],
            warnings: vec![],
        }
    }

    #[gpui_kit::test]
    fn hosted_connection_changes_preserve_unsaved_prompt_and_byok_form(cx: &mut TestAppContext) {
        let (handle, view, directory) = fixture(cx);
        for mode in [
            ProviderMode::Palepale,
            ProviderMode::Byok,
            ProviderMode::Palepale,
        ] {
            cx.update_window(handle, |_, window, cx| {
                view.update(cx, |view, cx| {
                    edit_form(view, window, cx);
                    let saved_prompt = view.settings.system_prompt.clone();
                    let mut saved = view.settings.clone();
                    saved.provider_mode = mode;
                    view.settings_ui.provider_mode_dirty = true;
                    view.settings_ui.provider_save_pending = true;
                    view.sender
                        .send(Reply::ProviderConnection(Ok(config(saved))))
                        .unwrap();
                    view.tick(window, cx);
                    assert_eq!(view.settings.provider_mode, mode);
                    assert_eq!(view.settings_ui.provider_mode, mode);
                    assert!(!view.settings_ui.provider_save_pending);
                    assert!(!view.settings_ui.provider_mode_dirty);
                    assert_eq!(view.settings.system_prompt, saved_prompt);
                    assert_eq!(
                        view.settings_ui.provider_prompt.read(cx).value().as_str(),
                        "Unsaved system prompt"
                    );
                    assert_eq!(
                        view.endpoint.read(cx).value().as_str(),
                        "http://127.0.0.1:4567/v1"
                    );
                    assert_eq!(view.model.read(cx).value().as_str(), "draft-chat");
                    assert_eq!(
                        view.stt_model.read(cx).value().as_str(),
                        "draft-transcription"
                    );
                    assert_eq!(
                        view.settings_ui.chunk_seconds.read(cx).value().as_str(),
                        "11"
                    );
                    assert_eq!(
                        view.api_key.read(cx).value().as_str(),
                        "synthetic-unsaved-key"
                    );
                    assert!(view.settings_ui.remove_key);
                    assert!(view.settings_ui.provider_dirty.contains("prompt"));
                    assert!(view.provider_draft_dirty(cx));
                });
            })
            .expect("receive connection acknowledgement");
        }
        assert!(!directory.exists());
    }

    #[gpui_kit::test]
    fn explicit_provider_save_clears_only_the_committed_form_draft(cx: &mut TestAppContext) {
        let (handle, view, directory) = fixture(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                edit_form(view, window, cx);
                let mut saved = view.settings.clone();
                saved.api_base_url = view.endpoint.read(cx).value().to_string();
                saved.chat_model = view.model.read(cx).value().to_string();
                saved.transcription_model = view.stt_model.read(cx).value().to_string();
                saved.audio_chunk_seconds = 11;
                saved.system_prompt = "Unsaved system prompt".into();
                view.settings_ui.provider_save_pending = true;
                view.sender
                    .send(Reply::ProviderConfig(ProviderMode::Byok, Ok(config(saved))))
                    .unwrap();
                view.tick(window, cx);
                assert!(!view.provider_draft_dirty(cx));
                assert!(view.api_key.read(cx).value().is_empty());
                assert!(!view.settings_ui.remove_key);
                let mut refreshed = view.settings.clone();
                refreshed.system_prompt = "New saved prompt".into();
                view.set_config(config(refreshed), window, cx);
                assert_eq!(
                    view.settings_ui.provider_prompt.read(cx).value().as_str(),
                    "New saved prompt"
                );
            });
        })
        .expect("receive explicit save acknowledgement");
        assert!(!directory.exists());
    }

    #[gpui_kit::test]
    fn failed_connection_keeps_the_form_editable_and_its_drafts(cx: &mut TestAppContext) {
        let (handle, view, directory) = fixture(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                edit_form(view, window, cx);
                let previous_mode = view.settings.provider_mode;
                view.settings_ui.provider_save_pending = true;
                view.sender
                    .send(Reply::ProviderConnection(Err(
                        "Synthetic connection failure".into(),
                    )))
                    .unwrap();
                view.tick(window, cx);
                assert_eq!(view.settings.provider_mode, previous_mode);
                assert!(!view.settings_ui.provider_save_pending);
                assert_eq!(view.error.as_deref(), Some("Synthetic connection failure"));
                assert_eq!(
                    view.settings_ui.provider_prompt.read(cx).value().as_str(),
                    "Unsaved system prompt"
                );
                assert!(view.provider_draft_dirty(cx));
            });
        })
        .expect("receive failed connection acknowledgement");
        assert!(!directory.exists());
    }

    #[gpui_kit::test]
    fn saved_byok_key_caption_only_describes_the_committed_endpoint(cx: &mut TestAppContext) {
        let (handle, view, directory) = fixture(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.settings.api_base_url = "https://old.example/v1".into();
                view.settings_ui.has_byok_key = true;
                view.endpoint.update(cx, |input, cx| {
                    input.set_value("https://old.example/v1/", window, cx)
                });
                assert!(view.byok_key_status(cx).starts_with("A BYOK key is saved"));
                view.endpoint.update(cx, |input, cx| {
                    input.set_value("https://new.example/v1", window, cx)
                });
                assert!(view.byok_key_status(cx).starts_with("Save the endpoint"));
                view.endpoint.update(cx, |input, cx| {
                    input.set_value("invalid endpoint", window, cx)
                });
                assert!(
                    view.byok_key_status(cx)
                        .starts_with("Enter a valid provider endpoint")
                );
                assert!(view.engine.is_none());
            });
        })
        .expect("inspect endpoint-specific key status");
        assert!(!directory.exists());
    }
    #[gpui_kit::test]
    fn byok_check_cancellation_rejects_stale_results_and_audio_drafts_survive(
        cx: &mut TestAppContext,
    ) {
        let (handle, view, directory) = fixture(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.settings_ui
                    .transcription_endpoint
                    .update(cx, |input, cx| {
                        input.set_value("https://audio.example/v1", window, cx)
                    });
                view.settings_ui
                    .transcription_key
                    .update(cx, |input, cx| input.set_value("audio-draft", window, cx));
                view.settings_ui.provider_dirty.insert("audio_endpoint");
                view.settings_ui.byok_generation = 20;
                let cancel = tokio_util::sync::CancellationToken::new();
                view.settings_ui.byok_cancel = Some(cancel.clone());
                view.invalidate_byok_check();
                assert!(cancel.is_cancelled());
                view.sender
                    .send(Reply::ByokModels(20, Ok(vec!["stale".into()])))
                    .unwrap();
                view.tick(window, cx);
                assert!(view.settings_ui.byok_models.is_empty());
                view.sync_provider_config(window, cx);
                assert_eq!(
                    view.settings_ui
                        .transcription_endpoint
                        .read(cx)
                        .value()
                        .as_str(),
                    "https://audio.example/v1"
                );
                assert_eq!(
                    view.settings_ui.transcription_key.read(cx).value().as_str(),
                    "audio-draft"
                );
                let generation = view.settings_ui.byok_generation;
                view.sender
                    .send(Reply::ByokModels(generation, Ok(vec!["fresh".into()])))
                    .unwrap();
                view.tick(window, cx);
                assert_eq!(view.settings_ui.byok_models, ["fresh"]);
                assert!(view.settings_ui.byok_status.contains("not verified"));
            });
        })
        .unwrap();
        assert!(!directory.exists());
    }
    #[gpui_kit::test]
    fn saved_key_indicators_follow_endpoint_and_pending_edits(cx: &mut TestAppContext) {
        use crate::ui::settings::connections::ConnectionRole;
        let (handle, view, directory) = fixture(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.sync_provider_config(window, cx);
                view.settings_ui.has_byok_key = true;
                assert_eq!(
                    view.connection_key_label(ConnectionRole::Text, cx),
                    "API key · •••••••• Saved"
                );
                view.api_key
                    .update(cx, |input, cx| input.set_value("replacement", window, cx));
                assert_eq!(
                    view.connection_key_label(ConnectionRole::Text, cx),
                    "API key · Unsaved"
                );
                view.api_key
                    .update(cx, |input, cx| input.set_value("", window, cx));
                view.settings_ui.remove_key = true;
                assert_eq!(
                    view.connection_key_label(ConnectionRole::Text, cx),
                    "API key · Removal pending"
                );
                view.settings_ui.remove_key = false;
                view.endpoint.update(cx, |input, cx| {
                    input.set_value("https://different.example/v1", window, cx)
                });
                assert_eq!(
                    view.connection_key_label(ConnectionRole::Text, cx),
                    "API key · Not saved"
                );
                view.settings.image_base_url = "https://image.example/v1".into();
                view.settings_ui.image_endpoint.update(cx, |input, cx| {
                    input.set_value("https://image.example/v1", window, cx)
                });
                view.settings_ui.has_image_key = true;
                assert_eq!(
                    view.connection_key_label(ConnectionRole::Images, cx),
                    "API key · •••••••• Saved"
                );
                assert!(view.settings_ui.image_key.read(cx).value().is_empty());
            });
        })
        .unwrap();
        assert!(!directory.exists());
    }

    #[gpui_kit::test]
    fn model_dropdowns_update_the_correct_role_and_preserve_discovery(cx: &mut TestAppContext) {
        use crate::ui::settings::connections::ConnectionRole;
        let (handle, view, directory) = fixture(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.sync_provider_config(window, cx);
                view.settings_ui.byok_check_audio = false;
                view.settings_ui.byok_check_image = false;
                view.remember_connection_models(&["discovered-text".into()]);
                let scope = (0, view.settings.api_base_url.clone());
                view.select_connection_model(ConnectionRole::Text, "discovered-text", window, cx);
                assert_eq!(view.model.read(cx).value().as_str(), "discovered-text");
                assert_eq!(
                    view.settings_ui.connection_models[&scope],
                    ["discovered-text"]
                );
                view.choose_connection(ConnectionRole::Images, "shared", window, cx);
                view.select_connection_model(ConnectionRole::Images, "shared-vision", window, cx);
                assert_eq!(view.model.read(cx).value().as_str(), "shared-vision");
                view.select_connection_model(ConnectionRole::Audio, "speech-only", window, cx);
                assert_eq!(view.stt_model.read(cx).value().as_str(), "speech-only");
                assert_eq!(view.model.read(cx).value().as_str(), "shared-vision");
                view.choose_connection(ConnectionRole::Images, "deepseek", window, cx);
                view.select_connection_model(ConnectionRole::Images, "separate-vision", window, cx);
                assert_eq!(
                    view.settings_ui.image_model.read(cx).value().as_str(),
                    "separate-vision"
                );
                assert_eq!(view.model.read(cx).value().as_str(), "shared-vision");
                assert!(view.settings_ui.provider_dirty.contains("image_model"));
                view.settings_ui.provider_save_pending = true;
                view.select_connection_model(ConnectionRole::Text, "blocked", window, cx);
                assert_eq!(view.model.read(cx).value().as_str(), "shared-vision");
            });
        })
        .unwrap();
        assert!(!directory.exists());
    }

    #[gpui_kit::test]
    fn compact_roles_preserve_drafts_and_sync_clean_fields_across_tabs(cx: &mut TestAppContext) {
        use crate::ui::settings::connections::ConnectionRole;
        let (handle, view, directory) = fixture(cx);
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.sync_provider_config(window, cx);
                view.choose_connection(ConnectionRole::Images, "shared", window, cx);
                view.choose_connection(ConnectionRole::Text, "deepseek", window, cx);
                assert_eq!(view.settings_ui.image_enabled_draft, Some(true));
                view.choose_connection(ConnectionRole::Images, "deepseek", window, cx);
                assert_eq!(
                    view.settings_ui.image_model.read(cx).value().as_str(),
                    "deepseek-flash"
                );
                assert_eq!(
                    view.settings_ui.image_endpoint.read(cx).value().as_str(),
                    "https://api.deepseek.com"
                );
                view.choose_connection(ConnectionRole::Text, "anthropic", window, cx);
                assert_eq!(view.model.read(cx).value().as_str(), "claude-sonnet-4-6");
                view.api_key.update(cx, |input, cx| {
                    input.set_value("draft-claude-key", window, cx)
                });
                view.choose_connection(ConnectionRole::Audio, "groq", window, cx);
                assert_eq!(view.api_key.read(cx).value().as_str(), "draft-claude-key");
                assert_eq!(
                    view.stt_model.read(cx).value().as_str(),
                    "whisper-large-v3-turbo"
                );
                view.choose_connection(ConnectionRole::Images, "gemini", window, cx);
                assert_eq!(
                    view.settings_ui.image_model.read(cx).value().as_str(),
                    "gemini-3.8-flash"
                );
                view.choose_connection(ConnectionRole::Text, "deepseek", window, cx);
                assert!(view.api_key.read(cx).value().is_empty());
                view.choose_connection(ConnectionRole::Text, "anthropic", window, cx);
                assert_eq!(view.api_key.read(cx).value().as_str(), "draft-claude-key");
                view.settings_ui.tab = SettingsTab::General;
                view.sync_provider_config(window, cx);
                view.settings_ui.tab = SettingsTab::Provider;
                assert_eq!(
                    view.settings_ui.image_endpoint.read(cx).value().as_str(),
                    "https://generativelanguage.googleapis.com/v1beta/openai"
                );
                view.provider_saved(ProviderMode::Byok, window, cx);
                let mut saved = view.settings.clone();
                saved.image_base_url = "https://images.example/v1".into();
                saved.image_model = "saved-vision".into();
                saved.transcription_base_url = "https://speech.example/v1".into();
                view.set_config(config(saved), window, cx);
                assert_eq!(
                    view.settings_ui.image_model.read(cx).value().as_str(),
                    "saved-vision"
                );
                assert_eq!(
                    view.settings_ui
                        .transcription_endpoint
                        .read(cx)
                        .value()
                        .as_str(),
                    "https://speech.example/v1"
                );
                assert!(view.api_key.read(cx).value().is_empty());
                assert!(view.settings_ui.image_key.read(cx).value().is_empty());
                assert!(!view.settings_ui.byok_advanced);
            });
        })
        .unwrap();
        assert!(!directory.exists());
    }
}
