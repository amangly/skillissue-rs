//! Native windows coordination.
use super::*;

impl NativeView {
    pub(super) fn drag_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.gallery {
            return;
        }
        if let Some(native) = self.native {
            if let Err(error) = native.start_drag() {
                self.error = Some(error);
                cx.notify();
            }
        } else {
            window.start_window_move();
        }
    }

    pub(super) fn begin_typing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.typing = true;
        if let Some(native) = &self.native {
            let result = if self.is_overlay {
                native.set_always_on_top(true)
            } else {
                Ok(())
            }
            .and_then(|_| native.set_no_focus(false))
            .and_then(|_| native.show(true));
            if let Err(error) = result {
                self.typing = false;
                self.error = Some(error);
                return;
            }
        }
        self.question.focus_handle(cx).focus(window, cx);
        cx.notify();
    }
    pub(super) fn restore_focus_mode(&mut self) {
        self.typing = false;
        if self.is_overlay
            && let Some(native) = &self.native
            && let Err(error) =
                native.set_no_focus(self.settings.capture_protection && self.settings.no_focus)
        {
            self.error = Some(error);
        }
    }

    pub(super) fn reveal_overlay_window(
        &mut self,
        chat: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.is_overlay
            && let Some(native) = self.native
            && let Err(error) = native
                .set_always_on_top(true)
                .and_then(|_| native.show(false))
        {
            self.error = Some(error);
            cx.notify();
            return false;
        }
        self.shown = true;
        self.preview_hidden = false;
        if chat {
            self.overlay_ui.chat_shown = true;
        }
        if self.is_overlay {
            window.resize(size(
                px(if self.overlay_ui.chat_shown {
                    560.
                } else {
                    183.
                }),
                px(if !self.overlay_ui.chat_shown {
                    70.
                } else if self.expanded {
                    550.
                } else {
                    style::OVERLAY_COLLAPSED_HEIGHT
                }),
            ));
        }
        window.refresh();
        cx.notify();
        true
    }

    pub(super) fn show_overlay(&mut self, cx: &mut Context<Self>) {
        if self.gallery {
            self.preview_hidden = false;
            self.review_id = "chat".into();
            self.page = Page::Live;
            self.overlay_ui = Default::default();
            self.expanded = false;
            self.modal_backdrop = None;
            self.reset_dashboard_review();
            cx.notify();
            return;
        }
        if let Some((handle, view)) = self.overlay.clone() {
            let provider_status = self.provider_status_snapshot();
            let result = handle.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.apply_provider_status_snapshot(&provider_status);
                    view.reveal_overlay_window(true, window, cx);
                })
            });
            if result.is_ok() {
                return;
            }
            self.overlay = None;
        }
        let Some(engine) = self.engine.clone() else {
            return;
        };
        let runtime = self.runtime.clone();
        let data_dir = self.data_dir.clone();
        let settings = self.settings.clone();
        let provider_status = self.provider_status_snapshot();
        let has_key = self.has_key;
        let has_palepale_key = self.settings_ui.has_palepale_key;
        let has_byok_key = self.settings_ui.has_byok_key;
        let own_windows = self.own_windows.clone();
        let parent = (self.window_handle, cx.weak_entity());
        let result = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(560.), px(style::OVERLAY_COLLAPSED_HEIGHT)),
                    cx,
                ))),
                window_min_size: Some(size(px(183.), px(70.))),
                is_resizable: false,
                titlebar: None,
                kind: WindowKind::Floating,
                focus: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let mut view = Self::new_inner(
                        runtime,
                        data_dir,
                        false,
                        Some(engine.clone()),
                        Some(own_windows),
                        #[cfg(test)]
                        true,
                        window,
                        cx,
                    );
                    view.engine = Some(engine.clone());
                    view.events = Some(engine.subscribe());
                    view.settings = settings;
                    view.has_key = has_key;
                    view.settings_ui.has_palepale_key = has_palepale_key;
                    view.settings_ui.has_byok_key = has_byok_key;
                    view.apply_provider_status_snapshot(&provider_status);
                    view.is_overlay = true;
                    view.page = Page::Live;
                    view.expanded = false;
                    view.parent = Some(parent);
                    view.gallery = false;
                    if let Some(native) = &view.native
                        && let Err(error) = native.set_always_on_top(true)
                    {
                        view.error = Some(error);
                    }
                    view
                })
            },
        );
        match result {
            Ok(pair) => {
                let _ = pair.0.update(cx, |_, window, cx| {
                    base::Root::update(window, cx, |root, _, _| {
                        root.style().background = Some(rgba(0).into());
                    });
                });
                self.overlay = Some(pair);
            }
            Err(error) => self.error = Some(format!("Open overlay: {error:#}")),
        }
    }
    pub(super) fn quit(&mut self, cx: &mut Context<Self>) {
        self.cancel_palepale_pairing(cx);
        self.cancel_palepale_checks();
        if self.engine.is_some()
            && (self.preference_saving
                || !self.preference_queue.is_empty()
                || self.settings_ui.provider_save_pending)
        {
            self.quit_after_preferences = true;
            self.status = "Saving preferences before closing…".into();
            return;
        }
        if !self.prepare_dashboard_shutdown(cx) {
            return;
        }
        if self.closing {
            return;
        }
        self.closing = true;
        self.own_windows
            .closing
            .store(true, std::sync::atomic::Ordering::SeqCst);
        if let Ok(requests) = self.own_windows.requests.lock() {
            for (_, cancel) in requests.iter() {
                cancel.cancel();
            }
        }
        self.cancel_request();
        if self.notes_busy || self.saved_question_busy {
            self.status = "Finishing and saving session work before closing…".into();
            return;
        }
        if let Some(engine) = self.engine.clone() {
            engine.cancel_assist();
            self.status = "Saving and releasing audio devices…".into();
            let sender = self.sender.clone();
            self.runtime.spawn(async move {
                let _ = sender.send(Reply::Quit(engine.shutdown().await));
            });
        } else {
            cx.quit();
        }
    }
}

