use super::*;

impl NativeView {
    pub(in crate::ui) fn render_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.captured_screen.is_none() {
            self.overlay_ui.screenshot.replace(None);
        }
        let dark = self.is_dark(cx);
        let ink = if dark { 0xe6efff } else { 0x263246 };
        let secondary = if dark { 0xa8bddb } else { 0x61748f };
        let panel = if dark { 0x213047eb } else { 0xeaf0f8eb };
        let rim = if dark { 0xd9eaff38 } else { 0xffffffc7 };
        let review = self
            .gallery
            .then(|| catalog::screen(&self.review_id))
            .flatten();
        let control_only =
            review.is_some_and(|screen| screen.view == "control") || !self.overlay_ui.chat_shown;
        let page = review.and_then(|screen| screen.page).unwrap_or("");
        let sample = matches!(page, "sample" | "metadata");
        let has_error = page == "error" || self.error.is_some();
        let expanded = self.expanded
            || sample
            || has_error
            || self.overlay_ui.audio_notice.is_some()
            || self.context.pending_transcriptions > 0;
        let protected = self.settings.capture_protection;
        let messages = if sample {
            vec![Message { id: "review-user".into(), role: "user".into(), content: "Summarize the decisions from this conversation.".into(), created_at: 1000 }, Message { id: "review-assistant".into(), role: "assistant".into(), content: "**Key decisions**\n\nKeep the first version small: a floating control widget, contextual chat, and a local session transcript.\n\nSave session history locally, choose your own provider, and attach documents to a mode when they are relevant.".into(), created_at: 2000 }]
        } else {
            self.snapshot.messages.clone()
        };
        let content = div()
            .relative()
            .size_full()
            .max_w(px(540.))
            .mx_auto()
            .font_family("Segoe UI")
            .text_color(rgb(ink))
            .text_size(px(14.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.))
            .px(px(2.))
            .child(track_overlay_bounds(self.overlay_ui.root_bounds.clone()))
            .child(
                div()
                    .relative()
                    .w_full()
                    .max_w(px(540.))
                    .h(px(50.))
                    .flex_shrink_0()
                    .flex()
                    .justify_start()
                    .when(!self.gallery, |view| {
                        view.child(
                            div()
                                .debug_selector(|| "overlay-toolbar-drag-area".into())
                                .absolute()
                                .inset_0()
                                .cursor(CursorStyle::OpenHand)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, window, cx| {
                                        this.drag_overlay(window, cx)
                                    }),
                                ),
                        )
                    })
                    .child(self.overlay_control(control_only, dark, cx)),
            )
            .when(!control_only, |view| {
                view.child(
                    div()
                        .relative()
                        .w_full()
                        .max_w(px(540.))
                        .flex_shrink_0()
                        .when(expanded, |view| view.flex_1().min_h(px(220.)))
                        .when(!expanded, |view| {
                            view.h(px(if protected { 176. } else { 164. }))
                        })
                        .when(protected, |view| view.child(protected_outline(dark)))
                        .child(
                            div()
                                .absolute()
                                .inset(px(2.))
                                .rounded(px(8.))
                                .border_1()
                                .border_color(cx.theme().border)
                                .bg(cx.theme().popover)
                                .overflow_hidden()
                                .group("overlay-chat")
                                .debug_selector(|| "overlay-chat-panel".into())
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .debug_selector(|| "overlay-provider-label".into())
                                        .w_full()
                                        .h(px(24.))
                                        .flex_shrink_0()
                                        .when(!self.gallery, |view| {
                                            view.cursor(CursorStyle::OpenHand).on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|this, _, window, cx| {
                                                    this.drag_overlay(window, cx)
                                                }),
                                            )
                                        })
                                        .px(px(14.))
                                        .flex()
                                        .justify_end()
                                        .items_center()
                                        .text_size(px(10.))
                                        .text_color(rgb(secondary))
                                        .child(self.provider_label()),
                                )
                                .when(expanded, |view| {
                                    view.child(
                                        overlay_icon_button(
                                            "expanded-hide-chat",
                                            Glyph::X,
                                            16.,
                                            secondary,
                                        )
                                        .absolute()
                                        .left(px(8.))
                                        .top(px(8.))
                                        .w(px(28.))
                                        .h(px(28.))
                                        .opacity(0.)
                                        .group_hover("overlay-chat", |view| view.opacity(1.))
                                        .accessibility_label("Hide chat")
                                        .on_click(
                                            cx.listener(|this, _, window, cx| {
                                                this.toggle_chat(window, cx)
                                            }),
                                        ),
                                    )
                                })
                                .child(div().relative().flex_1().min_h_0().when(expanded, |view| {
                                    view.pt_3().child(
                                        div()
                                            .id("chat-message-viewport")
                                            .size_full()
                                            .min_h_0()
                                            .overflow_y_scroll()
                                            .track_scroll(&self.scroll)
                                            .px(px(6.))
                                            .children(messages.iter().map(|message| {
                                                self.overlay_message(
                                                    message,
                                                    page == "metadata",
                                                    ink,
                                                    secondary,
                                                    cx,
                                                )
                                            }))
                                            .when(self.busy, |view| {
                                                if self.stream.is_empty() {
                                                    let dots = ".".repeat((self.ticks / 4) % 4);
                                                    view.child(
                                                        div()
                                                            .id("chat-thinking")
                                                            .px_3()
                                                            .py_3()
                                                            .flex()
                                                            .items_center()
                                                            .gap(px(7.))
                                                            .text_size(px(13.))
                                                            .text_color(rgb(secondary))
                                                            .child(
                                                                div()
                                                                    .w(px(7.))
                                                                    .h(px(7.))
                                                                    .rounded_full()
                                                                    .bg(overlay_blue()),
                                                            )
                                                            .child(format!("Thinking{dots}")),
                                                    )
                                                } else {
                                                    view.child(div().px_3().py_3().child(markdown(
                                                        "chat-stream",
                                                        self.stream.clone(),
                                                    )))
                                                }
                                            }),
                                    )
                                }))
                                .child(self.overlay_suggestions(secondary, cx))
                                .when_some(self.overlay_ui.audio_notice.as_ref(), |view, notice| {
                                    view.child(
                                        div()
                                            .id("overlay-audio-notice")
                                            .debug_selector(|| "overlay-audio-notice".into())
                                            .flex_shrink_0()
                                            .px_3()
                                            .py_1()
                                            .text_size(px(11.))
                                            .text_color(rgb(secondary))
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(8.))
                                                    .child(div().flex_1().child(notice.clone()))
                                                    .when(self.audio_retry_prompt.is_some() && !self.busy, |view| {
                                                        view.child(
                                                            Button::new("overlay-retry-completed-audio")
                                                                .debug_selector(|| "overlay-retry-completed-audio".into())
                                                                .ghost()
                                                                .xsmall()
                                                                .label("Ask again")
                                                                .flex_shrink_0()
                                                                .disabled(!self.can_retry_completed_audio(cx))
                                                                .on_click(cx.listener(|this, _, window, cx| {
                                                                    this.retry_with_completed_audio(window, cx);
                                                                })),
                                                        )
                                                    }),
                                            ),
                                    )
                                })
                                .when(self.context.pending_transcriptions > 0, |view| {
                                    view.child(
                                        div()
                                            .id("overlay-audio-pending")
                                            .debug_selector(|| "overlay-audio-pending".into())
                                            .flex_shrink_0()
                                            .px_3()
                                            .py_1()
                                            .text_size(px(11.))
                                            .text_color(rgb(secondary))
                                            .child(format!(
                                                "Transcribing {} audio segments…",
                                                self.context.pending_transcriptions
                                            )),
                                    )
                                })
                                .child(self.overlay_composer(expanded, dark, cx))
                                .when(has_error, |view| view.child(self.overlay_error(cx))),
                        ),
                )
            })
            .when(!control_only, |view| {
                view.child(
                    div().absolute().right(px(10.)).top(px(9.)).child(
                        overlay_icon_button(
                            "session-tools",
                            Glyph::Ellipsis,
                            15.,
                            if dark { 0xb1c7e6 } else { 0x75859d },
                        )
                        .w(px(32.))
                        .h(px(32.))
                        .debug_selector(|| "session-tools-trigger".into())
                        .child(track_overlay_bounds(self.overlay_ui.tools_trigger.clone()))
                        .accessibility_label("Session tools")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.overlay_ui.tools = !this.overlay_ui.tools;
                            this.overlay_ui.tool_selection = None;
                            this.overlay_ui.menu = false;
                            if this.overlay_ui.tools && this.is_overlay {
                                this.expanded = true;
                                window.resize(size(px(560.), px(550.)));
                            }
                            cx.notify();
                        })),
                    ),
                )
            })
            .when(self.overlay_ui.menu, |view| {
                view.child(self.overlay_menu(cx))
            })
            .when(self.overlay_ui.tools, |view| {
                view.child(self.overlay_tools(cx))
            })
            .when(self.overlay_ui.transcript, |view| {
                view.child(
                    div()
                        .absolute()
                        .left(px(10.))
                        .right(px(10.))
                        .top(px(62.))
                        .bottom(px(10.))
                        .rounded(px(16.))
                        .bg(rgba(panel))
                        .border_1()
                        .border_color(rgba(rim))
                        .p_4()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .items_center()
                                .child("Session transcript")
                                .child(
                                    overlay_icon_button(
                                        "close-transcript",
                                        Glyph::X,
                                        16.,
                                        secondary,
                                    )
                                    .accessibility_label("Close transcript")
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            this.overlay_ui.transcript = false;
                                            cx.notify();
                                        },
                                    )),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .text_size(px(11.))
                                .text_color(rgb(secondary))
                                .child(div().size(px(6.)).rounded_full().bg(rgb(
                                    if self.snapshot.active {
                                        0x65a30d
                                    } else {
                                        0x71717a
                                    },
                                )))
                                .child(if self.context.pending_transcriptions > 0 {
                                    format!(
                                        "Transcribing {} segments",
                                        self.context.pending_transcriptions
                                    )
                                } else if self.snapshot.active {
                                    "Listening".into()
                                } else {
                                    "Session stopped".into()
                                })
                                .child(div().flex_1())
                                .child(div().text_size(px(10.)).child(format!(
                                    "Microphone {}% · System {}%",
                                    (self.overlay_ui.levels[0] * 100.).round(),
                                    (self.overlay_ui.levels[1] * 100.).round()
                                ))),
                        )
                        .child(
                            div()
                                .id("overlay-transcript")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .children(self.snapshot.transcript.iter().map(|entry| {
                                    div()
                                        .py_2()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_size(px(11.))
                                                .text_color(rgb(secondary))
                                                .child(if entry.source == "microphone" {
                                                    "Me"
                                                } else {
                                                    "Them"
                                                }),
                                        )
                                        .child(markdown(
                                            SharedString::from(format!("drawer-{}", entry.id)),
                                            entry.text.clone(),
                                        ))
                                }))
                                .when(self.snapshot.transcript.is_empty(), |view| {
                                    view.child("No transcript yet.")
                                }),
                        )
                        .when_some(self.captured_screen.as_ref(), |view, data| {
                            view.child(self.overlay_screenshot(data, secondary))
                        }),
                )
            });
        div()
            .relative()
            .size_full()
            .when(!self.gallery, |view| view.p(px(10.)))
            .child(content)
            .when(!self.gallery, |view| {
                view.children((0..4).map(|edge| {
                    div()
                        .debug_selector(move || format!("overlay-drag-edge-{edge}"))
                        .absolute()
                        .cursor(CursorStyle::OpenHand)
                        .when(edge == 0, |view| view.top_0().left_0().right_0().h(px(10.)))
                        .when(edge == 1, |view| {
                            view.bottom_0().left_0().right_0().h(px(10.))
                        })
                        .when(edge == 2, |view| {
                            view.top(px(10.)).bottom(px(10.)).left_0().w(px(10.))
                        })
                        .when(edge == 3, |view| {
                            view.top(px(10.)).bottom(px(10.)).right_0().w(px(10.))
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| this.drag_overlay(window, cx)),
                        )
                }))
            })
            .into_any_element()
    }
    pub(in crate::ui) fn overlay_control(
        &self,
        control_only: bool,
        dark: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let gray_ink = if dark { 0xdbe8fc } else { 0x506887 };
        div()
            .id("control-widget")
            .relative()
            .when(control_only, |view| view.w(px(163.)))
            .when(!control_only, |view| view.w_full().max_w(px(540.)))
            .h(px(50.))
            .p(px(2.))
            .flex_shrink_0()
            .child(
                div()
                    .relative()
                    .size_full()
                    .debug_selector(|| "overlay-header-panel".into())
                    .rounded(px(8.))
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().popover)
                    .child(div().absolute().inset_0().on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| this.drag_overlay(window, cx)),
                    ))
                    .child(
                        div()
                            .absolute()
                            .left(px(9.))
                            .top(px(7.))
                            .id("open-dashboard-mark")
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(brand_mark(32., dark))
                            .on_click(cx.listener(|this, _, _, cx| this.show_dashboard(cx))),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .pr(px(8.))
                            .when(!control_only, |view| view.pr(px(46.)))
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(px(6.))
                            .child(
                                Button::new("widget-ask")
                                    .debug_selector(|| "overlay-toggle-chat".into())
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .ghost()
                                    .rounded(px(style::CONTROL_RADIUS))
                                    .w(px(66.))
                                    .h(px(32.))
                                    .p_0()
                                    .accessibility_label(if control_only {
                                        "Show chat"
                                    } else {
                                        "Hide chat"
                                    })
                                    .when(control_only, |button| button.primary())
                                    .icon(if control_only {
                                        Glyph::Sparkles
                                    } else if self.settings.hide_widget {
                                        Glyph::X
                                    } else {
                                        Glyph::ChevronDown
                                    })
                                    .label(if control_only { "Show" } else { "Hide" })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.toggle_chat(window, cx)
                                    })),
                            )
                            .child(
                                Button::new("widget-recording")
                                    .debug_selector(|| "session-recording-button".into())
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .ghost()
                                    .disabled(!self.supports_audio_input() && !self.snapshot.active)
                                    .rounded(px(style::CONTROL_RADIUS))
                                    .w(px(32.))
                                    .h(px(32.))
                                    .p_0()
                                    .accessibility_label(if self.snapshot.active {
                                        "Stop session"
                                    } else {
                                        if !self.supports_audio_input() {
                                            "Palepale audio unavailable"
                                        } else {
                                            "Start session"
                                        }
                                    })
                                    .child(
                                        (if self.snapshot.active {
                                            solid_square(12.)
                                        } else {
                                            solid_mic()
                                        })
                                        .text_color(rgb(gray_ink)),
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.toggle_capture();
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }
    pub(in crate::ui) fn overlay_error(&self, cx: &mut Context<Self>) -> Div {
        let dark = self.is_dark(cx);
        let ink = if dark { 0xffd9df } else { 0x803247 };
        let edit = Button::new("review-error-edit")
            .debug_selector(|| "overlay-error-edit".into())
            .ghost()
            .h(px(28.))
            .rounded_full()
            .bg(rgba(0xfff1f21f))
            .accessibility_label("Edit question")
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(11.))
                    .text_color(rgb(ink))
                    .child(Icon::new(Glyph::Pencil).size(px(12.)).text_color(rgb(ink)))
                    .child("Edit question"),
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.error = None;
                if this.gallery {
                    this.select_review("chat", window, cx);
                } else {
                    this.begin_typing(window, cx);
                }
                cx.notify();
            }));
        let dismiss = Button::new("review-error-dismiss")
            .debug_selector(|| "overlay-error-dismiss".into())
            .ghost()
            .h(px(28.))
            .rounded_full()
            .bg(rgba(0xfff1f21f))
            .accessibility_label("Dismiss")
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(11.))
                    .text_color(rgb(ink))
                    .child(Icon::new(Glyph::X).size(px(14.)).text_color(rgb(ink)))
                    .child("Dismiss"),
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.error = None;
                if this.gallery {
                    this.select_review("chat", window, cx);
                }
                cx.notify();
            }));
        div()
            .debug_selector(|| "overlay-error-panel".into())
            .w_full()
            .flex_shrink_0()
            .px(px(10.))
            .pb(px(10.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p(px(10.))
                    .rounded(px(11.))
                    .border_1()
                    .border_color(rgba(if dark { 0xf5a4b333 } else { 0xb3546b3b }))
                    .bg(rgba(if dark { 0x85374d38 } else { 0xffe6edb0 }))
                    .text_size(px(12.))
                    .line_height(px(18.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(ink))
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap_2()
                            .child(
                                Icon::new(Glyph::CircleAlert)
                                    .flex_shrink_0()
                                    .size(px(16.))
                                    .text_color(rgb(ink)),
                            )
                            .child(
                                div()
                                    .id("overlay-error-message")
                                    .flex_1()
                                    .min_w_0()
                                    .max_h(px(66.))
                                    .overflow_y_scroll()
                                    .child(
                                        self.error
                                            .clone()
                                            .unwrap_or_else(|| "Something went wrong.".into()),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(edit)
                            .child(dismiss),
                    ),
            )
    }
    pub(in crate::ui) fn overlay_screenshot(&self, data: &str, secondary: u32) -> AnyElement {
        let mut cache = self.overlay_ui.screenshot.borrow_mut();
        if cache.as_ref().is_none_or(|preview| preview.source != data) {
            *cache = Some(ScreenshotPreview {
                source: data.into(),
                image: screenshot_image(data),
            });
        }
        match cache.as_ref().map(|preview| preview.image.clone()) {
            Some(Ok(image)) => div()
                .flex_shrink_0()
                .w_full()
                .max_h(px(120.))
                .child(
                    img(image)
                        .max_w_full()
                        .max_h(px(120.))
                        .object_fit(ObjectFit::Contain),
                )
                .into_any_element(),
            Some(Err(error)) => div()
                .text_xs()
                .text_color(rgb(secondary))
                .child(format!("Failed to load screenshot: {error}"))
                .into_any_element(),
            None => div().into_any_element(),
        }
    }
    pub(in crate::ui) fn overlay_message(
        &self,
        message: &Message,
        metadata: bool,
        ink: u32,
        secondary: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if message.role == "user" {
            return div()
                .w_full()
                .flex()
                .justify_end()
                .pt(px(6.))
                .pb(px(20.))
                .pr(px(8.))
                .child(
                    div()
                        .max_w(relative(0.7))
                        .rounded_lg()
                        .px(px(10.))
                        .py(px(6.))
                        .bg(overlay_material(155., 0x6f9fdfff, 0x5587caff))
                        .shadow(vec![
                            BoxShadow::new(px(0.), px(4.), rgba(0x4277b526).into())
                                .blur_radius(px(13.)),
                            BoxShadow::new(px(0.), px(1.), rgba(0xe2efff88).into()).inset(),
                        ])
                        .text_color(rgb(0xffffff))
                        .text_xs()
                        .child(message.content.clone()),
                )
                .into_any_element();
        }
        let content = message.content.clone();
        let id = message.id.clone();
        div()
            .px(px(12.))
            .pb(px(14.))
            .flex()
            .flex_col()
            .gap_1()
            .text_size(px(14.))
            .line_height(relative(1.6))
            .text_color(rgb(ink))
            .when(metadata, |view| {
                view.child(
                    Button::new("viewed-screen-metadata")
                        .ghost()
                        .p_0()
                        .h(px(16.))
                        .accessibility_label("Viewed screen")
                        .child(
                            div()
                                .w_full()
                                .text_size(px(12.))
                                .text_color(rgb(secondary))
                                .child("Viewed screen"),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.overlay_ui.metadata_open = !this.overlay_ui.metadata_open;
                            cx.notify();
                        })),
                )
                .when(self.overlay_ui.metadata_open, |view| {
                    view.child(if let Some(data) = self.captured_screen.as_ref() {
                        self.overlay_screenshot(data, secondary)
                    } else {
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(secondary))
                            .child("Failed to load screenshot")
                            .into_any_element()
                    })
                })
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(secondary))
                        .child("Viewed files"),
                )
            })
            .child(markdown(
                SharedString::from(format!("overlay-{}", message.id)),
                content.clone(),
            ))
            .child(
                Button::new(SharedString::from(format!("copy-{}", message.id)))
                    .ghost()
                    .xsmall()
                    .icon(if self.overlay_ui.copied.as_ref() == Some(&id) {
                        Glyph::Check
                    } else {
                        Glyph::Copy
                    })
                    .accessibility_label("Copy response")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(content.clone()));
                        this.overlay_ui.copied = Some(id.clone());
                        this.overlay_ui.copied_at = Some(Instant::now());
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}
