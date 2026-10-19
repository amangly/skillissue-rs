//! Native runtime coordination.
use super::*;
use std::time::Instant;

type ShortcutState = (BTreeMap<String, String>, ShortcutContext);

#[derive(Default)]
pub(super) struct LatencyCache {
    pub(super) summary: Option<skillissue_core::LatencySummary>,
    pending: bool,
    refresh_at: Option<Instant>,
}

impl LatencyCache {
    fn begin(&mut self, now: Instant, visible: bool, engine_ready: bool) -> bool {
        if !visible || !engine_ready || self.pending || self.refresh_at.is_some_and(|at| now < at) {
            return false;
        }
        self.pending = true;
        true
    }

    fn complete(&mut self, summary: skillissue_core::LatencySummary, now: Instant) {
        self.summary = Some(summary);
        self.pending = false;
        self.refresh_at = Some(now + Duration::from_secs(5));
    }
}

#[cfg(test)]
mod latency_tests {
    use super::LatencyCache;
    use std::time::{Duration, Instant};

    #[test]
    fn refresh_is_visible_only_single_flight_and_throttled_after_completion() {
        let mut cache = LatencyCache::default();
        let start = Instant::now();
        assert!(!cache.begin(start, false, true));
        assert!(!cache.begin(start, true, false));
        assert!(cache.begin(start, true, true));
        let completed = start + Duration::from_secs(30);
        assert!(
            !cache.begin(completed, true, true),
            "slow reads must not queue more work"
        );
        cache.complete(
            skillissue_core::LatencySummary {
                requests: 7,
                ..Default::default()
            },
            completed,
        );
        assert_eq!(cache.summary.as_ref().unwrap().requests, 7);
        assert!(!cache.begin(completed + Duration::from_secs(4), true, true));
        assert!(!cache.begin(completed + Duration::from_secs(5), false, true));
        assert!(cache.begin(completed + Duration::from_secs(5), true, true));
        assert_eq!(
            cache.summary.as_ref().unwrap().requests,
            7,
            "retain the displayed values during refresh"
        );
    }
}

#[derive(Default)]
pub(super) struct ShortcutRegistrationRetry {
    retry_at: Option<Instant>,
    error: Option<String>,
}

impl ShortcutRegistrationRetry {
    pub(super) fn due(
        &self,
        previous: Option<&ShortcutState>,
        requested: &ShortcutState,
        now: Instant,
    ) -> bool {
        previous != Some(requested) || self.retry_at.is_some_and(|retry_at| now >= retry_at)
    }

    pub(super) fn complete(
        &mut self,
        result: Result<(), String>,
        now: Instant,
        visible_error: &mut Option<String>,
    ) -> bool {
        let previous_error = visible_error.clone();
        match result {
            Ok(()) => {
                self.retry_at = None;
                if let Some(error) = self.error.take()
                    && visible_error.as_ref() == Some(&error)
                {
                    *visible_error = None;
                }
            }
            Err(error) => {
                self.retry_at = Some(now + Duration::from_secs(2));
                self.error = Some(error.clone());
                *visible_error = Some(error);
            }
        }
        *visible_error != previous_error
    }
}

impl NativeView {
    pub(super) fn global_shortcut_context(&self, cx: &Context<Self>) -> ShortcutContext {
        let chat_shown = self.overlay.as_ref().is_some_and(|(_, view)| {
            let view = view.read(cx);
            view.shown && view.overlay_ui.chat_shown
        });
        ShortcutContext {
            active: self.snapshot.active || self.snapshot.session_id.is_some(),
            ambient_enabled: self.settings.ambient_enabled,
            chat_shown,
            recording: self.recording_key.is_some(),
        }
    }

    pub(super) fn reconcile_global_shortcuts(&mut self, cx: &Context<Self>) -> bool {
        let context = self.global_shortcut_context(cx);
        let requested = (self.settings.shortcuts.clone(), context);
        let Some(manager) = &mut self.hotkeys else {
            return false;
        };
        let now = Instant::now();
        if !self
            .shortcut_retry
            .due(self.shortcut_state.as_ref(), &requested, now)
        {
            return false;
        }
        let changed = self.shortcut_retry.complete(
            manager.reconcile(&requested.0, context),
            now,
            &mut self.error,
        );
        self.shortcut_state = Some(requested);
        changed
    }