pub(super) fn native_window(window: &Window) -> Option<NativeWindow> {
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => NativeWindow::from_handle(handle.hwnd.get()).ok(),
        _ => None,
    }
}
pub(super) fn capture_screen(
    registry: Arc<WindowRegistry>,
    monitor_point: Option<(i32, i32)>,
    hosted: bool,
) -> Result<String, String> {
    let _capture = registry
        .capture
        .lock()
        .map_err(|_| "Screen capture lock unavailable")?;
    let handles = registry
        .handles
        .lock()
        .map_err(|_| "Window list unavailable")?
        .clone();
    let mut restore = vec![];
    for handle in handles {
        let window = NativeWindow::from_handle(handle)?;
        if let Some(guard) = window.hide_for_capture()? {
            restore.push(guard);
        }
    }
    std::thread::sleep(Duration::from_millis(100));
    let screenshot = skillissue_core::screenshot::capture(monitor_point);
    let errors: Vec<_> = restore
        .iter_mut()
        .filter_map(|guard| guard.restore().err())
        .collect();
    if !errors.is_empty() {
        return Err(format!(
            "Restore skillissue windows after capture: {}",
            errors.join("; ")
        ));
    }
    screenshot
        .and_then(|screenshot| {
            if hosted {
                skillissue_core::screenshot::fit_hosted(screenshot)
            } else {
                Ok(screenshot)
            }
        })
        .map(|screenshot| screenshot.data_url)
}

impl Drop for NativeView {
    fn drop(&mut self) {
        if let Some(cancel) = self.settings_ui.palepale_pairing_cancel.take() {
            if let Some(engine) = &self.engine {
                engine.cancel_palepale_pairing(cancel);
            } else {
                cancel.cancel();
            }
        }
        if let Some(native) = &self.native
            && let Ok(mut handles) = self.own_windows.handles.lock()
        {
            handles.retain(|handle| *handle != native.handle());
        }
    }
}

impl NativeView {
    pub(super) fn show_dashboard(&mut self, cx: &mut Context<Self>) {
        if self.gallery {
            self.preview_hidden = false;
            self.review_id = if self.page == Page::Modes {
                "modes"
            } else {
                "dashboard"
            }
            .into();
            self.reset_dashboard_review();
            self.page = if self.review_id == "modes" {
                Page::Modes
            } else {
                Page::Sessions
            };
            self.modal_backdrop = None;
            self.backdrop_attempted = false;
            cx.notify();
            return;
        }
        if let Some((handle, parent)) = self.parent.clone() {
            let page = if self.page == Page::Modes {
                Page::Modes
            } else {
                Page::Sessions
            };
            let _ = handle.update(cx, |_, window, cx| {
                parent.update(cx, |parent, cx| {
                    parent.select_dashboard_page(
                        if page == Page::Modes { "modes" } else { "home" },
                        window,
                        cx,
                    );
                    if let Some(native) = parent.native {
                        let _ = native.show(true);
                    }
                    cx.notify();
                })
            });
        } else {
            if self.page != Page::Modes {
                self.page = Page::Sessions;
            }
            if let Some(native) = self.native {
                let _ = native.show(true);
            }
        }
        self.overlay_ui.menu = false;
        self.overlay_ui.tools = false;
        if self.is_overlay {
            self.page = Page::Live;
        }
        cx.notify();
    }
}
