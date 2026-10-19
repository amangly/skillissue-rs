//! Authenticated readiness is separate from having a saved credential or model list.

use super::{NativeEngine, lock};
use crate::provider::{self, PalepaleAccountStatus};
use std::sync::atomic::Ordering;
use tokio_util::sync::CancellationToken;

impl NativeEngine {
    pub async fn palepale_account(
        &self,
        cancel: CancellationToken,
    ) -> Result<PalepaleAccountStatus, String> {
        self.check_palepale_account(cancel.clone(), |key| async move {
            provider::palepale_account_status(&self.client, &key, cancel).await
        })
        .await
    }

    async fn check_palepale_account<F, R>(
        &self,
        cancel: CancellationToken,
        fetch: F,
    ) -> Result<PalepaleAccountStatus, String>
    where
        F: FnOnce(String) -> R,
        R: std::future::Future<Output = Result<PalepaleAccountStatus, String>>,
    {
        let (key, revision) = {
            let _transition = lock(&self.transition)?;
            let configuration = lock(&self.configuration)?;
            if !configuration.settings.is_palepale() || configuration.key.is_empty() {
                return Err(
                    "Select a saved Palepale connection before checking this device.".into(),
                );
            }
            (
                configuration.key.clone(),
                self.provider_revision.load(Ordering::SeqCst),
            )
        };
        let result = fetch(key.clone()).await;
        let _transition = lock(&self.transition)?;
        let configuration = lock(&self.configuration)?;
        if cancel.is_cancelled()
            || !configuration.settings.is_palepale()
            || configuration.key != key
            || self.provider_revision.load(Ordering::SeqCst) != revision
        {
            return Err("Palepale account check was superseded.".into());
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        engine::credentials::Credentials,
        models::{ProviderMode, Settings},
    };
    use std::sync::Arc;

    struct TestCredentials;
    impl Credentials for TestCredentials {
        fn load_saved(&self, _: &str) -> Result<Option<String>, String> {
            Ok(None)
        }
        fn save(&self, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn load_provider(&self, settings: &Settings) -> Result<String, String> {
            Ok(if settings.is_palepale() {
                "synthetic-test-key"
            } else {
                ""
            }
            .into())
        }
    }

    #[tokio::test]
    async fn account_result_is_rejected_after_switching_away_and_back_to_the_same_key() {
        let directory =
            std::env::temp_dir().join(format!("skillissue-account-{}", uuid::Uuid::new_v4()));
        let engine =
            NativeEngine::open_with_credentials(directory.clone(), Arc::new(TestCredentials))
                .unwrap();
        engine
            .update_preferences(|settings| {
                settings.provider_mode = ProviderMode::Palepale;
                settings.palepale.chat_model = "synthetic-model".into();
                Ok(())
            })
            .unwrap();
        let (entered, started) = tokio::sync::oneshot::channel();
        let (release, finished) = tokio::sync::oneshot::channel();
        let task = tokio::spawn({
            let engine = engine.clone();
            async move {
                engine
                    .check_palepale_account(CancellationToken::new(), |_| async move {
                        entered.send(()).unwrap();
                        finished.await.unwrap();
                        Ok(PalepaleAccountStatus {
                            can_spend: true,
                            ..Default::default()
                        })
                    })
                    .await
            }
        });
        started.await.unwrap();
        for mode in [ProviderMode::Byok, ProviderMode::Palepale] {
            engine
                .update_preferences(|settings| {
                    settings.provider_mode = mode;
                    Ok(())
                })
                .unwrap();
        }
        release.send(()).unwrap();
        assert!(task.await.unwrap().unwrap_err().contains("superseded"));
        drop(engine);
        let _ = std::fs::remove_dir_all(directory);
    }
}
