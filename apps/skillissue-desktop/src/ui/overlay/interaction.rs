use super::*;

fn should_follow_response(offset: Pixels, previous_max: Pixels, current_max: Pixels) -> bool {
    // Compare against the preceding extent: a newly laid-out token can increase
    // the current extent before the timer sees it, without any user scrolling.
    offset + previous_max.min(current_max) <= px(32.)
}

impl NativeView {
    pub(in crate::ui) fn overlay_audio_level(&mut self, level: skillissue_core::audio::AudioLevel) {
        self.overlay_ui.levels[usize::from(level.source != "microphone")] =
            level.level.clamp(0., 1.);
    }
    pub(in crate::ui) fn overlay_tick(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.is_overlay && self.page != Page::Live && !self.gallery {
            return false;
        }
        let mut changed = false;
        if self
            .overlay_ui
            .copied_at
            .is_some_and(|at| at.elapsed() >= Duration::from_secs(2))
        {
            self.overlay_ui.copied = None;
            self.overlay_ui.copied_at = None;
            changed = true;
        }
        let signature = (self.snapshot.messages.len(), self.stream.len());
        let follow_response = should_follow_response(
            self.scroll.offset().y,
            self.overlay_ui.previous_scroll_max,
            self.scroll.max_offset().y,
        );
        self.overlay_ui.previous_scroll_max = self.scroll.max_offset().y;
        if signature != self.overlay_ui.previous_content
            || self.busy != self.overlay_ui.previous_busy
        {
            if signature.0 > self.overlay_ui.previous_content.0
                || (self.busy && !self.overlay_ui.previous_busy)
            {
                self.expanded = true;
                if self.is_overlay && self.overlay_ui.chat_shown && self.shown {
                    window.resize(size(px(560.), px(550.)));
                }
            }
            if follow_response {
                self.scroll.scroll_to_bottom();
            }
            self.overlay_ui.previous_content = signature;
            self.overlay_ui.previous_busy = self.busy;
            changed = true;
        }
        if !self.snapshot.active {
            self.overlay_ui.levels = [0.; 2];
        }
        let active = window.is_window_active();
        if self.overlay_ui.was_active && !active && (self.overlay_ui.menu || self.overlay_ui.tools)
        {
            if !self
                .overlay_ui
                .preserve_blur_until
                .is_some_and(|until| Instant::now() < until)
            {
                self.close_overlay_popups();
                changed = true;
            }
            self.overlay_ui.preserve_blur_until = None;
        }
        self.overlay_ui.was_active = active;
        if changed {
            cx.notify();
        }
        changed
    }
    pub(in crate::ui) fn overlay_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.is_overlay && self.page != Page::Live && !self.gallery {
            return false;
        }
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            if self.overlay_ui.menu || self.overlay_ui.tools {
                self.close_overlay_popups();
                cx.notify();
                return true;
            }
            if self.overlay_ui.focused {
                window.blur(cx);
                self.restore_focus_mode();
                return true;
            }
            return false;
        }
        if event.keystroke.modifiers.modified() {
            return false;
        }
        if self.overlay_ui.menu {
            let count = if self.settings.capture_protection {
                4
            } else {
                3
            };
            if self.overlay_ui.modes {
                let modes = self.settings.modes.len() + 2;
                match key {
                    "down" => {
                        self.overlay_ui.mode_selection =
                            (self.overlay_ui.mode_selection + 1) % modes
                    }
                    "up" => {
                        self.overlay_ui.mode_selection =
                            (self.overlay_ui.mode_selection + modes - 1) % modes
                    }
                    "left" => self.overlay_ui.modes = false,
                    "return" | "enter" => {
                        let selected = self.overlay_ui.mode_selection.min(modes - 1);
                        if selected == modes - 1 {
                            self.close_overlay_popups();
                            self.page = Page::Modes;
                            self.show_dashboard(cx);
                        } else {
                            let id = if selected == 0 {
                                "general".into()
                            } else {
                                self.settings.modes[selected - 1].id.clone()
                            };
                            self.change_preference(move |settings| settings.active_mode_id = id);
                            self.close_overlay_popups();
                        }
                    }
                    _ => return false,
                }
            } else {
                match key {
                    "down" => {
                        self.overlay_ui.menu_selection = Some(
                            self.overlay_ui
                                .menu_selection
                                .map_or(0, |index| (index + 1) % count),
                        )
                    }
                    "up" => {
                        self.overlay_ui.menu_selection = Some(
                            self.overlay_ui
                                .menu_selection
                                .map_or(count - 1, |index| (index + count - 1) % count),
                        )
                    }
                    "right" => {
                        self.overlay_ui.menu_selection = Some(count - 1);
                        self.overlay_ui.modes = true;
                        self.overlay_ui.mode_selection = 0;
                    }
                    "return" | "enter" => {
                        if let Some(index) = self.overlay_ui.menu_selection {
                            self.activate_overlay_menu(index, cx);
                        }
                    }
                    _ => return false,
                }
            }
            cx.notify();
            return true;
        }
        if self.overlay_ui.tools {
            match key {
                "down" => {
                    self.overlay_ui.tool_selection = Some(
                        self.overlay_ui
                            .tool_selection
                            .map_or(0, |index| (index + 1) % 7),
                    )
                }
                "up" => {
                    self.overlay_ui.tool_selection = Some(
                        self.overlay_ui
                            .tool_selection
                            .map_or(6, |index| (index + 6) % 7),
                    )
                }
                "return" | "enter" => {
                    if let Some(index) = self.overlay_ui.tool_selection {
                        self.activate_overlay_tool(
                            [
                                "dashboard",
                                "transcript",
                                "capture",
                                "export",
                                "clear",
                                "provider",
                                "hide",
                            ][index],
                            window,
                            cx,
                        );
                    }
                }
                _ => return false,
            }
            cx.notify();
            return true;
        }
        false
    }
    pub(in crate::ui) fn close_overlay_popups(&mut self) {
        self.overlay_ui.menu = false;
        self.overlay_ui.modes = false;
        self.overlay_ui.tools = false;
        self.overlay_ui.menu_selection = None;
        self.overlay_ui.tool_selection = None;
    }
    pub(in crate::ui) fn activate_overlay_menu(&mut self, index: usize, cx: &mut Context<Self>) {
        if index == 0 {
            self.close_overlay_popups();
            self.open_settings("keybinds", cx);
            return;
        }
        let protected = self.settings.capture_protection;
        match index {
            1 => self.change_preference(|settings| {
                settings.capture_protection = !settings.capture_protection
            }),
            2 if protected => {
                if !self.settings.no_focus {
                    self.overlay_ui.preserve_blur_until =
                        Some(Instant::now() + Duration::from_secs(1));
                }
                self.change_preference(|settings| settings.no_focus = !settings.no_focus);
            }
            _ => {
                self.overlay_ui.modes = !self.overlay_ui.modes;
                self.overlay_ui.mode_selection = 0;
            }
        }
    }
    pub(in crate::ui) fn activate_overlay_tool(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            "dashboard" => self.show_dashboard(cx),
            "transcript" => self.overlay_ui.transcript = true,
            "capture" => self.capture_preview(),
            "export" => self.export_session(cx),
            "clear" => {
                self.clear_conversation();
            }
            "provider" => self.open_settings("provider", cx),
            "hide" => {
                if self.gallery {
                    self.select_review("control", window, cx);
                    self.preview_hidden = self.settings.hide_widget;
                } else {
                    self.shown = false;
                    if let Some(native) = self.native {
                        let _ = native.hide();
                    }
                }
            }
            _ => {}
        }
        self.overlay_ui.tools = false;
    }
    pub(in crate::ui) fn toggle_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.gallery {
            if !self.overlay_ui.chat_shown
                && catalog::screen(&self.review_id).is_some_and(|screen| screen.view == "control")
            {
                self.select_review("chat", window, cx);
                return;
            }
            if self.overlay_ui.chat_shown && self.settings.hide_widget {
                self.select_review("control", window, cx);
                self.preview_hidden = true;
                return;
            }
        }
        if self.overlay_ui.chat_shown && self.settings.hide_widget && self.is_overlay {
            self.shown = false;
            if let Some(native) = self.native {
                let _ = native.hide();
            }
        }
        self.overlay_ui.chat_shown = !self.overlay_ui.chat_shown;
        if !self.overlay_ui.chat_shown {
            self.close_overlay_popups();
            self.overlay_ui.transcript = false;
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
        cx.notify();
    }
}

#[cfg(test)]
mod scroll_tests {
    use super::should_follow_response;
    use gpui_kit::px;

    #[test]
    fn follows_growth_but_preserves_reading_position() {
        assert!(should_follow_response(px(-600.), px(600.), px(850.)));
        assert!(!should_follow_response(px(-200.), px(600.), px(850.)));
        assert!(should_follow_response(px(-830.), px(850.), px(850.)));
        assert!(should_follow_response(px(0.), px(0.), px(0.)));
        assert!(should_follow_response(px(-200.), px(850.), px(200.)));
    }
}
