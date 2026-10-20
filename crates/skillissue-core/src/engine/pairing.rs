//! Hosted pairing, one-time credential recovery and provider supersession.

use super::{EngineEvent, NativeEngine, lock};
use crate::{config, provider};
use skillissue_domain::models::{ConfigView, ProviderMode, Settings};
use skillissue_domain::provider::ProviderModel;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

/// Native-only pairing handle. Neither the device code nor revision is serialized.
pub struct PalepalePairing {
    native: provider::PalepalePairing,
    generation: u64,
    provider_revision: u64,
}

impl PalepalePairing {
    pub fn user_code(&self) -> &str {
        self.native.user_code()
    }

    pub fn browser_url(&self) -> String {
        self.native.browser_url()
    }

    pub fn ttl(&self) -> Duration {
        self.native.ttl()
    }
}

/// Approved secret bound to its requesting engine transaction, never UI state.
pub struct PalepaleCredential {
    key: String,
    generation: u64,
    provider_revision: u64,
    lease: Option<PairingLease>,
}

#[derive(Default)]
pub(super) struct PairingWork {
    active: AtomicUsize,
    done: Notify,
    shutdown: CancellationToken,
    storage_error: Mutex<Option<String>>,
}

struct PairingLease(Arc<PairingWork>);

impl Drop for PairingLease {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
        self.0.done.notify_waiters();
    }
}

fn hosted_scope() -> String {
    config::provider_credential_id(&Settings {
        provider_mode: ProviderMode::Palepale,
        ..Settings::default()
    })
}

pub(super) fn pending_scope() -> String {
    format!("{}:paired-pending", hosted_scope())
}

impl NativeEngine {
    pub(super) async fn drain_palepale_pairing(&self) -> Result<(), String> {
        {
            let _transition = lock(&self.transition)?;
            self.pairing_work.shutdown.cancel();
        }
        tokio::time::timeout(Duration::from_secs(25), async {
            loop {
                let done = self.pairing_work.done.notified();
                if self.pairing_work.active.load(Ordering::SeqCst) == 0 {
                    break;
                }
                done.await;
            }
        })
        .await
        .map_err(|_| {
            "Palepale is still collecting an approved device credential. Wait and close again."
                .to_string()
        })?;
        if let Some(error) = lock(&self.pairing_work.storage_error)?.take() {
            return Err(format!(
                "{error} Close and restart skillissue before pairing again."
            ));
        }
        Ok(())
    }

    /// Serialize cancellation with promotion of a consumed one-time credential.
    pub fn cancel_palepale_pairing(&self, cancel: CancellationToken) {
        let _transition = self.transition.lock();
        cancel.cancel();
    }

    pub async fn start_palepale_pairing(
        &self,
        cancel: CancellationToken,
    ) -> Result<PalepalePairing, String> {
        let (generation, provider_revision) = {
            let _transition = lock(&self.transition)?;
            if self.pairing_work.shutdown.is_cancelled() {
                return Err("Palepale pairing is closing with the application.".into());
            }
            if self.pairing_work.active.load(Ordering::SeqCst) > 0 {
                return Err(
                    "Palepale is finishing an earlier device approval. Wait before pairing again."
                        .into(),
                );
            }
            if self.active.load(Ordering::SeqCst)
                || self.stopping.load(Ordering::SeqCst)
                || lock(&self.assistant)?.is_some()
                || self.saved_questions.available_permits() == 0
                || self.note_jobs.available_permits() == 0
                || lock(&self.jobs)?.pending_count() > 0
                || self.live_workers.load(Ordering::SeqCst) > 0
            {
                return Err("Stop the active session and wait for answers, notes and transcription to finish before pairing Palepale.".into());
            }
            if cancel.is_cancelled() {
                return Err("Pairing cancelled.".into());
            }
            (
                self.pairing_generation.fetch_add(1, Ordering::SeqCst) + 1,
                self.provider_revision.load(Ordering::SeqCst),
            )
        };
        let native = provider::start_pairing(&self.client, cancel).await?;
        Ok(PalepalePairing {
            native,
            generation,
            provider_revision,
        })
    }

    /// Finish collecting a dispatched poll even after cancellation; the next
    /// operation preserves its consumed credential without activating setup.
    pub async fn wait_palepale_pairing(
        &self,
        pairing: PalepalePairing,
        cancel: CancellationToken,
    ) -> Result<PalepaleCredential, String> {
        let lease = {
            let _transition = lock(&self.transition)?;
            if self.pairing_work.shutdown.is_cancelled() {
                return Err("Palepale pairing is closing with the application.".into());
            }
            if pairing.generation != self.pairing_generation.load(Ordering::SeqCst) {
                return Err("Pairing was superseded by a newer connection or disconnect.".into());
            }
            self.pairing_work.active.fetch_add(1, Ordering::SeqCst);
            PairingLease(self.pairing_work.clone())
        };
        let poll = provider::wait_pairing(&self.client, &pairing.native, cancel.clone());
        tokio::pin!(poll);
        let key = tokio::select! {
            biased;
            _ = self.pairing_work.shutdown.cancelled() => {
                cancel.cancel();
                poll.await
            },
            result = &mut poll => result,
        }?;
        Ok(PalepaleCredential {
            key,
            generation: pairing.generation,
            provider_revision: pairing.provider_revision,
            lease: Some(lease),
        })
    }