    pub(super) fn boot(&mut self) {
        if self.opening || self.engine.is_some() {
            return;
        }
        self.opening = true;
        let path = self.data_dir.clone();
        let sender = self.sender.clone();
        self.runtime.spawn_blocking(move || {
            let _ = sender.send(Reply::Open(NativeEngine::open(path)));
        });
    }
    pub(super) fn background(&self, f: impl FnOnce(Arc<NativeEngine>) -> Reply + Send + 'static) {
        if let Some(engine) = self.engine.clone() {
            let sender = self.sender.clone();
            self.runtime.spawn_blocking(move || {
                let _ = sender.send(f(engine));
            });
        }
    }
    pub(super) fn refresh_sessions(&self) {
        let query = self.search_query.clone();
        self.background(move |engine| {
            let result = engine.list_sessions(&query, 200, 0);
            Reply::Sessions(query, result)
        });
    }

    pub(super) fn set_config(
        &mut self,
        config: ConfigView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let provider_mode_changed = self.settings.provider_mode != config.settings.provider_mode;
        let capture_target_changed = provider_mode_changed
            || self.settings.provider_base_url() != config.settings.provider_base_url()
            || self.settings.provider_chat_model() != config.settings.provider_chat_model()
            || self.settings.image_base_url != config.settings.image_base_url
            || self.settings.image_model != config.settings.image_model;
        if capture_target_changed {
            self.invalidate_capture_preview();
        }
        let hosted_target_changed = provider_mode_changed
            || self.settings.palepale.chat_model != config.settings.palepale.chat_model
            || self.settings.palepale.transcription_model
                != config.settings.palepale.transcription_model;
        self.settings = config.settings;
        if hosted_target_changed {
            self.invalidate_hosted_provider_status(provider_mode_changed);
        }
        for patch in &self.preference_queue {
            if let Err(error) = apply_preference_patch(&mut self.settings, patch) {
                self.error = Some(error);
            }
        }
        self.has_key = config.has_api_key;
        self.settings_ui.has_byok_key = config.has_byok_key;
        self.settings_ui.has_audio_key = config.has_audio_key;
        self.settings_ui.has_image_key = config.has_image_key;
        self.settings_ui.has_palepale_key = config.has_palepale_key;
        if !config.has_palepale_key {
            self.cancel_palepale_checks();
            if matches!(
                self.settings_ui.palepale_pairing,
                settings::PalepalePairingState::Ready
            ) {
                self.settings_ui.palepale_pairing = settings::PalepalePairingState::Idle;
            }
        }
        for (name, input) in &self.shortcuts {
            if let Some(value) = self.settings.shortcuts.get(name) {
                input.update(cx, |input, cx| input.set_value(value.clone(), window, cx));
            }
        }
        self.sync_provider_config(window, cx);
        self.sync_provider_status_from_parent(cx);
        if self.settings.is_palepale()
            && config.has_palepale_key
            && self.settings_ui.catalog_models.is_empty()
            && !self.settings_ui.catalog_loading
            && self.settings_ui.catalog_error.is_none()
        {
            self.refresh_palepale_catalog(cx);
        }
        if self.settings.is_palepale()
            && config.has_palepale_key
            && self.settings_ui.palepale_account_result.is_none()
            && !self.settings_ui.palepale_account_loading
            && self.selected_hosted_model().is_some()
        {
            self.refresh_palepale_account(cx);
        }
        let mode = if self.settings.theme == "dark" {
            ThemeMode::Dark
        } else if self.settings.theme == "light" {
            ThemeMode::Light
        } else {
            ThemeMode::from(cx.window_appearance())
        };
        if Theme::global(cx).mode != mode {
            Theme::change(mode, Some(window), cx);
        }
        crate::theme::apply(cx);
        if let Some(native) = &self.native {
            if let Err(error) = native.configure_tray_window() {
                self.error = Some(error);
            }
            if let Err(error) = native.set_capture_protection(self.settings.capture_protection) {
                self.error = Some(error);
            }
            if self.is_overlay
                && let Err(error) = native.set_no_focus(
                    self.settings.capture_protection && self.settings.no_focus && !self.typing,
                )
            {
                self.error = Some(error);
            }
        }
        if !config.warnings.is_empty() {
            self.status = config.warnings.join(" · ");
        }
        self.sync_provider_status_to_overlay(cx);
    }

