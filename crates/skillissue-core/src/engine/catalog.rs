//! Public discovery with a transaction guard for provider and model changes.

use super::{NativeEngine, lock};
use skillissue_domain::provider::ProviderModel;
use std::sync::atomic::Ordering;
use tokio_util::sync::CancellationToken;

impl NativeEngine {
    pub async fn palepale_models(
        &self,
        cancel: CancellationToken,
    ) -> Result<Vec<ProviderModel>, String> {
        self.refresh_palepale_models(cancel).await
    }

    /// Select independently qualified hosted screen and transcription models.
    pub async fn refresh_palepale_models(
        &self,
        cancel: CancellationToken,
    ) -> Result<Vec<ProviderModel>, String> {
        self.refresh_palepale_models_with(cancel.clone(), async {
            crate::provider::palepale_models(&self.client, cancel).await
        })
        .await
    }

    pub(super) async fn refresh_palepale_models_with(
        &self,
        cancel: CancellationToken,
        fetch: impl std::future::Future<Output = Result<Vec<ProviderModel>, String>>,
    ) -> Result<Vec<ProviderModel>, String> {
        let revision = {
            let _transition = lock(&self.transition)?;
            self.provider_revision.load(Ordering::SeqCst)
        };
        let result = fetch.await;
        let transition = lock(&self.transition)?;
        if cancel.is_cancelled() {
            return Err("Palepale refresh cancelled.".into());
        }
        if revision != self.provider_revision.load(Ordering::SeqCst) {
            return Err("The Palepale connection changed while refreshing. Refresh again.".into());
        }
        let models = match result {
            Ok(models) => models,
            Err(error) => {
                lock(&self.published_models)?.clear();
                return Err(error);
            }
        };
        *lock(&self.published_models)? = models.clone();
        let mut settings = {
            let configuration = lock(&self.configuration)?;
            if !configuration.settings.is_palepale() || configuration.key.is_empty() {
                return Ok(models);
            }
            configuration.settings.clone()
        };
        let selected = select_model(&settings.palepale.chat_model, &models).ok_or_else(|| {
            "Palepale has no available streaming model. Refresh the connection later.".to_string()
        })?;
        let audio = select_audio_model(
            &settings.palepale.transcription_model,
            &settings.palepale.chat_model,
            &models,
        )
        .unwrap_or_default();
        if selected != settings.palepale.chat_model
            || audio != settings.palepale.transcription_model
        {
            if selected != settings.palepale.chat_model {
                settings.palepale.reasoning_effort = None;
            }
            settings.palepale.chat_model = selected;
            settings.palepale.transcription_model = audio;
            self.save_settings_locked(crate::config::validate(settings)?, None, &transition)?;
        }
        Ok(models)
    }
}

pub(super) fn select_model(current: &str, models: &[ProviderModel]) -> Option<String> {
    let available = || models.iter().filter(|model| model.capabilities.streaming);
    let current_model = available().find(|model| model.id == current);
    let supports_context = |model: &ProviderModel| model.capabilities.image_input;
    if current_model.is_some_and(supports_context) {
        return current_model.map(|model| model.id.clone());
    }
    available()
        .find(|model| model.id == "glm-5.3-flash" && supports_context(model))
        .or_else(|| available().find(|model| supports_context(model)))
        .or(current_model)
        .or_else(|| available().next())
        .map(|model| model.id.clone())
}

pub(super) fn select_audio_model(
    current: &str,
    legacy_chat: &str,
    models: &[ProviderModel],
) -> Option<String> {
    let available = || {
        models
            .iter()
            .filter(|model| model.capabilities.streaming && model.capabilities.audio_input)
    };
    available()
        .find(|model| model.id == current)
        .or_else(|| available().find(|model| model.id == "gemini-3.8-flash"))
        .or_else(|| available().find(|model| model.id == legacy_chat))
        .or_else(|| available().next())
        .map(|model| model.id.clone())
}
