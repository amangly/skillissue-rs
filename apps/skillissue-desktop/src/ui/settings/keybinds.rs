use super::*;

impl NativeView {
    pub(in crate::ui) fn set_shortcut_binding(
        &mut self,
        action: &str,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut values = self
            .shortcuts
            .iter()
            .map(|(name, input)| (name.clone(), input.read(cx).value().to_string()))
            .collect::<BTreeMap<_, _>>();
        values.insert(action.into(), value);
        self.apply_shortcut_bindings(values, window, cx)
    }

    pub(in crate::ui) fn apply_shortcut_bindings(
        &mut self,
        mut values: BTreeMap<String, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let validation = skillissue_core::shortcuts::validate(&mut values).and_then(|()| {
            if let Some(manager) = &mut self.hotkeys {
                manager.validate_changes(&self.settings.shortcuts, &values)
            } else {
                Ok(())
            }
        });
        if let Err(error) = validation {
            self.error = Some(error);
            cx.notify();
            return false;
        }
        for (action, value) in values {
            if let Some(input) = self.shortcuts.get(&action) {
                input.update(cx, |input, cx| input.set_value(value, window, cx));
            }
        }
        self.recording_key = None;
        self.error = None;
        self.save_shortcuts(cx);
        self.reconcile_global_shortcuts(cx);
        cx.notify();
        true
    }

    pub(in crate::ui) fn settings_shortcuts(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.settings_palette();
        let mut content = div()
            .track_focus(&self.settings_ui.shortcut_focus)
            .flex()
            .flex_col()
            .gap(px(24.))
            .child(section_title(
                "Keyboard shortcuts",
                "Click to change. Esc cancels; Delete clears. Shortcuts work outside the app.",
                p,
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(div().text_size(px(11.)).text_color(rgb(p.muted)).child(
                        if self.recording_key.is_some() {
                            "Shortcuts paused while recording."
                        } else {
                            "Changes save automatically."
                        },
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .when(self.recording_key.is_some(), |view| {
                                view.child(
                                    Button::new("keybind-cancel")
                                        .ghost()
                                        .small()
                                        .label("Cancel")
                                        .accessibility_label("Cancel shortcut recording")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.recording_key = None;
                                            this.error = None;
                                            this.reconcile_global_shortcuts(cx);
                                            cx.notify();
                                        })),
                                )
                            })
                            .child(
                                Button::new("keybind-reset-all")
                                    .ghost()
                                    .small()
                                    .label("Reset defaults")
                                    .accessibility_label(
                                        "Reset every keyboard shortcut to its default",
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.apply_shortcut_bindings(
                                            default_shortcuts(),
                                            window,
                                            cx,
                                        );
                                    })),
                            ),
                    ),
            );
        for (title, rows) in [
            (
                "General",
                vec![
                    (
                        "toggleVisibility",
                        "Toggle visibility of skillissue",
                        Lucide::Eye,
                    ),
                    (
                        "ask",
                        "Ask skillissue about your screen or audio",
                        Lucide::Sparkles,
                    ),
                    (
                        "clear",
                        "Clear the draft and collapse chat",
                        Lucide::RefreshCw,
                    ),
                    (
                        "toggleSession",
                        "Start or stop a skillissue session",
                        Lucide::Mic,
                    ),
                    ("settings", "Open settings", Lucide::Settings),
                ],
            ),
            (
                "Window",
                vec![
                    ("moveUp", "Move the window position up", Lucide::ArrowUp),
                    (
                        "moveDown",
                        "Move the window position down",
                        Lucide::ArrowDown,
                    ),
                    (
                        "moveLeft",
                        "Move the window position left",
                        Lucide::ArrowLeft,
                    ),
                    (
                        "moveRight",
                        "Move the window position right",
                        Lucide::ArrowRight,
                    ),
                ],
            ),
            (
                "Scroll",
                vec![
                    ("scrollUp", "Scroll the response window up", Lucide::ArrowUp),
                    (
                        "scrollDown",
                        "Scroll the response window down",
                        Lucide::ArrowDown,
                    ),
                ],
            ),
        ] {
            content = content.child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(14.))
                            .line_height(px(21.))
                            .h(px(21.))
                            .mb_1()
                            .child(title),
                    )
                    .children(rows.into_iter().map(|(action, label, icon)| {
                        let value = self
                            .shortcuts
                            .get(action)
                            .map(|input| input.read(cx).value().to_string())
                            .unwrap_or_default();
                        let recording = self.recording_key.as_deref() == Some(action);
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .min_h(px(40.))
                            .child(
                                Button::new(SharedString::from(format!("keybind-row-{action}")))
                                    .debug_selector(move || format!("keybind-row-{action}"))
                                    .ghost()
                                    .accessibility_label(format!("Change shortcut: {label}"))
                                    .min_h(px(40.))
                                    .flex_1()
                                    .min_w_0()
                                    .mx_neg_3()
                                    .px_0()
                                    .py_0()
                                    .rounded_lg()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_3()
                                    .cursor_pointer()
                                    .child(
                                        div()
                                            .w_full()
                                            .min_w_0()
                                            .h_full()
                                            .px(px(12.))
                                            .py_2()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .gap_3()
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_1()
                                                    .items_center()
                                                    .gap(px(12.))
                                                    .min_w_0()
                                                    .text_size(px(13.))
                                                    .line_height(px(18.))
                                                    .font_weight(FontWeight::NORMAL)
                                                    .text_color(rgb(p.foreground))
                                                    .child(
                                                        Icon::new(icon)
                                                            .size(px(18.))
                                                            .flex_shrink_0()
                                                            .text_color(rgb(p.muted)),
                                                    )
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .truncate()
                                                            .child(label),
                                                    ),
                                            )
                                            .child(if recording {
                                                div()
                                                    .flex_shrink_0()
                                                    .text_size(px(11.))
                                                    .text_color(rgb(p.muted))
                                                    .child("Press keys…")
                                                    .into_any_element()
                                            } else {
                                                keycaps(&value, p).into_any_element()
                                            }),
                                    )
                                    .on_click(cx.listener(
                                        move |this, event: &ClickEvent, window, cx| {
                                            if event.click_count() == 2 {
                                                if let Some(default) =
                                                    default_shortcuts().get(action)
                                                {
                                                    this.set_shortcut_binding(
                                                        action,
                                                        default.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                }
                                            } else {
                                                this.settings_ui.shortcut_focus.focus(window, cx);
                                                this.recording_key = if this
                                                    .recording_key
                                                    .as_deref()
                                                    == Some(action)
                                                {
                                                    None
                                                } else {
                                                    Some(action.into())
                                                };
                                                this.error = None;
                                                this.reconcile_global_shortcuts(cx);
                                                cx.notify();
                                            }
                                        },
                                    )),
                            )
                            .child(
                                Button::new(SharedString::from(format!("keybind-reset-{action}")))
                                    .ghost()
                                    .small()
                                    .icon(Lucide::RefreshCw)
                                    .accessibility_label(format!("Reset shortcut: {label}"))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if let Some(default) = default_shortcuts().get(action) {
                                            this.set_shortcut_binding(
                                                action,
                                                default.clone(),
                                                window,
                                                cx,
                                            );
                                        }
                                    })),
                            )
                            .child(
                                Button::new(SharedString::from(format!(
                                    "keybind-unassign-{action}"
                                )))
                                .ghost()
                                .small()
                                .icon(Lucide::X)
                                .disabled(value.is_empty())
                                .accessibility_label(format!("Unassign shortcut: {label}"))
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.set_shortcut_binding(
                                            action,
                                            String::new(),
                                            window,
                                            cx,
                                        );
                                    },
                                )),
                            )
                    })),
            );
        }
        content.into_any_element()
    }
}

