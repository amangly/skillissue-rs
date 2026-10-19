use super::*;

impl NativeView {
    pub(in crate::ui) fn overlay_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let dark = self.is_dark(cx);
        let ink = if dark { 0xe4edfb } else { 0x354b6c };
        let secondary = if dark { 0xa8bddb } else { 0x7a8aa2 };
        let viewport_width = f32::from(self.modal_backdrop_size.width);
        let viewport_height = f32::from(self.modal_backdrop_size.height);
        let root = self.overlay_ui.root_bounds.borrow();
        let root_x = root
            .as_ref()
            .map_or(0., |bounds| f32::from(bounds.origin.x));
        let root_y = root
            .as_ref()
            .map_or(0., |bounds| f32::from(bounds.origin.y));
        let root_height = root
            .as_ref()
            .map_or(viewport_height, |bounds| f32::from(bounds.size.height));
        let trigger = self.overlay_ui.menu_trigger.borrow();
        let trigger_x = trigger
            .as_ref()
            .map_or(root_x + 92., |bounds| f32::from(bounds.origin.x));
        let trigger_y = trigger
            .as_ref()
            .map_or(root_y + root_height - 74., |bounds| {
                f32::from(bounds.origin.y)
            });
        let menu_width = 248_f32.min((viewport_width - 16.).max(1.));
        let menu_left = trigger_x.clamp(8., (viewport_width - menu_width - 8.).max(8.));
        let menu_height = if self.settings.capture_protection {
            180.
        } else {
            148.
        };
        let menu_top = (trigger_y - menu_height - 8.).max(8.);
        let submenu_width = 168_f32.min((viewport_width - 16.).max(1.));
        let (submenu_left, submenu_bottom) = if viewport_width <= 420. {
            ((menu_width - submenu_width).max(0.), 32.)
        } else if menu_left + menu_width + submenu_width <= viewport_width - 8. {
            (menu_width - 1., 0.)
        } else if menu_left - submenu_width >= 8. {
            (-submenu_width, 0.)
        } else {
            ((menu_width - submenu_width).max(0.), 32.)
        };
        let mut menu = div()
            .id("overlay-more-menu")
            .absolute()
            .w(px(menu_width))
            .left(px(menu_left - root_x))
            .top(px(menu_top - root_y))
            .max_h(px((viewport_height - 16.).max(1.)))
            .p(px(4.))
            .rounded(px(8.))
            .border_1()
            .border_color(rgba(if dark { 0xdbeaff29 } else { 0xffffffe6 }))
            .bg(cx.theme().popover)
            .shadow(vec![
                BoxShadow::new(px(0.), px(12.), rgba(0x47608426).into()).blur_radius(px(38.)),
                BoxShadow::new(
                    px(0.),
                    px(1.),
                    rgba(if dark { 0xecf5ff3d } else { 0xfffffff0 }).into(),
                )
                .inset(),
            ])
            .flex()
            .flex_col()
            .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                let trigger = this
                    .overlay_ui
                    .menu_trigger
                    .borrow()
                    .is_some_and(|bounds| bounds.contains(&event.position));
                let submenu = this.overlay_ui.modes
                    && this
                        .overlay_ui
                        .mode_bounds
                        .borrow()
                        .is_some_and(|bounds| bounds.contains(&event.position));
                if !trigger && !submenu {
                    this.overlay_ui.menu = false;
                    this.overlay_ui.modes = false;
                    this.overlay_ui.menu_selection = None;
                    cx.notify();
                }
            }))
            .on_hover(cx.listener(|this, hovered, window, cx| {
                if !hovered
                    && this.overlay_ui.modes
                    && !this
                        .overlay_ui
                        .mode_bounds
                        .borrow()
                        .is_some_and(|bounds| bounds.contains(&window.mouse_position()))
                {
                    this.overlay_ui.modes = false;
                    cx.notify();
                }
            }))
            .child(
                div()
                    .px(px(8.))
                    .py(px(6.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(secondary))
                    .child("Session options"),
            );
        menu = menu.child(
            menu_button(
                "menu-shortcuts",
                Some(Glyph::Keyboard),
                "Keyboard shortcuts",
                ink,
                secondary,
                Icon::new(Glyph::ExternalLink).size(px(14.)),
            )
            .when(self.overlay_ui.menu_selection == Some(0), |view| {
                view.bg(cx.theme().muted)
            })
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_overlay_popups();
                this.open_settings("keybinds", cx);
            })),
        );
        menu = menu
            .child(menu_separator(dark))
            .child(
                menu_button(
                    "menu-protection",
                    Some(if self.settings.capture_protection {
                        Glyph::EyeOff
                    } else {
                        Glyph::Eye
                    }),
                    "Capture protection",
                    ink,
                    secondary,
                    visual_switch(self.settings.capture_protection, dark),
                )
                .when(self.overlay_ui.menu_selection == Some(1), |view| {
                    view.bg(rgba(if dark { 0xb7d4ff18 } else { 0x8baee12e }))
                })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.change_preference(|settings| {
                        settings.capture_protection = !settings.capture_protection
                    });
                    cx.notify();
                })),
            )
            .when(self.settings.capture_protection, |view| {
                view.child(
                    menu_button(
                        "menu-focus",
                        Some(Glyph::Focus),
                        "Avoid stealing focus",
                        ink,
                        secondary,
                        visual_switch(self.settings.no_focus, dark),
                    )
                    .when(self.overlay_ui.menu_selection == Some(2), |view| {
                        view.bg(rgba(if dark { 0xb7d4ff18 } else { 0x8baee12e }))
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        if !this.settings.no_focus {
                            this.overlay_ui.preserve_blur_until =
                                Some(Instant::now() + Duration::from_secs(1));
                        }
                        this.change_preference(|settings| settings.no_focus = !settings.no_focus);
                        cx.notify();
                    })),
                )
            })
            .child(
                menu_button(
                    "menu-modes",
                    Some(Glyph::Layers),
                    "Modes",
                    ink,
                    secondary,
                    Icon::new(Glyph::ChevronRight)
                        .size(px(14.))
                        .text_color(rgb(secondary)),
                )
                .when(
                    self.overlay_ui.menu_selection
                        == Some(if self.settings.capture_protection {
                            3
                        } else {
                            2
                        }),
                    |view| view.bg(rgba(if dark { 0xb7d4ff18 } else { 0x8baee12e })),
                )
                .on_hover(cx.listener(|this, hovered, _, cx| {
                    if *hovered && !this.overlay_ui.modes {
                        this.overlay_ui.modes = true;
                        this.overlay_ui.mode_selection = 0;
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.overlay_ui.modes = !this.overlay_ui.modes;
                    cx.notify();
                })),
            )
            .when(self.overlay_ui.modes, |view| {
                view.child(
                    div()
                        .id("overlay-modes-submenu")
                        .absolute()
                        .left(px(submenu_left))
                        .bottom(px(submenu_bottom))
                        .w(px(submenu_width))
                        .max_h(px(400_f32.min((viewport_height - 16.).max(1.))))
                        .overflow_y_scroll()
                        .p_1()
                        .rounded(px(8.))
                        .border_1()
                        .border_color(rgba(if dark { 0xdbeaff29 } else { 0xffffffe6 }))
                        .bg(overlay_material(
                            140.,
                            if dark { 0x2a3b52fa } else { 0xfffffff5 },
                            if dark { 0x14253df5 } else { 0xebf5fff5 },
                        ))
                        .shadow_lg()
                        .flex()
                        .flex_col()
                        .child(track_overlay_bounds(self.overlay_ui.mode_bounds.clone()))
                        .on_hover(cx.listener(|this, hovered, _, cx| {
                            if !hovered {
                                this.overlay_ui.modes = false;
                                cx.notify();
                            }
                        }))
                        .children(
                            std::iter::once(("general".to_owned(), "General".to_owned()))
                                .chain(self.settings.modes.iter().map(|mode| {
                                    (
                                        mode.id.clone(),
                                        if mode.name.is_empty() {
                                            "Untitled Mode".into()
                                        } else {
                                            mode.name.clone()
                                        },
                                    )
                                }))
                                .enumerate()
                                .map(|(index, (id, name))| {
                                    menu_button(
                                        SharedString::from(format!("menu-mode-{id}")),
                                        None,
                                        name,
                                        ink,
                                        secondary,
                                        div().w(px(14.)).when(
                                            self.settings.active_mode_id == id,
                                            |view| {
                                                view.child(
                                                    Icon::new(Glyph::Check)
                                                        .size(px(14.))
                                                        .text_color(rgb(ink)),
                                                )
                                            },
                                        ),
                                    )
                                    .when(
                                        self.overlay_ui.menu_selection.is_some()
                                            && self.overlay_ui.mode_selection == index,
                                        |view| {
                                            view.bg(rgba(if dark {
                                                0xb7d4ff18
                                            } else {
                                                0x8baee12e
                                            }))
                                        },
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            let id = id.clone();
                                            this.change_preference(move |settings| {
                                                settings.active_mode_id = id
                                            });
                                            this.overlay_ui.menu = false;
                                            this.overlay_ui.modes = false;
                                            cx.notify();
                                        },
                                    ))
                                }),
                        )
                        .child(menu_separator(dark))
                        .child(
                            menu_button(
                                "menu-manage-modes",
                                Some(Glyph::Pencil),
                                "Manage Modes",
                                ink,
                                secondary,
                                div(),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.overlay_ui.menu = false;
                                this.page = Page::Modes;
                                this.show_dashboard(cx);
                                cx.notify();
                            })),
                        ),
                )
            });
        menu.into_any_element()
    }
    pub(in crate::ui) fn overlay_tools(&self, cx: &mut Context<Self>) -> AnyElement {
        let dark = self.is_dark(cx);
        let ink = if dark { 0xe4edfb } else { 0x354b6c };
        let secondary = if dark { 0xa8bddb } else { 0x7a8aa2 };
        div()
            .id("overlay-session-tools-menu")
            .debug_selector(|| "session-tools-menu".into())
            .absolute()
            .right(px(10.))
            .top(px(48.))
            .w(px(164.))
            .rounded(px(8.))
            .p_1()
            .border_1()
            .border_color(rgba(if dark { 0xdbeaff29 } else { 0xffffffe6 }))
            .bg(cx.theme().popover)
            .shadow_lg()
            .flex()
            .flex_col()
            .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                if !this
                    .overlay_ui
                    .tools_trigger
                    .borrow()
                    .is_some_and(|bounds| bounds.contains(&event.position))
                {
                    this.overlay_ui.tools = false;
                    this.overlay_ui.tool_selection = None;
                    cx.notify();
                }
            }))
            .children(
                [
                    ("dashboard", Glyph::LayoutDashboard, "Home"),
                    ("transcript", Glyph::FileText, "Transcript"),
                    ("capture", Glyph::Monitor, "Screenshot"),
                    ("export", Glyph::Download, "Export"),
                    ("clear", Glyph::RotateCw, "Clear chat"),
                    ("provider", Glyph::Settings, "AI providers"),
                    ("hide", Glyph::EyeOff, "Hide"),
                ]
                .into_iter()
                .enumerate()
                .map(|(index, (action, glyph, label))| {
                    menu_button(
                        SharedString::from(format!("tools-{action}")),
                        Some(glyph),
                        label,
                        ink,
                        secondary,
                        div(),
                    )
                    .when(self.overlay_ui.tool_selection == Some(index), |view| {
                        view.bg(rgba(if dark { 0xb7d4ff18 } else { 0x8baee12e }))
                    })
                    .text_size(px(12.))
                    .h(px(28.))
                    .disabled(action == "capture" && !self.supports_image_input())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.activate_overlay_tool(action, window, cx);
                        cx.notify();
                    }))
                }),
            )
            .into_any_element()
    }
}
