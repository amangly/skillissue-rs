//! Native session coordination.
use super::*;
use std::{fs::OpenOptions, io::Write, time::Instant};

#[derive(Default)]
pub(super) struct RequestDraft {
    pending: Option<(u64, String)>,
}

impl RequestDraft {
    pub(super) fn begin(&mut self, generation: u64, prompt: String) {
        self.pending = Some((generation, prompt));
    }

    pub(super) fn resolve(
        &mut self,
        generation: u64,
        restore: bool,
        composer: &str,
    ) -> Option<String> {
        if self
            .pending
            .as_ref()
            .is_none_or(|(pending, _)| *pending != generation)
        {
            return None;
        }
        let (_, prompt) = self.pending.take()?;
        (restore && composer.is_empty() && !prompt.is_empty()).then_some(prompt)
    }

    pub(super) fn clear(&mut self) {
        self.pending = None;
    }
}

impl NativeView {
    pub(super) fn reveal_export(&self, path: PathBuf) {
        let sender = self.sender.clone();
        self.runtime.spawn_blocking(move || {
            let result = reveal_path(&path).map_err(|error| {
                format!(
                    "Export saved to {}, but could not open its location: {error}",
                    path.display()
                )
            });
            let _ = sender.send(Reply::ExportRevealed(result));
        });
    }

    pub(super) fn toggle_capture(&mut self) {
        if !self.supports_audio_input() && !self.snapshot.active {
            self.error = Some(self.hosted_audio_unavailable_message().into());
            return;
        }
        if self.notes_busy || self.closing {
            return;
        }
        if let Some(engine) = self.engine.clone() {
            let sender = self.sender.clone();
            let active = self.snapshot.active;
            self.notes_busy = active;
            self.status = if active {
                "Finishing transcription…"
            } else {
                "Starting audio capture…"
            }
            .into();
            self.runtime.spawn(async move {
                if active {
                    let result = async {
                        engine.stop_capture().await?;
                        let snapshot = engine.snapshot()?;
                        match snapshot.session_id {
                            Some(id)
                                if !snapshot.transcript.is_empty()
                                    || !snapshot.messages.is_empty() =>
                            {
                                engine.generate_notes(&id).await.map(Some)
                            }
                            _ => Ok(None),
                        }
                    }
                    .await;
                    let _ = sender.send(Reply::Stopped(result));
                } else {
                    let _ = sender.send(Reply::Status(engine.start_capture().await));
                }
            });
        }
    }
    pub(super) fn ask(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.busy
            || self
                .own_windows
                .closing
                .load(std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        let prompt = self.question.read(cx).value().to_string();
        let screen = self.settings.include_screenshot && self.supports_image_input();
        let references = self
            .settings
            .modes
            .iter()
            .any(|mode| mode.id == self.settings.active_mode_id && !mode.files.is_empty());
        if prompt.trim().is_empty()
            && self
                .snapshot
                .transcript
                .iter()
                .all(|entry| entry.text.trim().is_empty())
            && !self.snapshot.active
            && !references
            && !screen
        {
            self.error = None;
            self.status = "Type a question to start a chat.".into();
            self.begin_typing(_window, cx);
            cx.notify();
            return;
        }
        self.restore_focus_mode();
        self.expanded = true;
        self.overlay_ui.chat_shown = true;
        if self.is_overlay {
            _window.resize(size(px(560.), px(550.)));
        }
        if let Some(engine) = self.engine.clone() {
            let target = match engine.prepare_assist_target(&self.settings) {
                Ok(target) => target,
                Err(error) => {
                    self.error = Some(error);
                    cx.notify();
                    return;
                }
            };
            self.question
                .update(cx, |input, cx| input.set_value("", _window, cx));
            self.busy = true;
            self.stream.clear();
            self.overlay_ui.audio_notice = None;
            self.audio_retry_prompt = None;
            self.last_submitted_prompt = Some(prompt.clone());
            self.error = None;
            let sender = self.sender.clone();
            let hosted = self.settings.is_palepale();
            let cancel = tokio_util::sync::CancellationToken::new();
            self.request_cancel = Some(cancel.clone());
            let scope = RequestScope {
                windows: self.own_windows.clone(),
                id: uuid::Uuid::new_v4(),
            };
            if let Ok(mut requests) = self.own_windows.requests.lock() {
                requests.push((scope.id, cancel.clone()));
            }
            self.request_generation += 1;
            let generation = self.request_generation;
            self.request_started_at = Some((generation, Instant::now()));
            self.request_draft.begin(generation, prompt.clone());
            let handles = self.own_windows.clone();
            let monitor_point = self.native.as_ref().and_then(|native| native.center().ok());
            self.runtime.spawn(async move {
                let _scope = scope;
                let request_started = Instant::now();
                let capture_started = Instant::now();
                let screenshot = if screen {
                    match tokio::task::spawn_blocking(move || {
                        capture_screen(handles, monitor_point, hosted)
                    })
                    .await
                    {
                        Ok(Ok(data)) => Some(data),
                        Ok(Err(error)) => {
                            let _ = sender.send(Reply::Assist(generation, Err(error)));
                            return;
                        }
                        Err(error) => {
                            let _ = sender.send(Reply::Assist(
                                generation,
                                Err(format!("Screen capture task: {error}")),
                            ));
                            return;
                        }
                    }
                } else {
                    None
                };
                let capture_ms = capture_started.elapsed().as_millis();
                let provider_started = Instant::now();
                let result = engine
                    .assist_for_target(prompt, screenshot, cancel.clone(), target)
                    .await;
                if let Ok(directory) = crate::bootstrap::data_directory()
                    && let Ok(mut log) = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(directory.join("ui-latency.log"))
                {
                    let entry = serde_json::json!({
                        "captureMs": capture_ms,
                        "providerPipelineMs": provider_started.elapsed().as_millis(),
                        "requestMs": request_started.elapsed().as_millis(),
                        "screen": screen,
                        "ok": result.is_ok() && !cancel.is_cancelled(),
                        "timestampMs": skillissue_core::models::now_ms(),
                    });
                    let _ = writeln!(log, "{entry}");
                }
                let _ = sender.send(Reply::Assist(
                    generation,
                    if cancel.is_cancelled() {
                        Ok(())
                    } else {
                        result
                    },
                ));
            });
        }
    }

    pub(super) fn record_display_latency(&mut self, generation: u64, ok: bool) {
        let Some((started_generation, started_at)) = self.request_started_at.take() else {
            return;
        };
        if started_generation != generation {
            return;
        }
        if let Ok(mut log) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.data_dir.join("ui-latency.log"))
        {
            let entry = serde_json::json!({
                "event": "display",
                "displayMs": started_at.elapsed().as_millis(),
                "ok": ok,
                "timestampMs": skillissue_core::models::now_ms(),
            });
            let _ = writeln!(log, "{entry}");
        }
    }

