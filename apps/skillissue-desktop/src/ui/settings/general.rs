use super::*;

impl NativeView {
    pub(in crate::ui) fn toggle_microphone_test(&mut self, cx: &mut Context<Self>) {
        if let Some(stop) = self.settings_ui.microphone_stop.take() {
            stop.store(true, Ordering::Release);
            cx.notify();
            return;
        }
        self.settings_ui.microphone_generation += 1;
        let generation = self.settings_ui.microphone_generation;
        self.settings_ui
            .microphone_levels
            .make_contiguous()
            .fill(0.);
        let stop = Arc::new(AtomicBool::new(false));
        self.settings_ui.microphone_stop = Some(stop.clone());
        self.settings_ui.microphone_status = "Connecting to your microphone...".into();
        let sender = self.settings_ui.microphone_sender.clone();
        self.runtime.spawn_blocking(move || {
            let levels = sender.clone();
            let errors = sender.clone();
            let capture = skillissue_core::audio::AudioCapture::start(
                skillissue_core::audio::CaptureOptions {
                    microphone: true,
                    system_audio: false,
                    chunk_seconds: 8,
                },
                Arc::new(|_| {}),
                Arc::new(move |level| {
                    let _ = levels.send((
                        generation,
                        MicrophoneEvent::Level((level.level * 6.).clamp(0., 1.)),
                    ));
                }),
                Arc::new(move |error| {
                    let _ = errors.send((generation, MicrophoneEvent::Error(error)));
                }),
            );
            match capture {
                Ok(capture) => {
                    let _ = sender.send((
                        generation,
                        MicrophoneEvent::Status(
                            "Speak into your microphone to test the input level.".into(),
                        ),
                    ));
                    while !stop.load(Ordering::Acquire) {
                        std::thread::sleep(Duration::from_millis(30));
                    }
                    capture.stop();
                }
                Err(error) => {
                    let _ = sender.send((generation, MicrophoneEvent::Error(error)));
                }
            }
            let _ = sender.send((generation, MicrophoneEvent::Finished));
        });
        cx.notify();
    }
    pub(in crate::ui) fn settings_general(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.settings_palette();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(section_title(
                "General",
                "Appearance and behavior",
                p,
            ))
            .child(self.update_controls(cx))
            .child(self.settings_row(
                if self.settings.capture_protection {
                    Lucide::EyeOff
                } else {
                    Lucide::Eye
                },
                if self.settings.capture_protection {
                    "Capture protection on"
                } else {
                    "Capture protection off"
                },
                if self.settings.capture_protection {
                    "Hides all windows. Reopen via shortcut or tray. Capture exclusion depends on the recorder."
                } else {
                    "The overlay may appear in screen captures"
                },
                self.setting_switch(
                    "settings-capture-protection",
                    "Capture protection",
                    self.settings.capture_protection,
                    |s, v| s.capture_protection = v,
                    cx,
                ),
            ))
            .child(self.settings_row(
                Lucide::Sparkles,
                "Ambient chat",
                "Chat with skillissue outside meetings",
                self.setting_switch(
                    "settings-ambient",
                    "Ambient chat",
                    self.settings.ambient_enabled,
                    |s, v| s.ambient_enabled = v,
                    cx,
                ),
            ))
            .child(self.settings_row(
                if self.settings.theme == "dark" {
                    Lucide::Moon
                } else {
                    Lucide::Sun
                },
                "Theme",
                "Light, dark, or system",
                self.settings_select(
                    "settings-theme",
                    &self.settings.theme,
                    &[("system", "System"), ("light", "Light"), ("dark", "Dark")],
                    "theme",
                    cx,
                ),
            ))
            .child(
                div()
                    .mt_3()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(section_title(
                        "Audio",
                        "Local microphone test. No audio is sent.",
                        p,
                    ))
                    .when(!self.supports_audio_input(), |view| {
                        view.child(caption(self.hosted_audio_unavailable_message(), p))
                    })
                    .child(
                        div()
                            .p_4()
                            .rounded(px(8.))
                            .bg(rgb(p.control))
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_4()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_size(px(13.))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child("Microphone"),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .text_size(px(12.))
                                                    .text_color(rgb(p.muted))
                                                    .child(Icon::new(Lucide::Mic).size(px(14.)))
                                                    .child("Default microphone"),
                                            ),
                                    )
                                    .child(
                                        settings_action(
                                            "settings-microphone-test",
                                            if self.settings_ui.microphone_stop.is_some() {
                                                "Stop test"
                                            } else {
                                                "Test mic"
                                            },
                                            p,
                                        )
                                        .debug_selector(|| "settings-microphone-test".into())
                                        .on_click(
                                            cx.listener(|this, _, _, cx| {
                                                this.toggle_microphone_test(cx)
                                            }),
                                        ),
                                    ),
                            )
                            .when(self.settings_ui.microphone_stop.is_some(), |view| {
                                view.child(
                                    div()
                                        .p_3()
                                        .rounded_lg()
                                        .border_1()
                                        .border_color(rgb(p.line))
                                        .flex()
                                        .flex_col()
                                        .gap_2()
                                        .child(
                                            div()
                                                .h(px(56.))
                                                .flex()
                                                .items_end()
                                                .justify_center()
                                                .gap_1()
                                                .children(
                                                    self.settings_ui.microphone_levels.iter().map(
                                                        |level| {
                                                            div()
                                                                .w(px(6.))
                                                                .h(px(8. + level * 28.))
                                                                .rounded_full()
                                                                .bg(rgb(0x6b95c9))
                                                        },
                                                    ),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(12.))
                                                .text_color(rgb(p.muted))
                                                .text_center()
                                                .child(self.settings_ui.microphone_status.clone()),
                                        ),
                                )
                            }),
                    ),
            )
            .child(section_title("History", "Local backup and restore", p))
            .child(self.settings_row(
                Lucide::Download,
                "Restore saved history",
                "Restore into empty history. Stop recording and finish pending work first.",
                Button::new("settings-restore-history").label("Restore")
                    .disabled(self.engine.is_none() || self.busy || self.snapshot.active || self.history_transfer_pending)
                    .on_click(cx.listener(|this, _, _, cx| this.restore_history(cx))),
            ))
            .child(self.settings_row(
                Lucide::Download,
                "Back up saved history",
                "Back up saved sessions to a new file. Settings, documents, keys, and unfinished work are excluded.",
                Button::new("settings-backup-history")
                    .label("Back up")
                    .disabled(self.engine.is_none() || self.history_transfer_pending)
                    .on_click(cx.listener(|this, _, _, cx| this.backup_history(cx))),
            ))
            .child({
                let summary = self.latency.summary.as_ref();
                let metric = |value: Option<u64>| {
                    value.map_or_else(|| "—".to_owned(), |ms| format!("{ms} ms"))
                };
                self.settings_row(
                    Lucide::Info,
                    "Recent latency",
                    "Last 256 timings. No prompts or audio stored here.",
                    div()
                        .flex()
                        .flex_col()
                        .items_end()
                        .gap(px(2.))
                        .text_size(px(11.))
                        .text_color(rgb(p.muted))
                        .child(format!(
                            "capture {} · first token {} · display {}",
                            metric(summary.as_ref().and_then(|s| s.capture_ms)),
                            metric(summary.as_ref().and_then(|s| s.first_token_ms)),
                            metric(summary.as_ref().and_then(|s| s.display_ms)),
                        ))
                        .child(format!(
                            "{} requests · {} transcriptions · total {}",
                            summary.as_ref().map_or(0, |s| s.requests),
                            summary.as_ref().map_or(0, |s| s.transcriptions),
                            metric(summary.as_ref().and_then(|s| s.transcription_ms)),
                        )),
                )
            })
            .when(self.settings.capture_protection, |view| {
                view.child(self.settings_row(
                    Lucide::Monitor,
                    "Keep focus in other apps",
                    "Show the overlay without interrupting typing.",
                    self.setting_switch("settings-no-focus", "Keep focus in other apps",
                        self.settings.no_focus, |s, v| s.no_focus = v, cx),
                ))
            })
            .into_any_element()
    }
    pub(in crate::ui) fn settings_language(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.settings_palette();
        div()
            .flex()
            .flex_col()
            .gap_6()
            .child(section_title(
                "Language",
                "Select the language you want to use for your meetings.",
                p,
            ))
            .child(self.settings_row(
                Lucide::Mic,
                "Transcription language",
                "Spoken language. Change between sessions.",
                self.settings_select(
                    "transcription-language",
                    &self.settings.language,
                    LANGUAGES,
                    "language",
                    cx,
                ),
            ))
            .child(self.settings_row(
                Lucide::Languages,
                "Output language",
                "Language for new answers and notes.",
                self.settings_select(
                    "output-language",
                    &self.settings.output_language,
                    &[
                        ("en-US", "English"),
                        ("fr-FR", "French (Français)"),
                        ("de-DE", "German (Deutsch)"),
                        ("it-IT", "Italian (Italiano)"),
                        ("pt-PT", "Portuguese (Português)"),
                        ("es-ES", "Spanish (Español)"),
                    ],
                    "output",
                    cx,
                ),
            ))
            .into_any_element()
    }
}

