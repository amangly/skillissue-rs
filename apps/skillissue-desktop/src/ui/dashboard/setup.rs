//! Advancing the setup flow never sends requests or opens devices.
use super::*;

const STEPS: [(&str, &str, &str, &str); 3] = [
    (
        "Connect AI",
        "Connect Palepale, or choose a Text provider, add your key and select a model. Save, then Test.",
        "AI providers",
        "provider",
    ),
    (
        "Check audio",
        "Want transcription? Add an Audio provider, then test your microphone in General.",
        "Test microphone",
        "general",
    ),
    (
        "Learn your shortcuts",
        "Capture protection hides the app. Use your shortcut or the tray to show it again.",
        "View shortcuts",
        "shortcuts",
    ),
];

impl NativeView {
    pub(in crate::ui) fn render_setup_checklist(&self, cx: &mut Context<Self>) -> AnyElement {
        let step = self.dashboard.setup_step.min(STEPS.len() - 1);
        let (title, description, action, tab) = STEPS[step];
        if self.dashboard.setup_dismissed {
            return div()
                .child(
                    button("setup-reopen", "Setup")
                        .debug_selector(|| "setup-reopen".into())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.dashboard.setup_dismissed = false;
                            cx.notify();
                        })),
                )
                .into_any_element();
        }
        panel()
            .p_3()
            .flex()
            .flex_col()
            .gap_3()
            .debug_selector(|| "setup-checklist".into())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(brand_mark(24., self.is_dark(cx)))
                    .child(div().flex_1().text_sm().child("Set up skillissue"))
                    .child(muted(format!("{} of 3", step + 1)).text_xs())
                    .child(
                        button("setup-later", "Later")
                            .debug_selector(|| "setup-later".into())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dashboard.setup_dismissed = true;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .id(("setup-step", step))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .debug_selector(move || format!("setup-step-{step}"))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(muted(description).text_xs())
                    .when(step == 0, |view| {
                        view.child(
                            muted("Tests may use credits. Add Images for screenshot questions.")
                                .text_xs(),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        button("setup-action", action)
                            .debug_selector(|| "setup-action".into())
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.open_settings(tab, cx)),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        button("setup-back", "Back")
                            .disabled(step == 0)
                            .debug_selector(|| "setup-back".into())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dashboard.setup_step =
                                    this.dashboard.setup_step.saturating_sub(1);
                                cx.notify();
                            })),
                    )
                    .child(
                        button("setup-next", if step == 2 { "Done" } else { "Next" })
                            .debug_selector(|| "setup-next".into())
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.dashboard.setup_step >= 2 {
                                    this.dashboard.setup_dismissed = true;
                                } else {
                                    this.dashboard.setup_step += 1;
                                }
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }
}
