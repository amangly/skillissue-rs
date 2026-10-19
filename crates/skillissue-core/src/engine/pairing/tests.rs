//! Transaction tests use an isolated credential store and controlled discovery futures.

use super::*;
use crate::engine::credentials::Credentials;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::{broadcast, oneshot};

#[derive(Default)]
struct TestCredentials {
    values: Mutex<HashMap<String, String>>,
    fail: Mutex<Option<(String, bool)>>,
}

impl Credentials for TestCredentials {
    fn load_saved(&self, scope: &str) -> Result<Option<String>, String> {
        Ok(self.values.lock().unwrap().get(scope).cloned())
    }

    fn save(&self, scope: &str, key: &str) -> Result<(), String> {
        if self.fail.lock().unwrap().as_ref() == Some(&(scope.to_owned(), key.is_empty())) {
            return Err("Synthetic credential store failure".into());
        }
        let mut values = self.values.lock().unwrap();
        if key.is_empty() {
            values.remove(scope);
        } else {
            values.insert(scope.into(), key.into());
        }
        Ok(())
    }

    fn load_provider(&self, settings: &Settings) -> Result<String, String> {
        Ok(self
            .load_saved(&config::provider_credential_id(settings))?
            .unwrap_or_default())
    }
}

struct Fixture {
    directory: PathBuf,
    engine: Arc<NativeEngine>,
    credentials: Arc<TestCredentials>,
}

