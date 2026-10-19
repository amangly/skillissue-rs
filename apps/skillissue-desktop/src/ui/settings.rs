use super::*;
use gpui_kit::{
    assets::IconName as Lucide,
    base::FocusTrapElement,
    component::{
        Icon,
        menu::{DropdownMenu, PopupMenuItem},
        switch::Switch,
    },
};
use std::{
    collections::{BTreeSet, VecDeque},
    sync::atomic::{AtomicBool, Ordering},
};

mod account;
mod byok;
mod connections;
mod controls;
mod general;
mod keybinds;
mod provider;
mod recovery;
use controls::*;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum SettingsTab {
    General,
    Shortcuts,
    Language,
    Provider,
}

enum MicrophoneEvent {
    Level(f32),
    Status(String),
    Error(String),
    Finished,
}

pub(super) struct SettingsUi {
    pub(super) tab: SettingsTab,
    system_dark: bool,
    notice: Option<(String, String)>,
    pub(super) provider_prompt: Entity<TextareaState>,
    pub(super) chunk_seconds: Entity<InputState>,
    pub(super) byok_generation: u64,
    pub(super) byok_cancel: Option<tokio_util::sync::CancellationToken>,
    pub(super) byok_models: Vec<String>,
    pub(super) connection_models: std::collections::BTreeMap<(u8, String), Vec<String>>,
    pub(super) byok_check_audio: bool,
    pub(super) byok_check_image: bool,
    pub(super) byok_status: String,
    byok_reveal: bool,
    pub(super) byok_advanced: bool,
    image_endpoint: Entity<InputState>,
    image_model: Entity<InputState>,
    image_key: Entity<InputState>,
    remove_image_key: bool,
    audio_enabled_draft: Option<bool>,
    image_enabled_draft: Option<bool>,
    connection_drafts: std::collections::BTreeMap<(u8, String), (String, String)>,
    byok_preset: Option<&'static str>,
    byok_filter: Entity<InputState>,
    remove_key: bool,
    transcription_endpoint: Entity<InputState>,
    transcription_key: Entity<InputState>,
    remove_transcription_key: bool,
    pub(super) provider_mode: ProviderMode,
    provider_mode_dirty: bool,
    pub(super) palepale_key: Entity<InputState>,
    pub(super) palepale_model: String,
    palepale_model_dirty: bool,
    pub(super) palepale_reasoning_effort: Option<String>,
    palepale_reasoning_dirty: bool,
    pub(super) remove_palepale_key: bool,
    pub(super) has_byok_key: bool,
    pub(super) has_audio_key: bool,
    pub(super) has_image_key: bool,
    pub(super) has_palepale_key: bool,
    /// Hosted authentication is an explicit browser pairing flow. Keep only the public
    /// approval code in the view; the device code and released credential stay in core.
    pub(super) palepale_pairing: PalepalePairingState,
    pub(super) palepale_pairing_generation: u64,
    pub(super) palepale_pairing_cancel: Option<tokio_util::sync::CancellationToken>,
    pub(super) palepale_account_generation: u64,
    pub(super) palepale_account_cancel: Option<tokio_util::sync::CancellationToken>,
    pub(super) palepale_account_loading: bool,
    pub(super) palepale_details: bool,
    pub(super) palepale_account_result:
        Option<Result<skillissue_core::PalepaleAccountStatus, String>>,
    pub(super) catalog_generation: u64,
    pub(super) catalog_cancel: Option<tokio_util::sync::CancellationToken>,
    pub(super) catalog_models: Vec<ProviderModel>,
    pub(super) catalog_loading: bool,
    pub(super) catalog_error: Option<String>,
    microphone_stop: Option<Arc<AtomicBool>>,
    microphone_generation: u64,
    microphone_sender: mpsc::Sender<(u64, MicrophoneEvent)>,
    microphone_receiver: mpsc::Receiver<(u64, MicrophoneEvent)>,
    microphone_levels: VecDeque<f32>,
    microphone_status: String,
    subscriptions: Vec<Subscription>,
    shortcut_focus: FocusHandle,
    modal_focus: FocusHandle,
    modal_initial: FocusHandle,
    modal_was_open: bool,
    restore_focus: Option<FocusHandle>,
    pub(super) provider_save_pending: bool,
    provider_dirty: BTreeSet<&'static str>,
    provider_bound: bool,
    saved_until: Option<std::time::Instant>,
    viewport_width: f32,
    viewport_height: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) enum PalepalePairingState {
    #[default]
    Idle,
    Starting,
    Waiting {
        code: String,
        url: String,
        ttl_seconds: u64,
    },
    Ready,
    Error(String),
}

