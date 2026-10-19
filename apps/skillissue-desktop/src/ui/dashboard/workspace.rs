use super::*;

impl NativeView {
    pub(in crate::ui) fn render_dashboard(&self, cx: &mut Context<Self>) -> AnyElement {
        DASHBOARD_DARK.set(self.is_dark(cx));
        let home = self.dashboard.screen == Screen::Home;
        let content = match self.dashboard.screen {
            Screen::Home => self.render_dashboard_home(cx),
            Screen::Session => self.render_dashboard_detail(cx),
        };
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .text_color(ink(INK))
            .bg(material(
                135., 0xffffff54, 0xebf3ff29, 0xffffff00, 0xffffff00,
            ))
            .child(
                div()
                    .h(px(44.))
                    .flex_shrink_0()
                    .px_4()
                    .flex()
                    .items_center()
                    .relative()
                    .bg(cx.theme().popover)
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .when(!home, |bar| {
                        bar.child(
                            chrome("dashboard-back", Glyph::ArrowLeft, 18.)
                                .w(px(28.))
                                .h(px(28.))
                                .mt(px(3.))
                                .debug_selector(|| "dashboard-back".into())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.navigate_dashboard_session(
                                        SessionDestination::Home,
                                        window,
                                        cx,
                                    )
                                })),
                        )
                    })
                    .child(
                        div()
                            .w(px(72.))
                            .h_full()
                            .window_control_area(WindowControlArea::Drag),
                    )
                    .when(self.updates.available.is_some(), |bar| {
                        bar.child(
                            Button::new("update-available")
                                .ghost()
                                .small()
                                .label("Update available")
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.open_settings("general", cx)),
                                ),
                        )
                    })
                    .child(
                        Button::new("dashboard-search")
                            .flex_1()
                            .min_w_0()
                            .debug_selector(|| "dashboard-search".into())
                            .label("Search sessions")
                            .icon(Glyph::Search)
                            .h(px(style::CONTROL_HEIGHT))
                            .mr_2()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dashboard.search_open = true;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .ml_auto()
                            .child(
                                button("dashboard-menu", "")
                                    .debug_selector(|| "dashboard-menu".into())
                                    .icon(icon(Glyph::Menu))
                                    .w(px(style::CONTROL_HEIGHT))
                                    .h(px(style::CONTROL_HEIGHT))
                                    .p_0()
                                    .rounded(px(6.))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.dashboard.menu_open = !this.dashboard.menu_open;
                                        cx.notify();
                                    })),
                            )
                            .when(!self.gallery, |view| {
                                view.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(2.))
                                        .ml_3()
                                        .mr(px(-8.))
                                        .child(
                                            chrome("dashboard-minimize", Glyph::Minus, 13.)
                                                .on_click(|_, window, _| window.minimize_window()),
                                        )
                                        .child(
                                            chrome("dashboard-maximize", Glyph::Square, 11.)
                                                .on_click(|_, window, _| window.zoom_window()),
                                        ),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .overflow_hidden()
                    .child(div().flex_1().min_w_0().h_full().child(content)),
            )
            .when(self.dashboard.menu_open, |view| {
                view.child(
                    div()
                        .rounded_lg()
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().popover)
                        .text_color(cx.theme().popover_foreground)
                        .shadow_lg()
                        .absolute()
                        .top(px(44.))
                        .right(px(if self.gallery { 16. } else { 106. }))
                        .w(px(152.))
                        .debug_selector(|| "workspace-menu-panel".into())
                        .p_1()
                        .flex()
                        .flex_col()
                        .gap_0()
                        .child(
                            style::action("workspace-modes")
                                .small()
                                .h(px(28.))
                                .px_2()
                                .ghost()
                                .w_full()
                                .justify_start()
                                .label("Modes")
                                .icon(Glyph::Layers)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.dashboard.menu_open = false;
                                    this.page = Page::Modes;
                                    cx.notify();
                                })),
                        )
                        .child(
                            style::action("workspace-settings")
                                .small()
                                .h(px(28.))
                                .px_2()
                                .debug_selector(|| "workspace-settings".into())
                                .ghost()
                                .w_full()
                                .justify_start()
                                .label("Settings")
                                .icon(Glyph::Settings)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.dashboard.menu_open = false;
                                    this.open_settings("general", cx);
                                    cx.notify();
                                })),
                        ),
                )
            })
            .when(
                self.dashboard.search_open && !self.capturing_backdrop,
                |view| view.child(self.render_dashboard_search(cx)),
            )
            .when(
                self.page == Page::Modes && !self.capturing_backdrop,
                |view| view.child(self.render_modes_modal(cx)),
            )
            .when(
                !self.dashboard.notice.is_empty()
                    && now_ms().saturating_sub(self.dashboard.notice_at) < 5500,
                |view| {
                    view.child(
                        panel()
                            .absolute()
                            .bottom(px(20.))
                            .left(px(100.))
                            .right(px(100.))
                            .p_3()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .text_sm()
                                    .child(self.dashboard.notice.clone()),
                            )
                            .child(
                                button("dismiss-notice", "")
                                    .ghost()
                                    .icon(icon(Glyph::X))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.dashboard.notice.clear();
                                        cx.notify();
                                    })),
                            ),
                    )
                },
            )
            .into_any_element()
    }
    pub(in crate::ui) fn render_dashboard_home(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("dashboard-home-scroll")
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .px_6()
                    .py_4()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .max_w(px(832.))
                            .mx_auto()
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(brand_mark(28., self.is_dark(cx)))
                                    .child(
                                        div()
                                            .text_xl()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("skillissue"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        Button::new("dashboard-start")
                                            .primary()
                                            .label(if self.snapshot.active {
                                                "Return to session"
                                            } else {
                                                "New session"
                                            })
                                            .icon(Glyph::Plus)
                                            .disabled(
                                                self.dashboard.session_action
                                                    || self.engine.is_none(),
                                            )
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.start_dashboard_session(cx)
                                            })),
                                    )
                                    .child(
                                        Button::new("home-modes")
                                            .label("Modes")
                                            .icon(Glyph::Layers)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.page = Page::Modes;
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        Button::new("home-settings")
                                            .label("Settings")
                                            .icon(Glyph::Settings)
                                            .debug_selector(|| "home-settings".into())
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.open_settings("general", cx);
                                                cx.notify();
                                            })),
                                    )
                                    .child(div().flex_1())
                                    .child(self.capture_button(cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .max_w(px(832.))
                    .mx_auto()
                    .py_6()
                    .px_6()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(self.render_setup_checklist(cx))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .gap_4()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(ink(SECONDARY))
                                    .child("Sessions"),
                            )
                            .when(!self.gallery, |view| {
                                view.child(
                                    div().w(px(260.)).child(
                                        Input::new(&self.search)
                                            .small()
                                            .aria_label("Search saved sessions"),
                                    ),
                                )
                            }),
                    )
                    .when(
                        self.dashboard.history_loading && !self.dashboard.history_loaded,
                        |view| view.child(muted("Loading saved sessions…")),
                    )
                    .when_some(self.dashboard.history_error.clone(), |view, error| {
                        view.child(
                            div()
                                .rounded_lg()
                                .border_1()
                                .border_color(glass(0xc34b4b40))
                                .p_3()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(div().flex_1().text_color(ink(0xc34b4b)).child(error))
                                .child(button("retry-history", "Retry").on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.load_dashboard_history();
                                        cx.notify();
                                    },
                                ))),
                        )
                    })
                    .when(
                        self.dashboard.history_rows.is_empty()
                            && self.dashboard.history_loaded
                            && self.dashboard.history_error.is_none(),
                        |view| {
                            view.child(if self.search_query.is_empty() {
                                self.dashboard_empty("Start your first session", cx)
                            } else {
                                muted("No saved sessions match this search.")
                                    .py_6()
                                    .into_any_element()
                            })
                        },
                    )
                    .children(self.dashboard.history_rows.iter().map(|session| {
                        let id = session.id.clone();
                        div()
                            .id(SharedString::from(format!("history-{}", session.id)))
                            .flex()
                            .items_center()
                            .gap_3()
                            .px_3()
                            .py_3()
                            .rounded_lg()
                            .cursor_pointer()
                            .hover(|style| style.bg(glass(0xd0dcf057)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_dashboard_session(id.clone(), window, cx)
                            }))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(div().font_weight(FontWeight::MEDIUM).child(
                                        if session.title.is_empty() {
                                            "Untitled Session".into()
                                        } else {
                                            session.title.clone()
                                        },
                                    ))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(ink(SECONDARY))
                                            .child(date(session.created_at)),
                                    ),
                            )
                            .child(
                                div()
                                    .rounded_full()
                                    .px_2()
                                    .py_1()
                                    .bg(glass(if session.state == "active" {
                                        0x10b98120
                                    } else {
                                        0xd0dcf057
                                    }))
                                    .text_xs()
                                    .text_color(ink(if session.state == "active" {
                                        0x16815d
                                    } else {
                                        SECONDARY
                                    }))
                                    .child(state_label(&session.state)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(ink(SECONDARY))
                                    .child(format!("{} segments", session.transcript_count)),
                            )
                    }))
                    .when(
                        self.dashboard.history_offset > 0 || self.dashboard.history_has_next,
                        |view| {
                            view.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .gap_4()
                                    .py_2()
                                    .mt_3()
                                    .child(
                                        button("history-previous", "Previous")
                                            .icon(icon(Glyph::ArrowLeft))
                                            .disabled(
                                                self.dashboard.history_offset == 0
                                                    || self.dashboard.history_loading,
                                            )
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.dashboard.history_offset = this
                                                    .dashboard
                                                    .history_offset
                                                    .saturating_sub(20);
                                                this.load_dashboard_history();
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        muted(format!(
                                            "Page {}",
                                            self.dashboard.history_offset / 20 + 1
                                        ))
                                        .text_xs(),
                                    )
                                    .child(
                                        button("history-next", "Next")
                                            .icon(icon(Glyph::ArrowRight))
                                            .disabled(
                                                !self.dashboard.history_has_next
                                                    || self.dashboard.history_loading,
                                            )
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.dashboard.history_offset = this
                                                    .dashboard
                                                    .history_offset
                                                    .saturating_add(20);
                                                this.load_dashboard_history();
                                                cx.notify();
                                            })),
                                    ),
                            )
                        },
                    ),
            )
            .into_any_element()
    }
    pub(in crate::ui) fn dashboard_empty(&self, title: &str, cx: &mut Context<Self>) -> AnyElement {
        div().mx_6().rounded(px(14.)).border_1().border_dashed().border_color(glass(0x7c91b22e)).bg(glass(0xffffff26)).px_12().py_8().flex().flex_col().items_center().gap_2()
            .child(div().size_10().rounded_xl().bg(material(145.,0xffffffd9,0xdbe9fc7a,0xd7e8ff12,0xa9c9ff06)).shadow(vec![BoxShadow::new(px(0.),px(3.),rgba(0x31446b12).into()).blur_radius(px(9.)),BoxShadow::new(px(0.),px(1.),rgba(0xffffffe6).into()).inset()]).text_color(ink(0x7188a8)).flex().items_center().justify_center().mb_2().child(Icon::new(Glyph::Monitor).size(px(20.))))
            .child(div().text_size(px(18.)).font_weight(FontWeight::MEDIUM).text_color(ink(INK)).child(title.to_owned()))
            .child(muted("skillissue provides live AI help during your conversation. Save your transcript, generate notes, and return to previous sessions in the desktop app.").max_w(px(360.)).line_height(relative(1.65)).text_center())
            .child(button("empty-start", "New session").mt_2().disabled(self.dashboard.session_action || self.engine.is_none()).on_click(cx.listener(|this, _, _, cx| this.start_dashboard_session(cx))))
            .into_any_element()
    }
    pub(in crate::ui) fn render_dashboard_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let query = self.search.read(cx).value().to_lowercase();
        div()
            .absolute()
            .inset_0()
            .child(self.modal_material(cx))
            .bg(glass(0x53668b22))
            .pt(px(112.))
            .flex()
            .justify_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.dashboard.search_open = false;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                panel()
                    .w(px(480.))
                    .max_h(px(470.))
                    .self_start()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .px(px(14.))
                            .h(px(49.))
                            .flex_shrink_0()
                            .py(px(10.))
                            .flex()
                            .items_center()
                            .gap_3()
                            .border_b_1()
                            .border_color(glass(LINE))
                            .child(Icon::new(Glyph::Search).size(px(18.)))
                            .child(
                                div().flex_1().child(
                                    Input::new(&self.search)
                                        .appearance(false)
                                        .h(px(28.))
                                        .text_size(px(14.))
                                        .bordered(false)
                                        .aria_label("Search saved sessions and actions"),
                                ),
                            )
                            .child(
                                chrome("close-search", Glyph::X, 15.)
                                    .w(px(28.))
                                    .h(px(28.))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.dashboard.search_open = false;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .id("command-results")
                            .max_h(px(420.))
                            .min_h_0()
                            .overflow_y_scroll()
                            .p_1()
                            .flex()
                            .flex_col()
                            .when(!self.gallery, |view| {
                                view.child(
                                    muted("Saved sessions").text_size(px(12.)).px_2().py(px(6.)),
                                )
                                .when(self.sessions.is_empty(), |view| {
                                    view.child(muted("No saved sessions found.").p_3())
                                })
                                .children(
                                    self.sessions.iter().take(8).map(|session| {
                                        let id = session.id.clone();
                                        button(
                                            SharedString::from(format!(
                                                "command-session-{}",
                                                session.id
                                            )),
                                            "",
                                        )
                                        .ghost()
                                        .bg(rgba(0xffffff00))
                                        .border_0()
                                        .shadow_none()
                                        .h(px(37.))
                                        .px_2()
                                        .rounded(px(8.))
                                        .child(nav_label(
                                            Glyph::FileText,
                                            session.title.clone(),
                                            false,
                                        ))
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.open_dashboard_session(id.clone(), window, cx)
                                            }),
                                        )
                                    }),
                                )
                            })
                            .children(["Actions", "Settings"].into_iter().filter_map(|group| {
                                let commands: Vec<_> = [
                                    ("View Sessions", 0, Glyph::Monitor, "Actions"),
                                    ("Start Listening", 1, Glyph::Play, "Actions"),
                                    ("Manage Modes", 2, Glyph::Layers, "Actions"),
                                    ("General", 3, Glyph::Settings, "Settings"),
                                    (
                                        "Keyboard shortcuts",
                                        4,
                                        Glyph::SlidersHorizontal,
                                        "Settings",
                                    ),
                                ]
                                .into_iter()
                                .filter(|(name, _, _, section)| {
                                    *section == group && name.to_lowercase().contains(&query)
                                })
                                .collect();
                                (!commands.is_empty()).then(|| {
                                    div()
                                        .flex()
                                        .flex_col()
                                        .child(muted(group).text_size(px(12.)).px_2().py(px(6.)))
                                        .children(commands.into_iter().map(
                                            |(label, action, glyph, _)| {
                                                button(
                                                    SharedString::from(format!("command-{action}")),
                                                    "",
                                                )
                                                .ghost()
                                                .bg(rgba(0xffffff00))
                                                .border_0()
                                                .shadow_none()
                                                .h(px(37.))
                                                .px_2()
                                                .rounded(px(8.))
                                                .disabled(
                                                    action == 1
                                                        && (self.dashboard.session_action
                                                            || self.engine.is_none()),
                                                )
                                                .child(
                                                    div()
                                                        .w_full()
                                                        .flex()
                                                        .items_center()
                                                        .gap_2()
                                                        .text_size(px(14.))
                                                        .child(Icon::new(glyph).size(px(18.)))
                                                        .child(div().flex_1().child(label)),
                                                )
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.dashboard.search_open = false;
                                                    match action {
                                                        0 => {
                                                            this.dashboard.screen = Screen::Home;
                                                            this.page = Page::Sessions;
                                                        }
                                                        1 => this.start_dashboard_session(cx),
                                                        2 => this.page = Page::Modes,
                                                        3 => this.open_settings("general", cx),
                                                        4 => this.open_settings("keybinds", cx),
                                                        _ => {}
                                                    }
                                                    cx.notify();
                                                }))
                                            },
                                        ))
                                })
                            })),
                    )
                    .map(|view| self.dashboard_focus_trap("search-modal", view)),
            )
            .into_any_element()
    }
}