impl Fixture {
    fn new() -> Self {
        let directory =
            std::env::temp_dir().join(format!("skillissue-pairing-{}", uuid::Uuid::new_v4()));
        let settings = Settings {
            api_base_url: "http://127.0.0.1:43321/v1".into(),
            chat_model: "independent-byok-model".into(),
            ..Settings::default()
        };
        config::save(&directory.join("settings.json"), &settings).unwrap();
        let credentials = Arc::new(TestCredentials::default());
        let engine =
            NativeEngine::open_with_credentials(directory.clone(), credentials.clone()).unwrap();
        Self {
            directory,
            engine,
            credentials,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn credential(engine: &NativeEngine) -> PalepaleCredential {
    PalepaleCredential {
        key: "synthetic-pairing-key".into(),
        generation: engine.pairing_generation.load(Ordering::SeqCst),
        provider_revision: engine.provider_revision.load(Ordering::SeqCst),
        lease: None,
    }
}

fn model(id: &str) -> ProviderModel {
    ProviderModel {
        id: id.into(),
        capabilities: skillissue_domain::provider::ModelCapabilities {
            streaming: true,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn media_model(id: &str) -> ProviderModel {
    let mut model = model(id);
    model.capabilities.audio_input = true;
    model.capabilities.image_input = true;
    model
}

fn assert_one_config_event(
    events: &mut broadcast::Receiver<EngineEvent>,
    mode: ProviderMode,
    has_hosted_key: bool,
) {
    let EngineEvent::ConfigChanged(view) = events.try_recv().unwrap() else {
        panic!("expected configuration event");
    };
    assert_eq!(view.settings.provider_mode, mode);
    assert_eq!(view.has_palepale_key, has_hosted_key);
    assert!(matches!(
        events.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));
}

#[test]
fn automatic_selection_retains_a_published_model_and_has_no_claimed_server_default() {
    let models = [model("server/first"), model("server/second")];
    assert_eq!(
        super::super::catalog::select_model("server/second", &models).as_deref(),
        Some("server/second")
    );
    assert_eq!(
        super::super::catalog::select_model("withdrawn", &models).as_deref(),
        Some("server/first")
    );
    assert_eq!(super::super::catalog::select_model("", &[]), None);
}

#[test]
fn automatic_selection_prefers_screen_context_independently_of_audio() {
    let mut audio_only = model("server/audio-only");
    audio_only.capabilities.audio_input = true;
    let mut image_only = model("server/image-only");
    image_only.capabilities.image_input = true;
    let models = [
        model("server/text"),
        audio_only,
        image_only,
        media_model("server/context"),
    ];
    for current in ["", "withdrawn", "server/text", "server/audio-only"] {
        assert_eq!(
            super::super::catalog::select_model(current, &models).as_deref(),
            Some("server/image-only"),
            "current model: {current}"
        );
    }
    assert_eq!(
        super::super::catalog::select_model("server/image-only", &models).as_deref(),
        Some("server/image-only")
    );
}

#[test]
fn independent_audio_selection_uses_only_published_streaming_audio_models() {
    use super::super::catalog::select_audio_model;
    let mut preferred = model("gemini-3.8-flash");
    preferred.capabilities.audio_input = true;
    let legacy = media_model("legacy");
    let mut image = model("glm-5.3-flash");
    image.capabilities.image_input = true;
    let models = [image, preferred.clone(), legacy.clone()];
    assert_eq!(
        super::super::catalog::select_model("", &models).as_deref(),
        Some("glm-5.3-flash")
    );
    assert_eq!(
        select_audio_model("", "glm-5.3-flash", &models).as_deref(),
        Some("gemini-3.8-flash")
    );
    assert_eq!(
        select_audio_model("legacy", "glm-5.3-flash", &models).as_deref(),
        Some("legacy")
    );
    preferred.capabilities.streaming = false;
    assert_eq!(
        select_audio_model("gemini-3.8-flash", "legacy", &[preferred, legacy]).as_deref(),
        Some("legacy")
    );
    assert_eq!(
        select_audio_model("removed", "text-only", &[model("text-only")]),
        None
    );
}

#[tokio::test]
async fn legacy_hosted_settings_migrate_to_independent_audio_without_changing_image_model() {
    let fixture = Fixture::new();
    let mut image = model("glm-5.3-flash");
    image.capabilities.image_input = true;
    let audio = media_model("gemini-3.8-flash");
    let view = fixture
        .engine
        .finish_palepale_pairing_with(
            credential(&fixture.engine),
            CancellationToken::new(),
            async { Ok(vec![image, audio]) },
        )
        .await
        .unwrap();
    assert_eq!(view.settings.palepale.chat_model, "glm-5.3-flash");
    assert_eq!(
        view.settings.palepale.transcription_model,
        "gemini-3.8-flash"
    );
    let saved = config::load(&fixture.directory.join("settings.json")).unwrap();
    assert_eq!(saved.palepale.transcription_model, "gemini-3.8-flash");
    assert_eq!(
        saved.transcription_model,
        Settings::default().transcription_model
    );
    fixture.engine.active.store(true, Ordering::SeqCst);
    let error = fixture
        .engine
        .update_preferences(|settings| {
            settings.palepale.transcription_model = "other-audio".into();
            Ok(())
        })
        .err()
        .unwrap();
    assert!(error.contains("Stop recording and wait"));
    fixture.engine.active.store(false, Ordering::SeqCst);
    let job = lock(&fixture.engine.jobs).unwrap().submit();
    let revision = fixture.engine.provider_revision.load(Ordering::SeqCst);
    let mut image = model("glm-5.3-flash");
    image.capabilities.image_input = true;
    let replacement = vec![image, media_model("replacement-audio")];
    let error = fixture
        .engine
        .refresh_palepale_models_with(CancellationToken::new(), async { Ok(replacement.clone()) })
        .await
        .err()
        .unwrap();
    assert!(error.contains("Stop recording and wait"));
    assert_eq!(
        fixture
            .engine
            .config()
            .unwrap()
            .settings
            .palepale
            .transcription_model,
        "gemini-3.8-flash"
    );
    assert_eq!(
        fixture.engine.provider_revision.load(Ordering::SeqCst),
        revision
    );
    lock(&fixture.engine.jobs).unwrap().complete(job);
    fixture
        .engine
        .refresh_palepale_models_with(CancellationToken::new(), async { Ok(replacement) })
        .await
        .unwrap();
    let migrated = fixture.engine.config().unwrap().settings;
    assert_eq!(migrated.palepale.chat_model, "glm-5.3-flash");
    assert_eq!(migrated.palepale.transcription_model, "replacement-audio");
}

#[test]
fn automatic_selection_preserves_a_current_fully_capable_model() {
    let models = [
        model("server/text"),
        media_model("server/first-context"),
        media_model("server/current-context"),
    ];
    assert_eq!(
        super::super::catalog::select_model("server/current-context", &models).as_deref(),
        Some("server/current-context")
    );
}

#[test]
fn automatic_selection_excludes_models_without_streaming_support() {
    let mut unavailable = media_model("server/non-streaming");
    unavailable.capabilities.streaming = false;
    let models = [unavailable.clone(), model("server/text")];
    assert_eq!(
        super::super::catalog::select_model("server/non-streaming", &models).as_deref(),
        Some("server/text")
    );
    assert_eq!(
        super::super::catalog::select_model("server/non-streaming", &[unavailable]),
        None
    );
}

#[tokio::test]
async fn refreshed_media_capabilities_upgrade_a_saved_text_model_without_pairing_again() {
    let fixture = Fixture::new();
    fixture
        .engine
        .finish_palepale_pairing_with(
            credential(&fixture.engine),
            CancellationToken::new(),
            async { Ok(vec![model("server/text")]) },
        )
        .await
        .unwrap();
    let models = vec![model("server/text"), media_model("server/context")];
    fixture
        .engine
        .refresh_palepale_models_with(CancellationToken::new(), async { Ok(models) })
        .await
        .unwrap();
    assert_eq!(
        fixture
            .engine
            .config()
            .unwrap()
            .settings
            .palepale
            .chat_model,
        "server/context"
    );
    assert_eq!(
        config::load(&fixture.directory.join("settings.json"))
            .unwrap()
            .palepale
            .chat_model,
        "server/context"
    );
    assert_eq!(
        lock(&fixture.engine.configuration).unwrap().key,
        "synthetic-pairing-key"
    );
}

#[tokio::test]
async fn catalog_media_upgrade_waits_for_recording_and_all_provider_jobs_to_finish() {
    for busy in [
        "capture",
        "stopping",
        "assistant",
        "saved-question",
        "notes",
        "transcription",
    ] {
        let fixture = Fixture::new();
        fixture
            .engine
            .finish_palepale_pairing_with(
                credential(&fixture.engine),
                CancellationToken::new(),
                async { Ok(vec![model("server/text")]) },
            )
            .await
            .unwrap();
        let before = std::fs::read(fixture.directory.join("settings.json")).unwrap();
        let revision = fixture.engine.provider_revision.load(Ordering::SeqCst);
        let mut events = fixture.engine.subscribe();
        let mut job = None;
        let mut permit = None;
        match busy {
            "capture" => fixture.engine.active.store(true, Ordering::SeqCst),
            "stopping" => fixture.engine.stopping.store(true, Ordering::SeqCst),
            "assistant" => {
                *lock(&fixture.engine.assistant).unwrap() = Some(CancellationToken::new())
            }
            "saved-question" => {
                permit = Some(fixture.engine.saved_questions.try_acquire().unwrap())
            }
            "notes" => permit = Some(fixture.engine.note_jobs.try_acquire().unwrap()),
            "transcription" => job = Some(lock(&fixture.engine.jobs).unwrap().submit()),
            _ => unreachable!(),
        }
        let models = vec![model("server/text"), media_model("server/context")];
        let error = fixture
            .engine
            .refresh_palepale_models_with(CancellationToken::new(), async { Ok(models.clone()) })
            .await
            .unwrap_err();
        assert!(error.contains("Stop recording and wait"), "{busy}: {error}");
        assert_eq!(
            fixture
                .engine
                .config()
                .unwrap()
                .settings
                .palepale
                .chat_model,
            "server/text"
        );
        assert_eq!(
            fixture.engine.provider_revision.load(Ordering::SeqCst),
            revision
        );
        assert_eq!(
            std::fs::read(fixture.directory.join("settings.json")).unwrap(),
            before
        );
        assert!(matches!(
            events.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        if let Some(job) = job {
            lock(&fixture.engine.jobs).unwrap().complete(job);
        }
        drop(permit);
        fixture.engine.active.store(false, Ordering::SeqCst);
        fixture.engine.stopping.store(false, Ordering::SeqCst);
        *lock(&fixture.engine.assistant).unwrap() = None;
        fixture
            .engine
            .refresh_palepale_models_with(CancellationToken::new(), async { Ok(models) })
            .await
            .unwrap();
        assert_eq!(
            fixture
                .engine
                .config()
                .unwrap()
                .settings
                .palepale
                .chat_model,
            "server/context"
        );
    }
}

#[tokio::test]
async fn cancelled_approval_is_saved_inactively_and_needs_explicit_activation() {
    let fixture = Fixture::new();
    let cancel = CancellationToken::new();
    fixture.engine.cancel_palepale_pairing(cancel.clone());
    let mut events = fixture.engine.subscribe();
    let error = fixture
        .engine
        .finish_palepale_pairing_with(credential(&fixture.engine), cancel, async {
            panic!("cancelled approval must not fetch")
        })
        .await
        .err()
        .unwrap();
    assert!(error.contains("saved securely"));
    assert_one_config_event(&mut events, ProviderMode::Byok, true);
    assert!(lock(&fixture.engine.configuration).unwrap().key.is_empty());
    assert!(
        fixture
            .credentials
            .load_saved(&hosted_scope())
            .unwrap()
            .is_none()
    );
    assert!(
        fixture
            .credentials
            .load_saved(&pending_scope())
            .unwrap()
            .is_some()
    );
    let activated = fixture.engine.activate_saved_palepale().unwrap();
    assert!(activated.settings.is_palepale());
    assert_eq!(activated.settings.chat_model, "independent-byok-model");
    assert!(
        fixture
            .credentials
            .load_saved(&pending_scope())
            .unwrap()
            .is_none()
    );
    assert_eq!(
        lock(&fixture.engine.configuration).unwrap().key,
        "synthetic-pairing-key"
    );
}

#[tokio::test]
async fn cancelled_approval_does_not_replace_an_existing_active_hosted_account() {
    let fixture = Fixture::new();
    fixture
        .engine
        .update_configuration(
            |settings| {
                settings.provider_mode = ProviderMode::Palepale;
                settings.palepale.chat_model = "existing-model".into();
                Ok(())
            },
            Some("existing-account-key".into()),
        )
        .unwrap();
    let approved = credential(&fixture.engine);
    let cancel = CancellationToken::new();
    fixture.engine.cancel_palepale_pairing(cancel.clone());
    assert!(
        fixture
            .engine
            .finish_palepale_pairing_with(approved, cancel, async { Ok(vec![]) })
            .await
            .is_err()
    );
    assert_eq!(
        lock(&fixture.engine.configuration).unwrap().key,
        "existing-account-key"
    );
    assert_eq!(
        fixture
            .credentials
            .load_saved(&hosted_scope())
            .unwrap()
            .as_deref(),
        Some("existing-account-key")
    );
    assert_eq!(
        fixture
            .engine
            .config()
            .unwrap()
            .settings
            .palepale
            .chat_model,
        "existing-model"
    );
}

#[tokio::test]
async fn failed_settings_commit_preserves_the_consumed_key_for_retry() {
    let fixture = Fixture::new();
    let previous = std::fs::read(fixture.directory.join("settings.json")).unwrap();
    std::fs::create_dir(fixture.directory.join("settings.json.tmp")).unwrap();
    let error = fixture
        .engine
        .finish_palepale_pairing_with(
            credential(&fixture.engine),
            CancellationToken::new(),
            async { Ok(vec![model("unused")]) },
        )
        .await
        .err()
        .unwrap();
    assert!(error.contains("saved securely"));
    assert_eq!(
        std::fs::read(fixture.directory.join("settings.json")).unwrap(),
        previous
    );
    assert!(!fixture.engine.config().unwrap().settings.is_palepale());
    assert!(
        fixture
            .credentials
            .load_saved(&hosted_scope())
            .unwrap()
            .is_none()
    );
    assert!(
        fixture
            .credentials
            .load_saved(&pending_scope())
            .unwrap()
            .is_some()
    );
    std::fs::remove_dir(fixture.directory.join("settings.json.tmp")).unwrap();
    assert!(
        fixture
            .engine
            .activate_saved_palepale()
            .unwrap()
            .settings
            .is_palepale()
    );
}

#[tokio::test]
async fn failed_credential_storage_is_exposed_without_a_false_connection_event() {
    let fixture = Fixture::new();
    *fixture.credentials.fail.lock().unwrap() = Some((pending_scope(), false));
    let mut events = fixture.engine.subscribe();
    let error = fixture
        .engine
        .finish_palepale_pairing_with(
            credential(&fixture.engine),
            CancellationToken::new(),
            async { Ok(vec![]) },
        )
        .await
        .err()
        .unwrap();
    assert!(error.contains("could not be saved"));
    assert!(!fixture.engine.config().unwrap().has_palepale_key);
    assert!(matches!(
        events.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn busy_provider_preserves_approval_without_changing_the_active_request() {
    let fixture = Fixture::new();
    *lock(&fixture.engine.assistant).unwrap() = Some(CancellationToken::new());
    let error = fixture
        .engine
        .finish_palepale_pairing_with(
            credential(&fixture.engine),
            CancellationToken::new(),
            async { Ok(vec![]) },
        )
        .await
        .err()
        .unwrap();
    assert!(error.contains("activation failed"));
    assert!(!fixture.engine.config().unwrap().settings.is_palepale());
    assert!(
        fixture
            .credentials
            .load_saved(&pending_scope())
            .unwrap()
            .is_some()
    );
    *lock(&fixture.engine.assistant).unwrap() = None;
    assert!(
        fixture
            .engine
            .activate_saved_palepale()
            .unwrap()
            .settings
            .is_palepale()
    );
}

#[tokio::test]
async fn disconnect_supersedes_an_already_consumed_approval() {
    let fixture = Fixture::new();
    let approved = credential(&fixture.engine);
    let mut events = fixture.engine.subscribe();
    assert!(
        !fixture
            .engine
            .disconnect_palepale()
            .unwrap()
            .has_palepale_key
    );
    assert_one_config_event(&mut events, ProviderMode::Byok, false);
    let error = fixture
        .engine
        .finish_palepale_pairing_with(approved, CancellationToken::new(), async {
            Ok(vec![model("late")])
        })
        .await
        .err()
        .unwrap();
    assert!(error.contains("superseded"));
    assert!(fixture.credentials.values.lock().unwrap().is_empty());
    assert!(matches!(
        events.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn late_catalog_cannot_reactivate_after_a_provider_switch_or_disconnect() {
    for disconnect in [false, true] {
        let fixture = Fixture::new();
        let approved = credential(&fixture.engine);
        let (entered, started) = oneshot::channel();
        let (release, released) = oneshot::channel();
        let engine = fixture.engine.clone();
        let task = tokio::spawn(async move {
            engine
                .finish_palepale_pairing_with(approved, CancellationToken::new(), async {
                    entered.send(()).unwrap();
                    released.await.unwrap();
                    Ok(vec![model("late-model")])
                })
                .await
        });
        started.await.unwrap();
        let mut events = fixture.engine.subscribe();
        if disconnect {
            fixture.engine.disconnect_palepale().unwrap();
        } else {
            fixture
                .engine
                .update_preferences(|settings| {
                    settings.provider_mode = ProviderMode::Byok;
                    Ok(())
                })
                .unwrap();
        }
        release.send(()).unwrap();
        let result = task.await.unwrap().unwrap();
        assert!(!result.settings.is_palepale());
        assert!(result.settings.palepale.chat_model.is_empty());
        assert_eq!(result.has_palepale_key, !disconnect);
        assert!(lock(&fixture.engine.published_models).unwrap().is_empty());
        assert_one_config_event(&mut events, ProviderMode::Byok, !disconnect);
    }
}

#[tokio::test]
async fn catalog_outage_recovers_a_missing_model_without_pairing_again() {
    let fixture = Fixture::new();
    let view = fixture
        .engine
        .finish_palepale_pairing_with(
            credential(&fixture.engine),
            CancellationToken::new(),
            async { Err("Synthetic outage".into()) },
        )
        .await
        .unwrap();
    assert!(view.has_palepale_key);
    assert!(view.settings.palepale.chat_model.is_empty());
    fixture
        .engine
        .refresh_palepale_models_with(CancellationToken::new(), async {
            Ok(vec![model("recovered-model")])
        })
        .await
        .unwrap();
    let current = fixture.engine.config().unwrap();
    assert_eq!(current.settings.palepale.chat_model, "recovered-model");
    assert_eq!(
        config::load(&fixture.directory.join("settings.json"))
            .unwrap()
            .palepale
            .chat_model,
        "recovered-model"
    );
    assert_eq!(
        lock(&fixture.engine.configuration).unwrap().key,
        "synthetic-pairing-key"
    );
}

#[tokio::test]
async fn disconnect_emits_only_the_post_delete_state_and_reports_partial_failure() {
    for fail_delete in [false, true] {
        let fixture = Fixture::new();
        fixture
            .engine
            .finish_palepale_pairing_with(
                credential(&fixture.engine),
                CancellationToken::new(),
                async { Ok(vec![model("usable")]) },
            )
            .await
            .unwrap();
        if fail_delete {
            *fixture.credentials.fail.lock().unwrap() = Some((hosted_scope(), true));
        }
        let mut events = fixture.engine.subscribe();
        let result = fixture.engine.disconnect_palepale();
        assert_eq!(result.is_err(), fail_delete);
        let current = fixture.engine.config().unwrap();
        assert!(!current.settings.is_palepale());
        assert_eq!(current.has_palepale_key, fail_delete);
        assert!(!current.has_api_key);
        assert_one_config_event(&mut events, ProviderMode::Byok, fail_delete);
    }
}

#[tokio::test]
async fn shutdown_drains_a_consumed_credential_before_releasing_the_runtime() {
    let fixture = Fixture::new();
    let mut approved = credential(&fixture.engine);
    fixture
        .engine
        .pairing_work
        .active
        .fetch_add(1, Ordering::SeqCst);
    approved.lease = Some(PairingLease(fixture.engine.pairing_work.clone()));
    let engine = fixture.engine.clone();
    let closing = tokio::spawn(async move { engine.shutdown().await });
    fixture.engine.pairing_work.shutdown.cancelled().await;
    assert!(!closing.is_finished());
    let result = fixture
        .engine
        .finish_palepale_pairing_with(approved, CancellationToken::new(), async {
            panic!("shutdown must not fetch")
        })
        .await;
    assert!(result.err().unwrap().contains("saved securely"));
    tokio::time::timeout(Duration::from_secs(2), closing)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(!fixture.engine.config().unwrap().settings.is_palepale());
    assert!(
        fixture
            .credentials
            .load_saved(&pending_scope())
            .unwrap()
            .is_some()
    );
    assert!(
        fixture
            .engine
            .start_palepale_pairing(CancellationToken::new())
            .await
            .err()
            .unwrap()
            .contains("closing")
    );
}

#[tokio::test]
async fn shutdown_does_not_wait_for_discovery_once_the_credential_is_durable() {
    let fixture = Fixture::new();
    let mut approved = credential(&fixture.engine);
    fixture
        .engine
        .pairing_work
        .active
        .fetch_add(1, Ordering::SeqCst);
    approved.lease = Some(PairingLease(fixture.engine.pairing_work.clone()));
    let (entered, started) = oneshot::channel();
    let (release, released) = oneshot::channel();
    let engine = fixture.engine.clone();
    let completing = tokio::spawn(async move {
        engine
            .finish_palepale_pairing_with(approved, CancellationToken::new(), async {
                entered.send(()).unwrap();
                released.await.unwrap();
                Ok(vec![model("late")])
            })
            .await
    });
    started.await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), fixture.engine.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert!(!completing.is_finished());
    assert!(
        fixture
            .credentials
            .load_saved(&hosted_scope())
            .unwrap()
            .is_some()
    );
    release.send(()).unwrap();
    completing.await.unwrap().unwrap();
}
