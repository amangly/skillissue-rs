//! Native, bounded ASCII horizon inspired by the supplied GradientASCII reference.
use super::*;

impl NativeView {
    pub(super) fn ascii_background(&self, cx: &App) -> impl IntoElement {
        let dark = self.is_dark(cx);
        let animate = self.shown
            && !self.gallery
            && !self.dashboard_modal_open()
            && !self.settings_modal_open()
            && !self.capturing_backdrop;
        canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                let active = animate && window.is_window_active();
                let phase = if active {
                    (now_ms() % 120_000) as f32 / 120_000. * std::f32::consts::TAU
                } else {
                    0.
                };
                let width = f32::from(bounds.size.width);
                let height = f32::from(bounds.size.height);
                let cell = (width / 100.).max(height / 65.).max(18.);
                let glyphs: Vec<_> = ["=", "+", "x", "X", "0", "S", "8", "#", "@"]
                    .into_iter()
                    .enumerate()
                    .map(|(i, glyph)| {
                        window.text_system().shape_line(
                            glyph.into(),
                            px(12.),
                            &[TextRun {
                                len: 1,
                                font: font("JetBrains Mono"),
                                color: rgb(if dark { 0x8cbaff } else { 0x496f9e })
                                    .opacity(0.07 + i as f32 * 0.012)
                                    .into(),
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            }],
                            None,
                        )
                    })
                    .collect();
                for row in 0..(height / cell) as usize {
                    for col in 0..(width / cell) as usize {
                        let x = (col as f32 + 0.5) * cell;
                        let y = (row as f32 + 0.5) * cell;
                        let nx = x / width.max(1.) * 2. - 1.;
                        let horizon = height * (0.65 + nx * nx * 0.17 + phase.sin() * 0.015);
                        let light = (1. - (y - horizon).abs() / (height * 0.42).max(1.)).max(0.);
                        let hash = ((col * 37 + row * 73 + col * row * 11) % 101) as f32 / 101.;
                        if light < 0.15 || hash > light * 0.62 {
                            continue;
                        }
                        let pointer = window.mouse_position();
                        let near = (f32::from(pointer.x - bounds.left()) - x)
                            .hypot(f32::from(pointer.y - bounds.top()) - y)
                            < 65.;
                        let index = ((light * 7.) as usize + usize::from(active && near)).min(8);
                        let _ = glyphs[index].paint(
                            point(bounds.left() + px(x), bounds.top() + px(y)),
                            px(cell),
                            TextAlign::Left,
                            None,
                            window,
                            cx,
                        );
                    }
                }
                if active {
                    window.request_animation_frame();
                }
            },
        )
        .absolute()
        .inset_0()
    }
}
