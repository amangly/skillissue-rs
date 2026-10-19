// Rust | ui/material.rs | native GPUI/DirectX | cached modal backdrop blur
use super::*;

impl NativeView {
    pub(super) fn prepare_modal_material(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let viewport = window.viewport_size();
        if viewport != self.modal_backdrop_size {
            self.modal_backdrop = None;
            self.backdrop_attempted = false;
            self.modal_backdrop_size = viewport;
        }
        let active = self.dashboard_modal_open() || self.settings_modal_open();
        let key = if active {
            format!(
                "{:?}:{}:{}:{}:{}",
                viewport,
                window.scale_factor(),
                self.is_dark(cx),
                self.gallery,
                self.review_id
            )
        } else {
            String::new()
        };
        if key != self.backdrop_key {
            self.backdrop_key = key;
            self.backdrop_generation = self.backdrop_generation.wrapping_add(1);
            self.modal_backdrop = None;
            self.capturing_backdrop = false;
            self.backdrop_attempted = false;
        }
        if !active {
            self.modal_backdrop = None;
            self.capturing_backdrop = false;
            self.backdrop_attempted = false;
            return;
        }
        #[cfg(feature = "native-effects")]
        if self.modal_backdrop.is_none() && !self.capturing_backdrop && !self.backdrop_attempted {
            self.capturing_backdrop = true;
            self.backdrop_attempted = true;
            let weak = cx.weak_entity();
            let generation = self.backdrop_generation;
            let sigma = if self.settings_modal_open() {
                10. / 3.
            } else {
                4.
            };
            window.on_next_frame(move |window, cx| {
                let result = (|| -> anyhow::Result<Arc<Image>> {
                    let pixels = window.render_to_image()?;
                    let small = image::imageops::resize(
                        &pixels,
                        pixels.width().div_ceil(3),
                        pixels.height().div_ceil(3),
                        image::imageops::FilterType::Triangle,
                    );
                    let blurred = image::imageops::blur(&small, sigma * window.scale_factor());
                    let mut bytes = std::io::Cursor::new(Vec::new());
                    image::DynamicImage::ImageRgba8(blurred)
                        .write_to(&mut bytes, image::ImageFormat::Png)?;
                    Ok(Arc::new(Image::from_bytes(
                        ImageFormat::Png,
                        bytes.into_inner(),
                    )))
                })();
                let _ = weak.update(cx, |this, cx| {
                    if this.backdrop_generation != generation {
                        return;
                    }
                    this.modal_backdrop = result
                        .map_err(|error| eprintln!("Render modal glass: {error:#}"))
                        .ok();
                    this.capturing_backdrop = false;
                    cx.notify();
                });
            });
            window.request_animation_frame();
        }
        #[cfg(not(feature = "native-effects"))]
        let _ = (window, cx);
    }

    pub(super) fn gallery_rail_width(&self) -> f32 {
        if f32::from(self.modal_backdrop_size.width) <= 900. {
            176.
        } else {
            216.
        }
    }

    pub(super) fn modal_material(&self, cx: &App) -> Div {
        self.modal_material_at(
            if self.gallery {
                self.gallery_rail_width()
            } else {
                0.
            },
            if self.gallery { 67. } else { 0. },
            if self.is_dark(cx) {
                0x060c1a70
            } else {
                0x53668b22
            },
        )
    }

    pub(super) fn settings_modal_material(&self) -> Div {
        let pad = if f32::from(self.modal_backdrop_size.width) <= 900. {
            10.
        } else {
            20.
        };
        let (x, y) = if self.gallery {
            (self.gallery_rail_width() + pad, 67. + pad)
        } else {
            (0., 0.)
        };
        self.modal_material_at(x, y, 0x53668b26)
    }
    fn modal_material_at(&self, x: f32, y: f32, tint: u32) -> Div {
        div().absolute().inset_0().overflow_hidden().when_some(
            self.modal_backdrop.clone(),
            |view, backdrop| {
                view.child(
                    img(backdrop)
                        .absolute()
                        .left(px(-x))
                        .top(px(-y))
                        .w(self.modal_backdrop_size.width)
                        .h(self.modal_backdrop_size.height)
                        .object_fit(ObjectFit::Fill),
                )
                .child(div().absolute().inset_0().bg(rgba(tint)))
            },
        )
    }
}
