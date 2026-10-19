// Overlay state, icons and shared visual primitives.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use gpui_kit::assets::IconName as Glyph;
use std::{cell::RefCell, rc::Rc, time::Instant};

mod composer;
mod interaction;
mod menu;
mod presentation;

fn solid_mic() -> Icon {
    Icon::new(Glyph::Mic).size(px(18.))
}
fn solid_send() -> Icon {
    Icon::new(Glyph::ArrowUp).size(px(16.))
}
fn solid_square(size: f32) -> Icon {
    Icon::new(Glyph::Square).size(px(size))
}
struct ScreenshotPreview {
    source: String,
    image: Result<Arc<Image>, String>,
}

pub(super) struct OverlayState {
    pub(super) audio_notice: Option<String>,
    pub(super) chat_shown: bool,
    pub(super) menu: bool,
    pub(super) modes: bool,
    pub(super) tools: bool,
    pub(super) transcript: bool,
    pub(super) focused: bool,
    copied: Option<String>,
    copied_at: Option<Instant>,
    metadata_open: bool,
    menu_selection: Option<usize>,
    mode_selection: usize,
    tool_selection: Option<usize>,
    root_bounds: Rc<RefCell<Option<Bounds<Pixels>>>>,
    menu_trigger: Rc<RefCell<Option<Bounds<Pixels>>>>,
    tools_trigger: Rc<RefCell<Option<Bounds<Pixels>>>>,
    mode_bounds: Rc<RefCell<Option<Bounds<Pixels>>>>,
    previous_content: (usize, usize),
    previous_busy: bool,
    previous_scroll_max: Pixels,
    levels: [f32; 2],
    was_active: bool,
    preserve_blur_until: Option<Instant>,
    screenshot: RefCell<Option<ScreenshotPreview>>,
}
impl Default for OverlayState {
    fn default() -> Self {
        Self {
            audio_notice: None,
            chat_shown: true,
            menu: false,
            modes: false,
            tools: false,
            transcript: false,
            focused: false,
            copied: None,
            copied_at: None,
            metadata_open: false,
            menu_selection: None,
            mode_selection: 0,
            tool_selection: None,
            root_bounds: Default::default(),
            menu_trigger: Default::default(),
            tools_trigger: Default::default(),
            mode_bounds: Default::default(),
            previous_content: (0, 0),
            previous_busy: false,
            previous_scroll_max: px(0.),
            levels: [0.; 2],
            was_active: false,
            preserve_blur_until: None,
            screenshot: RefCell::new(None),
        }
    }
}

fn screenshot_image(data: &str) -> Result<Arc<Image>, String> {
    let encoded = data
        .strip_prefix("data:image/png;base64,")
        .ok_or("Expected a PNG screen capture.")?;
    if encoded.len() > 24 * 1024 * 1024 {
        return Err("The screen capture exceeds the preview size limit.".into());
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "The screen capture contains invalid image data.")?;
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("The screen capture is not a PNG image.".into());
    }
    Ok(Arc::new(Image::from_bytes(ImageFormat::Png, bytes)))
}

fn overlay_material(angle: f32, top: u32, bottom: u32) -> Background {
    linear_gradient(
        angle,
        linear_color_stop(rgba(top), 0.),
        linear_color_stop(rgba(bottom), 1.),
    )
}

fn overlay_blue() -> Background {
    overlay_material(155., 0x6d9fe4ff, 0x4c86d4ff)
}

fn overlay_icon_button(id: &'static str, glyph: Glyph, size: f32, color: u32) -> Button {
    Button::new(id)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .ghost()
        .relative()
        .w(px(style::COMPACT_HEIGHT))
        .h(px(style::COMPACT_HEIGHT))
        .p_0()
        .rounded(px(style::CONTROL_RADIUS))
        .child(Icon::new(glyph).size(px(size)).text_color(rgb(color)))
}

fn track_overlay_bounds(target: Rc<RefCell<Option<Bounds<Pixels>>>>) -> impl IntoElement {
    canvas(
        move |bounds, _, _| {
            target.replace(Some(bounds));
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
}

fn menu_button(
    id: impl Into<ElementId>,
    glyph: Option<Glyph>,
    label: impl Into<String>,
    ink: u32,
    secondary: u32,
    right: impl IntoElement,
) -> Button {
    let label = label.into();
    Button::new(id)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .ghost()
        .w_full()
        .h(px(style::CONTROL_HEIGHT))
        .px(px(8.))
        .py_0()
        .rounded(px(7.))
        .accessibility_label(label.clone())
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(14.))
                .line_height(px(20.))
                .text_color(rgb(ink))
                .when_some(glyph, |view, glyph| {
                    view.child(Icon::new(glyph).size(px(16.)).text_color(rgb(secondary)))
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(label),
                )
                .child(right),
        )
}

fn menu_separator(dark: bool) -> Div {
    div()
        .h(px(1.))
        .my(px(4.))
        .mx(px(-4.))
        .bg(rgba(if dark { 0xffffff1a } else { 0x7c94b829 }))
}

fn visual_switch(checked: bool, dark: bool) -> Div {
    div()
        .relative()
        .w(px(style::COMPACT_HEIGHT))
        .h(px(14.))
        .flex_shrink_0()
        .rounded_full()
        .bg(if checked {
            overlay_material(180., 0x74a7e6ff, 0x4d8bd9ff)
        } else {
            overlay_material(
                180.,
                if dark { 0x3f3f46ff } else { 0x8da3c247 },
                if dark { 0x3f3f46ff } else { 0x8da3c247 },
            )
        })
        .child(
            div()
                .absolute()
                .left(px(if checked { 12. } else { 2. }))
                .top(px(2.))
                .size(px(10.))
                .rounded_full()
                .bg(rgb(0xffffff)),
        )
}

fn protected_outline(dark: bool) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let width = f32::from(bounds.size.width);
            let height = f32::from(bounds.size.height);
            let color = rgba(if dark { 0x10121b85 } else { 0x647ea548 });
            let mut dot = |x: f32, y: f32| {
                window.paint_quad(
                    fill(
                        Bounds::new(
                            bounds.origin + point(px(x - 2.), px(y - 2.)),
                            size(px(4.), px(4.)),
                        ),
                        color,
                    )
                    .corner_radii(px(2.)),
                )
            };
            let radius = 20.;
            let mut x = radius + 2.;
            while x < width - radius {
                dot(x, 2.);
                dot(x, height - 2.);
                x += 8.;
            }
            let mut y = radius + 2.;
            while y < height - radius {
                dot(2., y);
                dot(width - 2., y);
                y += 8.;
            }
            for (center_x, center_y, start) in [
                (radius + 2., radius + 2., 180.),
                (width - radius - 2., radius + 2., 270.),
                (width - radius - 2., height - radius - 2., 0.),
                (radius + 2., height - radius - 2., 90.),
            ] {
                for index in 0..4 {
                    let angle = (start + index as f32 * 22.5).to_radians();
                    dot(
                        center_x + radius * angle.cos(),
                        center_y + radius * angle.sin(),
                    );
                }
            }
        },
    )
    .absolute()
    .inset_0()
}
