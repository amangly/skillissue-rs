use super::*;

impl NativeView {
    pub(in crate::ui) fn load_dashboard_history(&mut self) {
        let Some(engine) = self.engine.clone() else {
            return;
        };
        self.dashboard.history_generation += 1;
        self.dashboard.history_loading = true;
        self.dashboard.history_error = None;
        let generation = self.dashboard.history_generation;
        let query = self.search_query.clone();
        let offset = self.dashboard.history_offset;
        let sender = self.dashboard.sender.clone();
        self.runtime.spawn_blocking(move || {
            let result = engine.list_sessions(&query, 21, offset);
            let _ = sender.send(DashboardReply::History { generation, result });
        });
    }
    pub(in crate::ui) fn prepare_dashboard_shutdown(&mut self, cx: &mut Context<Self>) -> bool {
        if self.dashboard.save_error.is_some() || self.mode_save_failed() {
            self.dashboard.quit_requested = false;
            self.status =
                "Changes could not be saved. Correct the error or retry before closing.".into();
            return false;
        }
        self.save_dashboard_notes(cx);
        let mode_ready = self.prepare_mode_shutdown(cx);
        let ready = mode_ready && !self.dashboard.dirty && !self.dashboard.saving;
        self.dashboard.quit_requested = !ready;
        if !ready {
            self.status = "Saving mode and session edits before closing…".into();
        }
        ready
    }
    pub(in crate::ui) fn poll_dashboard(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let replies: Vec<_> = self.dashboard.receiver.try_iter().collect();
        let mut dirty = !replies.is_empty();
        for reply in replies {
            match reply {
                DashboardReply::History { generation, result } => {
                    if generation != self.dashboard.history_generation {
                        continue;
                    }
                    self.dashboard.history_loading = false;
                    match result {
                        Ok(mut rows) => {
                            self.dashboard.history_has_next = rows.len() > 20;
                            rows.truncate(20);
                            self.dashboard.history_rows = rows;
                            self.dashboard.history_loaded = true;
                        }
                        Err(error) => {
                            self.dashboard.history_loaded = true;
                            self.dashboard.history_error = Some(error);
                        }
                    }
                }
                DashboardReply::Loaded { id, result } => {
                    if self.dashboard.requested_session.as_deref() != Some(&id) {
                        continue;
                    }
                    match result {
                        Ok(record) => {
                            self.dashboard.note_evidence = None;
                            self.title.update(cx, |input, cx| {
                                input.set_value(record.title.clone(), window, cx)
                            });
                            self.notes.update(cx, |input, cx| {
                                input.set_value(record.summary.clone(), window, cx)
                            });
                            self.selected = Some(record);
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                DashboardReply::Saved {
                    id,
                    revision,
                    result,
                } => {
                    self.dashboard.saving = false;
                    match result {
                        Ok(record) => {
                            if self
                                .selected
                                .as_ref()
                                .is_some_and(|selected| selected.id == id)
                            {
                                self.selected = Some(record);
                            }
                            if revision == self.dashboard.save_revision {
                                self.dashboard.dirty = false;
                                self.dashboard.save_error = None;
                            }
                            self.refresh_sessions();
                            if self.dashboard.dirty {
                                self.save_dashboard_notes(cx);
                            }
                        }
                        Err(error) => self.dashboard.save_error = Some(error),
                    }
                }
                DashboardReply::Started(result) => {
                    self.dashboard.session_action = false;
                    match result {
                        Ok(()) => {
                            self.invalidate_capture_previews(cx);
                            self.show_overlay(cx);
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                DashboardReply::Resumed(result) => {
                    self.dashboard.session_action = false;
                    match result {
                        Ok(()) => {
                            self.invalidate_capture_previews(cx);
                            self.show_overlay(cx);
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                DashboardReply::Deleted { id, result } => {
                    self.dashboard.session_action = false;
                    self.dashboard.delete_confirm = false;
                    match result {
                        Ok(()) => {
                            if self.dashboard.requested_session.as_deref() == Some(&id) {
                                self.dashboard.requested_session = None;
                                self.selected = None;
                                self.dashboard.screen = Screen::Home;
                            }
                            self.refresh_sessions();
                            self.load_dashboard_history();
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                DashboardReply::Export(result) => match result {
                    Ok(path) => {
                        self.dashboard_notice(
                            format!("Session exported to {}", path.display()),
                            cx,
                        );
                        self.reveal_export(path);
                    }
                    Err(error) => self.error = Some(error),
                },
            }
        }
        let signature = format!(
            "{}:{}",
            self.sessions.len(),
            self.sessions
                .iter()
                .take(20)
                .map(|session| format!("{}@{}", session.id, session.updated_at))
                .collect::<Vec<_>>()
                .join("|")
        );
        if self.engine.is_some()
            && (!self.dashboard.history_loaded && !self.dashboard.history_loading
                || self.dashboard.history_signature != signature
                || self.dashboard.history_query != self.search_query)
        {
            if self.dashboard.history_query != self.search_query {
                self.dashboard.history_offset = 0;
            }
            self.dashboard.history_query = self.search_query.clone();
            self.dashboard.history_signature = signature;
            self.load_dashboard_history();
            dirty = true;
        }
        if self
            .mode_editor
            .as_ref()
            .is_none_or(|editor| editor.saved && !editor.pending && !editor.saving)
            && let Some(destination) = self.dashboard.mode_destination.take()
        {
            self.navigate_dashboard_mode(destination, window, cx);
            dirty = true;
        }
        if self.dashboard.quit_requested && self.prepare_dashboard_shutdown(cx) {
            self.quit(cx);
            dirty = true;
        }
        if self.dashboard.dirty
            && !self.dashboard.saving
            && self.dashboard.save_error.is_none()
            && now_ms().saturating_sub(self.dashboard.notes_edited_at) >= 400
        {
            self.save_dashboard_notes(cx);
            dirty = true;
        }
        if !self.dashboard.dirty
            && !self.dashboard.saving
            && let Some(destination) = self.dashboard.session_destination.take()
        {
            self.navigate_dashboard_session(destination, window, cx);
            dirty = true;
        }
        dirty |= self.sync_dashboard_modal_focus(window, cx);
        dirty
    }
    pub(in crate::ui) fn setup_dashboard_detail(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dashboard.subscribed {
            return;
        }
        self.dashboard.subscribed = true;
        let inputs: [AnyInputState; 2] = [self.title.clone().into(), self.notes.clone().into()];
        for field in inputs {
            match field {
                AnyInputState::Input(input) => self.subscriptions.push(cx.subscribe_in(
                    &input,
                    window,
                    |this, _, event, _, cx| match event {
                        EditorEvent::Change => {
                            this.dashboard.dirty = true;
                            this.dashboard.save_revision += 1;
                            this.dashboard.notes_edited_at = now_ms();
                            this.dashboard.save_error = None;
                            cx.notify();
                        }
                        EditorEvent::Blur => this.save_dashboard_notes(cx),
                        _ => {}
                    },
                )),
                AnyInputState::Textarea(input) => self.subscriptions.push(cx.subscribe_in(
                    &input,
                    window,
                    |this, _, event, _, cx| match event {
                        EditorEvent::Change => {
                            this.dashboard.dirty = true;
                            this.dashboard.save_revision += 1;
                            this.dashboard.notes_edited_at = now_ms();
                            this.dashboard.save_error = None;
                            cx.notify();
                        }
                        EditorEvent::Blur => this.save_dashboard_notes(cx),
                        _ => {}
                    },
                )),
                _ => {}
            }
        }
        self.subscriptions.push(cx.subscribe_in(
            &self.saved_question,
            window,
            |this, _, event, _, cx| {
                if matches!(
                    event,
                    EditorEvent::PressEnter {
                        secondary: false,
                        shift: false
                    }
                ) {
                    this.ask_dashboard_session(cx);
                }
            },
        ));
    }
    pub(in crate::ui) fn ask_dashboard_session(&mut self, cx: &mut Context<Self>) {
        if self.saved_question_busy || self.notes_busy {
            return;
        }
        if self.dashboard.dirty {
            self.save_dashboard_notes(cx);
            self.dashboard_notice("Saving changes. Ask again after Saved locally appears.", cx);
            return;
        }
        let Some(engine) = self.engine.clone() else {
            return;
        };
        let Some(session) = &self.selected else {
            return;
        };
        let prompt = self.saved_question.read(cx).value().trim().to_owned();
        if prompt.is_empty() {
            return;
        }
        let id = session.id.clone();
        let sender = self.sender.clone();
        self.saved_question_busy = true;
        self.dashboard.detail_tab = DetailTab::Usage;
        self.error = None;
        self.runtime.spawn(async move {
            let _ = sender.send(Reply::SavedAnswer(
                engine.ask_saved_session(&id, prompt).await,
            ));
        });
        cx.notify();
    }
    pub(in crate::ui) fn open_dashboard_session(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate_dashboard_session(SessionDestination::Session(id), window, cx);
    }
    pub(super) fn navigate_dashboard_session(
        &mut self,
        destination: SessionDestination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dashboard.delete_confirm = false;
        if self.dashboard.dirty || self.dashboard.saving {
            self.dashboard.session_destination = Some(destination);
            if self.dashboard.save_error.is_none() {
                self.save_dashboard_notes(cx);
            }
            cx.notify();
            return;
        }
        let id = match destination {
            SessionDestination::Home => {
                self.dashboard.screen = Screen::Home;
                self.dashboard.requested_session = None;
                self.selected = None;
                cx.notify();
                return;
            }
            SessionDestination::Session(id) => id,
        };
        self.setup_dashboard_detail(window, cx);
        self.dashboard.screen = Screen::Session;
        self.dashboard.detail_tab = DetailTab::Summary;
        self.dashboard.search_open = false;
        self.dashboard.menu_open = false;
        self.dashboard.save_error = None;
        self.dashboard.requested_session = Some(id.clone());
        self.selected = None;
        self.title
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.notes
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.saved_question
            .update(cx, |input, cx| input.set_value("", window, cx));
        if let Some(engine) = self.engine.clone() {
            let sender = self.dashboard.sender.clone();
            self.runtime.spawn_blocking(move || {
                let result = engine.saved_session(&id);
                let _ = sender.send(DashboardReply::Loaded { id, result });
            });
        }
        cx.notify();
    }
    pub(in crate::ui) fn save_dashboard_notes(&mut self, cx: &mut Context<Self>) {
        if !self.dashboard.dirty || self.dashboard.saving || self.notes_busy {
            return;
        }
        let Some(engine) = self.engine.clone() else {
            return;
        };
        let Some(session) = &self.selected else {
            return;
        };
        let id = session.id.clone();
        let revision = self.dashboard.save_revision;
        let title = self.title.read(cx).value().to_string();
        let notes = self.notes.read(cx).value().to_string();
        let sender = self.dashboard.sender.clone();
        self.dashboard.saving = true;
        self.runtime.spawn_blocking(move || {
            let result = engine
                .update_notes(&id, &title, &notes)
                .and_then(|_| engine.saved_session(&id));
            let _ = sender.send(DashboardReply::Saved {
                id,
                revision,
                result,
            });
        });
    }
    pub(in crate::ui) fn start_dashboard_session(&mut self, cx: &mut Context<Self>) {
        if self.dashboard.session_action || self.closing {
            return;
        }
        if self.gallery {
            self.show_overlay(cx);
            return;
        }
        if self.snapshot.active {
            self.show_overlay(cx);
            return;
        }
        let Some(engine) = self.engine.clone() else {
            return;
        };
        self.dashboard.session_action = true;
        let sender = self.dashboard.sender.clone();
        let ambient = self.settings.ambient_enabled || self.settings.is_palepale();
        self.runtime.spawn(async move {
            let result = engine.new_session();
            let result = match result {
                Ok(()) if !ambient => engine.start_capture().await,
                other => other,
            };
            let _ = sender.send(DashboardReply::Started(result));
        });
        cx.notify();
    }
    pub(in crate::ui) fn resume_dashboard_session(&mut self, cx: &mut Context<Self>) {
        if self.dashboard.session_action || self.closing {
            return;
        }
        if self.dashboard.dirty {
            self.save_dashboard_notes(cx);
            self.dashboard_notice(
                "Saving changes. Resume again when Saved locally appears.",
                cx,
            );
            return;
        }
        let Some(session) = &self.selected else {
            self.dashboard_notice(
                if self.dashboard.requested_session.is_some() {
                    "Wait for the selected session to finish loading."
                } else {
                    "Select a saved session before resuming it."
                },
                cx,
            );
            return;
        };
        if self.snapshot.active && self.snapshot.session_id.as_deref() == Some(&session.id) {
            self.show_overlay(cx);
            return;
        }
        let id = session.id.clone();
        let sender = self.dashboard.sender.clone();
        self.dashboard.session_action = true;
        self.background(move |engine| {
            let result = engine.resume_session(&id);
            let _ = sender.send(DashboardReply::Resumed(result));
            Reply::Status(Ok(()))
        });
        cx.notify();
    }
    fn copy_dashboard_summary(&mut self, cx: &mut Context<Self>) {
        if self.selected.is_none() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(
            self.notes.read(cx).value().to_string(),
        ));
        self.dashboard_notice("Summary copied", cx);
    }
    pub(in crate::ui) fn render_dashboard_detail(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.selected.as_ref();
        let state = selected
            .map(|session| state_label(&session.state))
            .unwrap_or("Current session");
        div().relative().size_full().flex().flex_col()
            .child(div().id("session-detail-scroll").flex_1().min_h_0().overflow_y_scroll().child(div().max_w(px(672.)).mx_auto().px_6().pt_7().pb(px(100.)).flex().flex_col().gap_3()
                .child(div().flex().justify_between().items_center().child(muted(format!("{}  ·  {state}", selected.map(|session| date(session.created_at)).unwrap_or_else(|| date(now_ms())))))
                    .child(button("session-export", "Export").ghost().icon(icon(Glyph::Download)).disabled(selected.is_none()).on_click(cx.listener(|this, _, _, cx| this.export_dashboard_session(cx))))
                    .child(button("session-delete", if self.dashboard.delete_confirm { "Delete again" } else { "Delete" }).ghost().icon(icon(Glyph::Trash)).text_color(ink(0xdc2626)).disabled(selected.is_none() || self.dashboard.session_action || self.snapshot.active).on_click(cx.listener(|this, _, _, cx| this.delete_dashboard_session(cx)))))
                .child(Input::new(&self.title).appearance(false).bordered(false).h(px(44.)).text_size(px(24.)).line_height(px(32.)).disabled(self.notes_busy || selected.is_none()).aria_label("Session title"))
                .child(div().text_xs().text_color(ink(SECONDARY)).child(if let Some(error) = &self.dashboard.save_error { format!("Changes were not saved: {error}") } else if self.dashboard.saving || self.dashboard.dirty { "Saving…".into() } else { format!("Saved locally{}", selected.and_then(|session| session.mode_name.as_ref()).map(|name| format!(" · {name}")).unwrap_or_default()) }))
                .when(self.dashboard.save_error.is_some(), |view| view.child(button("retry-notes-save", "Retry").on_click(cx.listener(|this, _, _, cx| this.save_dashboard_notes(cx)))))
                .child(div().flex().items_center().justify_between().gap_3().mt_1()
                    .child(div().flex().p_1().rounded_lg().border_1().border_color(glass(LINE)).bg(glass(0xd3e1f55e)).children([(DetailTab::Summary,"Summary"),(DetailTab::Transcript,"Transcript"),(DetailTab::Usage,"Usage")].into_iter().map(|(tab,label)| button(SharedString::from(format!("detail-tab-{label}")), label).ghost().bg(if self.dashboard.detail_tab == tab { glass(0xffffffdf) } else { glass(0xffffff00) }).on_click(cx.listener(move |this, _, _, cx| { this.dashboard.detail_tab = tab; cx.notify(); })))))
                    .when(self.dashboard.detail_tab == DetailTab::Summary, |view| view.child(div().flex().items_center().gap_2()
                        .child(button("generate-summary", if self.notes_busy { "Generating…" } else if self.notes.read(cx).value().trim().is_empty() { "Generate notes" } else { "Regenerate" }).ghost().icon(icon(Glyph::RefreshCw)).disabled(selected.is_none() || self.notes_busy || self.busy || self.snapshot.active).on_click(cx.listener(|this, _, _, cx| {
                            if this.dashboard.dirty { this.save_dashboard_notes(cx); this.dashboard_notice("Saving changes. Generate again after Saved locally appears.", cx); return; }
                            let Some(engine) = this.engine.clone() else { return; }; let Some(session) = &this.selected else { return; }; let id = session.id.clone(); let sender = this.sender.clone(); this.notes_busy = true;
                            this.runtime.spawn(async move { let _ = sender.send(Reply::Notes(engine.generate_notes(&id).await)); }); cx.notify();
                        })))
                        .child(button("copy-summary", "Copy summary").ghost().icon(icon(Glyph::Copy)).disabled(selected.is_none()).on_click(cx.listener(|this, _, _, cx| this.copy_dashboard_summary(cx)))))))
                .child(div().mt_3().when(self.dashboard.detail_tab == DetailTab::Summary, |view| view
                    .when_some(selected.and_then(|session| session.summary_error.clone()), |view, error| view.child(muted(format!("Notes generation failed: {error}")).text_color(ink(0xc34b4b))))
                    .child(Textarea::new(&self.notes).appearance(false).bordered(false).h(px(260.)).disabled(self.notes_busy || selected.is_none()).aria_label("Session notes"))
                    .when_some(selected, |view, session| {
                        let sources = skillissue_core::provider::note_sources(self.notes.read(cx).value().as_str(), &session.transcript);
                        view.child(div().flex().flex_wrap().gap_2().children(sources.into_iter().map(|(number, entry)| {
                            let id = entry.id.clone();
                            div().debug_selector(move || format!("notes-evidence-{number}")).child(button(SharedString::from(format!("notes-evidence-{number}")), SharedString::from(format!("[T{number}] {}", entry.speaker_label()))).ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.dashboard.note_evidence = Some(id.clone());
                                    cx.notify();
                                })))
                        })))
                        .when_some(session.transcript.iter().find(|entry| self.dashboard.note_evidence.as_deref() == Some(entry.id.as_str())), |view, entry| {
                            view.child(div().debug_selector(|| "notes-source-excerpt".into()).p_3().rounded_lg().border_1().border_color(glass(LINE)).flex().flex_col().gap_2()
                                .child(muted(format!("{} · {}", entry.speaker_label(), date(entry.created_at))))
                                .child(markdown(SharedString::from(format!("notes-source-{}",entry.id)), entry.text.clone())))
                        })
                    }))
                    .when(self.dashboard.detail_tab == DetailTab::Transcript, |view| view.child(div().flex().flex_col().gap_5()
                        .when(selected.is_none_or(|session| session.transcript.is_empty()), |view| view.child(muted("No transcript")))
                        .children(selected.into_iter().flat_map(|session| session.transcript.iter()).map(|entry| div().flex().flex_col().gap_1()
                            .child(div().flex().gap_2().text_xs().child(div().text_color(ink(if entry.source == "microphone" { BLUE } else { SECONDARY })).child(entry.speaker_label())).child(div().text_color(ink(SECONDARY)).child(date(entry.created_at))))
                            .child(markdown(SharedString::from(format!("detail-transcript-{}",entry.id)), entry.text.clone()))))))
                    .when(self.dashboard.detail_tab == DetailTab::Usage, |view| view.child(div().flex().flex_col().gap_5()
                        .when(selected.is_none_or(|session| session.messages.is_empty()), |view| view.child(muted("No usage messages yet.")))
                        .children(selected.into_iter().flat_map(|session| session.messages.iter()).map(|message| div().flex().flex_col().gap_1().child(div().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(ink(SECONDARY)).child(if message.role == "user" { "You" } else { "skillissue" })).child(markdown(SharedString::from(format!("detail-message-{}", message.id)), message.content.clone())))))))
                .child(div().mt_5().text_xs().text_center().text_color(ink(SECONDARY)).child("Saved on this device · notes use your configured provider"))))
            .child(div().absolute().bottom(px(24.)).left_0().right_0().flex().justify_center().items_center().gap_2().px_5()
                .child(button("resume-detail", if self.snapshot.active && selected.is_some_and(|session| Some(&session.id) == self.snapshot.session_id.as_ref()) { "Return to Session" } else { "Resume Session" }).icon(icon(Glyph::Play)).rounded_full().h(px(38.)).disabled(self.dashboard.session_action || selected.is_none()).on_click(cx.listener(|this, _, _, cx| this.resume_dashboard_session(cx))))
                .child(div().w(px(448.)).max_w_full().rounded_full().border_1().border_color(glass(0xffffffe6)).bg(glass(0xf7fbffdc)).p_1().pl_3().flex().items_center().gap_2().shadow_lg()
                    .child(div().flex_1().child(Input::new(&self.saved_question).appearance(false).bordered(false).disabled(self.saved_question_busy).aria_label("Ask about this meeting")))
                    .child(button("ask-saved-session", "").icon(icon(if self.saved_question_busy { Glyph::RefreshCw } else { Glyph::ArrowUp })).rounded_full().bg(ink(BLUE)).text_color(ink(0xffffff)).disabled(self.saved_question_busy || self.notes_busy || selected.is_none()).on_click(cx.listener(|this, _, _, cx| this.ask_dashboard_session(cx))))))
            .into_any_element()
    }
    fn export_dashboard_session(&mut self, cx: &mut Context<Self>) {
        if self.dashboard.dirty || self.dashboard.saving {
            self.save_dashboard_notes(cx);
            self.dashboard_notice(
                "Saving changes. Export again after Saved locally appears.",
                cx,
            );
            return;
        }
        let Some(session) = self.selected.clone() else {
            return;
        };
        let directory = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .map(|home| home.join("Documents/skillissue"));
        let Some(directory) = directory else {
            self.error = Some("Documents folder is unavailable".into());
            return;
        };
        let _ = std::fs::create_dir_all(&directory);
        let receiver =
            cx.prompt_for_new_path(&directory, Some(&format!("session-{}.md", now_ms())));
        let sender = self.dashboard.sender.clone();
        self.runtime.spawn(async move {
            let result = match receiver.await {
                Ok(Ok(Some(path))) => {
                    let mut content = format!(
                        "# {}\n\n## Notes\n\n{}\n\n## Transcript\n\n",
                        session.title, session.summary
                    );
                    let mut transcript = session.transcript;
                    transcript.sort_by_key(|entry| (entry.created_at, entry.id.clone()));
                    for (index, entry) in transcript
                        .into_iter()
                        .filter(|entry| !entry.text.trim().is_empty())
                        .enumerate()
                    {
                        content.push_str(&format!(
                            "[T{}] **{}**: {}\n\n",
                            index + 1,
                            entry.speaker_label(),
                            entry.text
                        ));
                    }
                    content.push_str("## Conversation\n\n");
                    for message in session.messages {
                        content
                            .push_str(&format!("**{}**\n\n{}\n\n", message.role, message.content));
                    }
                    std::fs::write(&path, content)
                        .map(|()| path)
                        .map_err(|error| error.to_string())
                }
                Ok(Ok(None)) => return,
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = sender.send(DashboardReply::Export(result));
        });
        cx.notify();
    }

    fn delete_dashboard_session(&mut self, cx: &mut Context<Self>) {
        if self.dashboard.session_action
            || self.dashboard.saving
            || self.dashboard.dirty
            || self.notes_busy
            || self.saved_question_busy
            || self.busy
            || self.snapshot.active
        {
            self.dashboard.delete_confirm = false;
            self.dashboard_notice("Finish pending work and save edits before deleting.", cx);
            return;
        }
        if !self.dashboard.delete_confirm {
            self.dashboard.delete_confirm = true;
            self.dashboard_notice(
                "Click Delete again to permanently remove this saved session.",
                cx,
            );
            return;
        }
        let Some(engine) = self.engine.clone() else {
            return;
        };
        let Some(session) = &self.selected else {
            return;
        };
        let id = session.id.clone();
        let sender = self.dashboard.sender.clone();
        self.dashboard.session_action = true;
        self.runtime.spawn_blocking(move || {
            let result = engine.delete_saved_session(&id);
            let _ = sender.send(DashboardReply::Deleted { id, result });
        });
        cx.notify();
    }
}

#[cfg(all(test, feature = "native-effects"))]
mod action_tests {
    use super::{DashboardReply, NativeView, Page, SessionDestination};
    use gpui_kit::{
        AppContext, ClipboardItem, Context, TestAppContext, Window, WindowOptions,
        component::{Theme, ThemeMode},
    };
    use skillissue_domain::models::SessionRecord;
    use std::sync::Arc;

    fn with_view(
        cx: &mut TestAppContext,
        test: impl FnOnce(&mut NativeView, &mut Window, &mut Context<NativeView>),
    ) {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap(),
        );
        let directory = std::env::temp_dir().join(format!(
            "skillissue-dashboard-test-{}",
            uuid::Uuid::new_v4()
        ));
        let (handle, view) = cx.update(|cx| {
            if !cx.has_global::<Theme>() {
                gpui_kit::init(cx);
            }
            Theme::change(ThemeMode::Light, None, cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut view = NativeView::new_for_test(runtime, directory.clone(), window, cx);
                    view.gallery = true;
                    view.page = Page::Sessions;
                    view
                })
            })
            .unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| test(view, window, cx));
        })
        .unwrap();
        assert!(
            !directory.exists(),
            "inert dashboard checks must not open a profile"
        );
    }

    #[gpui_kit::test]
    fn navigation_and_pending_edits_reset_delete_confirmation(cx: &mut TestAppContext) {
        with_view(cx, |view, window, cx| {
            view.dashboard.delete_confirm = true;
            view.navigate_dashboard_session(SessionDestination::Home, window, cx);
            assert!(!view.dashboard.delete_confirm);
            view.dashboard.delete_confirm = true;
            view.dashboard.dirty = true;
            view.delete_dashboard_session(cx);
            assert!(!view.dashboard.delete_confirm);
            assert!(!view.dashboard.session_action);
        });
    }

    #[gpui_kit::test]
    fn resume_waits_for_a_loaded_selection_without_opening_other_context(cx: &mut TestAppContext) {
        with_view(cx, |view, _, cx| {
            for requested in [Some("loading-session".into()), None] {
                view.dashboard.requested_session = requested;
                view.selected = None;
                view.resume_dashboard_session(cx);
                assert!(view.page == Page::Sessions);
                assert!(view.overlay.is_none());
                assert!(!view.dashboard.session_action);
                assert!(!view.dashboard.notice.is_empty());
            }
        });
    }

    #[gpui_kit::test]
    fn repeated_start_and_shutdown_cannot_dispatch_another_session_action(cx: &mut TestAppContext) {
        with_view(cx, |view, _, cx| {
            for (pending, closing) in [(true, false), (false, true)] {
                view.dashboard.session_action = pending;
                view.closing = closing;
                view.start_dashboard_session(cx);
                assert!(view.page == Page::Sessions);
                assert!(view.overlay.is_none());
                assert_eq!(view.dashboard.session_action, pending);
                assert!(view.engine.is_none());
            }
        });
    }

    #[gpui_kit::test]
    fn changing_sessions_preserves_blocked_edits_then_clears_stale_fields_and_copy(
        cx: &mut TestAppContext,
    ) {
        with_view(cx, |view, window, cx| {
            let old = SessionRecord {
                id: "old".into(),
                title: "Old title".into(),
                created_at: 1,
                updated_at: 1,
                ended_at: None,
                state: "ready".into(),
                summary: "Old notes".into(),
                summary_error: None,
                mode_name: None,
                mode_sections: vec![],
                transcript: vec![],
                messages: vec![],
            };
            view.selected = Some(old.clone());
            view.title
                .update(cx, |input, cx| input.set_value("Unsaved title", window, cx));
            view.notes
                .update(cx, |input, cx| input.set_value("Unsaved notes", window, cx));
            view.saved_question
                .update(cx, |input, cx| input.set_value("Old question", window, cx));
            view.dashboard.dirty = true;
            view.navigate_dashboard_session(SessionDestination::Session("new".into()), window, cx);
            assert_eq!(view.title.read(cx).value().as_ref(), "Unsaved title");
            assert_eq!(view.notes.read(cx).value().as_ref(), "Unsaved notes");
            assert_eq!(view.selected.as_ref().unwrap().id, "old");

            view.dashboard.dirty = false;
            view.dashboard.session_destination = None;
            cx.write_to_clipboard(ClipboardItem::new_string("Unchanged clipboard".into()));
            view.navigate_dashboard_session(SessionDestination::Session("new".into()), window, cx);
            assert!(view.selected.is_none());
            assert!(view.title.read(cx).value().is_empty());
            assert!(view.notes.read(cx).value().is_empty());
            assert!(view.saved_question.read(cx).value().is_empty());
            view.copy_dashboard_summary(cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("Unchanged clipboard")
            );

            view.dashboard
                .sender
                .send(DashboardReply::Loaded {
                    id: "old".into(),
                    result: Ok(old),
                })
                .unwrap();
            view.dashboard
                .sender
                .send(DashboardReply::Loaded {
                    id: "new".into(),
                    result: Err("Fixture load failed".into()),
                })
                .unwrap();
            view.poll_dashboard(window, cx);
            assert!(view.selected.is_none());
            assert!(view.title.read(cx).value().is_empty());
            assert!(view.notes.read(cx).value().is_empty());
            assert!(view.saved_question.read(cx).value().is_empty());
            assert_eq!(view.error.as_deref(), Some("Fixture load failed"));
            view.copy_dashboard_summary(cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("Unchanged clipboard")
            );
        });
    }
}