#[cfg(all(test, feature = "native-effects"))]
mod recorder_focus_tests {
    use super::super::{NativeView, Page, SettingsTab};
    use gpui_kit::{
        AnyWindowHandle, AppContext, Bounds, Entity, Focusable, InputEvent as _, KeyDownEvent,
        KeyUpEvent, Keystroke, Modifiers, TestAppContext, VisualTestContext, WindowBounds,
        WindowOptions, component::Theme, point, px, size,
    };
    use skillissue_domain::shortcuts::{default_shortcuts, parse};
    use std::{path::PathBuf, sync::Arc};

    struct Fixture {
        handle: AnyWindowHandle,
        view: Entity<NativeView>,
        unused_directory: PathBuf,
    }

    impl Fixture {
        fn new(cx: &mut TestAppContext) -> Self {
            let runtime = Arc::new(
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("inert recorder test runtime"),
            );
            let unused_directory = std::env::temp_dir().join(format!(
                "skillissue-recorder-focus-{}",
                uuid::Uuid::new_v4()
            ));
            let (handle, view) = cx.update(|cx| {
                if !cx.has_global::<Theme>() {
                    gpui_kit::init(cx);
                }
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds {
                            origin: point(px(0.), px(0.)),
                            size: size(px(1100.), px(800.)),
                        })),
                        focus: false,
                        titlebar: None,
                        ..Default::default()
                    },
                    cx,
                    |window, cx| {
                        cx.new(|cx| {
                            let mut view = NativeView::new_for_test(
                                runtime,
                                unused_directory.clone(),
                                window,
                                cx,
                            );
                            view.gallery = false;
                            view.page = Page::Live;
                            view
                        })
                    },
                )
                .expect("inert recorder window")
            });
            let fixture = Self {
                handle,
                view,
                unused_directory,
            };
            fixture.draw(cx);
            cx.update_window(handle, |_, window, cx| {
                fixture.view.update(cx, |view, cx| {
                    view.question.focus_handle(cx).focus(window, cx);
                    view.page = Page::Keybinds;
                    view.settings_ui.tab = SettingsTab::Shortcuts;
                    cx.notify();
                });
            })
            .expect("leave a focused composer while opening keybinds");
            fixture.draw(cx);
            fixture
        }

        fn draw(&self, cx: &mut TestAppContext) {
            cx.update_window(self.handle, |_, window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            })
            .expect("render recorder controls");
            cx.run_until_parked();
        }

        fn click_row(&self, action: &str, cx: &mut TestAppContext) {
            {
                let mut visual = VisualTestContext::from_window(self.handle, cx);
                let selector = match action {
                    "settings" => "keybind-row-settings",
                    "ask" => "keybind-row-ask",
                    "toggleSession" => "keybind-row-toggleSession",
                    _ => panic!("Unsupported fixture row {action}"),
                };
                let bounds = visual.debug_bounds(selector).expect("visible shortcut row");
                visual.simulate_click(bounds.center(), Modifiers::default());
            }
            self.draw(cx);
            cx.update_window(self.handle, |_, window, cx| {
                self.view.update(cx, |view, _| {
                    assert_eq!(view.recording_key.as_deref(), Some(action));
                    assert!(view.settings_ui.shortcut_focus.is_focused(window));
                });
            })
            .expect("row click owns recorder focus");
        }

        fn dispatch(&self, key: Keystroke, cx: &mut TestAppContext) {
            cx.update_window(self.handle, |_, window, cx| {
                window.dispatch_event(
                    KeyDownEvent {
                        keystroke: key.clone(),
                        is_held: false,
                        prefer_character_input: false,
                    }
                    .to_platform_input(),
                    cx,
                );
                window.dispatch_event(KeyUpEvent { keystroke: key }.to_platform_input(), cx);
            })
            .expect("dispatch through the focused render tree");
            self.draw(cx);
        }

        fn binding(&self, action: &str, cx: &TestAppContext) -> String {
            self.view.read_with(cx, |view, cx| {
                view.shortcuts[action].read(cx).value().to_string()
            })
        }

        fn assert_finished(&self, cx: &TestAppContext) {
            self.view.read_with(cx, |view, _| {
                assert!(view.recording_key.is_none());
                assert!(view.error.is_none());
                assert!(view.engine.is_none());
                assert!(view.native.is_none());
                assert!(view.hotkeys.is_none());
            });
            assert!(!self.unused_directory.exists());
        }
    }

    #[gpui_kit::test]
    fn row_click_focuses_recorder_after_composer_is_removed_and_records_custom_chords(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::new(cx);
        for (action, chord) in [("settings", "ctrl-alt-f11"), ("ask", "ctrl-shift-f10")] {
            fixture.click_row(action, cx);
            fixture.dispatch(Keystroke::parse(chord).unwrap(), cx);
            fixture.assert_finished(cx);
            let expected = match action {
                "settings" => "Ctrl+Alt+F11",
                _ => "Ctrl+Shift+F10",
            };
            assert_eq!(
                parse(&fixture.binding(action, cx)).unwrap().id(),
                parse(expected).unwrap().id()
            );
        }
    }

    #[gpui_kit::test]
    fn focused_recorder_accepts_windows_shifted_default_punctuation(cx: &mut TestAppContext) {
        let fixture = Fixture::new(cx);
        for (action, symbol) in [("settings", "<"), ("toggleSession", "|")] {
            fixture.click_row(action, cx);
            let mut key = Keystroke::parse("ctrl-k").unwrap();
            key.key = symbol.into();
            key.modifiers.shift = false;
            fixture.dispatch(key, cx);
            fixture.assert_finished(cx);
            assert_eq!(
                parse(&fixture.binding(action, cx)).unwrap().id(),
                parse(&default_shortcuts()[action]).unwrap().id()
            );
        }
    }

    #[gpui_kit::test]
    fn focused_recorder_routes_escape_and_unassign_through_the_render_tree(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::new(cx);
        let original = fixture.binding("settings", cx);
        fixture.click_row("settings", cx);
        fixture.dispatch(Keystroke::parse("escape").unwrap(), cx);
        fixture.assert_finished(cx);
        assert_eq!(fixture.binding("settings", cx), original);
        fixture.click_row("settings", cx);
        fixture.dispatch(Keystroke::parse("delete").unwrap(), cx);
        fixture.assert_finished(cx);
        assert!(fixture.binding("settings", cx).is_empty());
    }
}
