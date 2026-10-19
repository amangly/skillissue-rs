//! Product surfaces and isolated interface review navigation.
use super::*;

impl NativeView {
    pub(super) fn is_dark(&self, cx: &App) -> bool {
        self.settings.theme == "dark"
            || (self.settings.theme == "system"
                && matches!(
                    cx.window_appearance(),
                    WindowAppearance::Dark | WindowAppearance::VibrantDark
                ))
    }

    pub(super) fn render_surface(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.is_overlay || self.page == Page::Live {
            return self.render_overlay(cx);
        }
        match self.page {
            Page::Settings | Page::Keybinds => self.render_settings_panel(cx),
            Page::Splash => self.render_splash(cx),
            _ => self.render_dashboard(cx),
        }
    }

    fn render_splash(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .pt(px(40.))
            .px(px(32.))
            .pb(px(32.))
            .gap(px(64.))
            .text_color(rgb(0xffffff))
            .bg(linear_gradient(
                169.,
                linear_color_stop(rgb(0x898aa1), -0.2862),
                linear_color_stop(rgb(0x555571), 0.917),
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.))
                    .child(brand_mark(112., true))
                    .child(
                        div()
                            .font_family("JetBrains Mono")
                            .text_size(px(36.))
                            .line_height(px(43.2))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("skillissue"),
                    ),
            )
            .child(
                Icon::new(gpui_kit::assets::IconName::LoaderCircle)
                    .size(px(24.))
                    .text_color(rgb(0xffffff)),
            )
            .child(
                Button::new("close-splash")
                    .ghost()
                    .small()
                    .absolute()
                    .right(px(6.))
                    .top(px(6.))
                    .bg(rgba(0))
                    .text_color(rgba(0xffffffbb))
                    .icon(gpui_kit::assets::IconName::X)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.select_review("dashboard", window, cx)
                    })),
            )
            .into_any_element()
    }

    pub(super) fn render_gallery(&self, cx: &mut Context<Self>) -> AnyElement {
        let screen = catalog::screen(&self.review_id).unwrap_or(&catalog::REVIEW_SCREENS[0]);
        let dark = self.is_dark(cx);
        let content = match screen.view {
            "dashboard" => self.render_dashboard(cx),
            "settings" => self.render_settings_panel(cx),
            "control" | "overlay" => div()
                .size_full()
                .flex()
                .justify_center()
                .pt(px(if screen.view == "control" { 60. } else { 24. }))
                .pb(px(24.))
                .child(
                    div()
                        .w(px(580.))
                        .max_w_full()
                        .h_full()
                        .px(px(10.))
                        .child(self.render_overlay(cx)),
                )
                .into_any_element(),
            "splash" => self.render_splash(cx),
            _ => div().size_full().bg(rgb(0xffffff)).into_any_element(),
        };
        let content = if self.preview_hidden {
            div().into_any_element()
        } else {
            content
        };
        let border = if dark { 0xd8eaff1c } else { 0xffffff77 };
        div()
            .size_full()
            .flex()
            .child(
                div()
                    .w(px(self.gallery_rail_width()))
                    .h_full()
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .bg(rgba(if dark { 0x17263f62 } else { 0xf8fbffb0 }))
                    .border_r_1()
                    .border_color(rgba(if dark { border } else { 0xffffffaa }))
                    .child(
                        div()
                            .px(px(22.))
                            .pt(px(24.))
                            .pb(px(20.))
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(brand_mark(36., dark))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .line_height(px(21.))
                                    .child(
                                        div()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_size(px(14.))
                                            .child("skillissue"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(0x7b8495))
                                            .line_height(px(16.5))
                                            .child("Interface review"),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("native-review-screens")
                            .track_scroll(&self.gallery_scroll)
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .px(px(12.))
                            .pt(px(4.))
                            .pb(px(16.))
                            .children(catalog::REVIEW_GROUPS.into_iter().enumerate().map(
                                |(index, group)| {
                                    div()
                                        .when(index > 0, |view| view.mt(px(18.)))
                                        .flex()
                                        .flex_col()
                                        .child(
                                            div()
                                                .px(px(10.))
                                                .mb(px(6.))
                                                .text_size(px(10.))
                                                .line_height(px(15.))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(rgb(0x9098a7))
                                                .child(group.to_uppercase()),
                                        )
                                        .children(catalog::group_screens(group).map(|entry| {
                                            let id = entry.id;
                                            let selected = id == self.review_id;
                                            Button::new(id)
                                                .ghost()
                                                .w_full()
                                                .h(px(32.))
                                                .px(px(10.))
                                                .py(px(7.))
                                                .rounded(px(9.))
                                                .bg(rgba(if selected {
                                                    if dark { 0xd7e9ff17 } else { 0xffffffbb }
                                                } else {
                                                    0
                                                }))
                                                .child(
                                                    div()
                                                        .w_full()
                                                        .text_size(px(12.))
                                                        .line_height(px(18.))
                                                        .text_color(rgb(if selected {
                                                            if dark { 0xe4efff } else { 0x305f9c }
                                                        } else if dark {
                                                            0xaabdd9
                                                        } else {
                                                            0x667084
                                                        }))
                                                        .child(entry.label),
                                                )
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.select_review(id, window, cx)
                                                    },
                                                ))
                                        }))
                                },
                            )),
                    )
                    .child(
                        div()
                            .px(px(22.))
                            .py(px(16.))
                            .border_t_1()
                            .border_color(rgba(border))
                            .text_size(px(10.))
                            .line_height(px(17.))
                            .text_color(rgb(0x939aaa))
                            .child(concat!(
                                "skillissue ",
                                env!("CARGO_PKG_VERSION"),
                                "\nNative interface review"
                            )),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(67.))
                            .flex_shrink_0()
                            .px(px(24.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_b_1()
                            .border_color(rgba(border))
                            .bg(rgba(if dark { 0x17263f62 } else { 0xf8fbff38 }))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .line_height(px(19.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(screen.label),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.))
                                            .line_height(px(15.))
                                            .text_color(rgb(0x7b8495))
                                            .child(screen.service),
                                    ),
                            )
                            .child(
                                Button::new("open-review-surface")
                                    .ghost()
                                    .xsmall()
                                    .icon(gpui_kit::assets::IconName::ExternalLink)
                                    .text_color(rgb(0x7b8495))
                                    .accessibility_label(
                                        "Open this screen without the review sidebar",
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.gallery = false;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .when(screen.view == "settings", |view| {
                                view.p(px(if self.gallery_rail_width() == 176. {
                                    10.
                                } else {
                                    20.
                                }))
                            })
                            .child(content),
                    )
                    .child(
                        div()
                            .h(px(30.))
                            .flex_shrink_0()
                            .px(px(16.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_t_1()
                            .border_color(rgba(border))
                            .text_size(px(9.))
                            .text_color(rgb(0x8c95a6))
                            .child(screen.source)
                            .child("Native Rust · local review fixtures"),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn select_review(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(screen) = catalog::screen(id) else {
            return;
        };
        self.preview_hidden = false;
        self.modal_backdrop = None;
        self.backdrop_attempted = false;
        self.recording_key = None;
        self.reset_settings_review();
        self.reset_dashboard_review();
        self.gallery_scroll.set_offset(point(px(0.), px(0.)));
        self.review_id = id.to_owned();
        self.overlay_ui = Default::default();
        self.expanded = false;
        match screen.view {
            "dashboard" => self.select_dashboard_page(screen.page.unwrap_or("home"), window, cx),
            "settings" => self.open_settings(screen.tab.unwrap_or("general"), cx),
            "control" => {
                self.page = Page::Live;
                self.overlay_ui.chat_shown = false;
            }
            "overlay" => {
                self.page = Page::Live;
                self.overlay_ui.chat_shown = true;
                self.expanded = matches!(screen.page, Some("sample" | "metadata"));
            }
            "splash" => self.page = Page::Splash,
            _ => {}
        }
        cx.notify();
    }
}

pub(super) fn brand_mark(size: f32, dark: bool) -> Div {
    div().size(px(size)).flex_shrink_0().child(
        svg()
            .path("brand/logo.svg")
            .size_full()
            .text_color(rgb(if dark { 0xe9ebee } else { 0x263246 })),
    )
}