impl SettingsUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<NativeView>) -> Self {
        let (microphone_sender, microphone_receiver) = mpsc::channel();
        Self {
            tab: SettingsTab::General,
            system_dark: matches!(
                cx.window_appearance(),
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            ),
            notice: None,
            provider_prompt: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .rows(5)
                    .default_value(Settings::default().system_prompt)
            }),
            chunk_seconds: cx.new(|cx| InputState::new(window, cx).default_value("8")),
            byok_generation: 0,
            byok_cancel: None,
            byok_models: Vec::new(),
            connection_models: Default::default(),
            byok_check_audio: false,
            byok_check_image: false,
            byok_status: "Not checked. Save your connection, then check model access.".into(),
            byok_reveal: false,
            byok_advanced: false,
            image_endpoint: cx.new(|cx| InputState::new(window, cx)),
            image_model: cx.new(|cx| InputState::new(window, cx)),
            image_key: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder("API key")
            }),
            remove_image_key: false,
            audio_enabled_draft: None,
            image_enabled_draft: None,
            connection_drafts: Default::default(),
            byok_preset: None,
            byok_filter: cx.new(|cx| InputState::new(window, cx).placeholder("Filter model IDs")),
            remove_key: false,
            transcription_endpoint: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Blank uses the chat connection")
            }),
            transcription_key: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder("Separate transcription API key")
            }),
            remove_transcription_key: false,
            provider_mode: ProviderMode::Byok,
            provider_mode_dirty: false,
            palepale_key: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Palepale account or device key")
                    .masked(true)
            }),
            palepale_model: String::new(),
            palepale_model_dirty: false,
            palepale_reasoning_effort: None,
            palepale_reasoning_dirty: false,
            remove_palepale_key: false,
            has_byok_key: false,
            has_audio_key: false,
            has_image_key: false,
            has_palepale_key: false,
            palepale_pairing: PalepalePairingState::Idle,
            palepale_pairing_generation: 0,
            palepale_pairing_cancel: None,
            palepale_account_generation: 0,
            palepale_account_cancel: None,
            palepale_account_loading: false,
            palepale_details: false,
            palepale_account_result: None,
            catalog_generation: 0,
            catalog_cancel: None,
            catalog_models: Vec::new(),
            catalog_loading: false,
            catalog_error: None,
            microphone_stop: None,
            microphone_generation: 0,
            microphone_sender,
            microphone_receiver,
            microphone_levels: VecDeque::from(vec![0.; 17]),
            microphone_status: "Connecting to your microphone...".into(),
            subscriptions: Vec::new(),
            shortcut_focus: cx.focus_handle(),
            modal_focus: cx.focus_handle(),
            modal_initial: cx.focus_handle(),
            modal_was_open: false,
            restore_focus: None,
            provider_save_pending: false,
            provider_dirty: BTreeSet::new(),
            provider_bound: false,
            saved_until: None,
            viewport_width: f32::from(window.viewport_size().width),
            viewport_height: f32::from(window.viewport_size().height),
        }
    }
}
impl Drop for SettingsUi {
    fn drop(&mut self) {
        if let Some(cancel) = self.byok_cancel.take() {
            cancel.cancel();
        }
        if let Some(cancel) = self.catalog_cancel.take() {
            cancel.cancel();
        }
        if let Some(cancel) = self.palepale_pairing_cancel.take() {
            cancel.cancel();
        }
        if let Some(cancel) = self.palepale_account_cancel.take() {
            cancel.cancel();
        }
        if let Some(stop) = self.microphone_stop.take() {
            stop.store(true, Ordering::Release);
        }
    }
}

#[derive(Clone, Copy)]
struct Palette {
    foreground: u32,
    muted: u32,
    line: u32,
    control: u32,
}