    fn receive_provider_configuration(
        &mut self,
        saved_form: Option<ProviderMode>,
        result: Result<ConfigView, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(config) => {
                if let Some(mode) = saved_form {
                    self.provider_saved(mode, window, cx);
                } else {
                    self.palepale_connection_saved(window, cx);
                }
                let config = self
                    .engine
                    .as_ref()
                    .and_then(|engine| engine.config().ok())
                    .unwrap_or(config);
                self.set_config(config, window, cx);
                self.status = if saved_form.is_some() {
                    "Settings saved".into()
                } else {
                    "Connection updated".into()
                };
                if saved_form == Some(ProviderMode::Byok) {
                    self.auto_load_byok_models(cx);
                }
                if self.quit_after_preferences
                    && !self.preference_saving
                    && self.preference_queue.is_empty()
                {
                    self.quit_after_preferences = false;
                    self.quit(cx);
                }
            }
            Err(error) => {
                self.settings_ui.provider_save_pending = false;
                self.quit_after_preferences = false;
                self.error = Some(error);
            }
        }
    }

    pub(super) fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut dirty = self.busy && self.stream.is_empty() && self.shown;
        while let Ok(reply) = self.receiver.try_recv() {
            dirty = true;
            match reply {
                Reply::UpdateChecked(result) => {
                    self.updates.busy = false;
                    match result {
                        Ok(update) => {
                            self.updates.status = update.as_ref().map_or_else(
                                || "You're up to date.".into(),
                                |update| format!("Version {} is available.", update.version),
                            );
                            if update.as_ref().map(|u| &u.version)
                                != self.updates.available.as_ref().map(|u| &u.version)
                            {
                                self.updates.installer = None;
                            }
                            self.updates.available = update;
                        }
                        Err(error) => self.updates.status = format!("Could not check: {error}"),
                    }
                }
                Reply::UpdateDownloaded(result) => {
                    self.updates.busy = false;
                    match result {
                        Ok(path) => {
                            self.updates.installer = Some(path);
                            self.updates.status =
                                "Update verified. Open the installer when ready.".into();
                        }
                        Err(error) => self.updates.status = error,
                    }
                }
                Reply::UpdateLaunched(result) => {
                    self.updates.busy = false;
                    self.updates.status = match result {
                        Ok(()) => "Follow the installer to finish updating.".into(),
                        Err(error) => format!("Could not open installer: {error}"),
                    };
                }
                Reply::Open(Ok(engine)) => {
                    self.opening = false;
                    self.events = Some(engine.subscribe());
                    // Configuration reconciliation can start hosted readiness checks.
                    self.engine = Some(engine.clone());
                    match engine.config() {
                        Ok(config) => self.set_config(config, window, cx),
                        Err(error) => self.error = Some(error),
                    }
                    match engine.snapshot() {
                        Ok(snapshot) => self.snapshot = snapshot,
                        Err(error) => self.error = Some(error),
                    }
                    self.flush_preferences();
                    self.status = "Ready · everything is saved on this device".into();
                    self.refresh_sessions();
                    if !self.is_overlay
                        && !self.gallery
                        && !self.smoke
                        && !self.smoke_gallery
                        && !std::env::args().any(|arg| arg.starts_with("--review-output="))
                    {
                        // These are product defaults now, not hidden user-facing switches.
                        // Queue through the normal transaction so login changes retain rollback.
                        self.change_preference(|settings| {
                            settings.include_screenshot = true;
                            settings.hide_widget = true;
                            settings.open_at_login = true;
                        });
                        self.hotkeys = GlobalShortcuts::new()
                            .map_err(|e| self.error = Some(e))
                            .ok();
                        self.shortcut_state = None;
                        self.tray = NativeTray::new().map_err(|e| self.error = Some(e)).ok();
                        self.check_updates(cx);
                        if let Some(native) = self.native {
                            self.shown = !self.settings.capture_protection || self.tray.is_none();
                            let result = if self.shown {
                                native.show(true)
                            } else {
                                native.hide()
                            };
                            if let Err(error) = result {
                                self.error = Some(error);
                            }
                        }
                    }
                }
                Reply::Open(Err(error)) => {
                    if let Some(native) = self.native {
                        let _ = native.show(true);
                        self.shown = true;
                    }
                    self.opening = false;
                    self.error = Some(error);
                    self.status =
                        "Workspace could not open. Your existing files were preserved.".into();
                }
                Reply::Sessions(query, result) => {
                    if query == self.search_query {
                        match result {
                            Ok(sessions) => self.sessions = sessions,
                            Err(error) => self.error = Some(error),
                        }
                    }
                }
                Reply::Snapshot(Ok(snapshot)) => self.apply_session_snapshot(snapshot),
                Reply::Latency(summary) => self.latency.complete(summary, Instant::now()),
                Reply::Context(result) => {
                    if let Ok(status) = result {
                        self.context = status;
                    }
                }
                Reply::Assist(generation, result) => {
                    if generation == self.request_generation {
                        self.record_display_latency(generation, result.is_ok());
                        self.finish_request_draft(generation, result.is_err(), window, cx);
                        self.busy = false;
                        if let Err(error) = result {
                            self.error = Some(error);
                        }
                        self.refresh_sessions();
                    }
                }
                Reply::Stopped(result) => {
                    self.notes_busy = false;
                    match result {
                        Ok(Some(session)) => {
                            let _ = self.sender.send(Reply::Notes(Ok(session)));
                        }
                        Ok(None) => self.refresh_sessions(),
                        Err(error) => self.error = Some(error),
                    }
                    if self.closing {
                        self.closing = false;
                        self.quit(cx);
                    }
                }
                Reply::Notes(result) => {
                    self.notes_busy = false;
                    match result {
                        Ok(session) => {
                            if self
                                .selected
                                .as_ref()
                                .is_some_and(|current| current.id == session.id)
                            {
                                self.title.update(cx, |input, cx| {
                                    input.set_value(session.title.clone(), window, cx)
                                });
                                self.notes.update(cx, |input, cx| {
                                    input.set_value(session.summary.clone(), window, cx)
                                });
                                self.selected = Some(session);
                            }
                            self.refresh_sessions();
                        }
                        Err(error) => self.error = Some(error),
                    }
                    if self.closing {
                        self.closing = false;
                        self.quit(cx);
                    }
                }
                Reply::SavedAnswer(result) => {
                    self.saved_question_busy = false;
                    match result {
                        Ok(session) => {
                            self.saved_question
                                .update(cx, |input, cx| input.set_value("", window, cx));
                            if self
                                .selected
                                .as_ref()
                                .is_some_and(|current| current.id == session.id)
                            {
                                self.selected = Some(session);
                            }
                            self.refresh_sessions();
                        }
                        Err(error) => self.error = Some(error),
                    }
                    if self.closing {
                        self.closing = false;
                        self.quit(cx);
                    }
                }
                Reply::Captured(generation, result) if generation == self.capture_generation => {
                    match result {
                        Ok(data) => {
                            self.captured_screen = Some(data);
                            self.overlay_ui.transcript = true;
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                Reply::Captured(_, _) => {}
                Reply::Cleared(result) => match result {
                    Ok(()) => {
                        self.invalidate_capture_preview();
                        self.stream.clear();
                        self.error = None;
                        self.status = "Session cleared".into();
                        self.refresh_sessions();
                    }
                    Err(error) => self.error = Some(error),
                },
                Reply::Preference(result) => {
                    self.preference_queue.pop_front();
                    self.preference_saving = false;
                    match result {
                        Ok(config) => {
                            let config = self
                                .engine
                                .as_ref()
                                .and_then(|engine| engine.config().ok())
                                .unwrap_or(config);
                            self.set_config(config, window, cx);
                            self.status = "Settings saved".into();
                        }
                        Err(error) => {
                            self.quit_after_preferences = false;
                            if let Some(engine) = &self.engine
                                && let Ok(config) = engine.config()
                            {
                                self.set_config(config, window, cx);
                            }
                            self.error = Some(error);
                        }
                    }
                    self.flush_preferences();
                    if self.quit_after_preferences
                        && !self.preference_saving
                        && self.preference_queue.is_empty()
                    {
                        self.quit_after_preferences = false;
                        self.quit(cx);
                    }
                }
                Reply::Catalog(generation, result) => {
                    let current = generation == self.settings_ui.catalog_generation
                        && self.settings_ui.catalog_loading;
                    self.receive_provider_catalog(generation, result);
                    if current
                        && let Some(engine) = &self.engine
                        && let Ok(config) = engine.config()
                    {
                        self.set_config(config, window, cx);
                    }
                    if current {
                        self.sync_hosted_connections(window, cx);
                        self.sync_provider_status_to_overlay(cx);
                    }
                }
                Reply::PalepaleAccount(generation, result) => {
                    let current = generation == self.settings_ui.palepale_account_generation
                        && self.settings_ui.palepale_account_loading;
                    self.receive_palepale_account(generation, result);
                    if current {
                        self.sync_provider_status_to_overlay(cx);
                    }
                }
                Reply::PairingStarted(generation, result) => {
                    self.receive_palepale_pairing_started(generation, result, cx)
                }
                Reply::PairingReady(generation, result) => {
                    self.receive_palepale_pairing_ready(generation, result, window, cx)
                }
                Reply::ByokTest(generation, role, result) => {
                    if generation == self.settings_ui.byok_generation {
                        self.settings_ui.byok_cancel = None;
                        self.settings_ui.byok_status = match result {
                            Ok(()) => format!(
                                "{role}: test passed. The provider accepted the sample request."
                            ),
                            Err(error) => format!("{role}: {error}"),
                        };
                    }
                }
                Reply::ByokModelLists(generation, result) => {
                    if generation == self.settings_ui.byok_generation {
                        self.settings_ui.byok_cancel = None;
                        self.settings_ui.byok_status = match result {
                            Ok(lists) => {
                                let mut failures = Vec::new();
                                for list in lists {
                                    match list.models {
                                        Ok(models) => {
                                            self.settings_ui
                                                .connection_models
                                                .insert((list.role, list.endpoint), models);
                                        }
                                        Err(error) => failures.push(format!(
                                            "{}: {error}",
                                            match list.role {
                                                1 => "Audio",
                                                2 => "Images",
                                                _ => "Text",
                                            }
                                        )),
                                    }
                                }
                                if failures.is_empty() {
                                    "Models loaded. Choose a model, then Test.".into()
                                } else {
                                    failures.join(" · ")
                                }
                            }
                            Err(error) => error,
                        };
                    }
                }
                Reply::ByokModels(generation, result) => {
                    if generation == self.settings_ui.byok_generation {
                        self.settings_ui.byok_cancel = None;
                        match result {
                            Ok(models) => {
                                self.remember_connection_models(&models);
                                self.settings_ui.byok_status = format!(
                                    "Model list accessible: {} models. Inference, audio and image support are not verified.",
                                    models.len()
                                );
                                self.settings_ui.byok_models = models;
                            }
                            Err(error) => {
                                self.settings_ui.byok_status = error;
                                self.settings_ui.byok_models.clear();
                            }
                        }
                    }
                }
                Reply::ProviderConfig(mode, result) => {
                    self.receive_provider_configuration(Some(mode), result, window, cx)
                }
                Reply::ProviderConnection(result) => {
                    self.receive_provider_configuration(None, result, window, cx)
                }
                Reply::Exported(result) => match result {
                    Ok(path) => {
                        self.status = format!("Session exported to {}", path.display());
                        self.reveal_export(path);
                    }
                    Err(error) => self.error = Some(error),
                },
                Reply::ExportRevealed(result) => {
                    if let Err(error) = result {
                        self.error = Some(error);
                    }
                }
                Reply::HistoryTransferCancelled => self.history_transfer_pending = false,
                Reply::HistoryBackup(result) => {
                    self.history_transfer_pending = false;
                    match result {
                        Ok(path) => {
                            self.status = format!("History backup saved to {}", path.display());
                            self.reveal_export(path);
                        }
                        Err(error) => self.error = Some(format!("History backup: {error}")),
                    }
                }
                Reply::HistoryRestored(result) => {
                    self.history_transfer_pending = false;
                    match result {
                        Ok(count) => {
                            self.status = format!("Restored {count} saved sessions.");
                            self.refresh_sessions();
                        }
                        Err(error) => self.error = Some(format!("Restore history: {error}")),
                    }
                }
                Reply::Status(Ok(())) => {
                    self.status = "Saved locally".into();
                    self.refresh_sessions();
                }
                Reply::Quit(result) => match result {
                    Ok(()) => cx.quit(),
                    Err(error) => {
                        self.closing = false;
                        self.own_windows
                            .closing
                            .store(false, std::sync::atomic::Ordering::SeqCst);
                        self.error = Some(error);
                        self.status = "skillissue remains open so pending work can finish.".into();
                    }
                },
                Reply::Snapshot(Err(error)) | Reply::Status(Err(error)) => {
                    self.error = Some(error);
                }
            }
        }
        let mut events = vec![];
        if let Some(receiver) = &mut self.events {
            loop {
                match receiver.try_recv() {
                    Ok(event) => events.push(event),
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                        self.status = "Refreshing after a burst of activity".into();
                        dirty = true;
                        break;
                    }
                    Err(_) => break,
                }
            }
        }
        if self.status == "Refreshing after a burst of activity" {
            self.background(|engine| Reply::Snapshot(engine.snapshot()));
            self.status = "Ready".into();
        }
        for event in events {
            dirty = true;
            self.on_event(event, window, cx);
        }
        if self.poll_mode_editor(window, cx) {
            dirty = true;
        }
        self.settings_tick(window, cx);
        if !self.is_overlay
            && !self.gallery
            && !self.smoke
            && !self.smoke_gallery
            && self
                .updates
                .next_check
                .is_some_and(|next| Instant::now() >= next)
            && !self.updates.busy
        {
            self.check_updates(cx);
        }
        if self.overlay_tick(window, cx) {
            dirty = true;
        }
        if self.poll_dashboard(window, cx) {
            dirty = true;
        }
        if !self.is_overlay {
            dirty |= self.reconcile_global_shortcuts(cx);
            let actions = self
                .hotkeys
                .as_ref()
                .map(|manager| manager.poll())
                .unwrap_or_default();
            for action in actions {
                if self.recording_key.is_none() {
                    self.dispatch(action, window, cx);
                    dirty = true;
                }
            }
            let actions = self
                .tray
                .as_ref()
                .map(|tray| tray.poll())
                .unwrap_or_default();
            for action in actions {
                match action {
                    TrayAction::Show => {
                        self.shown = true;
                        if let Some(native) = &self.native {
                            let _ = native.show(true);
                        }
                    }
                    TrayAction::Settings => {
                        self.shown = true;
                        self.page = Page::Settings;
                        if let Some(native) = &self.native {
                            let _ = native.show(true);
                        }
                    }
                    TrayAction::Quit => self.quit(cx),
                }
                dirty = true;
            }
        }
        if self.expand_overlay_for_error(window) {
            dirty = true;
        }
        self.ticks += 1;
        if self.ticks.is_multiple_of(750)
            && !self.is_overlay
            && self.shown
            && self.page == Page::Settings
            && self.settings_ui.tab == settings::SettingsTab::Provider
            && !self.settings_ui.palepale_account_loading
        {
            self.refresh_palepale_account(cx);
        }
        let latency_visible =
            self.page == Page::Settings && self.settings_ui.tab == settings::SettingsTab::General;
        if self
            .latency
            .begin(Instant::now(), latency_visible, self.engine.is_some())
        {
            self.background(|engine| Reply::Latency(engine.latency_summary()));
        }
        #[cfg(feature = "ui-review")]
        if self.poll_review(window, cx) {
            dirty = true;
        }
        if self.ticks.is_multiple_of(12) {
            self.background(|engine| Reply::Context(engine.context_status()));
        }
        if self.smoke_gallery
            && self.ticks.is_multiple_of(6)
            && let Some(screen) = catalog::REVIEW_SCREENS.get(self.ticks / 6)
        {
            self.select_review(screen.id, window, cx);
            dirty = true;
        }
        let finished_gallery =
            self.smoke_gallery && self.ticks >= catalog::REVIEW_SCREENS.len() * 6;
        if (self.smoke && self.ticks >= 45) || finished_gallery {
            let report = serde_json::json!({"native":true,"engineReady":self.engine.is_some(),"sessions":self.sessions.len(),"error":self.error,"renderer":"GPUI Kit 0.7.3 / gpui-pre 0.3.9","renderedScreens":self.rendered_screens,"expectedScreens":if self.smoke_gallery {catalog::REVIEW_SCREENS.len()} else {0}});
            let _ = std::fs::write(self.data_dir.join("native-smoke.json"), report.to_string());
            cx.quit();
        }
        if dirty {
            cx.notify();
        }
    }

    pub(super) fn on_event(
        &mut self,
        event: EngineEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            EngineEvent::ConfigChanged(config) => {
                let config = self
                    .engine
                    .as_ref()
                    .and_then(|engine| engine.config().ok())
                    .unwrap_or(*config);
                self.set_config(config, window, cx);
            }
            EngineEvent::Snapshot(snapshot) => self.apply_session_snapshot(snapshot),
            EngineEvent::Transcript(entry) => {
                if !self
                    .snapshot
                    .transcript
                    .iter()
                    .any(|existing| existing.id == entry.id)
                {
                    self.snapshot.transcript.push(entry);
                    self.snapshot
                        .transcript
                        .sort_by_key(|entry| entry.created_at);
                }
            }
            EngineEvent::Assistant { id, kind, text } => {
                if kind != "start" && self.active_assistant_id.as_deref() != Some(id.as_str()) {
                    return;
                }
                match kind.as_str() {
                    "start" => {
                        self.active_assistant_id = Some(id);
                        self.last_submitted_prompt = Some(text);
                        self.overlay_ui.audio_notice = None;
                        self.audio_retry_prompt = None;
                        self.busy = true;
                        self.stream.clear();
                        cx.notify();
                    }
                    "context" => {
                        self.overlay_ui.audio_notice = Some(text);
                        self.audio_retry_prompt = self.last_submitted_prompt.clone();
                        cx.notify();
                    }
                    "delta" => {
                        self.stream.push_str(&text);
                        cx.notify();
                    }
                    "done" | "cancelled" => {
                        self.active_assistant_id = None;
                        self.busy = false;
                        self.stream.clear();
                        self.refresh_sessions();
                        cx.notify();
                    }
                    "error" => {
                        self.active_assistant_id = None;
                        self.busy = false;
                        self.error = Some(text);
                        cx.notify();
                    }
                    _ => {}
                }
            }
            EngineEvent::HistoryChanged => self.refresh_sessions(),
            EngineEvent::Error(error) => {
                self.error = Some(error);
            }
            EngineEvent::CaptureGap {
                source, message, ..
            } => self.error = Some(format!("{source}: {message}")),
            EngineEvent::AudioLevel(level) => self.overlay_audio_level(level),
        }
    }
}