    pub(super) fn quick_assist(
        &mut self,
        prompt: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy
            || self
                .own_windows
                .closing
                .load(std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        if !prompt.is_empty() {
            self.question
                .update(cx, |input, cx| input.set_value(prompt, window, cx));
        }
        self.ask(window, cx);
    }

    pub(super) fn can_retry_completed_audio(&self, cx: &Context<Self>) -> bool {
        self.audio_retry_prompt.is_some()
            && !self.busy
            && !self.closing
            && !self
                .own_windows
                .closing
                .load(std::sync::atomic::Ordering::SeqCst)
            && self.context.pending_transcriptions == 0
            && self.question.read(cx).value().is_empty()
            && self.engine.is_some()
    }

    pub(super) fn retry_with_completed_audio(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_retry_completed_audio(cx) {
            return;
        }
        let Some(prompt) = self.audio_retry_prompt.clone() else {
            return;
        };
        self.question
            .update(cx, |input, cx| input.set_value(prompt, window, cx));
        self.ask(window, cx);
        // Preparation can reject the request. Keep the retry available and
        // restore the empty composer rather than treating it as submitted.
        if !self.busy {
            self.question
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
    }

    pub(super) fn finish_request_draft(
        &mut self,
        generation: u64,
        failed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if generation != self.request_generation {
            return;
        }
        let cancelled = self
            .request_cancel
            .take()
            .is_some_and(|cancel| cancel.is_cancelled());
        let composer = self.question.read(cx).value().to_string();
        if let Some(prompt) = self
            .request_draft
            .resolve(generation, failed || cancelled, &composer)
        {
            self.question
                .update(cx, |input, cx| input.set_value(prompt, window, cx));
        }
    }

    pub(super) fn cancel_request(&mut self) {
        if let Some(cancel) = &self.request_cancel {
            cancel.cancel();
        }
        if let Some(engine) = &self.engine {
            engine.cancel_assist();
        }
        self.restore_focus_mode();
    }
    pub(super) fn clear_conversation(&mut self) {
        if self.busy {
            self.status = "Stop the current response before clearing the session.".into();
            return;
        }
        self.request_draft.clear();
        self.clear_audio_retry();
        self.invalidate_capture_preview();
        self.background(|engine| Reply::Cleared(engine.new_session()));
    }
}

impl NativeView {
    pub(super) fn clear_audio_retry(&mut self) {
        self.active_assistant_id = None;
        self.audio_retry_prompt = None;
        self.last_submitted_prompt = None;
        self.overlay_ui.audio_notice = None;
    }

    pub(super) fn capture_preview(&mut self) {
        if !self.supports_image_input() {
            self.error = Some(self.hosted_image_unavailable_message().into());
            return;
        }
        self.capture_generation = self.capture_generation.wrapping_add(1);
        let generation = self.capture_generation;
        let registry = self.own_windows.clone();
        let sender = self.sender.clone();
        let point = self.native.and_then(|window| window.center().ok());
        let hosted = self.settings.is_palepale();
        self.runtime.spawn_blocking(move || {
            let _ = sender.send(Reply::Captured(
                generation,
                capture_screen(registry, point, hosted),
            ));
        });
    }

    pub(super) fn invalidate_capture_preview(&mut self) {
        self.capture_generation = self.capture_generation.wrapping_add(1);
        self.captured_screen = None;
    }

    pub(super) fn invalidate_capture_previews(&mut self, cx: &mut Context<Self>) {
        self.invalidate_capture_preview();
        self.request_draft.clear();
        if let Some((_, overlay)) = &self.overlay {
            overlay.update(cx, |overlay, cx| {
                overlay.invalidate_capture_preview();
                overlay.request_draft.clear();
                cx.notify();
            });
        }
        cx.notify();
    }

    pub(super) fn apply_session_snapshot(&mut self, snapshot: SessionSnapshot) {
        if self.snapshot.session_id != snapshot.session_id {
            self.invalidate_capture_preview();
            if self.snapshot.session_id.is_some() {
                self.clear_audio_retry();
            }
        }
        self.snapshot = snapshot;
    }

    pub(super) fn export_session(&mut self, cx: &mut Context<Self>) {
        let snapshot = self.snapshot.clone();
        let directory = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.data_dir.clone());
        let chooser = cx.prompt_for_new_path(&directory, Some(&format!("session-{}.md", now_ms())));
        let sender = self.sender.clone();
        self.runtime.spawn(async move {
            let path = match chooser.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) => return,
                Ok(Err(error)) => {
                    let _ = sender.send(Reply::Exported(Err(error.to_string())));
                    return;
                }
                Err(error) => {
                    let _ = sender.send(Reply::Exported(Err(error.to_string())));
                    return;
                }
            };
            let result = tokio::task::spawn_blocking(move || {
                let mut content = String::from("# skillissue session\n\n## Transcript\n\n");
                for entry in snapshot.transcript {
                    content.push_str(&format!("**{}**: {}\n\n", entry.source, entry.text));
                }
                content.push_str("## Conversation\n\n");
                for message in snapshot.messages {
                    content.push_str(&format!("**{}**\n\n{}\n\n", message.role, message.content));
                }
                std::fs::write(&path, content).map_err(|error| error.to_string())?;
                Ok(path)
            })
            .await
            .map_err(|error| format!("Export task: {error}"))
            .and_then(|result| result);
            let _ = sender.send(Reply::Exported(result));
        });
    }
}