impl NativeView {
    fn settings_palette(&self) -> Palette {
        if self.settings.theme == "dark"
            || (self.settings.theme == "system" && self.settings_ui.system_dark)
        {
            Palette {
                foreground: 0xe5edf9,
                muted: 0xb9c7dc,
                line: 0x405064,
                control: 0x213047,
            }
        } else {
            Palette {
                foreground: 0x263246,
                muted: 0x5c6a7d,
                line: 0xb3c1d4,
                control: 0xeaf0f8,
            }
        }
    }
    pub(super) fn open_settings(&mut self, tab: &str, cx: &mut Context<Self>) {
        if let Some((handle, parent)) = self.parent.clone() {
            self.close_overlay_popups();
            self.page = Page::Live;
            let tab = tab.to_string();
            // The parent reconciles shortcuts by reading its overlay. Release this
            // child's update before navigating so that read cannot reenter it.
            let child = cx.weak_entity();
            cx.defer(move |cx| {
                let result = handle.update(cx, |_, window, cx| {
                    parent.update(cx, |parent, cx| {
                        parent.open_settings(&tab, cx);
                        parent.shown = true;
                        parent.preview_hidden = false;
                        parent.root_focus.focus(window, cx);
                        if let Some(native) = parent.native
                            && let Err(error) = native.show(true)
                        {
                            parent.error = Some(error);
                        }
                        cx.notify();
                    })
                });
                if let Err(error) = result.and_then(|result| result) {
                    let _ = child.update(cx, |child, cx| {
                        child.error = Some(format!("Open settings: {error}"));
                        cx.notify();
                    });
                }
            });
            cx.notify();
            return;
        }
        self.settings_ui.tab = match tab {
            "shortcuts" | "keybinds" => SettingsTab::Shortcuts,
            "language" => SettingsTab::Language,
            "provider" => SettingsTab::Provider,
            _ => SettingsTab::General,
        };
        self.page = if self.settings_ui.tab == SettingsTab::Shortcuts {
            Page::Keybinds
        } else {
            Page::Settings
        };
        self.recording_key = None;
        if self.gallery {
            self.review_id = catalog::settings_screen(tab).id.into();
        }
        self.reconcile_global_shortcuts(cx);
        cx.notify();
    }
    fn settings_preference(&mut self, change: impl FnOnce(&mut Settings)) {
        self.change_preference(change);
    }
    pub(super) fn settings_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut changed = false;
        if self.recording_key.is_some() && !window.is_window_active() {
            self.recording_key = None;
            self.reconcile_global_shortcuts(cx);
            changed = true;
        }
        let height = f32::from(window.viewport_size().height);
        if height != self.settings_ui.viewport_height {
            self.settings_ui.viewport_height = height;
            changed = true;
        }
        let width = f32::from(window.viewport_size().width);
        if width != self.settings_ui.viewport_width {
            self.settings_ui.viewport_width = width;
            changed = true;
        }
        if self
            .settings_ui
            .saved_until
            .is_some_and(|until| until <= std::time::Instant::now())
        {
            self.settings_ui.saved_until = None;
            changed = true;
        }
        let modal_open = self.settings_modal_open();
        if modal_open && !self.settings_ui.modal_was_open {
            self.settings_ui.restore_focus = window.focused(cx);
            self.settings_ui.modal_initial.focus(window, cx);
            self.settings_ui.modal_was_open = true;
            changed = true;
        } else if !modal_open && self.settings_ui.modal_was_open {
            if let Some(focus) = self.settings_ui.restore_focus.take() {
                focus.focus(window, cx);
            }
            self.settings_ui.modal_was_open = false;
        }
        let dark = matches!(
            cx.window_appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        if dark != self.settings_ui.system_dark {
            self.settings_ui.system_dark = dark;
            changed = true;
        }
        if (self.settings_ui.tab != SettingsTab::General
            || !matches!(self.page, Page::Settings | Page::Keybinds))
            && let Some(stop) = self.settings_ui.microphone_stop.take()
        {
            stop.store(true, Ordering::Release);
            changed = true;
        }
        while let Ok(event) = self.settings_ui.microphone_receiver.try_recv() {
            changed |= apply_microphone_event(
                self.settings_ui.microphone_generation,
                &mut self.settings_ui.microphone_stop,
                &mut self.settings_ui.microphone_levels,
                &mut self.settings_ui.microphone_status,
                &mut self.error,
                event,
            );
        }
        if changed {
            cx.notify();
        }
    }
    pub(super) fn settings_modal_open(&self) -> bool {
        self.settings_ui.notice.is_some()
    }
    pub(super) fn settings_keyboard_focus(&self) -> Option<FocusHandle> {
        self.settings_modal_open()
            .then(|| self.settings_ui.modal_initial.clone())
    }
    pub(super) fn reset_settings_review(&mut self) {
        self.settings_ui.notice = None;
    }
    pub(super) fn settings_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.settings_modal_open() {
            return false;
        }
        if event.keystroke.key == "escape" {
            self.settings_ui.notice = None;
            self.recording_key = None;
            self.reconcile_global_shortcuts(cx);
            cx.stop_propagation();
            window.prevent_default();
            cx.notify();
            return true;
        }
        false
    }
    fn settings_row(
        &self,
        icon: Lucide,
        title: impl Into<String>,
        description: impl Into<String>,
        control: impl IntoElement,
    ) -> AnyElement {
        let title = title.into();
        let description = description.into();
        let narrow = f32::from(self.modal_backdrop_size.width) <= 700.;
        div()
            .id(SharedString::from(format!("setting-field-{title}")))
            .w_full()
            .py_3()
            .border_b_1()
            .border_color(rgb(self.settings_palette().line))
            .child(
                gpui_kit::component::form::Form::horizontal()
                    .small()
                    .when(narrow, |form| form.label_layout(Axis::Vertical))
                    .label_width(px(180.))
                    .child(
                        gpui_kit::component::form::Field::new()
                            .label_fn(move |_, _| {
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(Icon::new(icon).size_4())
                                    .child(title.clone())
                            })
                            .description(description)
                            .child(div().min_w_0().child(control)),
                    ),
            )
            .into_any_element()
    }
    fn setting_switch(
        &self,
        id: &'static str,
        label: &'static str,
        checked: bool,
        change: fn(&mut Settings, bool),
        cx: &Context<Self>,
    ) -> AnyElement {
        let weak = cx.weak_entity();
        settings_switch(id, checked, label)
            .disabled(
                self.engine.is_none()
                    || self.settings_ui.provider_save_pending
                    || (id == "settings-screen" && !self.supports_image_input())
                    || (matches!(id, "provider-microphone" | "provider-system-audio")
                        && !self.supports_audio_input())
                    || ((id == "provider-microphone" || id == "provider-system-audio")
                        && self.snapshot.active),
            )
            .on_change(move |value, _, cx| {
                let value = *value;
                let _ = weak.update(cx, |this, cx| {
                    this.settings_preference(move |settings| change(settings, value));
                    cx.notify();
                });
            })
            .into_any_element()
    }
    fn settings_select(
        &self,
        id: &'static str,
        current: &str,
        choices: &[(&'static str, &'static str)],
        field: &'static str,
        cx: &Context<Self>,
    ) -> AnyElement {
        let label = choices
            .iter()
            .find(|(value, _)| *value == current)
            .map(|(_, label)| *label)
            .unwrap_or(current)
            .to_string();
        let selected = current.to_string();
        let choices = choices.to_vec();
        let weak = cx.weak_entity();
        Button::new(id)
            .accessibility_label(label.clone())
            .w(px(if field == "theme" { 150. } else { 200. }))
            .h(px(style::CONTROL_HEIGHT))
            .rounded(px(6.))
            .child(
                div()
                    .w_full()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(div().text_size(px(14.)).child(label))
                    .child(Icon::new(Lucide::ChevronDown).size(px(16.))),
            )
            .disabled(self.engine.is_none() || (field == "language" && self.snapshot.active))
            .dropdown_menu(move |mut menu, _, _| {
                for (value, label) in &choices {
                    let weak = weak.clone();
                    let value = value.to_string();
                    menu = menu.item(
                        PopupMenuItem::new(*label)
                            .checked(value == selected)
                            .on_click(move |_, _, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    let value = value.clone();
                                    this.settings_preference(move |settings| match field {
                                        "theme" => settings.theme = value,
                                        "language" => settings.language = value,
                                        "output" => settings.output_language = value,
                                        _ => {}
                                    });
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }
    pub(super) fn render_settings_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        self.render_settings_tab(
            if self.page == Page::Keybinds {
                SettingsTab::Shortcuts
            } else {
                self.settings_ui.tab
            },
            cx,
        )
    }
    fn render_settings_tab(&self, tab: SettingsTab, cx: &mut Context<Self>) -> AnyElement {
        let p = self.settings_palette();
        let width = f32::from(self.modal_backdrop_size.width);
        let compact = width <= 640.;
        // Hosted pairing commits itself after browser approval. Keep the shared prompt dirty
        // state visible, but never expose a second Save step for the Palepale account flow.
        let dirty = self.provider_draft_dirty(cx)
            && (self.settings_ui.provider_mode != ProviderMode::Palepale
                || self.settings_ui.provider_dirty.contains("prompt"));
        let tabs = [
            (SettingsTab::General, "General", Lucide::Settings),
            (SettingsTab::Shortcuts, "Keybinds", Lucide::Keyboard),
            (SettingsTab::Language, "Language", Lucide::Languages),
            (SettingsTab::Provider, "AI providers", Lucide::Monitor),
        ];
        let navigation = gpui_kit::component::tab::TabBar::new("settings-tabs")
            .underline()
            .small()
            .menu(true)
            .selected_index(tabs.iter().position(|(id, _, _)| *id == tab).unwrap_or(0))
            .children(tabs.into_iter().map(|(_, label, icon)| {
                gpui_kit::component::tab::Tab::new().label(label).prefix(
                    div()
                        .debug_selector(move || format!("settings-tab-{label}"))
                        .child(Icon::new(icon).size_3()),
                )
            }))
            .on_click(cx.listener(move |this, index: &usize, _, cx| {
                let Some((tab, _, _)) = tabs.get(*index) else {
                    return;
                };
                this.settings_ui.tab = *tab;
                this.page = if *tab == SettingsTab::Shortcuts {
                    Page::Keybinds
                } else {
                    Page::Settings
                };
                this.recording_key = None;
                this.reconcile_global_shortcuts(cx);
                cx.notify();
            }));
        let sidebar = div()
            .w_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(cx.theme().popover)
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div().px_3().py_2().flex().items_center().gap_2()
                    .child(
                        Button::new("settings-back").ghost().small().icon(Lucide::ArrowLeft)
                            .accessibility_label("Close settings")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.page = Page::Sessions;
                                this.recording_key = None;
                                this.reconcile_global_shortcuts(cx);
                                this.show_dashboard(cx);
                            })),
                    )
                    .child(div().flex_1().font_weight(FontWeight::SEMIBOLD).child("Settings"))
                    .child(Button::new("settings-about").ghost().small()
                        .accessibility_label("About skillissue")
                        .icon(Lucide::Info).label("About")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.settings_ui.notice = Some(("skillissue".into(), format!("Version {}
Native Rust desktop assistant. Session history and documents are stored locally. AI requests use your configured provider.", env!("CARGO_PKG_VERSION"))));
                            cx.notify();
                        })))
                    .child(Button::new("settings-quit").ghost().small()
                        .accessibility_label("Quit skillissue")
                        .icon(Lucide::Power).label("Quit")
                        .on_click(cx.listener(|this, _, _, cx| this.quit(cx)))),
            )
            .child(div().px_3().child(navigation));
        let content = match tab {
            SettingsTab::General => self.settings_general(cx),
            SettingsTab::Shortcuts => self.settings_shortcuts(cx),
            SettingsTab::Language => self.settings_language(cx),
            SettingsTab::Provider => self.settings_provider(cx),
        };
        div()
            .size_full()
            .flex()
            .items_start()
            .when(!self.gallery, |view| view.items_center())
            .justify_center()
            .p_0()
            .font_family("Segoe UI")
            .text_color(rgb(p.foreground))
            .text_size(px(14.))
            .child(
                div()
                    .relative()
                    .w_full()
                    .max_w_full()
                    .h_full()
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(sidebar)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .id("settings-content-scroll")
                                    .debug_selector(|| "settings-content".into())
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .p(px(if compact { 16. } else { 24. }))
                                    .child(div().w_full().max_w(px(840.)).mx_auto().child(content)),
                            )
                            .when(
                                self.error.is_some()
                                    || dirty
                                    || self.settings_ui.provider_save_pending
                                    || self.settings_ui.saved_until.is_some(),
                                |view| {
                                    view.child(
                                        div()
                                            .border_t_1()
                                            .border_color(rgb(p.line))
                                            .px_6()
                                            .py_3()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .gap_3()
                                            .child(
                                                div()
                                                    .text_size(px(11.))
                                                    .text_color(rgb(p.muted))
                                                    .child(self.error.clone().unwrap_or_else(
                                                        || {
                                                            if self
                                                                .settings_ui
                                                                .provider_save_pending
                                                            {
                                                                "Saving...".into()
                                                            } else if dirty {
                                                                "Unsaved changes".into()
                                                            } else {
                                                                "Saved".into()
                                                            }
                                                        },
                                                    )),
                                            )
                                            .when(dirty, |view| {
                                                view.child(
                                                    Button::new("save-provider-settings")
                                                        .primary()
                                                        .label("Save settings")
                                                        .disabled(
                                                            self.snapshot.active
                                                                || self.busy
                                                                || self
                                                                    .settings_ui
                                                                    .provider_save_pending
                                                                || self.engine.is_none(),
                                                        )
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.settings_save_provider(cx)
                                                        })),
                                                )
                                            }),
                                    )
                                },
                            ),
                    )
                    .when(
                        self.settings_ui.notice.is_some() && !self.capturing_backdrop,
                        |view| view.child(self.settings_dialog(cx)),
                    ),
            )
            .into_any_element()
    }
    fn settings_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some((title, text)) = &self.settings_ui.notice else {
            return div().into_any_element();
        };
        let p = self.settings_palette();
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .p_6()
            .child(self.settings_modal_material())
            .child(
                div()
                    .w(px(420.))
                    .max_w_full()
                    .p_6()
                    .rounded(px(18.))
                    .border_1()
                    .border_color(rgb(p.line))
                    .bg(cx.theme().popover)
                    .focus_trap("settings-notice", &self.settings_ui.modal_focus)
                    .child(section_title(title, text, p))
                    .child(
                        settings_action("settings-notice-close", "Close", p)
                            .track_focus(&self.settings_ui.modal_initial)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings_ui.notice = None;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }
}