    pub async fn finish_palepale_pairing(
        &self,
        credential: PalepaleCredential,
        cancel: CancellationToken,
    ) -> Result<ConfigView, String> {
        self.finish_palepale_pairing_with(credential, cancel.clone(), async {
            provider::palepale_models(&self.client, cancel.clone()).await
        })
        .await
    }

    async fn finish_palepale_pairing_with(
        &self,
        mut credential: PalepaleCredential,
        cancel: CancellationToken,
        fetch: impl std::future::Future<Output = Result<Vec<ProviderModel>, String>>,
    ) -> Result<ConfigView, String> {
        {
            let transition = lock(&self.transition)?;
            if credential.generation != self.pairing_generation.load(Ordering::SeqCst) {
                return Err("Pairing was superseded by a newer connection or disconnect.".into());
            }
            let key = credential.key.trim().to_owned();
            if key.is_empty() || key.len() > 4096 || key.chars().any(char::is_control) {
                return Err("Palepale returned an invalid pairing credential.".into());
            }
            // The recovery slot is never loaded automatically for active requests.
            if let Err(error) = self.credentials.save(&pending_scope(), &key) {
                let error = format!(
                    "Palepale approved this device, but its credential could not be saved. Pair again after fixing credential storage: {error}"
                );
                *lock(&self.pairing_work.storage_error)? = Some(error.clone());
                return Err(error);
            }
            drop(credential.lease.take());
            *lock(&self.pairing_work.storage_error)? = None;
            if cancel.is_cancelled()
                || self.pairing_work.shutdown.is_cancelled()
                || credential.provider_revision != self.provider_revision.load(Ordering::SeqCst)
            {
                self.emit(EngineEvent::ConfigChanged(Box::new(self.config()?)));
                return Err("Pairing approval was saved securely. The active provider was unchanged. Reconnect Palepale in Settings when ready.".into());
            }
            let mut settings = lock(&self.configuration)?.settings.clone();
            settings.provider_mode = ProviderMode::Palepale;
            settings.palepale.reasoning_effort = None;
            if let Err(error) = self.commit_settings_locked(
                config::validate(settings)?,
                Some(key),
                &transition,
                false,
            ) {
                self.emit(EngineEvent::ConfigChanged(Box::new(self.config()?)));
                return Err(format!(
                    "Pairing approval was saved securely, but activation failed. Reconnect Palepale in Settings: {error}"
                ));
            }
            self.clear_pending_after_activation();
            self.emit(EngineEvent::ConfigChanged(Box::new(self.config()?)));
        }
        // Discovery may outlive a switch, cancellation or disconnect. It checks
        // the provider revision under transition before committing metadata.
        if let Err(error) = self
            .refresh_palepale_models_with(cancel.clone(), fetch)
            .await
            && !cancel.is_cancelled()
        {
            lock(&self.warnings)?.push(format!(
                "Palepale approval was saved, but connection setup needs another refresh: {error}"
            ));
        }
        self.config()
    }

    /// Explicitly activate an existing approval, including a credential recovered
    /// after cancellation or a failed settings write.
    pub fn activate_saved_palepale(&self) -> Result<ConfigView, String> {
        let transition = lock(&self.transition)?;
        if self.pairing_work.active.load(Ordering::SeqCst) > 0 {
            return Err(
                "Palepale is finishing an earlier device approval. Wait before reconnecting."
                    .into(),
            );
        }
        self.pairing_generation.fetch_add(1, Ordering::SeqCst);
        let mut settings = lock(&self.configuration)?.settings.clone();
        settings.provider_mode = ProviderMode::Palepale;
        settings.palepale.reasoning_effort = None;
        let pending = self.credentials.load_saved(&pending_scope())?;
        let key = match pending {
            Some(key) if !key.is_empty() => key,
            _ => self.credentials.load_provider(&settings)?,
        };
        if key.is_empty() {
            return Err("Connect Palepale and approve this device in your browser.".into());
        }
        self.commit_settings_locked(config::validate(settings)?, Some(key), &transition, false)?;
        self.clear_pending_after_activation();
        let view = self.config()?;
        self.emit(EngineEvent::ConfigChanged(Box::new(view.clone())));
        Ok(view)
    }

    fn clear_pending_after_activation(&self) {
        if let Err(error) = self.credentials.save(&pending_scope(), "")
            && let Ok(mut warnings) = lock(&self.warnings)
        {
            warnings.push(format!(
                "Palepale is active, but its recovery credential could not be removed: {error}"
            ));
        }
    }

    /// Invalidate in-flight approval before deleting both credential scopes,
    /// and publish exactly the post-deletion configuration state.
    pub fn disconnect_palepale(&self) -> Result<ConfigView, String> {
        let transition = lock(&self.transition)?;
        self.pairing_generation.fetch_add(1, Ordering::SeqCst);
        self.provider_revision.fetch_add(1, Ordering::SeqCst);
        let mut settings = lock(&self.configuration)?.settings.clone();
        if settings.is_palepale() {
            settings.provider_mode = ProviderMode::Byok;
            self.commit_settings_locked(config::validate(settings)?, None, &transition, false)?;
        }
        let errors = [hosted_scope(), pending_scope()]
            .into_iter()
            .filter_map(|scope| self.credentials.save(&scope, "").err())
            .collect::<Vec<_>>();
        let view = self.config()?;
        self.emit(EngineEvent::ConfigChanged(Box::new(view.clone())));
        if errors.is_empty() {
            Ok(view)
        } else {
            Err(format!(
                "BYOK is active, but a hosted credential could not be removed: {}",
                errors.join("; ")
            ))
        }
    }
}

#[cfg(test)]
mod tests;
