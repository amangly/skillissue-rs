//! Root composition and reusable native controls.
use super::*;

impl Render for NativeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Theme changes reload kit defaults. Restore our palette before building controls.
        let background: Hsla = rgb(if cx.theme().mode.is_dark() {
            0x172236
        } else {
            0xe1e8f4
        })
        .into();
        if cx.theme().background != background {
            crate::theme::apply(cx);
        }
        self.overlay_ui.focused = self.question.focus_handle(cx).is_focused(window);
        if window.focused(cx).is_none() {
            window.focus(&self.root_focus, cx);
        }
        self.prepare_modal_material(window, cx);
        if self.gallery {
            self.rendered_screens.insert(self.review_id.clone());
        }
        let content = if self.gallery {
            self.render_gallery(cx)
        } else {
            self.render_surface(cx)
        };
        let transparent = self.is_overlay && !self.gallery;
        div()
            .id("skillissue-root")
            .track_focus(&self.root_focus)
            .relative()
            .size_full()
            .font_family("Segoe UI")
            .text_size(px(14.))
            .text_color(cx.theme().foreground)
            .when(!transparent, |view| {
                view.bg(linear_gradient(
                    140.,
                    linear_color_stop(
                        rgba(if self.is_dark(cx) {
                            0x172236c9
                        } else {
                            0xe1e8f4d9
                        }),
                        0.,
                    ),
                    linear_color_stop(
                        rgba(if self.is_dark(cx) {
                            0x182b45c9
                        } else {
                            0xd5e3f3d9
                        }),
                        1.,
                    ),
                ))
                .when(self.page != Page::Live, |view| {
                    view.child(self.ascii_background(cx))
                })
            })
            .capture_key_down(
                cx.listener(|this, event, window, cx| this.record_key(event, window, cx)),
            )
            .child(content)
            .when_some(
                self.error
                    .clone()
                    .filter(|_| !transparent && self.page != Page::Live),
                |view, error| {
                    view.child(
                        div()
                            .absolute()
                            .bottom(px(16.))
                            .left(px(16.))
                            .right(px(16.))
                            .rounded_xl()
                            .border_1()
                            .border_color(rgba(0xffffffb0))
                            .bg(rgba(if self.is_dark(cx) {
                                0x243751ed
                            } else {
                                0xffffffed
                            }))
                            .shadow_lg()
                            .p_3()
                            .flex()
                            .items_center()
                            .gap_3()
                            .text_size(px(12.))
                            .text_color(rgb(if self.is_dark(cx) { 0xffc5d0 } else { 0xa5363d }))
                            .child(div().flex_1().child(error))
                            .child(
                                Button::new("dismiss-error")
                                    .ghost()
                                    .xsmall()
                                    .icon(gpui_kit::assets::IconName::X)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.error = None;
                                        cx.notify();
                                    })),
                            ),
                    )
                },
            )
            .when(self.engine.is_none(), |view| {
                view.child(
                    Button::new("retry-open")
                        .absolute()
                        .bottom(px(65.))
                        .left(px(24.))
                        .label(if self.opening {
                            "Opening workspace…"
                        } else {
                            "Retry opening workspace"
                        })
                        .disabled(self.opening)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.error = None;
                            this.boot();
                            cx.notify();
                        })),
                )
            })
    }
}

pub(super) fn markdown(id: impl Into<ElementId>, text: String) -> TextView {
    TextView::markdown(id, text)
        .selectable(true)
        .scrollable(false)
        .on_link_click(|url, _, _, cx| {
            if url.starts_with("https://") || url.starts_with("http://") {
                cx.open_url(url);
            }
        })
}
pub(super) fn date(timestamp: u64) -> String {
    chrono::DateTime::from_timestamp_millis(timestamp as i64)
        .map(|time| time.format("%b %d, %H:%M").to_string())
        .unwrap_or_default()
}
