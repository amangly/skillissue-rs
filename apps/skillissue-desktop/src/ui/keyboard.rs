//! Native keyboard coordination.
use super::*;

impl NativeView {
    pub(super) fn record_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_modal_open() {
            if self.settings_key(event, window, cx) {
                cx.stop_propagation();
                window.prevent_default();
            }
            return;
        }
        if self.recording_key.is_none() && self.overlay_key(event, window, cx) {
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        if self.recording_key.is_none() && self.dashboard_key(event, window, cx) {
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        let Some(action) = self.recording_key.clone() else {
            return;
        };
        cx.stop_propagation();
        window.prevent_default();
        if event.is_held || event.prefer_character_input {
            return;
        }
        if event.keystroke.key == "escape" {
            self.recording_key = None;
            self.reconcile_global_shortcuts(cx);
            cx.notify();
            return;
        }
        let candidate = if !event.keystroke.modifiers.modified()
            && matches!(event.keystroke.key.as_str(), "backspace" | "delete")
        {
            Some(String::new())
        } else {
            accelerator(&event.keystroke)
        };
        let Some(value) = candidate else {
            return;
        };
        self.set_shortcut_binding(&action, value, window, cx);
        cx.notify();
    }
    pub(super) fn save_shortcuts(&mut self, cx: &mut Context<Self>) {
        let mut settings = self.settings.clone();
        settings.shortcuts = self
            .shortcuts
            .iter()
            .map(|(name, input)| (name.clone(), input.read(cx).value().to_string()))
            .collect();
        self.change_preference(move |current| current.shortcuts = settings.shortcuts);
    }

    pub(super) fn dispatch(
        &mut self,
        action: ShortcutAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            ShortcutAction::Settings => {
                self.open_settings("general", cx);
                if !self.is_overlay
                    && let Some(native) = self.native
                    && let Err(error) = native.show(true)
                {
                    self.error = Some(error);
                }
            }
            ShortcutAction::ToggleSession => self.toggle_capture(),
            _ => {
                if self.is_overlay || self.gallery {
                    self.overlay_action(action, window, cx);
                    return;
                }
                if self.overlay.is_none() {
                    if action == ShortcutAction::Clear {
                        self.overlay_action(action, window, cx);
                        return;
                    }
                    self.show_overlay(cx);
                    if action == ShortcutAction::ToggleVisibility {
                        return;
                    }
                }
                if let Some((handle, view)) = self.overlay.clone() {
                    let _ = handle.update(cx, |_, window, cx| {
                        view.update(cx, |view, cx| view.overlay_action(action, window, cx))
                    });
                }
            }
        }
        cx.notify();
    }
    pub(super) fn overlay_action(
        &mut self,
        action: ShortcutAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            ShortcutAction::Ask => {
                if self.reveal_overlay_window(true, window, cx) {
                    self.ask(window, cx);
                }
            }
            ShortcutAction::Clear => {
                self.request_draft.clear();
                self.clear_audio_retry();
                self.invalidate_capture_preview();
                self.cancel_request();
                self.error = None;
                self.question
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.stream.clear();
                self.expanded = false;
                if self.is_overlay {
                    window.resize(size(
                        px(if self.overlay_ui.chat_shown {
                            560.
                        } else {
                            183.
                        }),
                        px(if self.overlay_ui.chat_shown {
                            style::OVERLAY_COLLAPSED_HEIGHT
                        } else {
                            70.
                        }),
                    ));
                }
            }
            ShortcutAction::ToggleVisibility => {
                let visible = self.native.map(|native| native.is_visible()).transpose();
                let visible = match visible {
                    Ok(visible) => visible.unwrap_or(self.shown),
                    Err(error) => {
                        self.error = Some(error);
                        cx.notify();
                        return;
                    }
                };
                if !self.shown || !visible || self.preview_hidden {
                    self.reveal_overlay_window(true, window, cx);
                } else {
                    if self.is_overlay
                        && let Some(native) = self.native
                        && let Err(error) = native.set_always_on_top(true)
                    {
                        self.error = Some(error);
                        cx.notify();
                        return;
                    }
                    self.toggle_chat(window, cx);
                }
            }
            ShortcutAction::ScrollUp | ShortcutAction::ScrollDown => {
                if !self.reveal_overlay_window(true, window, cx) {
                    return;
                }
                let offset = self.scroll.offset();
                self.scroll.set_offset(point(
                    offset.x,
                    offset.y
                        + px(if action == ShortcutAction::ScrollUp {
                            240.
                        } else {
                            -240.
                        }),
                ));
            }
            ShortcutAction::MoveUp
            | ShortcutAction::MoveDown
            | ShortcutAction::MoveLeft
            | ShortcutAction::MoveRight => {
                if !self.reveal_overlay_window(false, window, cx) {
                    return;
                }
                if let Some(native) = &self.native
                    && let Err(error) = native.move_action(action)
                {
                    self.error = Some(error);
                }
            }
            _ => {}
        }
        cx.notify();
    }
}

pub(super) fn accelerator(key: &Keystroke) -> Option<String> {
    // GPUI Windows folds Shift into punctuation and clears the modifier bit.
    let (key_name, inferred_shift) = match key.key.as_str() {
        "control" | "alt" | "shift" | "super" | "fn" => return None,
        "return" => ("Enter", false),
        " " => ("Space", false),
        "~" => ("Backquote", true),
        "!" => ("Digit1", true),
        "@" => ("Digit2", true),
        "#" => ("Digit3", true),
        "$" => ("Digit4", true),
        "%" => ("Digit5", true),
        "^" => ("Digit6", true),
        "&" => ("Digit7", true),
        "*" => ("Digit8", true),
        "(" => ("Digit9", true),
        ")" => ("Digit0", true),
        "_" => ("Minus", true),
        "+" => ("Equal", true),
        "{" => ("BracketLeft", true),
        "}" => ("BracketRight", true),
        "|" => ("Backslash", true),
        ":" => ("Semicolon", true),
        "\"" => ("Quote", true),
        "<" => ("Comma", true),
        ">" => ("Period", true),
        "?" => ("Slash", true),
        name => (name, false),
    };
    let mut parts = vec![];
    if key.modifiers.control {
        parts.push("Ctrl");
    }
    if key.modifiers.alt {
        parts.push("Alt");
    }
    if key.modifiers.shift || inferred_shift {
        parts.push("Shift");
    }
    if key.modifiers.platform {
        parts.push("Super");
    }
    parts.push(key_name);
    let value = parts.join("+");
    skillissue_core::shortcuts::parse(&value)
        .ok()
        .map(|_| value)
}
