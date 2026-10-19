use super::{NativeView, Page, Reply, ShortcutAction, session::RequestDraft};
use gpui_kit::{
    AppContext, Context, Entity, TestAppContext, Window, WindowOptions, component::Theme,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

fn with_view(
    cx: &mut TestAppContext,
    test: impl FnOnce(&mut NativeView, &mut Window, &mut Context<NativeView>),
) {
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("inert request test runtime"),
    );
    let data_dir =
        std::env::temp_dir().join(format!("skillissue-request-{}", uuid::Uuid::new_v4()));
    let (handle, view): (_, Entity<NativeView>) = cx.update(|cx| {
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
                    let mut view = NativeView::new_for_test(runtime, data_dir.clone(), window, cx);
                    view.page = Page::Live;
                    view.gallery = false;
                    view.settings.include_screenshot = false;
                    view
                })
            },
        )
        .expect("mock request window")
    });
    cx.update_window(handle, |_, window, cx| {
        view.update(cx, |view, cx| test(view, window, cx));
    })
    .expect("run request state test");
    assert!(!data_dir.exists(), "request tests must not open a profile");
}

#[test]
fn draft_recovery_consumes_only_the_matching_request_once() {
    let mut recovery = RequestDraft::default();
    recovery.begin(2, "Current question".into());
    assert!(recovery.resolve(1, true, "").is_none());
    assert_eq!(
        recovery.resolve(2, true, "").as_deref(),
        Some("Current question")
    );
    assert!(recovery.resolve(2, true, "").is_none());
}

#[gpui_kit::test]
fn history_transfer_controls_recover_after_cancel_error_and_restore(cx: &mut TestAppContext) {
    with_view(cx, |view, window, cx| {
        for reply in [
            Reply::HistoryTransferCancelled,
            Reply::HistoryBackup(Err("Destination unavailable".into())),
            Reply::HistoryRestored(Err("History is occupied".into())),
            Reply::HistoryRestored(Ok(3)),
        ] {
            view.history_transfer_pending = true;
            view.sender.send(reply).unwrap();
            view.tick(window, cx);
            assert!(!view.history_transfer_pending);
        }
        assert_eq!(view.status, "Restored 3 saved sessions.");
    });
}

#[gpui_kit::test]
fn export_reveal_failure_preserves_saved_location_and_active_question(cx: &mut TestAppContext) {
    with_view(cx, |view, window, cx| {
        view.status = "Session exported to C:\\exports\\meeting.md".into();
        view.busy = true;
        view.stream = "Current answer".into();
        view.question
            .update(cx, |input, cx| input.set_value("Next question", window, cx));
        view.sender
            .send(Reply::ExportRevealed(Err("Explorer unavailable".into())))
            .unwrap();
        view.tick(window, cx);
        assert_eq!(view.status, "Session exported to C:\\exports\\meeting.md");
        assert_eq!(view.error.as_deref(), Some("Explorer unavailable"));
        assert!(view.busy);
        assert_eq!(view.stream, "Current answer");
        assert_eq!(view.question.read(cx).value().as_str(), "Next question");
    });
}

#[gpui_kit::test]
fn assistant_events_keep_the_broadcast_prompt_and_reject_other_requests(cx: &mut TestAppContext) {
    // Both the submitting window and a passive window receive the same events.
    for previous_prompt in [None, Some("An older question in this window")] {
        with_view(cx, |view, window, cx| {
            view.last_submitted_prompt = previous_prompt.map(str::to_owned);
            let mut emit = |id: &str, kind: &str, text: &str| {
                view.on_event(
                    skillissue_core::EngineEvent::Assistant {
                        id: id.into(),
                        kind: kind.into(),
                        text: text.into(),
                    },
                    window,
                    cx,
                );
            };
            emit("current", "start", "Question submitted in the other window");
            emit("current", "delta", "Current answer");
            emit("obsolete", "delta", "Wrong answer");
            emit("obsolete", "context", "Wrong warning");
            emit("obsolete", "done", "");
            assert!(view.busy);
            assert_eq!(view.stream, "Current answer");
            assert!(view.overlay_ui.audio_notice.is_none());
            view.on_event(
                skillissue_core::EngineEvent::Assistant {
                    id: "current".into(),
                    kind: "context".into(),
                    text: "Audio pending".into(),
                },
                window,
                cx,
            );
            assert_eq!(
                view.audio_retry_prompt.as_deref(),
                Some("Question submitted in the other window")
            );
            view.on_event(
                skillissue_core::EngineEvent::Assistant {
                    id: "current".into(),
                    kind: "done".into(),
                    text: String::new(),
                },
                window,
                cx,
            );
            assert!(!view.busy);
            view.on_event(
                skillissue_core::EngineEvent::Assistant {
                    id: "current".into(),
                    kind: "delta".into(),
                    text: "After completion".into(),
                },
                window,
                cx,
            );
            assert!(view.stream.is_empty());
        });
    }
}

