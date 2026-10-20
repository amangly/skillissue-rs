use super::{EngineEvent, NativeEngine, TRANSCRIPT_LIMIT, events::LiveAudioStatus, lock};
use crate::{
    audio::{StreamCallback, StreamEvent},
    jobs::TranscriptionState,
    models::{Settings, TranscriptEntry, now_ms},
    provider::live::{self, Command},
};
use skillissue_domain::models::SpeechMetadata;
use std::{
    io::Write,
    sync::{Arc, atomic::Ordering},
    time::Instant,
};
use tokio::sync::mpsc;

static LIVE_AUDIO_LOG: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct WorkerGuard(Arc<NativeEngine>);
impl Drop for WorkerGuard {
    fn drop(&mut self) {
        self.0.live_workers.fetch_sub(1, Ordering::SeqCst);
        self.0.jobs_done.notify_waiters();
    }
}

impl NativeEngine {
    fn log_live_audio(&self, record: serde_json::Value) {
        // Microphone and system streams can finish simultaneously.
        if let Ok(_guard) = LIVE_AUDIO_LOG.lock()
            && let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.directory.join("live-audio-latency.log"))
        {
            let _ = writeln!(file, "{record}");
        }
    }

    pub(super) fn prepare_live_audio(
        self: &Arc<Self>,
        settings: &Settings,
        key: &str,
    ) -> Result<StreamCallback, String> {
        let url = live::url(settings, key)?;
        lock(&self.live_audio)?.clear();
        let epoch = self.epoch.load(Ordering::SeqCst);
        let mut senders = std::collections::BTreeMap::new();
        for (enabled, source) in [
            (settings.microphone, "microphone"),
            (settings.system_audio, "system"),
        ] {
            if !enabled {
                continue;
            }
            // 100 ms frames: at most 10 seconds of unsent audio per source.
            let (sender, receiver) = mpsc::channel(100);
            senders.insert(source.to_owned(), sender);
            lock(&self.live_audio)?.push(LiveAudioStatus {
                source: source.into(),
                state: "Connecting".into(),
                interim: String::new(),
            });
            let engine = self.clone();
            let settings = settings.clone();
            let key = key.to_owned();
            let url = url.clone();
            self.live_workers.fetch_add(1, Ordering::SeqCst);
            let guard = WorkerGuard(engine.clone());
            self.runtime.spawn(async move {
                let _guard = guard;
                engine
                    .live_worker(source, settings, key, url, epoch, receiver)
                    .await;
            });
        }
        Ok(self.live_callback(senders))
    }

    fn live_callback(
        self: &Arc<Self>,
        senders: std::collections::BTreeMap<String, mpsc::Sender<Command>>,
    ) -> StreamCallback {
        let weak = Arc::downgrade(self);
        Arc::new(move |event| {
            let engine = weak.upgrade().ok_or("Audio engine closed.")?;
            let (source, command, job) = match event {
                StreamEvent::Frame(frame) => (frame.source.clone(), Command::Audio(frame), None),
                StreamEvent::Boundary { source, ending } => {
                    let job = lock(&engine.jobs)?.submit_segment(source.clone(), now_ms());
                    (source, Command::Boundary { job, ending }, Some(job))
                }
            };
            let sent = senders.get(&source).map(|sender| sender.try_send(command));
            if matches!(sent, Some(Ok(()))) {
                return Ok(());
            }
            if let Some(job) = job {
                lock(&engine.jobs)?.finish(job, TranscriptionState::Failed);
                engine.jobs_done.notify_waiters();
            }
            let error = match sent {
                Some(Err(mpsc::error::TrySendError::Closed(_))) => {
                    let reported = lock(&engine.live_audio)?
                        .iter()
                        .any(|status| status.source == source && status.state == "Disconnected");
                    if reported {
                        // The provider worker has already reported the cause.
                        // Capture can still deliver a frame or Stop boundary
                        // while shutdown is being scheduled. Do not overwrite
                        // that cause with a second, generic queue error.
                        return Ok(());
                    }
                    "Live transcription worker closed unexpectedly. Capture stopped; unsent audio was not replayed."
                }
                Some(Err(mpsc::error::TrySendError::Full(_))) => {
                    "Live audio queue is full. Capture stopped; unsent audio was not replayed."
                }
                _ => "No live transcription connection exists for this audio source.",
            };
            engine.gap(&source, now_ms(), error);
            Err(error.into())
        })
    }

    fn stream_status(&self, source: &str, epoch: u64, state: &str, interim: String) {
        if self.epoch.load(Ordering::SeqCst) != epoch {
            return;
        }
        if let Ok(mut statuses) = self.live_audio.lock()
            && let Some(status) = statuses.iter_mut().find(|status| status.source == source)
        {
            status.state = state.into();
            status.interim = interim;
        }
    }

    fn finish_live_boundary(&self, boundary: Option<(u64, bool)>, success: bool) {
        if let Some((job, _)) = boundary {
            if let Ok(mut jobs) = self.jobs.lock() {
                jobs.finish(
                    job,
                    if success {
                        TranscriptionState::Ready
                    } else {
                        TranscriptionState::Failed
                    },
                );
            }
            self.jobs_done.notify_waiters();
        }
    }

    async fn live_worker(
        self: Arc<Self>,
        source: &str,
        settings: Settings,
        key: String,
        url: String,
        epoch: u64,
        mut receiver: mpsc::Receiver<Command>,
    ) {
        while let Some(command) = receiver.recv().await {
            let first = match command {
                Command::Audio(frame) => frame,
                Command::Boundary { job, ending } => {
                    self.finish_live_boundary(Some((job, ending)), true);
                    if ending {
                        break;
                    }
                    continue;
                }
            };
            let origin = first.created_at;
            let stream_id = uuid::Uuid::new_v4().to_string();
            let started = Instant::now();
            let mut boundary = None;
            let result =
                live::connection(&url, &key, first, &mut receiver, &mut boundary,
                || self.stream_status(source, epoch, "Listening", String::new()),
                |result| {
                    self.stream_status(
                        source,
                        epoch,
                        "Listening",
                        if result.is_final {
                            String::new()
                        } else {
                            result.text.clone()
                        },
                    );
                    if result.is_final && !result.text.is_empty() {
                        self.log_live_audio(serde_json::json!({"source":source,"finalLagMs":now_ms().saturating_sub(origin.saturating_add(result.end_ms)),"timestampMs":now_ms()}));
                        self.save_live_transcript(
                            source, &settings, epoch, &stream_id, origin, result,
                        )?;
                    }
                    Ok(())
                })
                .await;
            self.finish_live_boundary(boundary, result.is_ok());
            self.log_live_audio(serde_json::json!({"source":source,"connectionMs":started.elapsed().as_millis(),"timestampMs":now_ms(),"ok":result.is_ok()}));
            if let Err(error) = result {
                self.stream_status(source, epoch, "Disconnected", String::new());
                self.gap(source, now_ms(), &error);
                // Reject future frames, resolve all accepted barriers, then stop capture.
                receiver.close();
                while let Some(command) = receiver.recv().await {
                    if let Command::Boundary { job, ending } = command {
                        self.finish_live_boundary(Some((job, ending)), false);
                    }
                }
                self.stop_after_failure();
                return;
            }
            self.stream_status(source, epoch, "Listening", String::new());
            if boundary.is_some_and(|(_, ending)| ending) {
                break;
            }
        }
        self.stream_status(source, epoch, "Stopped", String::new());
    }

    fn save_live_transcript(
        &self,
        source: &str,
        settings: &Settings,
        epoch: u64,
        stream_id: &str,
        origin: u64,
        result: live::ResultText,
    ) -> Result<(), String> {
        let _transition = lock(&self.transition)?;
        if self.epoch.load(Ordering::SeqCst) != epoch {
            return Err("The session changed before live transcription finished.".into());
        }
        let entry = TranscriptEntry {
            id: uuid::Uuid::new_v4().to_string(),
            source: source.into(),
            text: result.text,
            created_at: origin.saturating_add(result.start_ms),
            speech: Some(SpeechMetadata {
                stream_id: stream_id.into(),
                start_ms: result.start_ms,
                end_ms: result.end_ms,
                words: result.words,
            }),
        };
        let mut session = lock(&self.session)?;
        let id = session
            .id
            .clone()
            .ok_or("The capture session is not available.")?;
        lock(&self.history)?.append_transcript(
            &id,
            settings,
            self.active.load(Ordering::SeqCst),
            &entry,
        )?;
        let position = session
            .transcript
            .iter()
            .position(|saved| (saved.created_at, &saved.id) > (entry.created_at, &entry.id))
            .unwrap_or(session.transcript.len());
        session.transcript.insert(position, entry.clone());
        while session.transcript.len() > TRANSCRIPT_LIMIT {
            session.transcript.pop_front();
        }
        drop(session);
        self.emit(EngineEvent::Transcript(entry));
        self.emit(EngineEvent::HistoryChanged);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::LiveFrame;
    use skillissue_domain::models::SpeechWord;

    #[tokio::test]
    async fn live_boundary_finishes_only_after_final_transcript_is_saved() {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;
        let path = std::env::temp_dir().join(format!("skillissue-drain-{}", uuid::Uuid::new_v4()));
        let engine = NativeEngine::open(path.clone()).unwrap();
        lock(&engine.session).unwrap().id = Some("drain".into());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/v1/listen", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            assert!(matches!(
                socket.next().await.unwrap().unwrap(),
                Message::Binary(_)
            ));
            loop {
                if let Message::Text(text) = socket.next().await.unwrap().unwrap()
                    && text.contains("CloseStream")
                {
                    break;
                }
            }
            for response in [
                serde_json::json!({"type":"Results","start":0,"duration":0.1,"is_final":true,"channel":{"alternatives":[{"transcript":"Saved before answering.","words":[{"word":"Saved","start":0,"end":0.12}]}]}}),
                serde_json::json!({"type":"Metadata"}),
            ] {
                socket
                    .send(Message::Text(response.to_string().into()))
                    .await
                    .unwrap();
            }
        });
        let (sender, receiver) = mpsc::channel(10);
        let callback = engine.live_callback(std::collections::BTreeMap::from([(
            "system".into(),
            sender,
        )]));
        callback(StreamEvent::Frame(LiveFrame {
            source: "system".into(),
            pcm: vec![0; 3200],
            created_at: 1000,
        }))
        .unwrap();
        callback(StreamEvent::Boundary {
            source: "system".into(),
            ending: true,
        })
        .unwrap();
        assert_eq!(lock(&engine.jobs).unwrap().pending_count(), 1);
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            engine.clone().live_worker(
                "system",
                Settings::default(),
                "test-secret".into(),
                url,
                0,
                receiver,
            ),
        )
        .await
        .unwrap();
        server.await.unwrap();
        assert_eq!(lock(&engine.jobs).unwrap().pending_count(), 0);
        assert_eq!(
            lock(&engine.jobs).unwrap().progress()[0].state,
            TranscriptionState::Ready
        );
        assert_eq!(
            engine.saved_session("drain").unwrap().transcript[0].text,
            "Saved before answering."
        );
        drop(callback);
        drop(engine);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[tokio::test]
    async fn live_metadata_survives_history_and_stale_results_are_rejected() {
        let path = std::env::temp_dir().join(format!("skillissue-live-{}", uuid::Uuid::new_v4()));
        let engine = NativeEngine::open(path.clone()).unwrap();
        lock(&engine.session).unwrap().id = Some("test".into());
        let result = || live::ResultText {
            text: "Agreed.".into(),
            start_ms: 0,
            end_ms: 100,
            is_final: true,
            words: vec![SpeechWord {
                text: "Agreed.".into(),
                start_ms: 0,
                end_ms: 100,
                speaker: Some(1),
                confidence: Some(0.95),
            }],
        };
        engine
            .save_live_transcript(
                "system",
                &Settings::default(),
                0,
                "stream-one",
                1000,
                result(),
            )
            .unwrap();
        let saved = engine.saved_session("test").unwrap();
        assert_eq!(
            saved.transcript[0].speech.as_ref().unwrap().words[0].speaker,
            Some(1)
        );
        assert_eq!(saved.transcript[0].created_at, 1000);
        assert_eq!(
            saved.transcript[0].speaker_label(),
            "System audio · Speaker 2"
        );
        engine.epoch.store(1, Ordering::SeqCst);
        assert!(
            engine
                .save_live_transcript("system", &Settings::default(), 0, "old", 1000, result())
                .is_err()
        );
        assert_eq!(engine.saved_session("test").unwrap().transcript.len(), 1);
        engine.live_workers.store(1, Ordering::SeqCst);
        assert!(engine.new_session().is_err());
        engine.live_workers.store(0, Ordering::SeqCst);
        drop(engine);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[tokio::test]
    async fn full_stream_queue_resolves_rejected_boundary_and_never_replays() {
        let path =
            std::env::temp_dir().join(format!("skillissue-live-full-{}", uuid::Uuid::new_v4()));
        let engine = NativeEngine::open(path.clone()).unwrap();
        let (sender, mut receiver) = mpsc::channel(1);
        let callback = engine.live_callback(std::collections::BTreeMap::from([(
            "system".into(),
            sender,
        )]));
        callback(StreamEvent::Frame(LiveFrame {
            source: "system".into(),
            pcm: vec![0; 3200],
            created_at: 0,
        }))
        .unwrap();
        assert!(
            callback(StreamEvent::Boundary {
                source: "system".into(),
                ending: true
            })
            .is_err()
        );
        assert_eq!(lock(&engine.jobs).unwrap().pending_count(), 0);
        assert_eq!(
            lock(&engine.jobs).unwrap().progress()[0].state,
            TranscriptionState::Failed
        );
        assert!(matches!(receiver.try_recv(), Ok(Command::Audio(_))));
        assert!(receiver.try_recv().is_err());
        drop(callback);
        drop(engine);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[tokio::test]
    async fn reported_disconnect_keeps_original_error_and_resolves_late_boundary() {
        let path = std::env::temp_dir().join(format!("skillissue-closed-{}", uuid::Uuid::new_v4()));
        let engine = NativeEngine::open(path.clone()).unwrap();
        lock(&engine.live_audio).unwrap().push(LiveAudioStatus {
            source: "microphone".into(),
            state: "Disconnected".into(),
            interim: String::new(),
        });
        let original = "Deepgram connection failed; pending audio was not replayed.";
        engine.gap("microphone", 1000, original);
        let (sender, receiver) = mpsc::channel(1);
        let callback = engine.live_callback(std::collections::BTreeMap::from([(
            "microphone".into(),
            sender,
        )]));
        drop(receiver);
        callback(StreamEvent::Frame(LiveFrame {
            source: "microphone".into(),
            pcm: vec![0; 3200],
            created_at: 1000,
        }))
        .unwrap();
        callback(StreamEvent::Boundary {
            source: "microphone".into(),
            ending: true,
        })
        .unwrap();
        assert_eq!(lock(&engine.jobs).unwrap().pending_count(), 0);
        assert_eq!(
            lock(&engine.jobs).unwrap().progress()[0].state,
            TranscriptionState::Failed
        );
        let gaps = std::fs::read_to_string(path.join("capture-gaps.jsonl")).unwrap();
        assert_eq!(gaps.lines().count(), 1);
        assert!(gaps.contains(original));
        drop(callback);
        drop(engine);
        std::fs::remove_dir_all(path).unwrap();
    }
}
