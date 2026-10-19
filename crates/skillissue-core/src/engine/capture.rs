//! Audio capture, bounded transcription jobs, loss reporting and ordered shutdown.

use super::{EngineEvent, NativeEngine, QueuedSegment, TRANSCRIPT_LIMIT, lock};
use crate::{
    audio::{self, AudioCapture, AudioSegment, CaptureOptions},
    models::{TranscriptEntry, now_ms},
    provider,
};
use std::{
    fs::OpenOptions,
    io::Write,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

// Multiple transcription workers append concurrently; hold the lock across the
// complete formatted record, which can involve several underlying writes.
static TRANSCRIPTION_LOG: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl NativeEngine {
    pub async fn start_capture(self: &Arc<Self>) -> Result<(), String> {
        let refresh_hosted = {
            let configuration = lock(&self.configuration)?;
            if configuration.settings.is_palepale() && !self.active.load(Ordering::SeqCst) {
                if configuration.settings.palepale.chat_model.is_empty() {
                    return Err(
                        "Refresh Palepale in Settings > AI provider before recording.".into(),
                    );
                }
                if configuration.key.is_empty() {
                    return Err(
                        "Connect Palepale in Settings > AI provider before recording.".into(),
                    );
                }
                true
            } else {
                false
            }
        };
        if refresh_hosted {
            self.palepale_models(tokio_util::sync::CancellationToken::new())
                .await?;
        }
        let engine = self.clone();
        tokio::task::spawn_blocking(move || {
            let _transition = lock(&engine.transition)?;
            if engine.stopping.load(Ordering::SeqCst) {
                return Err("The previous recording is still finishing.".into());
            }
            if engine.active.load(Ordering::SeqCst) {
                return Ok(());
            }
            let (settings, _) = lock(&engine.configuration)?.transcription_configuration()?;
            if settings.is_palepale() && !lock(&engine.published_models)?.iter().any(|model| {
                model.id == settings.provider_transcription_model()
                    && model.capabilities.streaming && model.capabilities.audio_input
            }) {
                return Err("Palepale audio is unavailable. Refresh Palepale in Settings > AI provider, or use BYOK with a transcription provider.".into());
            }
            let mut slot = lock(&engine.audio)?;
            let weak_segments = Arc::downgrade(&engine);
            let weak_levels = Arc::downgrade(&engine);
            let weak_errors = Arc::downgrade(&engine);
            let recording = AudioCapture::start(
                CaptureOptions {
                    microphone: settings.microphone,
                    system_audio: settings.system_audio,
                    chunk_seconds: if settings.is_palepale() { settings.audio_chunk_seconds.min(3) } else { settings.audio_chunk_seconds },
                },
                Arc::new(move |segment| {
                    if let Some(engine) = weak_segments.upgrade() {
                        engine.queue_transcription(segment);
                    }
                }),
                Arc::new(move |level| {
                    if let Some(engine) = weak_levels.upgrade() {
                        engine.emit(EngineEvent::AudioLevel(level));
                    }
                }),
                Arc::new(move |error| {
                    if let Some(engine) = weak_errors.upgrade() {
                        engine.emit(EngineEvent::Error(error));
                        engine.stop_after_failure();
                    }
                }),
            )?;
            *slot = Some(recording);
            engine.active.store(true, Ordering::SeqCst);
            let persisted = (|| {
                let mut session = lock(&engine.session)?;
                let id = session
                    .id
                    .get_or_insert_with(|| uuid::Uuid::new_v4().to_string())
                    .clone();
                // An empty batch establishes the durable session before the UI reports recording.
                lock(&engine.history)?.append_messages(&id, &settings, true, &[])
            })();
            if let Err(error) = persisted {
                engine.active.store(false, Ordering::SeqCst);
                if let Some(recording) = slot.take() {
                    recording.stop();
                }
                return Err(error);
            }
            drop(slot);
            engine.publish_snapshot();
            engine.emit(EngineEvent::HistoryChanged);
            Ok(())
        })
        .await
        .map_err(|e| format!("Start audio worker: {e}"))?
    }

    fn stop_after_failure(self: &Arc<Self>) {
        let engine = self.clone();
        self.runtime.spawn(async move {
            if let Err(error) = engine.stop_capture().await {
                engine.emit(EngineEvent::Error(error));
            }
        });
    }

    pub(super) fn queue_transcription(self: &Arc<Self>, segment: AudioSegment) {
        let result = (|| {
            let configuration = lock(&self.configuration)?;
            let (settings, key) = configuration.transcription_configuration()?;
            let job = lock(&self.jobs)?.submit();
            let created_at = now_ms().saturating_sub(
                segment.samples.len() as u64 * 1000 / u64::from(segment.sample_rate.max(1)),
            );
            let queued = QueuedSegment {
                epoch: self.epoch.load(Ordering::SeqCst),
                job,
                created_at,
                queued_at: Instant::now(),
                segment,
                settings,
                key,
            };
            if let Err(error) = self.transcriptions.try_send(queued) {
                let queued = error.into_inner();
                lock(&self.jobs)?.complete(job);
                self.jobs_done.notify_waiters();
                self.gap(&queued.segment.source, created_at, "The transcription queue filled. Capture was stopped; this audio segment was not transcribed.");
                return Err("Transcription cannot keep up. Capture has stopped; pending audio will finish processing.".into());
            }
            Ok::<(), String>(())
        })();
        if let Err(error) = result {
            self.emit(EngineEvent::Error(error));
            self.stop_after_failure();
        }
    }

    pub(super) fn gap(&self, source: &str, created_at: u64, message: &str) {
        let record = serde_json::json!({ "sessionId": self.session.lock().ok().and_then(|s| s.id.clone()), "source":source,"createdAt":created_at,"message":message });
        let persisted = (|| {
            let mut file = OpenOptions::new()
                .append(true)
                .create(true)
                .open(self.directory.join("capture-gaps.jsonl"))
                .map_err(|e| e.to_string())?;
            writeln!(file, "{record}").map_err(|e| e.to_string())?;
            file.sync_data().map_err(|e| e.to_string())
        })();
        if let Err(error) = persisted {
            self.emit(EngineEvent::Error(format!(
                "Could not journal the capture gap: {error}"
            )));
        }
        self.emit(EngineEvent::CaptureGap {
            source: source.into(),
            created_at,
            message: message.into(),
        });
    }

    pub(super) async fn transcribe(self: Arc<Self>, queued: QueuedSegment) {
        let started = Instant::now();
        let queue_ms = started.duration_since(queued.queued_at).as_millis();
        let encoding_started = Instant::now();
        let wav = if queued.settings.is_palepale() {
            audio::encode_hosted_wav(&queued.segment)
        } else {
            audio::encode_wav(&queued.segment)
        };
        let encoding_ms = encoding_started.elapsed().as_millis();
        let provider_started = Instant::now();
        let provider_result = match wav {
            Ok(wav) => provider::transcribe(&self.client, &queued.settings, &queued.key, wav).await,
            Err(error) => Err(error),
        };
        let provider_ms = provider_started.elapsed().as_millis();
        let persisted = (|| {
            let _transition = lock(&self.transition)?;
            if self.epoch.load(Ordering::SeqCst) != queued.epoch {
                return Err("The session changed before transcription could be saved.".into());
            }
            let text = provider_result?;
            if text.trim().is_empty() {
                return Ok(());
            }
            let entry = TranscriptEntry {
                id: uuid::Uuid::new_v4().to_string(),
                source: queued.segment.source.clone(),
                text,
                created_at: queued.created_at,
            };
            let mut session = lock(&self.session)?;
            let id = session
                .id
                .get_or_insert_with(|| uuid::Uuid::new_v4().to_string())
                .clone();
            lock(&self.history)?.append_transcript(
                &id,
                &queued.settings,
                self.active.load(Ordering::SeqCst),
                &entry,
            )?;
            let position = session
                .transcript
                .iter()
                .position(|existing| {
                    (existing.created_at, &existing.id) > (entry.created_at, &entry.id)
                })
                .unwrap_or(session.transcript.len());
            session.transcript.insert(position, entry.clone());
            while session.transcript.len() > TRANSCRIPT_LIMIT {
                session.transcript.pop_front();
            }
            drop(session);
            self.emit(EngineEvent::Transcript(entry));
            self.emit(EngineEvent::HistoryChanged);
            Ok::<(), String>(())
        })();
        let total_ms = started.elapsed().as_millis();
        if let Ok(_guard) = TRANSCRIPTION_LOG.lock()
            && let Ok(mut log) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.directory.join("transcription-latency.log"))
        {
            let entry = serde_json::json!({
                "job": queued.job,
                "source": queued.segment.source.clone(),
                "model": queued.settings.provider_transcription_model(),
                "createdAt": queued.created_at,
                "timestampMs": now_ms(),
                "queueMs": queue_ms,
                "encodingMs": encoding_ms,
                "providerMs": provider_ms,
                "totalMs": total_ms,
                "ok": persisted.is_ok(),
            });
            let _ = writeln!(log, "{entry}");
        }
        if let Err(error) = persisted {
            self.gap(&queued.segment.source, queued.created_at, &error);
        }
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.complete(queued.job);
        }
        self.jobs_done.notify_waiters();
    }

    pub async fn stop_capture(self: &Arc<Self>) -> Result<(), String> {
        if self
            .stopping
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            loop {
                let done = self.stop_done.notified();
                if !self.stopping.load(Ordering::SeqCst) {
                    return lock(&self.last_stop_error)?.clone().map_or(Ok(()), Err);
                }
                done.await;
            }
        }
        let result = self.finish_capture().await;
        if let Ok(mut error) = self.last_stop_error.lock() {
            *error = result.as_ref().err().cloned();
        }
        self.stopping.store(false, Ordering::SeqCst);
        self.stop_done.notify_waiters();
        result
    }
    async fn finish_capture(self: &Arc<Self>) -> Result<(), String> {
        let engine = self.clone();
        tokio::task::spawn_blocking(move || {
            let _transition = lock(&engine.transition)?;
            engine.active.store(false, Ordering::SeqCst);
            if let Some(recording) = lock(&engine.audio)?.take() {
                recording.stop();
            }
            engine.publish_snapshot();
            Ok::<(), String>(())
        })
        .await
        .map_err(|e| format!("Stop audio worker: {e}"))??;
        let watermark = lock(&self.jobs)?.watermark();
        tokio::time::timeout(Duration::from_secs(150), async {
            loop { let notified = self.jobs_done.notified(); if lock(&self.jobs)?.finished_through(watermark) { return Ok::<(),String>(()); } notified.await; }
        }).await.map_err(|_| "Capture stopped, but transcription is still pending. Keep skillissue open until it finishes.".to_string())??;
        {
            let _transition = lock(&self.transition)?;
            if let Some(id) = &lock(&self.session)?.id {
                lock(&self.history)?.finish(id)?;
            }
        }
        self.publish_snapshot();
        self.emit(EngineEvent::HistoryChanged);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config,
        models::{ProviderMode, Settings},
    };

    #[tokio::test]
    async fn hosted_capture_fails_before_devices_or_session_storage_are_touched() {
        let directory = std::env::temp_dir().join(format!(
            "skillissue-hosted-capture-{}",
            uuid::Uuid::new_v4()
        ));
        config::save(
            &directory.join("settings.json"),
            &Settings {
                provider_mode: ProviderMode::Palepale,
                ..Settings::default()
            },
        )
        .unwrap();
        let engine = NativeEngine::open(directory.clone()).unwrap();
        let error = engine.start_capture().await.unwrap_err();
        assert!(error.contains("Refresh Palepale in Settings > AI provider"));
        assert!(lock(&engine.audio).unwrap().is_none());
        assert!(!engine.snapshot().unwrap().active);
        assert!(engine.snapshot().unwrap().session_id.is_none());
        assert!(engine.list_sessions("", 10, 0).unwrap().is_empty());
        engine.shutdown().await.unwrap();
        drop(engine);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
