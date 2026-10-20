//! Engine integration regressions using isolated profiles and loopback providers.

use super::*;
use crate::{audio::AudioSegment, config, models::*};
use std::sync::atomic::Ordering;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

struct Fixture(PathBuf);

#[tokio::test]
async fn automatic_model_loading_keeps_other_roles_when_audio_has_no_key() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let fixture = Fixture::new(&base);
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    engine
        .update_preferences(|s| {
            s.byok_audio_enabled = true;
            s.include_screenshot = true;
            s.transcription_base_url = "https://audio.example/v1".into();
            Ok(())
        })
        .unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut headers = Vec::new();
            while !headers.windows(4).any(|part| part == b"\r\n\r\n") {
                let mut buffer = [0; 1024];
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(count > 0);
                headers.extend_from_slice(&buffer[..count]);
            }
            assert!(String::from_utf8_lossy(&headers).starts_with("GET /v1/models "));
            let body = r#"{"data":[{"id":"fixture-model","input_modalities":["text","image"],"output_modalities":["text"]}]}"#;
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
    });
    let lists = tokio::time::timeout(
        Duration::from_secs(5),
        engine.byok_model_lists(CancellationToken::new()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(lists.len(), 3);
    assert_eq!(lists[0].endpoint, base);
    assert_eq!(lists[0].models.as_ref().unwrap(), &["fixture-model"]);
    assert!(lists[1].models.is_err());
    assert_eq!(lists[2].models.as_ref().unwrap(), &["fixture-model"]);
    server.await.unwrap();
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn history_restore_waits_for_idle_and_notifies_only_after_a_complete_import() {
    let source = Fixture::new("http://127.0.0.1:1/v1");
    let destination = Fixture::new("http://127.0.0.1:1/v1");
    let original = NativeEngine::open(source.0.clone()).unwrap();
    let engine = NativeEngine::open(destination.0.clone()).unwrap();
    lock(&original.history)
        .unwrap()
        .append_messages(
            "saved",
            &Settings::default(),
            false,
            &[Message {
                id: "saved-answer".into(),
                role: "assistant".into(),
                content: "Recovered answer".into(),
                created_at: now_ms(),
            }],
        )
        .unwrap();
    let backup = source.0.join("history-backup.sqlite3");
    original.backup_history(&backup).unwrap();
    let bytes = std::fs::read(&backup).unwrap();
    let mut events = engine.subscribe();
    engine.active.store(true, Ordering::SeqCst);
    assert!(engine.restore_history(&backup).is_err());
    engine.active.store(false, Ordering::SeqCst);
    *lock(&engine.assistant).unwrap() = Some(tokio_util::sync::CancellationToken::new());
    assert!(engine.restore_history(&backup).is_err());
    *lock(&engine.assistant).unwrap() = None;
    let job = lock(&engine.jobs).unwrap().submit();
    assert!(engine.restore_history(&backup).is_err());
    lock(&engine.jobs).unwrap().complete(job);
    assert!(events.try_recv().is_err());
    assert!(engine.saved_session("saved").is_err());
    assert_eq!(engine.restore_history(&backup).unwrap(), 1);
    assert!(matches!(
        events.try_recv().unwrap(),
        EngineEvent::HistoryChanged
    ));
    assert_eq!(
        engine.saved_session("saved").unwrap().messages[0].content,
        "Recovered answer"
    );
    assert!(engine.snapshot().unwrap().session_id.is_none());
    assert!(engine.restore_history(&backup).is_err());
    assert!(events.try_recv().is_err());
    assert_eq!(std::fs::read(&backup).unwrap(), bytes);
    engine.shutdown().await.unwrap();
    drop(engine);
    let reopened = NativeEngine::open(destination.0.clone()).unwrap();
    assert_eq!(
        reopened.saved_session("saved").unwrap().messages[0].content,
        "Recovered answer"
    );
    reopened.resume_session("saved").unwrap();
    assert_eq!(
        reopened.snapshot().unwrap().messages[0].content,
        "Recovered answer"
    );
    assert!(!reopened.snapshot().unwrap().active);
    assert!(!reopened.config().unwrap().has_api_key);
    assert_eq!(std::fs::read(&backup).unwrap(), bytes);
    reopened.shutdown().await.unwrap();
    original.shutdown().await.unwrap();
}

#[tokio::test]
async fn concurrent_resume_and_delete_never_install_a_deleted_session() {
    let fixture = Fixture::new("http://127.0.0.1:1/v1");
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    for index in 0..64 {
        engine.new_session().unwrap();
        let id = format!("resume-delete-{index}");
        lock(&engine.history)
            .unwrap()
            .append_messages(&id, &Settings::default(), false, &[])
            .unwrap();
        let barrier = std::sync::Barrier::new(2);
        let (resumed, deleted) = std::thread::scope(|scope| {
            let resume = scope.spawn(|| {
                barrier.wait();
                engine.resume_session(&id)
            });
            let delete = scope.spawn(|| {
                barrier.wait();
                engine.delete_saved_session(&id)
            });
            (resume.join().unwrap(), delete.join().unwrap())
        });
        assert_ne!(
            resumed.is_ok(),
            deleted.is_ok(),
            "exactly one transition must succeed"
        );
        if resumed.is_ok() {
            assert_eq!(
                engine.snapshot().unwrap().session_id.as_deref(),
                Some(id.as_str())
            );
            assert!(engine.saved_session(&id).is_ok());
        } else {
            assert!(engine.snapshot().unwrap().session_id.is_none());
            assert!(engine.saved_session(&id).is_err());
        }
    }
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn deletion_waits_for_saved_answers_and_notes() {
    let fixture = Fixture::new("http://127.0.0.1:1/v1");
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    let settings = Settings::default();
    lock(&engine.history)
        .unwrap()
        .append_messages(
            "delete-me",
            &settings,
            false,
            &[Message {
                id: "evidence".into(),
                role: "user".into(),
                content: "keep until idle".into(),
                created_at: now_ms(),
            }],
        )
        .unwrap();
    let answer = engine.saved_questions.try_acquire().unwrap();
    assert!(engine.delete_saved_session("delete-me").is_err());
    assert!(engine.saved_session("delete-me").is_ok());
    drop(answer);
    let notes = engine.note_jobs.try_acquire().unwrap();
    assert!(engine.delete_saved_session("delete-me").is_err());
    assert!(engine.saved_session("delete-me").is_ok());
    drop(notes);
    engine.delete_saved_session("delete-me").unwrap();
    assert!(engine.saved_session("delete-me").is_err());
}

#[tokio::test]
async fn transcript_wait_completes_without_waiting_for_future_speech() {
    let fixture = Fixture::new("http://127.0.0.1:1/v1");
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    let earlier = lock(&engine.jobs).unwrap().submit();
    let later = lock(&engine.jobs).unwrap().submit();
    let worker = engine.clone();
    let finish = async move {
        tokio::task::yield_now().await;
        lock(&worker.jobs).unwrap().complete(earlier);
        worker.jobs_done.notify_waiters();
    };
    let wait = engine.wait_for_transcript(earlier, Duration::from_secs(5));
    let (result, ()) =
        tokio::time::timeout(Duration::from_secs(1), async { tokio::join!(wait, finish) })
            .await
            .unwrap();
    let result = result.unwrap();
    assert_eq!(result.watermark, earlier);
    assert_eq!(result.pending, 0);
    assert!(!lock(&engine.jobs).unwrap().finished_through(later));
}

#[tokio::test]
async fn transcript_wait_budget_does_not_require_job_completion() {
    let fixture = Fixture::new("http://127.0.0.1:1/v1");
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    let job = lock(&engine.jobs).unwrap().submit();
    let outcome = tokio::time::timeout(
        Duration::from_secs(1),
        engine.wait_for_transcript(job, Duration::ZERO),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(outcome.pending, 1);
    assert_eq!(outcome.watermark, job);
    assert!(!lock(&engine.jobs).unwrap().finished_through(job));
}

#[tokio::test]
async fn late_transcription_is_saved_and_used_by_the_next_question() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fixture = Fixture::new(&format!("http://{}/v1", listener.local_addr().unwrap()));
        let engine = NativeEngine::open(fixture.0.clone()).unwrap();
        let mut events = engine.subscribe();
        let (release, released) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut audio, _) = listener.accept().await.unwrap();
            let (headers, _) = raw_request(&mut audio).await;
            assert!(headers.contains("/audio/transcriptions"));
            let transcription = tokio::spawn(async move {
                released.await.unwrap();
                let body = r#"{"text":"The agreed budget is forty dollars."}"#;
                audio.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            });
            for _ in 0..1 {
                let (mut chat, _) = listener.accept().await.unwrap();
                let payload = request(&mut chat).await.to_string();
                assert!(payload.contains("The agreed budget is forty dollars."));
                let body = "data: {\"choices\":[{\"delta\":{\"content\":\"Answer\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
                chat.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
            transcription.await.unwrap();
        });
        engine.queue_transcription(AudioSegment {
            source: "microphone".into(), sample_rate: 8_000, samples: vec![0.01; 80],
        });
        let error = engine.assist("What was agreed?".into(), None).await.unwrap_err();
        assert!(error.contains("has not been sent"));
        assert!(engine.snapshot().unwrap().messages.is_empty());
        let mut warned = false;
        while let Ok(event) = events.try_recv() {
            if let EngineEvent::Assistant { kind, .. } = event {
                warned |= kind == "context";
            }
        }
        assert!(warned);
        release.send(()).unwrap();
        let watermark = lock(&engine.jobs).unwrap().watermark();
        assert_eq!(engine.wait_for_transcript(watermark, Duration::from_secs(5)).await.unwrap().pending, 0);
        let snapshot = engine.snapshot().unwrap();
        assert_eq!(snapshot.transcript.len(), 1);
        assert_eq!(engine.saved_session(snapshot.session_id.as_deref().unwrap()).unwrap().transcript.len(), 1);
        engine.assist("What was agreed?".into(), None).await.unwrap();
        server.await.unwrap();
        let log = std::fs::read_to_string(fixture.0.join("provider-requests.log")).unwrap();
        let entries: Vec<serde_json::Value> = log.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
        assert_eq!(entries.len(), 1, "only the explicit retry sends an answer request");
        assert_eq!(entries[0]["audioPendingAtWaitEnd"], 0);
        assert_eq!(engine.context_status().unwrap().transcriptions[0].state, crate::jobs::TranscriptionState::Ready);
        engine.new_session().unwrap();
        assert!(engine.context_status().unwrap().transcriptions.is_empty());
        engine.shutdown().await.unwrap();
    }).await.expect("late transcription workflow timed out");
}

