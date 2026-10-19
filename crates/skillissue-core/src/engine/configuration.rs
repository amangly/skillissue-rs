//! Validated preferences, credential scoping and atomic durable commits.

use super::{Configuration, EngineEvent, NativeEngine, lock};
use crate::{
    config,
    models::{ConfigView, ProviderMode, Settings, ShortcutInfo},
    provider, startup,
};
use std::sync::atomic::Ordering;

pub struct ByokModelList {
    pub role: u8,
    pub endpoint: String,
    pub models: Result<Vec<String>, String>,
}

impl NativeEngine {
    pub async fn byok_model_lists(
        &self,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Vec<ByokModelList>, String> {
        let uses_hosted = {
            let config = lock(&self.configuration)?;
            [
                &config.settings.api_base_url,
                &config.settings.transcription_base_url,
                &config.settings.image_base_url,
            ]
            .iter()
            .any(|url| Settings::hosted_endpoint(url))
        };
        if uses_hosted {
            self.palepale_models(cancel.clone()).await?;
        }
        let (connections, revision) = {
            let config = lock(&self.configuration)?;
            if config.settings.is_palepale() {
                return Err("Switch to BYOK first.".into());
            }
            let mut connections = vec![(0, config.settings.clone(), config.key.clone())];
            if config.settings.byok_audio_enabled {
                let mut settings = config.settings.clone();
                let key = if settings.transcription_base_url.is_empty() {
                    config.key.clone()
                } else {
                    settings.api_base_url = settings.transcription_base_url.clone();
                    config.transcription_key.clone()
                };
                connections.push((1, settings, key));
            }
            if config.settings.include_screenshot {
                let mut settings = config.settings.clone();
                let key = if settings.image_base_url.is_empty() {
                    config.key.clone()
                } else {
                    settings.api_base_url = settings.image_base_url.clone();
                    config.image_key.clone()
                };
                connections.push((2, settings, key));
            }
            (connections, self.provider_revision.load(Ordering::SeqCst))
        };
        let mut lists = Vec::new();
        for (role, settings, key) in connections {
            if cancel.is_cancelled() {
                return Err("Model loading cancelled.".into());
            }
            let models =
                provider::byok_role_models(&self.client, &settings, &key, role, cancel.clone())
                    .await;
            lists.push(ByokModelList {
                role,
                endpoint: settings.api_base_url,
                models,
            });
        }
        if self.provider_revision.load(Ordering::SeqCst) != revision {
            return Err("Connection changed. Reload models.".into());
        }
        Ok(lists)
    }
    pub fn config(&self) -> Result<ConfigView, String> {
        let configuration = lock(&self.configuration)?;
        let mut warnings = lock(&self.warnings)?.clone();
        let mut inactive = configuration.settings.clone();
        inactive.provider_mode = if configuration.settings.is_palepale() {
            ProviderMode::Byok
        } else {
            ProviderMode::Palepale
        };
        let inactive_has_key = match self.credentials.load_provider(&inactive) {
            Ok(key) => !key.is_empty(),
            Err(error) => {
                warnings.push(error);
                false
            }
        };
        let active_has_key = !configuration.key.is_empty();
        let (has_byok_key, mut has_palepale_key) = if configuration.settings.is_palepale() {
            (inactive_has_key, active_has_key)
        } else {
            (active_has_key, inactive_has_key)
        };
        match self
            .credentials
            .load_saved(&super::pairing::pending_scope())
        {
            Ok(key) => has_palepale_key |= key.is_some_and(|key| !key.is_empty()),
            Err(error) => warnings.push(error),
        }
        Ok(ConfigView {
            settings: configuration.settings.clone(),
            has_api_key: active_has_key,
            has_byok_key,
            has_audio_key: !configuration.transcription_key.is_empty(),
            has_image_key: !configuration.image_key.is_empty(),
            has_palepale_key,
            platform: std::env::consts::OS.into(),
            shortcuts: configuration
                .settings
                .shortcuts
                .iter()
                .map(|(action, accelerator)| ShortcutInfo {
                    action: action.clone(),
                    accelerator: accelerator.clone(),
                })
                .collect(),
            warnings,
        })
    }

    pub fn save_settings(
        &self,
        settings: Settings,
        key: Option<String>,
    ) -> Result<ConfigView, String> {
        let settings = config::validate(settings)?;
        let transition = lock(&self.transition)?;
        self.save_settings_locked(settings, key, &transition)
    }

    /// Apply a preference change to the latest committed settings in one transaction.
    pub fn update_preferences(
        &self,
        change: impl FnOnce(&mut Settings) -> Result<(), String>,
    ) -> Result<ConfigView, String> {
        self.update_configuration(change, None)
    }

    /// Patch the latest settings and an optional provider key as one durable transaction.
    ///
    /// Editors should change only their owned fields in the callback so concurrent
    /// preference commits are preserved. `None` retains or loads the credential
    /// scoped to the resulting provider URL.
    pub fn update_configuration(
        &self,
        change: impl FnOnce(&mut Settings) -> Result<(), String>,
        key: Option<String>,
    ) -> Result<ConfigView, String> {
        let transition = lock(&self.transition)?;
        let mut settings = lock(&self.configuration)?.settings.clone();
        change(&mut settings)?;
        let settings = config::validate(settings)?;
        self.save_settings_locked(settings, key, &transition)
    }

    pub(super) fn save_settings_locked(
        &self,
        settings: Settings,
        key: Option<String>,
        transition: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<ConfigView, String> {
        self.commit_settings_locked(settings, key, transition, true)
    }

    /// Save both BYOK connections atomically, including separately scoped audio credentials.
    pub fn update_byok_configuration(
        &self,
        change: impl FnOnce(&mut Settings) -> Result<(), String>,
        key: Option<String>,
        transcription_key: Option<String>,
    ) -> Result<ConfigView, String> {
        self.update_media_configuration(change, key, transcription_key, None)
    }

    pub fn update_media_configuration(
        &self,
        change: impl FnOnce(&mut Settings) -> Result<(), String>,
        key: Option<String>,
        transcription_key: Option<String>,
        image_key: Option<String>,
    ) -> Result<ConfigView, String> {
        let transition = lock(&self.transition)?;
        let mut settings = lock(&self.configuration)?.settings.clone();
        change(&mut settings)?;
        let settings = config::validate(settings)?;
        self.commit_connections_locked(
            settings,
            key,
            transcription_key,
            image_key,
            &transition,
            true,
        )
    }

    pub(super) fn commit_settings_locked(
        &self,
        settings: Settings,
        key: Option<String>,
        _transition: &std::sync::MutexGuard<'_, ()>,
        publish: bool,
    ) -> Result<ConfigView, String> {
        self.commit_connections_locked(settings, key, None, None, _transition, publish)
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_connections_locked(
        &self,
        settings: Settings,
        key: Option<String>,
        transcription_key: Option<String>,
        image_key: Option<String>,
        _transition: &std::sync::MutexGuard<'_, ()>,
        publish: bool,
    ) -> Result<ConfigView, String> {
        if transcription_key.is_some() && settings.transcription_base_url.is_empty() {
            return Err("Enter a separate transcription URL before saving its key.".into());
        }
        if image_key.is_some() && settings.image_base_url.is_empty() {
            return Err("Choose an image provider before saving its key.".into());
        }
        if !settings.is_palepale()
            && ((Settings::hosted_endpoint(&settings.api_base_url) && key.is_some())
                || (Settings::hosted_endpoint(&settings.transcription_base_url)
                    && transcription_key.is_some())
                || (Settings::hosted_endpoint(&settings.image_base_url) && image_key.is_some()))
        {
            return Err("Palepale rows use the connected subscription. Connect or disconnect it in Palepale settings.".into());
        }
        for value in [
            key.as_deref(),
            transcription_key.as_deref(),
            image_key.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if value.len() > 16_384 || reqwest::header::HeaderValue::from_str(value.trim()).is_err()
            {
                return Err("The API key has an invalid format. Paste only the key, without a header or line breaks.".into());
            }
        }
        let mut current = lock(&self.configuration)?;
        let provider_changed = settings.provider_mode != current.settings.provider_mode
            || settings.provider_base_url() != current.settings.provider_base_url()
            || settings.provider_chat_model() != current.settings.provider_chat_model()
            || (settings.is_palepale()
                && (settings.palepale.reasoning_effort
                    != current.settings.palepale.reasoning_effort
                    || settings.palepale.transcription_model
                        != current.settings.palepale.transcription_model))
            || (!settings.is_palepale()
                && settings.transcription_model != current.settings.transcription_model)
            || settings.byok_audio_enabled != current.settings.byok_audio_enabled
            || settings.transcription_base_url != current.settings.transcription_base_url
            || settings.image_base_url != current.settings.image_base_url
            || settings.image_model != current.settings.image_model
            || image_key.is_some()
            || transcription_key.is_some()
            || key.is_some();
        if provider_changed
            && (self.active.load(Ordering::SeqCst)
                || self.stopping.load(Ordering::SeqCst)
                || lock(&self.assistant)?.is_some()
                || self.saved_questions.available_permits() == 0
                || self.note_jobs.available_permits() == 0
                || lock(&self.jobs)?.pending_count() > 0)
        {
            return Err("Stop recording and wait for answers, notes and transcription to finish before changing provider, model or API key.".into());
        }
        if self.active.load(Ordering::SeqCst)
            && (settings.microphone != current.settings.microphone
                || settings.system_audio != current.settings.system_audio
                || settings.audio_chunk_seconds != current.settings.audio_chunk_seconds
                || settings.language != current.settings.language)
        {
            return Err(
                "Stop the session before changing audio sources, input language, or chunk duration."
                    .into(),
            );
        }
        let credential_id = config::provider_credential_id(&settings);
        let previous_saved_key = if key.is_some() {
            self.credentials.load_saved(&credential_id)?
        } else {
            None
        };
        let credential_changed = key.is_some();
        let startup_before = if settings.open_at_login != current.settings.open_at_login {
            Some(startup::snapshot()?)
        } else {
            None
        };
        let audio_scope = config::transcription_credential_id(&settings);
        let previous_audio_key = if transcription_key.is_some() {
            self.credentials.load_saved(&audio_scope)?
        } else {
            None
        };
        let mut next_audio_key = if let Some(key) = transcription_key.as_ref() {
            key.trim().to_owned()
        } else if settings.transcription_base_url == current.settings.transcription_base_url
            && !Settings::hosted_endpoint(&settings.transcription_base_url)
        {
            current.transcription_key.clone()
        } else if settings.transcription_base_url.is_empty() {
            String::new()
        } else {
            self.credentials
                .load_saved(&audio_scope)?
                .unwrap_or_default()
        };
        let image_scope = config::image_credential_id(&settings);
        let previous_image_key = if image_key.is_some() {
            self.credentials.load_saved(&image_scope)?
        } else {
            None
        };
        let mut next_image_key = if let Some(key) = image_key.as_ref() {
            key.trim().to_owned()
        } else if settings.image_base_url == current.settings.image_base_url
            && !Settings::hosted_endpoint(&settings.image_base_url)
        {
            current.image_key.clone()
        } else if settings.image_base_url.is_empty() {
            String::new()
        } else {
            self.credentials
                .load_saved(&image_scope)?
                .unwrap_or_default()
        };
        let next_key = if let Some(key) = key {
            let key = key.trim().to_owned();
            self.credentials.save(&credential_id, &key)?;
            key
        } else if credential_id == config::provider_credential_id(&current.settings) {
            current.key.clone()
        } else {
            self.credentials.load_provider(&settings)?
        };
        if credential_changed && audio_scope == credential_id {
            next_audio_key = next_key.clone();
        }
        if credential_changed && image_scope == credential_id {
            next_image_key = next_key.clone();
        }
        let commit = (|| {
            if transcription_key.is_some() {
                self.credentials.save(&audio_scope, &next_audio_key)?;
            }
            if image_key.is_some() {
                self.credentials.save(&image_scope, &next_image_key)?;
            }
            if startup_before.is_some() {
                startup::set(settings.open_at_login)?;
            }
            config::save(&self.directory.join("settings.json"), &settings)
        })();
        if let Err(error) = commit {
            let startup_rollback = startup_before
                .as_ref()
                .and_then(|before| startup::restore(before).err());
            let rollback = if credential_changed {
                self.credentials
                    .save(&credential_id, previous_saved_key.as_deref().unwrap_or(""))
                    .err()
            } else {
                None
            };
            let audio_rollback = if transcription_key.is_some() {
                self.credentials
                    .save(&audio_scope, previous_audio_key.as_deref().unwrap_or(""))
                    .err()
            } else {
                None
            };
            let image_rollback = if image_key.is_some() {
                self.credentials
                    .save(&image_scope, previous_image_key.as_deref().unwrap_or(""))
                    .err()
            } else {
                None
            };
            return Err(format!(
                "{error}{}{}{}{}",
                image_rollback
                    .map(|error| format!(" Image credential rollback failed: {error}"))
                    .unwrap_or_default(),
                audio_rollback
                    .map(|error| format!(" Audio credential rollback failed: {error}"))
                    .unwrap_or_default(),
                rollback
                    .map(|error| format!(" Credential rollback failed: {error}"))
                    .unwrap_or_default(),
                startup_rollback
                    .map(|error| format!(" Login item rollback failed: {error}"))
                    .unwrap_or_default(),
            ));
        }
        *current = Configuration {
            settings,
            key: next_key,
            transcription_key: next_audio_key,
            image_key: next_image_key,
        };
        if provider_changed {
            self.provider_revision.fetch_add(1, Ordering::SeqCst);
        }
        drop(current);
        let view = self.config()?;
        if publish {
            self.emit(EngineEvent::ConfigChanged(Box::new(view.clone())));
        }
        Ok(view)
    }

    pub(super) fn provider_configuration(&self) -> Result<(Settings, String), String> {
        let configuration = lock(&self.configuration)?;
        if configuration.settings.is_palepale()
            && configuration.settings.provider_chat_model().is_empty()
        {
            return Err(
                "Palepale is waiting for its model list. Open Settings and refresh the connection before asking for assistance."
                    .into(),
            );
        }
        if !configuration.settings.is_palepale() && configuration.settings.chat_model.is_empty() {
            return Err("Choose a chat model in Settings: check your saved connection, select a model and save.".into());
        }
        let url = url::Url::parse(configuration.settings.provider_base_url())
            .map_err(|e| e.to_string())?;
        if configuration.key.is_empty() && !provider::is_loopback(&url) {
            return Err("Add your provider API key in Settings before using audio transcription or assistance.".into());
        }
        let mut settings = configuration.settings.clone();
        settings.resolve_hosted_role(false);
        if settings.is_palepale() {
            if settings.smart_mode
                && !lock(&self.published_models)?.iter().any(|model| {
                    model.id == settings.palepale.chat_model
                        && model.supports_reasoning_effort(&settings.smart_effort)
                })
            {
                return Err("This Palepale model does not publish the selected Smart effort. Choose another effort or turn Smart off.".into());
            }
            settings.include_screenshot &= lock(&self.published_models)?.iter().any(|model| {
                model.id == settings.palepale.chat_model && model.capabilities.image_input
            });
        }
        Ok((settings, configuration.key.clone()))
    }
}

impl Configuration {
    pub(super) fn transcription_configuration(&self) -> Result<(Settings, String), String> {
        if !self.settings.is_palepale() && !self.settings.byok_audio_enabled {
            return Err(
                "Audio is disabled for BYOK. Enable audio in Settings before recording.".into(),
            );
        }
        let mut settings = self.settings.clone();
        let key = if !settings.is_palepale() && !settings.transcription_base_url.is_empty() {
            settings.api_base_url = settings.transcription_base_url.clone();
            self.transcription_key.clone()
        } else {
            self.key.clone()
        };
        settings.resolve_hosted_role(true);
        if settings.is_palepale() && settings.provider_transcription_model().is_empty() {
            return Err("Refresh Palepale in Settings > AI provider before recording.".into());
        }
        let url = url::Url::parse(settings.provider_base_url())
            .map_err(|_| "Invalid transcription URL.".to_string())?;
        if key.is_empty() && !provider::is_loopback(&url) {
            return Err(
                "Add the transcription connection's API key in Settings before recording.".into(),
            );
        }
        Ok((settings, key))
    }
}

impl NativeEngine {
    /// Check only committed configuration, never credentials for an unsaved destination.
    pub async fn test_byok_connection(
        &self,
        audio: bool,
        image: bool,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<(), String> {
        let (settings, key, revision) = {
            let _transition = lock(&self.transition)?;
            let config = lock(&self.configuration)?;
            if config.settings.is_palepale() {
                return Err("Switch to BYOK and save first.".into());
            }
            let (mut settings, mut key) = if audio {
                config.transcription_configuration()?
            } else {
                (config.settings.clone(), config.key.clone())
            };
            if image && !settings.image_base_url.is_empty() {
                settings.api_base_url = settings.image_base_url.clone();
                settings.chat_model = settings.image_model.clone();
                key = config.image_key.clone();
            }
            settings.resolve_hosted_role(audio);
            (settings, key, self.provider_revision.load(Ordering::SeqCst))
        };
        let result = provider::test_byok(&self.client, &settings, &key, audio, image, cancel).await;
        if self.provider_revision.load(Ordering::SeqCst) != revision {
            return Err("Connection changed. Test again.".into());
        }
        result
    }

    pub async fn byok_models(
        &self,
        audio: bool,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Vec<String>, String> {
        let (settings, key, revision) = {
            let config = lock(&self.configuration)?;
            if config.settings.is_palepale() {
                return Err("Switch to BYOK and save before checking.".into());
            }
            let (settings, key) = if audio {
                config.transcription_configuration()?
            } else {
                (config.settings.clone(), config.key.clone())
            };
            (settings, key, self.provider_revision.load(Ordering::SeqCst))
        };
        let result = provider::byok_role_models(
            &self.client,
            &settings,
            &key,
            if audio { 1 } else { 0 },
            cancel,
        )
        .await;
        if self.provider_revision.load(Ordering::SeqCst) != revision {
            return Err("Connection changed. Run the check again.".into());
        }
        result
    }
}

impl NativeEngine {
    pub async fn byok_image_models(
        &self,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Vec<String>, String> {
        let (settings, key, revision) = {
            let _transition = lock(&self.transition)?;
            let configuration = lock(&self.configuration)?;
            let mut settings = configuration.settings.clone();
            if settings.is_palepale() {
                return Err("Switch to BYOK before checking Images.".into());
            }
            let key = if settings.image_base_url.is_empty() {
                configuration.key.clone()
            } else {
                settings.api_base_url = settings.image_base_url.clone();
                configuration.image_key.clone()
            };
            settings.provider_mode = ProviderMode::Byok;
            (settings, key, self.provider_revision.load(Ordering::SeqCst))
        };
        let result = provider::byok_role_models(&self.client, &settings, &key, 2, cancel).await;
        if self.provider_revision.load(Ordering::SeqCst) != revision {
            return Err("Connection changed. Check again.".into());
        }
        result
    }

    pub(super) fn image_configuration(&self) -> Result<(Settings, String), String> {
        let configuration = lock(&self.configuration)?;
        if configuration.settings.is_palepale() || configuration.settings.image_base_url.is_empty()
        {
            drop(configuration);
            return self.provider_configuration();
        }
        let mut settings = configuration.settings.clone();
        settings.api_base_url = settings.image_base_url.clone();
        settings.chat_model = settings.image_model.clone();
        settings.resolve_hosted_role(false);
        if settings.chat_model.is_empty() {
            return Err("Choose an image model in Settings > AI provider > Advanced.".into());
        }
        let url = url::Url::parse(&settings.api_base_url)
            .map_err(|_| "Invalid image provider URL.".to_string())?;
        if configuration.image_key.is_empty() && !provider::is_loopback(&url) {
            return Err("Add the Images API key in Settings.".into());
        }
        Ok((settings, configuration.image_key.clone()))
    }
}