fn apply_microphone_event(
    generation: u64,
    stop: &mut Option<Arc<AtomicBool>>,
    levels: &mut VecDeque<f32>,
    status: &mut String,
    error: &mut Option<String>,
    (event_generation, event): (u64, MicrophoneEvent),
) -> bool {
    if event_generation != generation || stop.is_none() {
        return false;
    }
    match event {
        MicrophoneEvent::Level(level) => {
            levels.pop_front();
            levels.push_back(level);
        }
        MicrophoneEvent::Status(message) => *status = message,
        MicrophoneEvent::Error(message) => {
            *status = message.clone();
            *error = Some(format!("Microphone test: {message}"));
            if let Some(stop) = stop {
                stop.store(true, Ordering::Release);
            }
        }
        MicrophoneEvent::Finished => *stop = None,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn stale_microphone_events_preserve_the_new_capture_and_its_stop_handle() {
        let new_stop = Arc::new(AtomicBool::new(false));
        let mut stop = Some(new_stop.clone());
        let mut levels = VecDeque::from([0., 0.]);
        let mut status = String::from("New capture");
        let mut error = None;
        for event in [
            MicrophoneEvent::Level(1.),
            MicrophoneEvent::Status("Old capture".into()),
            MicrophoneEvent::Error("Old error".into()),
            MicrophoneEvent::Finished,
        ] {
            assert!(!apply_microphone_event(
                2,
                &mut stop,
                &mut levels,
                &mut status,
                &mut error,
                (1, event)
            ));
            assert!(Arc::ptr_eq(stop.as_ref().unwrap(), &new_stop));
            assert!(!new_stop.load(Ordering::Acquire));
            assert_eq!(levels, VecDeque::from([0., 0.]));
            assert_eq!(status, "New capture");
            assert!(error.is_none());
        }
        assert!(apply_microphone_event(
            2,
            &mut stop,
            &mut levels,
            &mut status,
            &mut error,
            (2, MicrophoneEvent::Level(0.5))
        ));
        assert_eq!(levels, VecDeque::from([0., 0.5]));
        assert!(apply_microphone_event(
            2,
            &mut stop,
            &mut levels,
            &mut status,
            &mut error,
            (2, MicrophoneEvent::Error("Device disconnected".into()))
        ));
        assert!(new_stop.load(Ordering::Acquire));
        assert_eq!(status, "Device disconnected");
        assert_eq!(
            error.as_deref(),
            Some("Microphone test: Device disconnected")
        );
        assert!(apply_microphone_event(
            2,
            &mut stop,
            &mut levels,
            &mut status,
            &mut error,
            (2, MicrophoneEvent::Finished)
        ));
        assert!(stop.is_none());
        assert!(!apply_microphone_event(
            2,
            &mut stop,
            &mut levels,
            &mut status,
            &mut error,
            (2, MicrophoneEvent::Status("After finish".into()))
        ));
        assert_eq!(status, "Device disconnected");
        assert_eq!(
            error.as_deref(),
            Some("Microphone test: Device disconnected")
        );
    }
}
