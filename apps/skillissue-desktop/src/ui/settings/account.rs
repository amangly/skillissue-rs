use super::*;
use gpui_kit::component::{chart::BarChart, plot::shape::BarAlignment, progress::Progress};

fn dollars(value: u64) -> String {
    format!("${}.{:06}", value / 1_000_000, value % 1_000_000)
}

fn ratio(value: u64, total: u64) -> f32 {
    if total == 0 {
        0.
    } else {
        (value as f64 / total as f64).clamp(0., 1.) as f32
    }
}

fn count(value: u64) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.)
    } else {
        value.to_string()
    }
}

fn stat(title: &str, value: String, p: Palette) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_size(px(11.))
                .text_color(rgb(p.muted))
                .child(title.to_owned()),
        )
        .child(
            div()
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(value),
        )
}

fn meter(id: String, title: String, value: String, fraction: f32, p: Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .justify_between()
                .gap_2()
                .text_size(px(11.))
                .child(title.clone())
                .child(value),
        )
        .child(
            Progress::new(id)
                .value(fraction * 100.)
                .color(rgb(0x28ad83))
                .accessibility_label(title)
                .h(px(5.))
                .w_full(),
        )
        .text_color(rgb(p.foreground))
}

impl NativeView {
    pub(super) fn palepale_usage_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.settings_palette();
        let expanded = self.settings_ui.palepale_details;
        let mut panel = div()
            .id("palepale-usage-panel")
            .debug_selector(|| "palepale-usage-panel".into())
            .p(px(10.))
            .border_1()
            .border_color(rgb(p.line))
            .rounded(px(12.))
            .text_color(rgb(p.foreground))
            .flex()
            .flex_col()
            .gap(px(6.));
        if self.settings_ui.palepale_account_loading {
            return panel.child(div().text_size(px(12.)).child("Refreshing account…"));
        }
        let Some(Ok(status)) = &self.settings_ui.palepale_account_result else {
            return panel.child(
                div()
                    .text_size(px(12.))
                    .child("Refresh to load account usage."),
            );
        };
        panel = panel.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Account"),
                )
                .child(
                    Button::new("palepale-details")
                        .debug_selector(|| "palepale-details-toggle".into())
                        .ghost()
                        .h(px(24.))
                        .px_2()
                        .label(if expanded { "Less" } else { "Details" })
                        .accessibility_label(if expanded {
                            "Hide account details"
                        } else {
                            "Show account details"
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.settings_ui.palepale_details = !this.settings_ui.palepale_details;
                            cx.notify();
                        })),
                ),
        );
        let details = status.details.as_ref();
        let available = details
            .and_then(|d| d.available_microdollars)
            .map(|value| {
                if value < 0 {
                    format!("−{}", dollars(value.unsigned_abs()))
                } else {
                    dollars(value as u64)
                }
            })
            .unwrap_or_else(|| "Unavailable".into());
        panel = panel.child(
            div()
                .flex()
                .gap_3()
                .child(stat("Available", available, p))
                .child(stat(
                    "Reserved",
                    details
                        .and_then(|d| d.reserved_microdollars)
                        .map(dollars)
                        .unwrap_or_else(|| "Unavailable".into()),
                    p,
                ))
                .child(stat(
                    "Charged · all time",
                    status
                        .usage
                        .as_ref()
                        .map(|u| dollars(u.spent_microdollars))
                        .unwrap_or_else(|| "Unavailable".into()),
                    p,
                )),
        );
        if let Some(details) = details {
            for (index, pass) in details.passes.iter().flatten().enumerate() {
                let available = pass
                    .remaining_microdollars
                    .saturating_sub(pass.reserved_microdollars);
                panel = panel.child(meter(
                    format!("palepale-pass-{index}"),
                    pass.plan.clone(),
                    format!(
                        "{:.0}% left",
                        ratio(available, pass.allowance_microdollars) * 100.
                    ),
                    ratio(available, pass.allowance_microdollars),
                    p,
                ));
            }
            if details.passes.as_ref().is_some_and(Vec::is_empty) {
                panel = panel.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(p.muted))
                        .child("No active passes"),
                );
            } else if details.passes.is_none() {
                panel = panel.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(p.muted))
                        .child("Passes unavailable"),
                );
            }
            if let (Some(limit), Some(spent), Some(held)) = (
                details.wallet_spend_limit_microdollars,
                details.wallet_spent_microdollars,
                details.reserved_microdollars,
            ) {
                panel = panel.child(meter(
                    "palepale-wallet-limit".into(),
                    "Wallet limit · lifetime".into(),
                    format!(
                        "{} / {}",
                        dollars(spent.saturating_add(held)),
                        dollars(limit)
                    ),
                    ratio(spent.saturating_add(held), limit),
                    p,
                ));
            }
        }
        if let Some(usage) = &status.usage {
            panel = panel
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .text_size(px(11.))
                        .child(
                            div()
                                .text_color(rgb(p.muted))
                                .child("Tokens · all time · all devices"),
                        )
                        .child(format!("{} requests", count(usage.requests))),
                )
                .child(
                    div()
                        .id("palepale-token-chart")
                        .debug_selector(|| "palepale-token-chart".into())
                        .w_full()
                        .h(px(64.))
                        .child(
                            BarChart::new(vec![
                                ("Input", usage.prompt_tokens),
                                ("Output", usage.completion_tokens),
                            ])
                            .id("palepale-tokens")
                            .band(|d| d.0)
                            .value(|d| d.1 as f64)
                            .alignment(BarAlignment::Left)
                            .grid(false)
                            .appear(false)
                            .label(|d| count(d.1))
                            .label_color(move |_| rgb(p.foreground))
                            .fill(|d, _, _, _| {
                                rgb(if d.0 == "Input" { 0x28ad83 } else { 0x649bea })
                            })
                            .tooltip_value(|d, _| format!("{} tokens", d.1).into()),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_x_3()
                        .gap_y_1()
                        .text_size(px(11.))
                        .text_color(rgb(p.muted))
                        .child(format!(
                            "Cached {} ({:.0}%)",
                            count(usage.cached_tokens),
                            ratio(usage.cached_tokens, usage.prompt_tokens) * 100.
                        ))
                        .child(format!("Reasoning {}", count(usage.reasoning_tokens)))
                        .when(usage.unknown_requests > 0, |v| {
                            v.child(format!("{} unresolved", usage.unknown_requests))
                        }),
                );
        } else {
            panel = panel.child(
                div().text_size(px(11.)).text_color(rgb(p.muted)).child(
                    status
                        .usage_error
                        .clone()
                        .unwrap_or_else(|| "Usage unavailable. Refresh to retry.".into()),
                ),
            );
        }
        if expanded {
            let mut extra = div()
                .id("palepale-account-details")
                .debug_selector(|| "palepale-account-details".into())
                .pt_2()
                .border_t_1()
                .border_color(rgb(p.line))
                .flex()
                .flex_col()
                .gap_1()
                .text_size(px(11.))
                .text_color(rgb(p.muted));
            if let Some(details) = details {
                extra = extra.child(format!(
                    "Wallet fallback: {}",
                    match details.balance_fallback {
                        Some(true) => "On",
                        Some(false) => "Off",
                        None => "Unavailable",
                    }
                ));
                for pass in details.passes.iter().flatten() {
                    extra = extra
                        .child(format!(
                            "{}: {} / {} available",
                            pass.plan,
                            dollars(
                                pass.remaining_microdollars
                                    .saturating_sub(pass.reserved_microdollars)
                            ),
                            dollars(pass.allowance_microdollars)
                        ))
                        .child(format!(
                            "Expires {} · Reserved {}",
                            pass.ends_at,
                            dollars(pass.reserved_microdollars)
                        ));
                    if let Some(refill) = &pass.refills {
                        extra = extra.child(format!("Full refill: {refill}"));
                    }
                    if !pass.scope.is_empty() {
                        extra = extra.child(format!("Scope: {}", pass.scope.join(", ")));
                    }
                }
            }
            if let Some(usage) = &status.usage {
                extra = extra
                    .child(format!(
                        "Requests {} · Confirmed {} · Unresolved {}",
                        usage.requests, usage.confirmed_requests, usage.unknown_requests
                    ))
                    .child(format!(
                        "Input {} · Output {}",
                        usage.prompt_tokens, usage.completion_tokens
                    ))
                    .child(format!(
                        "Cached {} · Reasoning {}",
                        usage.cached_tokens, usage.reasoning_tokens
                    ));
            }
            panel = panel.child(extra.child("Cached and reasoning tokens are included in input and output. Unresolved requests may hold funds.")
                .child("Refreshes about every minute while open."));
        }
        panel
    }
}

#[cfg(test)]
mod tests {
    use super::{dollars, ratio};
    #[test]
    fn money_and_bars_keep_microdollars_and_handle_zero_or_overdrawn_limits() {
        assert_eq!(dollars(4_949_716), "$4.949716");
        assert_eq!(dollars(1), "$0.000001");
        assert_eq!(ratio(10, 0), 0.);
        assert_eq!(ratio(20, 10), 1.);
    }
}