#[gpui_kit::test]
fn failed_and_cancelled_requests_restore_the_question_into_empty_composer(cx: &mut TestAppContext) {
    with_view(cx, |view, window, cx| {
        for (generation, failed, cancelled) in [(1, true, false), (2, false, true)] {
            view.request_generation = generation;
            view.request_draft
                .begin(generation, "Keep this question".into());
            let cancel = CancellationToken::new();
            if cancelled {
                cancel.cancel();
            }
            view.request_cancel = Some(cancel);
            view.question
                .update(cx, |input, cx| input.set_value("", window, cx));
            view.sender
                .send(Reply::Assist(
                    generation,
                    if failed {
                        Err("Provider failed".into())
                    } else {
                        Ok(())
                    },
                ))
                .unwrap();
            view.tick(window, cx);
            assert_eq!(
                view.question.read(cx).value().as_ref(),
                "Keep this question"
            );
            assert!(view.request_cancel.is_none());
        }
    });
}

#[gpui_kit::test]
fn successful_response_does_not_restore_the_submitted_question(cx: &mut TestAppContext) {
    with_view(cx, |view, window, cx| {
        view.request_generation = 1;
        view.request_draft.begin(1, "Submitted".into());
        view.request_cancel = Some(CancellationToken::new());
        view.finish_request_draft(1, false, window, cx);
        assert!(view.question.read(cx).value().is_empty());
        assert!(view.request_cancel.is_none());
        assert!(view.request_draft.resolve(1, true, "").is_none());
    });
}

#[gpui_kit::test]
fn failure_preserves_a_new_composer_draft_even_when_it_contains_only_spaces(
    cx: &mut TestAppContext,
) {
    with_view(cx, |view, window, cx| {
        for draft in ["My next question", "   "] {
            view.request_generation = 1;
            view.request_draft.begin(1, "Failed question".into());
            view.question
                .update(cx, |input, cx| input.set_value(draft, window, cx));
            view.finish_request_draft(1, true, window, cx);
            assert_eq!(view.question.read(cx).value().as_ref(), draft);
        }
    });
}

#[gpui_kit::test]
fn stale_completion_cannot_consume_newer_question_or_cancellation_token(cx: &mut TestAppContext) {
    with_view(cx, |view, window, cx| {
        view.request_generation = 2;
        view.request_draft.begin(2, "New question".into());
        view.request_cancel = Some(CancellationToken::new());
        view.finish_request_draft(1, true, window, cx);
        assert!(view.question.read(cx).value().is_empty());
        assert!(view.request_cancel.is_some());
        view.finish_request_draft(2, true, window, cx);
        assert_eq!(view.question.read(cx).value().as_ref(), "New question");
    });
}

#[gpui_kit::test]
fn explicit_clear_prevents_cancelled_question_and_old_capture_from_returning(
    cx: &mut TestAppContext,
) {
    with_view(cx, |view, window, cx| {
        view.request_generation = 1;
        view.request_draft.begin(1, "Discard this question".into());
        let cancel = CancellationToken::new();
        view.request_cancel = Some(cancel.clone());
        view.capture_generation = 8;
        view.captured_screen = Some("old screenshot".into());
        view.question
            .update(cx, |input, cx| input.set_value("Unsent draft", window, cx));
        view.overlay_action(ShortcutAction::Clear, window, cx);
        assert!(cancel.is_cancelled());
        assert_eq!(view.capture_generation, 9);
        assert!(view.captured_screen.is_none());
        view.finish_request_draft(1, false, window, cx);
        assert!(view.question.read(cx).value().is_empty());
        view.sender
            .send(Reply::Captured(8, Ok("late screenshot".into())))
            .unwrap();
        view.tick(window, cx);
        assert!(view.captured_screen.is_none());
    });
}

#[gpui_kit::test]
fn quick_assist_keeps_existing_question_and_busy_presets_are_inert(cx: &mut TestAppContext) {
    with_view(cx, |view, window, cx| {
        view.question
            .update(cx, |input, cx| input.set_value("Keep my draft", window, cx));
        view.quick_assist("", window, cx);
        assert_eq!(view.question.read(cx).value().as_ref(), "Keep my draft");
        view.busy = true;
        let generation = view.request_generation;
        view.quick_assist("Replace my question", window, cx);
        view.quick_assist("", window, cx);
        assert_eq!(view.question.read(cx).value().as_ref(), "Keep my draft");
        assert_eq!(view.request_generation, generation);
    });
}

