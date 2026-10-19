use super::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};

impl NativeView {
    fn prompt_effort_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = if !self.settings.smart_mode {
            "Default"
        } else if self.settings.smart_effort == "low" {
            "Low"
        } else {
            "High"
        };
        let weak = cx.weak_entity();
        Button::new("prompt-effort")
            .debug_selector(|| "overlay-effort-picker".into())
            .ghost()
            .h(px(28.))
            .px(px(8.))
            .rounded(px(8.))
            .icon(Glyph::Sparkles)
            .label(selected)
            .accessibility_label("Reasoning effort for Text and Images")
            .when(selected == "High", |button| {
                button.text_color(rgb(0xbba8ed))
            })
            .dropdown_menu(move |mut menu, _, _| {
                for label in ["Default", "Low", "High"] {
                    let weak = weak.clone();
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .checked(label == selected)
                            .on_click(move |_, _, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.change_preference(|settings| {
                                        settings.smart_mode = label != "Default";
                                        if label != "Default" {
                                            settings.smart_effort = label.to_lowercase();
                                        }
                                    });
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            })
    }

    fn prompt_sparks(&self) -> impl IntoElement {
        let high = self.settings.smart_mode && self.settings.smart_effort == "high";
        let typing = self.typing || self.overlay_ui.focused;
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                if !high {
                    return;
                }
                window.paint_quad(fill(bounds, rgb(0xb39dff).opacity(0.055)).corner_radii(px(12.)));
                // Fixed particle count; only animate while the user edits in an active window.
                let animate = typing && window.is_window_active();
                let phase = if animate {
                    (now_ms() % 6000) as f32 / 6000.
                } else {
                    0.3
                };
                for index in 0..12 {
                    let x = ((index * 37 % 97) as f32 + 1.) / 100.;
                    let y = (phase + index as f32 / 12.).fract();
                    let opacity = (y * std::f32::consts::PI).sin() * 0.4;
                    let dot = Bounds::new(
                        point(
                            bounds.left() + bounds.size.width * x,
                            bounds.top() + px(5.) + (bounds.size.height - px(10.)) * (1. - y),
                        ),
                        size(px(2.), px(2.)),
                    );
                    window
                        .paint_quad(fill(dot, rgb(0xb39dff).opacity(opacity)).corner_radii(px(1.)));
                }
                if animate {
                    window.request_animation_frame();
                }
            },
        )
        .absolute()
        .inset_0()
    }

    pub(in crate::ui) fn overlay_suggestions(
        &self,
        _secondary: u32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("overlay-suggestions")
            .w_full()
            .flex_shrink_0()
            .h(px(36.))
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .overflow_x_scroll()
            .children([
                ("Assist", ""),
                ("Reply", "What should I say next? Output only the words I should say."),
                ("Follow-ups", "Suggest two follow-up questions that I can ask to carry forward the conversation. Output as two bullet points."),
                ("Recap", "Recap the most recent thing that happened in the conversation."),
            ].into_iter().enumerate().map(|(index, (label, prompt))| {
                Button::new(SharedString::from(format!("quick-{index}")))
                    .ghost()
                    .small()
                    .h(px(style::COMPACT_HEIGHT))
                    .rounded(px(style::CONTROL_RADIUS))
                    .label(label)
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.quick_assist(prompt, window, cx);
                        cx.notify();
                    }))
            }))
    }
    pub(in crate::ui) fn overlay_composer(
        &self,
        _expanded: bool,
        dark: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let avoid_focus =
            self.settings.capture_protection && self.settings.no_focus && !self.typing;
        let secondary = if dark { 0xa8bddb } else { 0x7889a2 };
        div()
            .debug_selector(|| "overlay-composer".into())
            .w_full()
            .flex_shrink_0()
            .pt(px(6.))
            .px(px(10.))
            .pb(px(10.))
            .child(
                div()
                    .debug_selector(|| "overlay-composer-input".into())
                    .relative()
                    .w_full()
                    .min_h(px(40.))
                    .rounded(px(12.))
                    .border_1()
                    .border_color(rgba(if dark { 0xd8eaff2b } else { 0xffffffcc }))
                    .bg(cx.theme().popover)
                    .child(self.prompt_sparks())
                    .child(
                        div()
                            .relative()
                            .min_h(px(40.))
                            .max_h(px(50.))
                            .w_full()
                            .child(
                                Textarea::new(&self.question)
                                    .appearance(false)
                                    .bordered(false)
                                    .min_h(px(40.))
                                    .max_h(px(50.))
                                    .pl(px(8.))
                                    .pr(px(44.))
                                    .py(px(10.))
                                    .text_size(px(13.))
                                    .line_height(px(20.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .aria_label(if !self.supports_image_input() {
                                        "Ask about your saved context"
                                    } else {
                                        "Ask about your screen or conversation"
                                    }),
                            )
                            .when(
                                self.question.read(cx).value().is_empty()
                                    && !self.typing
                                    && !self.overlay_ui.focused,
                                |view| {
                                    view.child(
                                        div()
                                            .debug_selector(|| {
                                                "overlay-question-placeholder".into()
                                            })
                                            .absolute()
                                            .left(px(8.))
                                            .right(px(44.))
                                            .top(px(10.))
                                            .h(px(20.))
                                            .flex()
                                            .items_center()
                                            .gap(px(3.))
                                            .whitespace_nowrap()
                                            .overflow_hidden()
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(secondary))
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|this, _, window, cx| {
                                                    this.begin_typing(window, cx)
                                                }),
                                            )
                                            .child("Ask anything…"),
                                    )
                                },
                            ),
                    )
                    .map(|view| {
                        view.child(
                            div()
                                .h(px(38.))
                                .pl(px(6.))
                                .pt(px(2.))
                                .pb(px(6.))
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .child(self.prompt_effort_picker(cx))
                                .child(
                                    overlay_icon_button(
                                        "chat-more",
                                        Glyph::SlidersHorizontal,
                                        14.,
                                        if dark { 0xb1c7e6 } else { 0x75859d },
                                    )
                                    .accessibility_label("Open more chat actions")
                                    .child(track_overlay_bounds(
                                        self.overlay_ui.menu_trigger.clone(),
                                    ))
                                    .on_click(cx.listener(
                                        |this, _, window, cx| {
                                            this.overlay_ui.menu = !this.overlay_ui.menu;
                                            this.overlay_ui.tools = false;
                                            this.overlay_ui.modes = false;
                                            this.overlay_ui.menu_selection = None;
                                            if this.overlay_ui.menu && this.is_overlay {
                                                this.expanded = true;
                                                window.resize(size(px(560.), px(550.)));
                                            }
                                            cx.notify();
                                        },
                                    )),
                                )
                                .when(avoid_focus, |view| {
                                    view.child(
                                        Button::new("chat-type")
                                            .ghost()
                                            .h(px(28.))
                                            .px(px(6.))
                                            .label("Type")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.begin_typing(window, cx)
                                            })),
                                    )
                                }),
                        )
                    })
                    .child(
                        Button::new("chat-send")
                            .debug_selector(|| "overlay-send-button".into())
                            .ghost()
                            .absolute()
                            .right(px(7.))
                            .bottom(px(7.))
                            .w(px(28.))
                            .h(px(28.))
                            .p_0()
                            .rounded(px(8.))
                            .bg(overlay_blue())
                            .accessibility_label(if self.busy {
                                "Stop response"
                            } else {
                                "Send message"
                            })
                            .child(
                                (if self.busy {
                                    solid_square(9.)
                                } else {
                                    solid_send()
                                })
                                .text_color(rgb(0xffffff)),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.busy {
                                    this.cancel_request();
                                } else {
                                    this.ask(window, cx);
                                }
                                cx.notify();
                            })),
                    ),
            )
    }
}