#[cfg(all(test, feature = "native-effects"))]
mod tests {
    use super::{NativeView, Page, SettingsTab};
    use gpui_kit::{
        AppContext, Bounds, Modifiers, TestAppContext, VisualTestContext, WindowBounds,
        WindowOptions, component::Theme, point, px, size,
    };
    use skillissue_domain::{models::ProviderMode, provider::ProviderModel};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[gpui_kit::test]
    fn local_microphone_test_stop_remains_clickable_for_text_only_palepale(
        cx: &mut TestAppContext,
    ) {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("inert microphone test runtime"),
        );
        let directory =
            std::env::temp_dir().join(format!("skillissue-microphone-ui-{}", uuid::Uuid::new_v4()));
        let stop = Arc::new(AtomicBool::new(false));
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
                        let mut view =
                            NativeView::new_for_test(runtime, directory.clone(), window, cx);
                        view.gallery = false;
                        view.page = Page::Settings;
                        view.settings_ui.tab = SettingsTab::General;
                        view.settings.provider_mode = ProviderMode::Palepale;
                        view.settings.palepale.chat_model = "text-only".into();
                        view.settings_ui.catalog_models = vec![ProviderModel {
                            id: "text-only".into(),
                            capabilities: skillissue_domain::provider::ModelCapabilities {
                                streaming: true,
                                ..Default::default()
                            },
                            ..Default::default()
                        }];
                        view.settings_ui.microphone_stop = Some(stop.clone());
                        view
                    })
                },
            )
            .expect("inert microphone test window")
        });
        cx.update_window(handle, |_, window, cx| window.draw(cx).clear(cx))
            .expect("draw microphone settings");
        {
            let mut visual = VisualTestContext::from_window(handle, cx);
            let button = visual
                .debug_bounds("settings-microphone-test")
                .expect("local microphone control remains visible");
            visual.simulate_click(button.center(), Modifiers::default());
        }
        view.read_with(cx, |view, _| {
            assert!(!view.supports_audio_input());
            assert!(view.settings_ui.microphone_stop.is_none());
            assert!(view.engine.is_none());
            assert!(view.error.is_none());
        });
        assert!(stop.load(Ordering::Acquire));
        assert!(!directory.exists());
    }
}
