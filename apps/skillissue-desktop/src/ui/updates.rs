use super::*;

#[derive(Default)]
pub(super) struct UpdateState {
    pub next_check: Option<Instant>,
    pub busy: bool,
    pub available: Option<skillissue_core::updates::Update>,
    pub installer: Option<PathBuf>,
    pub status: String,
}

impl NativeView {
    pub(super) fn check_updates(&mut self, cx: &mut Context<Self>) {
        if self.updates.busy || self.is_overlay {
            return;
        }
        self.updates.busy = true;
        self.updates.status = "Checking for updates…".into();
        self.updates.next_check = Some(Instant::now() + Duration::from_secs(6 * 60 * 60));
        let sender = self.sender.clone();
        self.runtime.spawn(async move {
            let result = skillissue_core::updates::check(env!("CARGO_PKG_VERSION")).await;
            let _ = sender.send(Reply::UpdateChecked(result));
        });
        cx.notify();
    }

    fn update_install_blocked(&self) -> bool {
        let requests_pending = self
            .own_windows
            .requests
            .lock()
            .map_or(true, |requests| !requests.is_empty());
        self.snapshot.active
            || requests_pending
            || self.busy
            || self.notes_busy
            || self.saved_question_busy
            || self.context.pending_transcriptions > 0
            || self.preference_saving
            || !self.preference_queue.is_empty()
            || self.settings_ui.provider_save_pending
    }

    pub(super) fn update_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_size(px(14.)).child("App updates"))
            .child(
                div()
                    .text_size(px(12.))
                    .child(if self.updates.status.is_empty() {
                        format!("Version {} · Stable releases", env!("CARGO_PKG_VERSION"))
                    } else {
                        self.updates.status.clone()
                    }),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("check-updates")
                            .ghost()
                            .small()
                            .label("Check for updates")
                            .disabled(self.updates.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.check_updates(cx))),
                    )
                    .when(self.updates.available.is_some(), |view| {
                        view.child(
                            Button::new("install-update")
                                .small()
                                .label(if self.updates.installer.is_some() {
                                    "Open installer"
                                } else {
                                    "Download update"
                                })
                                .disabled(self.updates.busy || self.update_install_blocked())
                                .on_click(cx.listener(|this, _, _, cx| this.start_update(cx))),
                        )
                    }),
            )
            .when(
                self.updates.available.is_some() && self.update_install_blocked(),
                |view| {
                    view.child(
                        div()
                            .text_size(px(12.))
                            .child("Finish the session and pending work to update."),
                    )
                },
            )
    }

    fn start_update(&mut self, cx: &mut Context<Self>) {
        if self.updates.busy || self.update_install_blocked() {
            return;
        }
        let sender = self.sender.clone();
        if let Some(path) = self.updates.installer.clone() {
            let Some(update) = self.updates.available.clone() else {
                return;
            };
            self.updates.busy = true;
            self.runtime.spawn_blocking(move || {
                // The interactive per-user installer handles closing the app and upgrade prompts.
                let result = skillissue_core::updates::verify_file(&update, &path).and_then(|()| {
                    std::process::Command::new(path)
                        .spawn()
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                });
                let _ = sender.send(Reply::UpdateLaunched(result));
            });
        } else if let Some(update) = self.updates.available.clone() {
            self.updates.busy = true;
            self.updates.status = "Downloading and verifying…".into();
            self.runtime.spawn(async move {
                let _ = sender.send(Reply::UpdateDownloaded(
                    skillissue_core::updates::download(update).await,
                ));
            });
        }
        cx.notify();
    }
}
