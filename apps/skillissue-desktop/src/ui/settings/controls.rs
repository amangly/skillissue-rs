use super::*;

pub(super) fn section_title(title: &str, subtitle: &str, p: Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(
            div()
                .text_size(px(14.))
                .line_height(px(20.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_owned()),
        )
        .child(
            div()
                .text_size(px(12.))
                .line_height(px(17.))
                .text_color(rgb(p.muted))
                .child(subtitle.to_owned()),
        )
}
pub(super) fn caption(text: &str, p: Palette) -> Div {
    div()
        .mt_3()
        .text_size(px(11.))
        .text_color(rgb(p.muted))
        .child(text.to_owned())
}
pub(super) fn settings_action(id: impl Into<ElementId>, label: &str, _p: Palette) -> Button {
    let label = label.to_owned();
    crate::ui::style::action(id)
        .label(label.clone())
        .accessibility_label(label.clone())
        .debug_selector(move || format!("settings-action-{label}"))
}
pub(super) fn settings_switch(
    id: impl Into<ElementId>,
    checked: bool,
    label: &str,
) -> gpui_kit::component::switch::Switch {
    gpui_kit::component::switch::Switch::new(id)
        .small()
        .checked(checked)
        .accessibility_label(label.to_owned())
}
pub(super) fn keycaps(accelerator: &str, p: Palette) -> Div {
    let chord = accelerator
        .to_lowercase()
        .replace("control", "ctrl")
        .replace("arrow", "")
        .replace('+', "-");
    let content = if accelerator.is_empty() {
        div()
            .text_xs()
            .text_color(rgb(p.muted))
            .child("Unassigned")
            .into_any_element()
    } else if let Ok(stroke) = Keystroke::parse(&chord) {
        gpui_kit::component::kbd::Kbd::new(stroke)
            .outline()
            .into_any_element()
    } else {
        // Unknown accelerator spellings remain visible without changing the saved binding.
        div()
            .text_xs()
            .child(accelerator.to_owned())
            .into_any_element()
    };
    div().flex().flex_shrink_0().child(content)
}
pub(super) const LANGUAGES: &[(&str, &str)] = &[
    ("", "Auto detect"),
    ("ar", "Arabic (العربية)"),
    ("bn", "Bengali (বাংলা)"),
    ("yue", "Cantonese (粵語)"),
    ("de", "German (Deutsch)"),
    ("en", "English"),
    ("fa", "Farsi (فارسی)"),
    ("fr", "French (Français)"),
    ("hi", "Hindi (हिंदी)"),
    ("it", "Italian (Italiano)"),
    ("ja", "Japanese (日本語)"),
    ("ko", "Korean (한국어)"),
    ("zh", "Mandarin (普通话)"),
    ("nl", "Dutch (Nederlands)"),
    ("pl", "Polish (Polski)"),
    ("pt", "Portuguese (Português)"),
    ("ru", "Russian (Русский)"),
    ("es", "Spanish (Español)"),
    ("ta", "Tamil (தமிழ்)"),
    ("th", "Thai (ไทย)"),
    ("tr", "Turkish (Türkçe)"),
    ("ur", "Urdu (اردو)"),
];
