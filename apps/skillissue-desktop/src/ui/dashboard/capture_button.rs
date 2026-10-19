use super::*;

impl NativeView {
    pub(super) fn capture_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let enabled = self.settings.capture_protection;
        let surface = cx.theme().popover;
        let foreground = cx.theme().foreground;
        let border = cx.theme().border;
        // Native edge lighting inspired by the supplied SpecularButton example.
        // Paint below the real button so text, focus, and hit testing stay intact.
        div()
            .id("capture-specular")
            .relative()
            .h(px(style::CONTROL_HEIGHT))
            .rounded(px(style::CONTROL_RADIUS))
            .overflow_hidden()
            .on_hover(cx.listener(|_, _, _, cx| cx.notify()))
            .on_mouse_move(cx.listener(|_, _, _, cx| cx.notify()))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let pointer = window.mouse_position();
                        let width = f32::from(bounds.size.width);
                        let height = f32::from(bounds.size.height);
                        let radius = style::CONTROL_RADIUS.min(height * 0.5);
                        let center = bounds.center();
                        let dx = f32::from(pointer.x - center.x);
                        let dy = f32::from(pointer.y - center.y);
                        let proximity = (1.
                            - (dx.abs() - width * 0.5)
                                .max(0.)
                                .hypot((dy.abs() - height * 0.5).max(0.))
                                / 180.)
                            .max(0.);
                        let strength = if window.is_window_active() {
                            proximity
                        } else {
                            0.
                        };
                        let spot = if strength > 0. { dy.atan2(dx) } else { -2.3 };
                        window.paint_quad(fill(bounds, surface).corner_radii(px(radius)));
                        window.paint_quad(
                            fill(
                                bounds,
                                rgb(0xb7d4ff).opacity(if enabled { 0.08 } else { 0.025 }),
                            )
                            .corner_radii(px(radius)),
                        );
                        // Follow the rounded perimeter with a bright key and a softer opposite reflection.
                        let samples = ((width + height) * 2.).ceil() as usize;
                        for i in 0..samples {
                            let angle = i as f32 / samples as f32 * std::f32::consts::TAU;
                            let (sin, cos) = angle.sin_cos();
                            let inner_x = (width * 0.5 - radius).max(0.);
                            let inner_y = (height * 0.5 - radius).max(0.);
                            let x = inner_x * cos.signum() + radius * cos;
                            let y = inner_y * sin.signum() + radius * sin;
                            // Interpolate straight sides between corner arcs separately below.
                            let point_angle = y.atan2(x);
                            let gap = (point_angle - spot).cos();
                            let shine = gap.max(0.).powi(18) + (-gap).max(0.).powi(18) * 0.4;
                            let alpha =
                                (0.16 + strength * 0.7) * shine * if enabled { 1. } else { 0.6 };
                            let origin = point(center.x + px(x), center.y + px(y));
                            window.paint_quad(fill(
                                Bounds::new(origin, size(px(1.), px(1.))),
                                rgb(0xd8eaff).opacity(alpha),
                            ));
                        }
                        for horizontal in [true, false] {
                            let length = if horizontal {
                                width - 2. * radius
                            } else {
                                height - 2. * radius
                            };
                            for i in 0..length.ceil() as usize {
                                for side in [-1., 1.] {
                                    let (x, y) = if horizontal {
                                        (
                                            -width * 0.5 + radius + i as f32,
                                            side * (height * 0.5 - 1.),
                                        )
                                    } else {
                                        (
                                            side * (width * 0.5 - 1.),
                                            -height * 0.5 + radius + i as f32,
                                        )
                                    };
                                    let gap = (y.atan2(x) - spot).cos();
                                    let shine =
                                        gap.max(0.).powi(18) + (-gap).max(0.).powi(18) * 0.4;
                                    let alpha = (0.2 + strength * 0.8)
                                        * shine
                                        * if enabled { 1. } else { 0.6 };
                                    let origin = point(center.x + px(x), center.y + px(y));
                                    window.paint_quad(fill(
                                        Bounds::new(origin, size(px(1.), px(1.))),
                                        rgb(0xd8eaff).opacity(alpha),
                                    ));
                                }
                            }
                        }
                    },
                )
                .absolute()
                .inset_0(),
            )
            .child(
                style::action("capture-status")
                    .relative()
                    .ghost()
                    .border_1()
                    .border_color(border.opacity(0.45))
                    .text_color(foreground)
                    .label(if enabled {
                        "Capture protection on"
                    } else {
                        "Capture protection off"
                    })
                    .icon(if enabled { Glyph::EyeOff } else { Glyph::Eye })
                    .debug_selector(|| "capture-status".into())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.change_preference(|settings| {
                            settings.capture_protection = !settings.capture_protection
                        });
                        cx.notify();
                    })),
            )
    }
}