#[tokio::test]
async fn pending_audio_notice_is_not_question_context() {
    let fixture = Fixture::new("http://127.0.0.1:1/v1");
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    lock(&engine.jobs).unwrap().submit();
    let error = engine.assist(String::new(), None).await.unwrap_err();
    assert!(error.contains("has not been sent"), "{error}");
    assert!(engine.snapshot().unwrap().messages.is_empty());
}

#[tokio::test]
async fn cancelling_while_waiting_for_speech_never_sends_an_answer_request() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let fixture = Fixture::new(&format!("http://{}/v1", listener.local_addr().unwrap()));
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    let job = lock(&engine.jobs).unwrap().submit();
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    let mut events = engine.subscribe();
    let canceller = async move {
        while let Ok(event) = events.recv().await {
            if matches!(event, EngineEvent::Assistant { kind, .. } if kind == "preparing") {
                trigger.cancel();
                break;
            }
        }
    };
    let request = engine.assist_with_cancel("What did they say?".into(), None, cancel);
    let (result, ()) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(request, canceller)
    })
    .await
    .unwrap();
    assert!(result.unwrap_err().contains("cancelled"));
    assert!(lock(&engine.assistant).unwrap().is_none());
    assert!(engine.snapshot().unwrap().messages.is_empty());
    assert!(
        tokio::time::timeout(Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
    lock(&engine.jobs).unwrap().complete(job);
    engine.shutdown().await.unwrap();
}
impl Fixture {
    fn new(base: &str) -> Self {
        let path = std::env::temp_dir().join(format!("skillissue-engine-{}", uuid::Uuid::new_v4()));
        let mut settings = Settings {
            api_base_url: base.into(),
            include_screenshot: false,
            ..Settings::default()
        };
        settings.shortcuts.insert("ask".into(), "Ctrl+Alt+K".into());
        config::save(&path.join("settings.json"), &settings).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn raw_request(stream: &mut TcpStream) -> (String, Vec<u8>) {
    let mut bytes = Vec::new();
    let header_end;
    loop {
        let mut buffer = [0; 4096];
        let size = stream.read(&mut buffer).await.unwrap();
        assert!(size > 0);
        bytes.extend_from_slice(&buffer[..size]);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            header_end = end + 4;
            break;
        }
    }
    let headers = String::from_utf8_lossy(&bytes[..header_end]).into_owned();
    assert!(
        !headers.to_ascii_lowercase().contains("authorization:"),
        "Loopback tests must never send a saved or environment production key"
    );
    let length: usize = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|value| value.trim().parse().unwrap())
        })
        .unwrap();
    while bytes.len() < header_end + length {
        let mut buffer = [0; 4096];
        let size = stream.read(&mut buffer).await.unwrap();
        assert!(size > 0);
        bytes.extend_from_slice(&buffer[..size]);
    }
    (headers, bytes[header_end..header_end + length].to_vec())
}