#[gpui_kit::test]
fn preview_generation_rejects_stale_success_and_error_and_accepts_current_reply(
    cx: &mut TestAppContext,
) {
    with_view(cx, |view, window, cx| {
        view.capture_generation = 2;
        view.sender
            .send(Reply::Captured(1, Ok("stale".into())))
            .unwrap();
        view.sender
            .send(Reply::Captured(1, Err("old capture failure".into())))
            .unwrap();
        view.tick(window, cx);
        assert!(view.captured_screen.is_none());
        assert!(view.error.is_none());
        view.sender
            .send(Reply::Captured(2, Ok("current".into())))
            .unwrap();
        view.tick(window, cx);
        assert_eq!(view.captured_screen.as_deref(), Some("current"));
        assert!(view.overlay_ui.transcript);
    });
}

#[gpui_kit::test]
fn audio_error_does_not_end_an_independently_streaming_answer(cx: &mut TestAppContext) {
    with_view(cx, |view, window, cx| {
        view.busy = true;
        view.stream = "Answer still streaming".into();
        view.request_cancel = Some(CancellationToken::new());
        view.on_event(
            skillissue_core::EngineEvent::Error("Microphone disconnected".into()),
            window,
            cx,
        );
        assert!(view.busy);
        assert_eq!(view.stream, "Answer still streaming");
        assert!(view.request_cancel.is_some());
        assert_eq!(view.error.as_deref(), Some("Microphone disconnected"));
    });
}

#[gpui_kit::test]
fn committed_provider_target_change_invalidates_pending_preview(cx: &mut TestAppContext) {
    with_view(cx, |view, window, cx| {
        view.settings.provider_mode = skillissue_domain::models::ProviderMode::Byok;
        view.capture_generation = 5;
        view.captured_screen = Some("old provider preview".into());
        let mut settings = view.settings.clone();
        settings.api_base_url = "https://another-provider.example/v1".into();
        view.set_config(
            skillissue_domain::models::ConfigView {
                settings,
                has_api_key: false,
                has_byok_key: false,
                has_audio_key: false,
                has_image_key: false,
                has_palepale_key: false,
                platform: "test".into(),
                shortcuts: vec![],
                warnings: vec![],
            },
            window,
            cx,
        );
        assert_ne!(view.capture_generation, 5);
        assert!(view.captured_screen.is_none());
        view.sender
            .send(Reply::Captured(5, Ok("late old provider preview".into())))
            .unwrap();
        view.tick(window, cx);
        assert!(view.captured_screen.is_none());
    });
}

#[gpui_kit::test]
fn changed_session_discards_preview_but_preserves_question_for_manual_editing(
    cx: &mut TestAppContext,
) {
    with_view(cx, |view, _, _| {
        view.snapshot.session_id = Some("old-session".into());
        view.capture_generation = 1;
        view.captured_screen = Some("old-session preview".into());
        view.request_draft.begin(1, "Old-session question".into());
        view.apply_session_snapshot(skillissue_domain::models::SessionSnapshot {
            session_id: Some("new-session".into()),
            active: false,
            transcript: vec![],
            messages: vec![],
        });
        assert_eq!(view.capture_generation, 2);
        assert!(view.captured_screen.is_none());
        assert_eq!(
            view.request_draft.resolve(1, true, "").as_deref(),
            Some("Old-session question")
        );
        view.captured_screen = Some("new-session preview".into());
        view.apply_session_snapshot(skillissue_domain::models::SessionSnapshot {
            session_id: Some("new-session".into()),
            active: true,
            transcript: vec![],
            messages: vec![],
        });
        assert_eq!(view.capture_generation, 2);
        assert_eq!(view.captured_screen.as_deref(), Some("new-session preview"));
    });
}

#[gpui_kit::test]
fn parent_and_existing_overlay_discard_previews_for_an_empty_new_session(cx: &mut TestAppContext) {
    with_view(cx, |view, _, cx| {
        let runtime = view.runtime.clone();
        let data_dir = view.data_dir.clone();
        let (handle, overlay) = gpui_kit::open_window(
            WindowOptions {
                focus: false,
                titlebar: None,
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| NativeView::new_for_test(runtime, data_dir, window, cx)),
        )
        .expect("mock overlay for new session");
        view.overlay = Some((handle, overlay.clone()));
        view.capture_generation = 3;
        view.captured_screen = Some("parent preview".into());
        view.request_draft
            .begin(1, "discard parent question".into());
        overlay.update(cx, |overlay, _| {
            overlay.capture_generation = 7;
            overlay.captured_screen = Some("overlay preview".into());
            overlay
                .request_draft
                .begin(1, "discard overlay question".into());
        });
        view.invalidate_capture_previews(cx);
        assert_eq!(view.capture_generation, 4);
        assert!(view.captured_screen.is_none());
        assert!(view.request_draft.resolve(1, true, "").is_none());
        overlay.update(cx, |overlay, _| {
            assert_eq!(overlay.capture_generation, 8);
            assert!(overlay.captured_screen.is_none());
            assert!(overlay.request_draft.resolve(1, true, "").is_none());
        });
    });
}