async fn request(stream: &mut TcpStream) -> serde_json::Value {
    let (headers, bytes) = raw_request(stream).await;
    assert!(headers.starts_with("POST /v1/chat/completions HTTP/1.1"));
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn prepared_answer_rejects_changed_provider_before_network_but_allows_preferences() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let fixture = Fixture::new(&format!("http://{}/v1", listener.local_addr().unwrap()));
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    let displayed = engine.config().unwrap().settings;
    let prepared = engine.prepare_assist_target(&displayed).unwrap();
    engine
        .update_preferences(|settings| {
            settings.chat_model = "replacement-model".into();
            Ok(())
        })
        .unwrap();
    assert!(
        engine
            .prepare_assist_target(&displayed)
            .unwrap_err()
            .contains("connection changed")
    );
    let error = engine
        .assist_for_target(
            "Prepared question".into(),
            None,
            tokio_util::sync::CancellationToken::new(),
            prepared,
        )
        .await
        .unwrap_err();
    assert!(error.contains("connection or session changed"), "{error}");
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
    assert!(engine.snapshot().unwrap().messages.is_empty());
    assert!(engine.list_sessions("", 10, 0).unwrap().is_empty());

    let current = engine
        .prepare_assist_target(&engine.config().unwrap().settings)
        .unwrap();
    engine.new_session().unwrap();
    assert!(
        engine
            .assist_for_target(
                "Previous session question".into(),
                None,
                tokio_util::sync::CancellationToken::new(),
                current,
            )
            .await
            .unwrap_err()
            .contains("session changed")
    );
    let current = engine
        .prepare_assist_target(&engine.config().unwrap().settings)
        .unwrap();
    engine
        .update_preferences(|settings| {
            settings.theme = "dark".into();
            Ok(())
        })
        .unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let value = request(&mut stream).await;
        assert_eq!(value["model"], "replacement-model");
        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"Allowed answer\"},\"finish_reason\":null}]}\n\ndata: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
    });
    engine
        .assist_for_target(
            "Current question".into(),
            None,
            tokio_util::sync::CancellationToken::new(),
            current,
        )
        .await
        .unwrap();
    server.await.unwrap();
    assert_eq!(
        engine.snapshot().unwrap().messages[1].content,
        "Allowed answer"
    );
    engine.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_stream_notes_and_restart_reuse_the_same_durable_profile() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let fixture = Fixture::new(&format!("http://{}/v1", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for index in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            requests.push(request(&mut stream).await);
            if index == 0 {
                let body = "data: {\"choices\":[{\"delta\":{\"content\":\"Measured answer\"},\"finish_reason\":null}]}\n\ndata: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            } else {
                let body = serde_json::json!({"choices":[{"message":{"content":"{\"title\":\"Native review\",\"summary\":\"## Decisions\\nKeep durable history.\"}"},"finish_reason":"stop"}]}).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
        }
        requests
    });
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    let mut events = engine.subscribe();
    assert!(!engine.config().unwrap().has_api_key);
    assert_eq!(
        engine.config().unwrap().settings.shortcuts["ask"],
        "Ctrl+Alt+K"
    );
    assert!(
        NativeEngine::open(fixture.0.clone()).is_err(),
        "Two native engines must not write the same profile simultaneously"
    );
    engine
        .assist("What did we decide?".into(), None)
        .await
        .unwrap();
    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.messages.len(), 2);
    assert_eq!(snapshot.messages[1].content, "Measured answer");
    let id = snapshot.session_id.unwrap();
    let mut delta = false;
    let mut done = false;
    let mut request_id = None;
    while let Ok(event) = events.try_recv() {
        if let EngineEvent::Assistant {
            id: event_id,
            kind,
            text,
        } = event
        {
            if kind == "start" {
                assert_eq!(text, "What did we decide?");
                assert!(!event_id.is_empty());
                assert!(request_id.replace(event_id.clone()).is_none());
            }
            assert_eq!(request_id.as_deref(), Some(event_id.as_str()));
            if kind == "delta" {
                delta |= text == "Measured answer";
            }
            if kind == "done" {
                assert_eq!(engine.saved_session(&id).unwrap().messages.len(), 2);
                done = true;
            }
        }
    }
    assert!(request_id.is_some() && delta && done);
    let notes = engine.generate_notes(&id).await.unwrap();
    assert_eq!(notes.title, "Native review");
    assert!(notes.summary.contains("Keep durable history"));
    engine
        .update_notes(&id, "Reviewed title", "Edited notes")
        .unwrap();
    let mut settings = engine.config().unwrap().settings;
    settings
        .shortcuts
        .insert("clear".into(), "Ctrl+Alt+J".into());
    engine.save_settings(settings, None).unwrap();
    engine.shutdown().await.unwrap();
    drop(engine);
    let reopened = NativeEngine::open(fixture.0.clone()).unwrap();
    assert_eq!(
        reopened.config().unwrap().settings.shortcuts["clear"],
        "Ctrl+Alt+J"
    );
    let saved = reopened.saved_session(&id).unwrap();
    assert_eq!(saved.title, "Reviewed title");
    assert_eq!(saved.summary, "Edited notes");
    assert_eq!(saved.messages.len(), 2);
    assert_eq!(reopened.list_sessions("Measured", 50, 0).unwrap().len(), 1);
    reopened.resume_session(&id).unwrap();
    assert_eq!(reopened.snapshot().unwrap().messages.len(), 2);
    reopened.new_session().unwrap();
    assert!(reopened.snapshot().unwrap().session_id.is_none());
    assert_eq!(reopened.list_sessions("", 50, 0).unwrap().len(), 1);
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[0]["messages"]
            .to_string()
            .contains("What did we decide?")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_native_answer_is_never_committed_as_a_complete_message() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let fixture = Fixture::new(&format!("http://{}/v1", listener.local_addr().unwrap()));
    let finish_server = Arc::new(Notify::new());
    let finish = finish_server.clone();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _ = request(&mut stream).await;
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"Partial\"},\"finish_reason\":null}]}\n\n").await.unwrap();
        finish.notified().await;
    });
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    let mut events = engine.subscribe();
    let task_engine = engine.clone();
    let answer = tokio::spawn(async move { task_engine.assist("Cancel me".into(), None).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let EngineEvent::Assistant { kind, text, .. } = events.recv().await.unwrap()
                && kind == "delta"
                && text == "Partial"
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    engine.cancel_assist();
    assert!(
        tokio::time::timeout(Duration::from_secs(3), answer)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err()
            .contains("cancelled")
    );
    assert!(engine.snapshot().unwrap().messages.is_empty());
    assert!(engine.list_sessions("", 50, 0).unwrap().is_empty());
    let mut cancelled = false;
    while let Ok(event) = events.try_recv() {
        if let EngineEvent::Assistant { kind, .. } = event {
            cancelled |= kind == "cancelled";
        }
    }
    assert!(cancelled);
    finish_server.notify_one();
    server.await.unwrap();
}

#[tokio::test]
async fn cancelled_screenshot_scope_never_opens_a_provider_request() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let fixture = Fixture::new(&format!("http://{}/v1", listener.local_addr().unwrap()));
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(
        engine
            .assist_with_cancel("Ask later".into(), None, cancel)
            .await
            .unwrap_err()
            .contains("cancelled")
    );
    assert!(lock(&engine.assistant).unwrap().is_none());
    assert!(engine.snapshot().unwrap().messages.is_empty());
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn saturated_transcription_queue_reports_gap_and_drains_every_accepted_segment() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let fixture = Fixture::new(&format!("http://{}/v1", listener.local_addr().unwrap()));
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    let mut events = engine.subscribe();
    let gate = Arc::new(Semaphore::new(0));
    let server_gate = gate.clone();
    let (seen, mut requests_seen) = mpsc::unbounded_channel();
    let server = tokio::spawn(async move {
        let mut workers = Vec::new();
        for index in 0..20 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let gate = server_gate.clone();
            let seen = seen.clone();
            workers.push(tokio::spawn(async move {
                let (headers, bytes) = raw_request(&mut stream).await;
                assert!(headers.starts_with("POST /v1/audio/transcriptions HTTP/1.1"));
                assert!(bytes.windows(4).any(|part|part == b"RIFF"));
                seen.send(()).unwrap();
                let _permit = gate.acquire().await.unwrap();
                let body = serde_json::json!({"text":format!("segment {index}")}).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }));
        }
        for worker in workers {
            worker.await.unwrap();
        }
    });
    let segment = || AudioSegment {
        source: "microphone".into(),
        sample_rate: 8_000,
        samples: vec![0.01; 80],
    };
    for _ in 0..4 {
        engine.queue_transcription(segment());
    }
    for _ in 0..4 {
        tokio::time::timeout(Duration::from_secs(3), requests_seen.recv())
            .await
            .unwrap()
            .unwrap();
    }
    for _ in 0..16 {
        engine.queue_transcription(segment());
    }
    assert_eq!(engine.context_status().unwrap().pending_transcriptions, 20);
    engine.queue_transcription(segment());
    assert_eq!(engine.context_status().unwrap().pending_transcriptions, 20);
    let mut gap = false;
    while let Ok(event) = events.try_recv() {
        if let EngineEvent::CaptureGap { message, .. } = event {
            gap |= message.contains("queue filled");
        }
    }
    assert!(
        gap,
        "The rejected segment must produce a visible, journaled gap"
    );
    assert!(
        std::fs::read_to_string(fixture.0.join("capture-gaps.jsonl"))
            .unwrap()
            .contains("queue filled")
    );
    gate.add_permits(20);
    tokio::time::timeout(Duration::from_secs(5), engine.stop_capture())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(engine.context_status().unwrap().pending_transcriptions, 0);
    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.transcript.len(), 20);
    assert_eq!(
        engine
            .saved_session(snapshot.session_id.as_deref().unwrap())
            .unwrap()
            .transcript
            .len(),
        20
    );
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
    let latency = std::fs::read_to_string(fixture.0.join("transcription-latency.log")).unwrap();
    let entries = latency
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 20);
    assert!(entries.iter().all(|entry| {
        entry["source"] == "microphone"
            && entry["model"] == Settings::default().transcription_model
            && entry["ok"] == true
            && entry["totalMs"].is_u64()
    }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn overlapping_stop_requests_wait_for_the_same_pending_transcription() {
    let fixture = Fixture::new("http://127.0.0.1:43109/v1");
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    let job = lock(&engine.jobs).unwrap().submit();
    let first_engine = engine.clone();
    let first = tokio::spawn(async move { first_engine.stop_capture().await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while !engine.stopping.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let second_engine = engine.clone();
    let second = tokio::spawn(async move { second_engine.stop_capture().await });
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(!first.is_finished() && !second.is_finished());
    lock(&engine.jobs).unwrap().complete(job);
    engine.jobs_done.notify_waiters();
    tokio::time::timeout(Duration::from_secs(2), first)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), second)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn capture_gap_is_a_durable_explicit_event_and_open_preserves_legacy_active_state() {
    let fixture = Fixture::new("http://127.0.0.1:43109/v1");
    {
        let mut history = History::open(&fixture.0.join("sessions.sqlite3")).unwrap();
        history
            .append_messages("legacy-active", &Settings::default(), true, &[])
            .unwrap();
    }
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    assert_eq!(
        engine.saved_session("legacy-active").unwrap().state,
        "active"
    );
    let mut events = engine.subscribe();
    engine.gap("system", 123, "Provider request failed");
    let line = std::fs::read_to_string(fixture.0.join("capture-gaps.jsonl")).unwrap();
    let gap: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(gap["source"], "system");
    assert_eq!(gap["createdAt"], 123);
    assert!(matches!(
        events.try_recv().unwrap(),
        EngineEvent::CaptureGap {
            created_at: 123,
            ..
        }
    ));
}

#[tokio::test]
async fn document_shared_by_modes_is_preserved_until_every_reference_is_detached() {
    let fixture = Fixture::new("http://127.0.0.1:43109/v1");
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let bytes = b"The shared meeting agenda.";
    assert_eq!(
        engine
            .import_document(&id, "agenda.txt", bytes)
            .unwrap()
            .status,
        "ready"
    );
    let mut settings = engine.config().unwrap().settings;
    settings.modes = ["first", "second"]
        .into_iter()
        .map(|name| AssistantMode {
            id: name.into(),
            name: name.into(),
            prompt: String::new(),
            sections: Vec::new(),
            files: vec![ModeFile {
                id: id.clone(),
                name: "agenda.txt".into(),
                size: bytes.len() as u64,
            }],
        })
        .collect();
    engine.save_settings(settings.clone(), None).unwrap();
    assert!(
        engine
            .remove_document(&id)
            .unwrap_err()
            .contains("Remove this file from its Modes")
    );
    assert_eq!(engine.document_status(&id).unwrap().status, "ready");
    settings.modes[0].files.clear();
    engine.save_settings(settings.clone(), None).unwrap();
    assert!(engine.remove_document(&id).is_err());
    assert_eq!(engine.document_status(&id).unwrap().status, "ready");
    settings.modes[1].files.clear();
    engine.save_settings(settings, None).unwrap();
    engine.remove_document(&id).unwrap();
    assert!(engine.document_status(&id).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn saved_questions_use_only_selected_evidence_and_preserve_other_live_context() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let fixture = Fixture::new(&format!("http://{}/v1", listener.local_addr().unwrap()));
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    let mut settings = engine.config().unwrap().settings;
    settings.smart_mode = true;
    settings.include_screenshot = true;
    settings.system_prompt = "PRIVATE-GENERAL-INSTRUCTIONS".into();
    settings.active_mode_id = "private-mode".into();
    settings.modes = vec![AssistantMode {
        id: "private-mode".into(),
        name: "Private mode".into(),
        prompt: "PRIVATE-MODE-INSTRUCTIONS".into(),
        sections: vec![],
        files: vec![],
    }];
    engine.save_settings(settings.clone(), None).unwrap();
    {
        let mut history = lock(&engine.history).unwrap();
        history
            .append_transcript(
                "selected",
                &settings,
                false,
                &TranscriptEntry {
                    id: "selected-speech".into(),
                    source: "system".into(),
                    text: "SELECTED-EVIDENCE: delivery is Tuesday".into(),
                    created_at: 100,
                },
            )
            .unwrap();
        history
            .update(
                "selected",
                Some("Selected planning session"),
                Some("SELECTED-NOTES: agreed delivery"),
            )
            .unwrap();
        history
            .append_transcript(
                "other",
                &settings,
                false,
                &TranscriptEntry {
                    id: "other-speech".into(),
                    source: "microphone".into(),
                    text: "OTHER-LIVE-PRIVATE-SPEECH".into(),
                    created_at: 200,
                },
            )
            .unwrap();
        history
            .append_messages(
                "other",
                &settings,
                false,
                &[Message {
                    id: "other-message".into(),
                    role: "user".into(),
                    content: "OTHER-LIVE-PRIVATE-QUESTION".into(),
                    created_at: 201,
                }],
            )
            .unwrap();
    }
    engine.resume_session("other").unwrap();
    engine.active.store(true, Ordering::SeqCst);
    lock(&engine.history).unwrap().mark_active("other").unwrap();
    let original_live = serde_json::to_value(engine.snapshot().unwrap()).unwrap();
    let gate = Arc::new(Semaphore::new(0));
    let server_gate = gate.clone();
    let (observed, mut requests) = mpsc::unbounded_channel();
    let server = tokio::spawn(async move {
        for index in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let body = request(&mut stream).await;
            observed.send(body).unwrap();
            if index == 0 {
                let permit = server_gate.acquire().await.unwrap();
                permit.forget();
            }
            let body = format!(
                "data: {{\"choices\":[{{\"delta\":{{\"content\":\"Delivery is Tuesday ({index}).\"}},\"finish_reason\":null}}]}}\n\ndata: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}]}}\n\ndata: [DONE]\n\n"
            );
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
    });
    let ask_engine = engine.clone();
    let answer = tokio::spawn(async move {
        ask_engine
            .ask_saved_session("selected", "When is delivery?".into())
            .await
    });
    let payload = tokio::time::timeout(Duration::from_secs(3), requests.recv())
        .await
        .unwrap()
        .unwrap();
    let payload_text = payload.to_string();
    assert!(payload_text.contains("Selected planning session"));
    assert!(payload_text.contains("SELECTED-EVIDENCE"));
    assert!(payload_text.contains("SELECTED-NOTES"));
    for excluded in [
        "OTHER-LIVE-PRIVATE",
        "PRIVATE-GENERAL-INSTRUCTIONS",
        "PRIVATE-MODE-INSTRUCTIONS",
        "image_url",
        "web_search",
    ] {
        assert!(
            !payload_text.contains(excluded),
            "Saved evidence leaked {excluded}"
        );
    }
    assert_eq!(payload["messages"].as_array().unwrap().len(), 2);
    assert!(
        engine
            .ask_saved_session("selected", "Overlapping question".into())
            .await
            .unwrap_err()
            .contains("already in progress")
    );
    gate.add_permits(1);
    let saved = tokio::time::timeout(Duration::from_secs(3), answer)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(saved.id, "selected");
    assert_eq!(saved.messages.len(), 2);
    assert_eq!(saved.messages[0].content, "When is delivery?");
    assert_eq!(saved.messages[1].content, "Delivery is Tuesday (0).");
    assert_eq!(
        serde_json::to_value(engine.snapshot().unwrap()).unwrap(),
        original_live
    );
    assert_eq!(engine.saved_session("other").unwrap().messages.len(), 1);
    assert_eq!(engine.saved_session("selected").unwrap().messages.len(), 2);

    engine.active.store(false, Ordering::SeqCst);
    lock(&engine.history).unwrap().finish("other").unwrap();
    engine.resume_session("selected").unwrap();
    let saved = engine
        .ask_saved_session("selected", "Confirm the date".into())
        .await
        .unwrap();
    assert_eq!(saved.messages.len(), 4);
    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.session_id.as_deref(), Some("selected"));
    assert_eq!(snapshot.messages.len(), 4);
    assert_eq!(snapshot.messages[3].content, "Delivery is Tuesday (1).");
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn concurrent_preference_updates_preserve_each_field_and_commit_event_order() {
    let fixture = Fixture::new("http://127.0.0.1:43109/v1");
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    let mut events = engine.subscribe();
    let barrier = Arc::new(std::sync::Barrier::new(4));
    std::thread::scope(|scope| {
        for worker in 0..4 {
            let engine = engine.clone();
            let barrier = barrier.clone();
            scope.spawn(move || {
                barrier.wait();
                for sequence in 0..8 {
                    engine
                        .update_preferences(|settings| {
                            match worker {
                                0 => settings.smart_mode = true,
                                1 => settings.hide_widget = true,
                                2 => settings.theme = "dark".into(),
                                3 => settings.output_language = "fr-FR".into(),
                                _ => unreachable!(),
                            }
                            settings
                                .system_prompt
                                .push_str(&format!("\npreference-worker-{worker}-{sequence}"));
                            Ok(())
                        })
                        .unwrap();
                }
            });
        }
    });
    let committed = engine.config().unwrap().settings;
    assert!(committed.smart_mode && committed.hide_widget);
    assert_eq!(committed.theme, "dark");
    assert_eq!(committed.output_language, "fr-FR");
    for worker in 0..4 {
        for sequence in 0..8 {
            assert_eq!(
                committed
                    .system_prompt
                    .matches(&format!("\npreference-worker-{worker}-{sequence}"))
                    .count(),
                1
            );
        }
    }
    let mut committed_events = 0;
    while let Ok(event) = events.try_recv() {
        if let EngineEvent::ConfigChanged(view) = event {
            committed_events += 1;
            assert_eq!(
                view.settings
                    .system_prompt
                    .matches("\npreference-worker-")
                    .count(),
                committed_events,
                "Subscribers must observe preference commits in transaction order"
            );
        }
    }
    assert_eq!(committed_events, 32);
    assert_eq!(
        serde_json::to_value(config::load(&fixture.0.join("settings.json")).unwrap()).unwrap(),
        serde_json::to_value(committed).unwrap()
    );
}

#[tokio::test]
async fn sequential_preference_toggles_use_the_last_committed_value() {
    let fixture = Fixture::new("http://127.0.0.1:43109/v1");
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    let original = engine.config().unwrap().settings.smart_mode;
    let first = engine
        .update_preferences(|settings| {
            settings.smart_mode = !settings.smart_mode;
            Ok(())
        })
        .unwrap();
    let second = engine
        .update_preferences(|settings| {
            settings.smart_mode = !settings.smart_mode;
            Ok(())
        })
        .unwrap();
    assert_eq!(first.settings.smart_mode, !original);
    assert_eq!(second.settings.smart_mode, original);
    assert_eq!(
        config::load(&fixture.0.join("settings.json"))
            .unwrap()
            .smart_mode,
        original
    );
}

#[tokio::test]
async fn failed_preference_updates_preserve_memory_disk_and_event_stream() {
    let fixture = Fixture::new("http://127.0.0.1:43109/v1");
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    let before = serde_json::to_value(engine.config().unwrap().settings).unwrap();
    let bytes = std::fs::read(fixture.0.join("settings.json")).unwrap();
    let mut events = engine.subscribe();
    assert_eq!(
        engine
            .update_preferences(|settings| {
                settings.hide_widget = !settings.hide_widget;
                Err("Invalid preference patch".into())
            })
            .err()
            .unwrap(),
        "Invalid preference patch"
    );
    assert_eq!(
        serde_json::to_value(engine.config().unwrap().settings).unwrap(),
        before
    );
    assert_eq!(
        std::fs::read(fixture.0.join("settings.json")).unwrap(),
        bytes
    );
    assert!(matches!(
        events.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));
    assert!(
        engine
            .update_preferences(|settings| {
                settings.hide_widget = !settings.hide_widget;
                settings.theme = "invalid-theme".into();
                Ok(())
            })
            .err()
            .unwrap()
            .contains("appearance")
    );
    assert_eq!(
        serde_json::to_value(engine.config().unwrap().settings).unwrap(),
        before
    );
    assert_eq!(
        std::fs::read(fixture.0.join("settings.json")).unwrap(),
        bytes
    );
    assert!(matches!(
        events.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    std::fs::create_dir(fixture.0.join("settings.json.tmp")).unwrap();
    assert!(
        engine
            .update_preferences(|settings| {
                settings.hide_widget = !settings.hide_widget;
                Ok(())
            })
            .err()
            .unwrap()
            .contains("Write settings")
    );
    assert_eq!(
        serde_json::to_value(engine.config().unwrap().settings).unwrap(),
        before
    );
    assert_eq!(
        std::fs::read(fixture.0.join("settings.json")).unwrap(),
        bytes
    );
    assert!(matches!(
        events.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn provider_field_updates_keep_newer_preferences_and_failed_commits_unchanged() {
    let fixture = Fixture::new("http://127.0.0.1:43109/v1");
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    let draft = engine.config().unwrap().settings;
    engine
        .update_preferences(|settings| {
            settings.smart_mode = !draft.smart_mode;
            settings.hide_widget = !draft.hide_widget;
            settings.theme = "dark".into();
            Ok(())
        })
        .unwrap();

    let mut events = engine.subscribe();
    let saved = engine
        .update_configuration(
            |settings| {
                assert_eq!(settings.smart_mode, !draft.smart_mode);
                assert_eq!(settings.hide_widget, !draft.hide_widget);
                settings.chat_model = "review-model".into();
                settings.transcription_model = "review-transcription".into();
                settings.system_prompt = "Provider form instructions".into();
                Ok(())
            },
            None,
        )
        .unwrap();
    assert_eq!(saved.settings.smart_mode, !draft.smart_mode);
    assert_eq!(saved.settings.hide_widget, !draft.hide_widget);
    assert_eq!(saved.settings.theme, "dark");
    assert_eq!(saved.settings.chat_model, "review-model");
    assert_eq!(saved.settings.transcription_model, "review-transcription");
    assert_eq!(saved.settings.system_prompt, "Provider form instructions");
    assert!(matches!(
        events.try_recv().unwrap(),
        EngineEvent::ConfigChanged(_)
    ));

    let committed = serde_json::to_value(&saved.settings).unwrap();
    let bytes = std::fs::read(fixture.0.join("settings.json")).unwrap();
    for rejection in ["callback", "validation", "disk"] {
        if rejection == "disk" {
            std::fs::create_dir(fixture.0.join("settings.json.tmp")).unwrap();
        }
        let error = engine
            .update_configuration(
                |settings| {
                    settings.chat_model = "uncommitted-model".into();
                    match rejection {
                        "callback" => return Err("Invalid provider draft".into()),
                        "validation" => settings.theme = "invalid-theme".into(),
                        "disk" => {}
                        _ => unreachable!(),
                    }
                    Ok(())
                },
                // Callback and validation failures must happen before credential writes.
                (rejection != "disk").then(|| "unused-key".into()),
            )
            .err()
            .unwrap();
        assert!(error.contains(match rejection {
            "callback" => "Invalid provider draft",
            "validation" => "appearance",
            "disk" => "Write settings",
            _ => unreachable!(),
        }));
        let current = engine.config().unwrap();
        assert!(!current.has_api_key);
        assert_eq!(serde_json::to_value(current.settings).unwrap(), committed);
        assert_eq!(
            std::fs::read(fixture.0.join("settings.json")).unwrap(),
            bytes
        );
        assert!(matches!(
            events.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
    }
}

#[tokio::test]
async fn provider_transitions_wait_for_recording_paid_requests_and_pending_transcription() {
    let fixture = Fixture::new("http://127.0.0.1:43109/v1");
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    for busy in [
        "capture",
        "stopping",
        "assistant",
        "saved-question",
        "notes",
        "transcription",
    ] {
        let mut job = None;
        let mut permit = None;
        match busy {
            "capture" => engine.active.store(true, Ordering::SeqCst),
            "stopping" => engine.stopping.store(true, Ordering::SeqCst),
            "assistant" => *lock(&engine.assistant).unwrap() = Some(CancellationToken::new()),
            "saved-question" => permit = Some(engine.saved_questions.try_acquire().unwrap()),
            "notes" => permit = Some(engine.note_jobs.try_acquire().unwrap()),
            "transcription" => job = Some(lock(&engine.jobs).unwrap().submit()),
            _ => unreachable!(),
        }
        for field in ["mode", "url", "chat-model", "speech-model", "key"] {
            let before = std::fs::read(fixture.0.join("settings.json")).unwrap();
            let error = engine
                .update_configuration(
                    |settings| {
                        match field {
                            "mode" => settings.provider_mode = ProviderMode::Palepale,
                            "url" => settings.api_base_url = "http://127.0.0.1:43110/v1".into(),
                            "chat-model" => settings.chat_model = "different-model".into(),
                            "speech-model" => {
                                settings.transcription_model = "different-speech-model".into()
                            }
                            "key" => {}
                            _ => unreachable!(),
                        }
                        Ok(())
                    },
                    (field == "key").then(|| "never-saved-key".into()),
                )
                .err()
                .unwrap();
            assert!(
                error.contains("Stop recording and wait"),
                "{busy}/{field}: {error}"
            );
            assert_eq!(
                std::fs::read(fixture.0.join("settings.json")).unwrap(),
                before
            );
        }
        engine
            .update_preferences(|settings| {
                settings.hide_widget = !settings.hide_widget;
                Ok(())
            })
            .unwrap();
        if let Some(job) = job {
            lock(&engine.jobs).unwrap().complete(job);
        }
        drop(permit);
        engine.active.store(false, Ordering::SeqCst);
        engine.stopping.store(false, Ordering::SeqCst);
        *lock(&engine.assistant).unwrap() = None;
    }
    engine
        .update_preferences(|settings| {
            settings.chat_model = "permitted-when-idle".into();
            Ok(())
        })
        .unwrap();
    assert_eq!(
        engine.config().unwrap().settings.chat_model,
        "permitted-when-idle"
    );
}

#[tokio::test]
async fn in_flight_notes_keep_provider_configuration_stable_until_the_response_finishes() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let fixture = Fixture::new(&format!("http://{}/v1", listener.local_addr().unwrap()));
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    lock(&engine.history)
        .unwrap()
        .append_transcript(
            "notes-lock",
            &engine.config().unwrap().settings,
            false,
            &TranscriptEntry {
                id: "evidence".into(),
                source: "microphone".into(),
                text: "We agreed to review the changes on Friday.".into(),
                created_at: 100,
            },
        )
        .unwrap();
    let gate = Arc::new(Semaphore::new(0));
    let server_gate = gate.clone();
    let (seen, observed) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _ = request(&mut stream).await;
        seen.send(()).unwrap();
        let _permit = server_gate.acquire().await.unwrap();
        let body = serde_json::json!({"choices":[{"message":{"content":"{\"title\":\"Review\",\"summary\":\"Review the changes on Friday.\"}"},"finish_reason":"stop"}]}).to_string();
        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    });
    let notes_engine = engine.clone();
    let notes = tokio::spawn(async move { notes_engine.generate_notes("notes-lock").await });
    tokio::time::timeout(Duration::from_secs(3), observed)
        .await
        .unwrap()
        .unwrap();
    let error = engine
        .update_preferences(|settings| {
            settings.chat_model = "other-model".into();
            Ok(())
        })
        .err()
        .unwrap();
    assert!(error.contains("Stop recording and wait"));
    engine
        .update_preferences(|settings| {
            settings.hide_widget = true;
            Ok(())
        })
        .unwrap();
    assert!(engine.generate_notes("notes-lock").await.is_err());
    gate.add_permits(1);
    let record = tokio::time::timeout(Duration::from_secs(3), notes)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(record.title, "Review");
    assert!(engine.config().unwrap().settings.hide_widget);
    engine
        .update_preferences(|settings| {
            settings.chat_model = "other-model".into();
            Ok(())
        })
        .unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn hosted_switches_preserve_byok_drafts_and_allow_inactive_draft_edits_during_answers() {
    let fixture = Fixture::new("http://127.0.0.1:43109/v1");
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    let byok = engine.config().unwrap().settings;
    engine
        .update_preferences(|settings| {
            settings.provider_mode = ProviderMode::Palepale;
            settings.smart_mode = true;
            settings.include_screenshot = true;
            Ok(())
        })
        .unwrap();
    assert!(
        engine
            .provider_configuration()
            .err()
            .unwrap()
            .contains("refresh the connection")
    );
    engine
        .update_preferences(|settings| {
            settings.palepale.chat_model = "hosted-model".into();
            Ok(())
        })
        .unwrap();
    lock(&engine.configuration).unwrap().key = "test-only-hosted-key".into();
    assert!(
        engine
            .provider_configuration()
            .unwrap_err()
            .contains("selected Smart effort")
    );
    lock(&engine.published_models)
        .unwrap()
        .push(skillissue_domain::provider::ProviderModel {
            id: "hosted-model".into(),
            capabilities: skillissue_domain::provider::ModelCapabilities {
                reasoning: true,
                ..Default::default()
            },
            reasoning_efforts: vec!["high".into()],
            ..Default::default()
        });
    let (effective, _) = engine.provider_configuration().unwrap();
    assert!(effective.smart_mode);
    assert!(!effective.include_screenshot);
    let persisted = engine.config().unwrap().settings;
    assert!(persisted.smart_mode);
    assert!(persisted.include_screenshot);
    *lock(&engine.assistant).unwrap() = Some(CancellationToken::new());
    engine
        .update_preferences(|settings| {
            settings.api_base_url = "http://127.0.0.1:43110/v1".into();
            settings.chat_model = "byok-draft-model".into();
            settings.transcription_model = "byok-draft-speech".into();
            Ok(())
        })
        .unwrap();
    assert!(
        engine
            .update_preferences(|settings| {
                settings.palepale.chat_model = "other-hosted-model".into();
                Ok(())
            })
            .is_err()
    );
    *lock(&engine.assistant).unwrap() = None;
    engine
        .update_preferences(|settings| {
            settings.provider_mode = ProviderMode::Byok;
            Ok(())
        })
        .unwrap();
    let restored = engine.config().unwrap().settings;
    assert_eq!(restored.provider_base_url(), "http://127.0.0.1:43110/v1");
    assert_eq!(restored.provider_chat_model(), "byok-draft-model");
    assert_eq!(restored.transcription_model, "byok-draft-speech");
    assert_eq!(restored.palepale.chat_model, "hosted-model");
    assert!(restored.smart_mode);
    assert!(restored.include_screenshot);
    assert_eq!(restored.language, byok.language);
}

#[tokio::test]
async fn live_settings_preserve_audio_fields_but_allow_existing_overlay_preferences() {
    let fixture = Fixture::new("http://127.0.0.1:43109/v1");
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    engine.active.store(true, Ordering::SeqCst);
    *lock(&engine.assistant).unwrap() = Some(CancellationToken::new());
    let mut settings = engine.config().unwrap().settings;
    settings.capture_protection = !settings.capture_protection;
    settings.no_focus = !settings.no_focus;
    settings.smart_mode = !settings.smart_mode;
    settings.hide_widget = !settings.hide_widget;
    settings.theme = "light".into();
    engine.save_settings(settings.clone(), None).unwrap();
    assert_eq!(engine.config().unwrap().settings.theme, "light");
    let bytes = std::fs::read(fixture.0.join("settings.json")).unwrap();
    for audio in ["microphone", "system", "duration", "language"] {
        let change_audio = |settings: &mut Settings| -> Result<(), String> {
            match audio {
                "microphone" => settings.microphone = !settings.microphone,
                "system" => settings.system_audio = !settings.system_audio,
                "duration" => {
                    settings.audio_chunk_seconds = if settings.audio_chunk_seconds == 8 {
                        9
                    } else {
                        8
                    }
                }
                "language" => {
                    settings.language = if settings.language == "en" {
                        "zh".into()
                    } else {
                        "en".into()
                    }
                }
                _ => unreachable!(),
            }
            Ok(())
        };
        let mut changed = settings.clone();
        change_audio(&mut changed).unwrap();
        assert!(
            engine
                .save_settings(changed, None)
                .err()
                .unwrap()
                .contains("Stop the session")
        );
        assert!(
            engine
                .update_preferences(change_audio)
                .err()
                .unwrap()
                .contains("Stop the session")
        );
        assert_eq!(
            std::fs::read(fixture.0.join("settings.json")).unwrap(),
            bytes
        );
        assert_eq!(
            serde_json::to_value(engine.config().unwrap().settings).unwrap(),
            serde_json::to_value(&settings).unwrap()
        );
    }
    engine.active.store(false, Ordering::SeqCst);
    *lock(&engine.assistant).unwrap() = None;
}

#[tokio::test]
async fn hosted_screen_preferences_require_exact_cached_published_model() {
    use skillissue_domain::provider::{ModelCapabilities, ProviderModel};
    let fixture = Fixture::new("http://127.0.0.1:43112/v1");
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    engine
        .update_preferences(|settings| {
            settings.provider_mode = ProviderMode::Palepale;
            settings.palepale.chat_model = "vision-model".into();
            settings.include_screenshot = true;
            Ok(())
        })
        .unwrap();
    lock(&engine.configuration).unwrap().key = "test-only-hosted-key".into();
    assert!(
        !engine
            .provider_configuration()
            .unwrap()
            .0
            .include_screenshot
    );
    *lock(&engine.published_models).unwrap() = vec![ProviderModel {
        id: "different-model".into(),
        capabilities: ModelCapabilities {
            streaming: true,
            image_input: true,
            ..Default::default()
        },
        ..Default::default()
    }];
    assert!(
        !engine
            .provider_configuration()
            .unwrap()
            .0
            .include_screenshot
    );
    lock(&engine.published_models).unwrap()[0].id = "vision-model".into();
    assert!(
        engine
            .provider_configuration()
            .unwrap()
            .0
            .include_screenshot
    );
    assert!(engine.config().unwrap().settings.include_screenshot);
    lock(&engine.published_models).unwrap().clear();
    assert!(
        !engine
            .provider_configuration()
            .unwrap()
            .0
            .include_screenshot
    );
    engine.shutdown().await.unwrap();
}

#[derive(Default)]
struct ByokTestCredentials(std::sync::Mutex<std::collections::HashMap<String, String>>);
impl credentials::Credentials for ByokTestCredentials {
    fn load_saved(&self, scope: &str) -> Result<Option<String>, String> {
        Ok(self.0.lock().unwrap().get(scope).cloned())
    }
    fn save(&self, scope: &str, key: &str) -> Result<(), String> {
        let mut keys = self.0.lock().unwrap();
        if key.is_empty() {
            keys.remove(scope);
        } else {
            keys.insert(scope.into(), key.into());
        }
        Ok(())
    }
    fn load_provider(&self, settings: &Settings) -> Result<String, String> {
        self.load_saved(&config::provider_credential_id(settings))
            .map(Option::unwrap_or_default)
    }
}

#[tokio::test]
async fn mixed_hosted_roles_share_account_and_clear_on_disconnect() {
    let fixture = Fixture::new("http://127.0.0.1:43112/v1");
    let engine = NativeEngine::open_with_credentials(
        fixture.0.clone(),
        Arc::new(ByokTestCredentials::default()),
    )
    .unwrap();
    engine
        .update_configuration(
            |s| {
                s.provider_mode = ProviderMode::Palepale;
                s.transcription_base_url = skillissue_domain::provider::PALEPALE_BASE_URL.into();
                s.image_base_url = skillissue_domain::provider::PALEPALE_BASE_URL.into();
                s.image_model = "hosted-image".into();
                s.transcription_model = "hosted-audio".into();
                Ok(())
            },
            Some("account-test-key".into()),
        )
        .unwrap();
    engine
        .update_configuration(
            |s| {
                s.provider_mode = ProviderMode::Byok;
                Ok(())
            },
            Some("text-test-key".into()),
        )
        .unwrap();
    let audio = lock(&engine.configuration)
        .unwrap()
        .transcription_configuration()
        .unwrap();
    assert!(audio.0.is_palepale());
    assert_eq!(audio.0.provider_transcription_model(), "hosted-audio");
    assert_eq!(audio.1, "account-test-key");
    let image = engine.image_configuration().unwrap();
    assert!(image.0.is_palepale());
    assert_eq!(image.0.palepale.chat_model, "hosted-image");
    assert_eq!(image.1, "account-test-key");
    assert_eq!(engine.provider_configuration().unwrap().1, "text-test-key");
    assert!(
        engine
            .update_byok_configuration(|_| Ok(()), None, Some("overwrite".into()))
            .is_err()
    );
    engine
        .update_configuration(
            |s| {
                s.provider_mode = ProviderMode::Palepale;
                Ok(())
            },
            Some(String::new()),
        )
        .unwrap();
    engine
        .update_configuration(
            |s| {
                s.provider_mode = ProviderMode::Byok;
                Ok(())
            },
            None,
        )
        .unwrap();
    assert!(engine.image_configuration().is_err());
    assert!(
        lock(&engine.configuration)
            .unwrap()
            .transcription_configuration()
            .is_err()
    );
    assert_eq!(engine.provider_configuration().unwrap().1, "text-test-key");
}

#[tokio::test]
async fn byok_audio_credentials_are_isolated_atomic_and_survive_restart() {
    let fixture = Fixture::new("http://127.0.0.1:43112/v1");
    let credentials = Arc::new(ByokTestCredentials::default());
    let engine =
        NativeEngine::open_with_credentials(fixture.0.clone(), credentials.clone()).unwrap();
    let configure = |s: &mut Settings| {
        s.transcription_base_url = "https://audio.example/v1".into();
        Ok(())
    };
    engine
        .update_byok_configuration(
            configure,
            Some("chat-secret".into()),
            Some("audio-secret".into()),
        )
        .unwrap();
    let audio = lock(&engine.configuration)
        .unwrap()
        .transcription_configuration()
        .unwrap();
    assert_eq!(audio.0.api_base_url, "https://audio.example/v1");
    assert_eq!(audio.1, "audio-secret");
    assert_eq!(engine.provider_configuration().unwrap().1, "chat-secret");
    let json = std::fs::read_to_string(fixture.0.join("settings.json")).unwrap();
    assert!(!json.contains("secret"));
    engine.active.store(true, Ordering::SeqCst);
    assert!(
        engine
            .update_byok_configuration(
                |s| {
                    s.transcription_base_url = "https://different.example/v1".into();
                    Ok(())
                },
                None,
                None
            )
            .is_err()
    );
    engine.active.store(false, Ordering::SeqCst);
    let job = lock(&engine.jobs).unwrap().submit();
    assert!(
        engine
            .update_byok_configuration(|_| Ok(()), None, Some("replacement".into()))
            .is_err()
    );
    lock(&engine.jobs).unwrap().complete(job);
    // A rejected settings publication must roll back both credential writes.
    let settings_path = fixture.0.join("settings.json");
    let saved_path = fixture.0.join("settings.before.json");
    std::fs::rename(&settings_path, &saved_path).unwrap();
    std::fs::create_dir(&settings_path).unwrap();
    assert!(
        engine
            .update_byok_configuration(
                |_| Ok(()),
                Some("new-chat".into()),
                Some("new-audio".into())
            )
            .is_err()
    );
    std::fs::remove_dir(&settings_path).unwrap();
    std::fs::rename(saved_path, settings_path).unwrap();
    assert_eq!(
        credentials
            .0
            .lock()
            .unwrap()
            .get("transcription:https://audio.example/v1")
            .unwrap(),
        "audio-secret"
    );
    assert_eq!(engine.provider_configuration().unwrap().1, "chat-secret");
    engine.shutdown().await.unwrap();
    drop(engine);
    let engine =
        NativeEngine::open_with_credentials(fixture.0.clone(), credentials.clone()).unwrap();
    assert_eq!(
        lock(&engine.configuration)
            .unwrap()
            .transcription_configuration()
            .unwrap()
            .1,
        "audio-secret"
    );
    // URL changes must not reuse either chat or prior audio credentials.
    engine
        .update_byok_configuration(
            |s| {
                s.transcription_base_url = "https://different.example/v1".into();
                Ok(())
            },
            None,
            None,
        )
        .unwrap();
    assert!(
        lock(&engine.configuration)
            .unwrap()
            .transcription_configuration()
            .is_err()
    );
    // Previously accepted snapshots retain the original destination and key.
    assert_eq!(audio.1, "audio-secret");
    engine
        .update_byok_configuration(configure, None, Some(String::new()))
        .unwrap();
    assert!(
        lock(&engine.configuration)
            .unwrap()
            .transcription_configuration()
            .is_err()
    );
    engine
        .update_byok_configuration(
            |s| {
                s.transcription_base_url.clear();
                Ok(())
            },
            None,
            None,
        )
        .unwrap();
    assert_eq!(
        lock(&engine.configuration)
            .unwrap()
            .transcription_configuration()
            .unwrap()
            .1,
        "chat-secret"
    );
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn byok_audio_override_routes_real_upload_without_chat_credentials() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let fixture = Fixture::new("http://127.0.0.1:1/v1");
    let engine = NativeEngine::open_with_credentials(
        fixture.0.clone(),
        Arc::new(ByokTestCredentials::default()),
    )
    .unwrap();
    engine
        .update_byok_configuration(
            |s| {
                s.transcription_base_url = base;
                s.transcription_model = "audio-model".into();
                Ok(())
            },
            Some("chat-only-secret".into()),
            Some("audio-only-secret".into()),
        )
        .unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        // This fixture deliberately uses two synthetic credentials to verify routing.
        let mut bytes = Vec::new();
        let (headers, body) = loop {
            let mut chunk = [0u8; 4096];
            let count = socket.read(&mut chunk).await.unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&chunk[..count]);
            if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
                let length: usize = headers
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                    .unwrap()
                    .1
                    .trim()
                    .parse()
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break (headers, bytes[end + 4..].to_vec());
                }
            }
        };
        assert!(headers.starts_with("POST /v1/audio/transcriptions "));
        assert!(
            headers
                .to_lowercase()
                .contains("authorization: bearer audio-only-secret")
        );
        assert!(!headers.contains("chat-only-secret"));
        assert!(String::from_utf8_lossy(&body).contains("audio-model"));
        assert!(body.windows(4).any(|part| part == b"RIFF"));
        let body = r#"{"text":"Independent audio works"}"#;
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    });
    engine.queue_transcription(AudioSegment {
        source: "microphone".into(),
        sample_rate: 8_000,
        samples: vec![0.01; 80],
    });
    let watermark = lock(&engine.jobs).unwrap().watermark();
    assert_eq!(
        engine
            .wait_for_transcript(watermark, Duration::from_secs(5))
            .await
            .unwrap()
            .pending,
        0
    );
    assert_eq!(
        engine.snapshot().unwrap().transcript[0].text,
        "Independent audio works"
    );
    server.await.unwrap();
    engine
        .update_preferences(|s| {
            s.byok_audio_enabled = false;
            Ok(())
        })
        .unwrap();
    assert!(
        engine
            .start_capture()
            .await
            .unwrap_err()
            .contains("Audio is disabled")
    );
    assert!(engine.provider_configuration().is_ok());
    assert!(!engine.snapshot().unwrap().active);
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn saved_connection_can_wait_for_model_selection_without_allowing_inference() {
    let fixture = Fixture::new("http://127.0.0.1:43112/v1");
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    engine
        .update_preferences(|s| {
            s.chat_model.clear();
            Ok(())
        })
        .unwrap();
    assert!(engine.config().unwrap().settings.chat_model.is_empty());
    assert!(
        engine
            .provider_configuration()
            .unwrap_err()
            .contains("Choose a chat model")
    );
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn image_connections_isolate_keys_roll_back_and_survive_restart() {
    let fixture = Fixture::new("http://127.0.0.1:43112/v1");
    let credentials = Arc::new(ByokTestCredentials::default());
    let engine =
        NativeEngine::open_with_credentials(fixture.0.clone(), credentials.clone()).unwrap();
    engine
        .update_media_configuration(
            |s| {
                s.transcription_base_url = "https://audio.example/v1".into();
                s.image_base_url = "https://image.example/v1".into();
                s.image_model = "vision".into();
                s.smart_mode = true;
                s.smart_effort = "low".into();
                Ok(())
            },
            Some("text-secret".into()),
            Some("audio-secret".into()),
            Some("image-secret".into()),
        )
        .unwrap();
    let (image, key) = engine.image_configuration().unwrap();
    assert_eq!(image.api_base_url, "https://image.example/v1");
    assert_eq!(image.chat_model, "vision");
    assert_eq!(key, "image-secret");
    assert!(image.smart_mode);
    assert_eq!(image.smart_effort, "low");
    assert_eq!(engine.provider_configuration().unwrap().1, "text-secret");
    let prepared = engine
        .prepare_assist_target(&engine.config().unwrap().settings)
        .unwrap();
    engine
        .update_preferences(|s| {
            s.image_model = "vision-two".into();
            Ok(())
        })
        .unwrap();
    assert!(
        engine
            .assist_for_target("stale".into(), None, CancellationToken::new(), prepared)
            .await
            .unwrap_err()
            .contains("changed")
    );
    engine.active.store(true, Ordering::SeqCst);
    assert!(
        engine
            .update_media_configuration(|_| Ok(()), None, None, Some("new".into()))
            .is_err()
    );
    engine.active.store(false, Ordering::SeqCst);
    let path = fixture.0.join("settings.json");
    let before = fixture.0.join("settings.before.json");
    std::fs::rename(&path, &before).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(
        engine
            .update_media_configuration(
                |_| Ok(()),
                Some("new-text".into()),
                Some("new-audio".into()),
                Some("new-image".into())
            )
            .is_err()
    );
    std::fs::remove_dir(&path).unwrap();
    std::fs::rename(before, &path).unwrap();
    assert_eq!(
        credentials
            .0
            .lock()
            .unwrap()
            .get("image:https://image.example/v1")
            .unwrap(),
        "image-secret"
    );
    assert_eq!(
        credentials
            .0
            .lock()
            .unwrap()
            .get("transcription:https://audio.example/v1")
            .unwrap(),
        "audio-secret"
    );
    engine.shutdown().await.unwrap();
    drop(engine);
    let engine = NativeEngine::open_with_credentials(fixture.0.clone(), credentials).unwrap();
    assert_eq!(engine.image_configuration().unwrap().1, "image-secret");
    engine
        .update_preferences(|s| {
            s.image_base_url = "https://other.example/v1".into();
            Ok(())
        })
        .unwrap();
    assert!(
        engine
            .image_configuration()
            .unwrap_err()
            .contains("Images API key")
    );
    assert!(!std::fs::read_to_string(&path).unwrap().contains("secret"));
    engine
        .update_preferences(|s| {
            s.image_base_url.clear();
            Ok(())
        })
        .unwrap();
    assert_eq!(engine.image_configuration().unwrap().1, "text-secret");
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn image_model_discovery_does_not_require_a_selected_model() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let fixture = Fixture::new("http://127.0.0.1:43112/v1");
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    engine
        .update_preferences(|s| {
            s.image_base_url = format!("http://{}/v1", listener.local_addr().unwrap());
            s.image_model.clear();
            Ok(())
        })
        .unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buffer = [0; 4096];
        let count = stream.read(&mut buffer).await.unwrap();
        assert!(String::from_utf8_lossy(&buffer[..count]).starts_with("GET /v1/models "));
        let body = r#"{"data":[{"id":"vision-fixture","input_modalities":["text","image"],"output_modalities":["text"]}]}"#;
        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    });
    let models = tokio::time::timeout(
        Duration::from_secs(5),
        engine.byok_image_models(CancellationToken::new()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(models, vec!["vision-fixture"]);
    server.await.unwrap();
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn screenshot_answers_use_image_endpoint_and_text_answers_use_text_endpoint() {
    let text_server = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let image_server = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let fixture = Fixture::new(&format!("http://{}/v1", text_server.local_addr().unwrap()));
    let engine = NativeEngine::open(fixture.0.clone()).unwrap();
    engine
        .update_preferences(|s| {
            s.image_base_url = format!("http://{}/v1", image_server.local_addr().unwrap());
            s.image_model = "vision-fixture".into();
            Ok(())
        })
        .unwrap();
    let server = tokio::spawn(async move {
        for (listener, image) in [(image_server, true), (text_server, false)] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let body = request(&mut stream).await;
            assert_eq!(body["model"] == "vision-fixture", image);
            let content =
                body["messages"].as_array().unwrap().last().unwrap()["content"].to_string();
            assert_eq!(content.contains("image_url"), image);
            let body = "data: {\"choices\":[{\"delta\":{\"content\":\"Answer\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
        }
    });
    tokio::time::timeout(
        Duration::from_secs(5),
        engine.assist(
            "Image question".into(),
            Some("data:image/png;base64,aGVsbG8=".into()),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    tokio::time::timeout(
        Duration::from_secs(5),
        engine.assist("Text question".into(), None),
    )
    .await
    .unwrap()
    .unwrap();
    server.await.unwrap();
    engine.shutdown().await.unwrap();
}
