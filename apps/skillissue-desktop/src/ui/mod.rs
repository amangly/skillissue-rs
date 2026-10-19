// Rust | apps/skillissue-desktop/src/ui.rs | GPUI | native workspace and overlay
use gpui_kit::component::input::InputEvent as EditorEvent;
use gpui_kit::component::{button::*, input::*, text::TextView, *};
use gpui_kit::prelude::*;
use gpui_kit::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use skillissue_core::{EngineEvent, NativeEngine};
use skillissue_domain::models::*;
use skillissue_domain::provider::ProviderModel;
use skillissue_platform::{
    reveal_path,
    shortcuts::{Action as ShortcutAction, GlobalShortcuts, ShortcutContext},
    tray::{NativeTray, TrayAction},
    window::NativeWindow,
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

#[cfg(test)]
mod accelerator_tests;
mod ascii_background;
mod catalog;
mod dashboard;
mod keyboard;
mod material;
mod modes;
mod overlay;
mod preferences;
mod provider_sync;
mod render;
#[cfg(all(test, feature = "native-effects"))]
mod request_state_tests;
#[cfg(feature = "ui-review")]
mod review;
#[cfg(all(test, feature = "native-effects"))]
mod root_focus_tests;
mod runtime;
mod session;
mod settings;
#[cfg(all(test, feature = "native-effects"))]
mod shortcut_dispatch_tests;
#[cfg(test)]
mod shortcut_retry_tests;
mod style;
mod surfaces;
#[cfg(all(test, feature = "native-effects"))]
mod tests;
mod updates;
mod windows;
use preferences::apply_preference_patch;
use render::{date, markdown};
use surfaces::brand_mark;
use windows::{capture_screen, native_window};

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Sessions,
    Live,
    Modes,
    Settings,
    Keybinds,
    Splash,
}

enum Reply {
    UpdateChecked(Result<Option<skillissue_core::updates::Update>, String>),
    UpdateDownloaded(Result<PathBuf, String>),
    UpdateLaunched(Result<(), String>),
    Open(Result<Arc<NativeEngine>, String>),
    Sessions(String, Result<Vec<SessionListEntry>, String>),
    Preference(Result<ConfigView, String>),
    ProviderConfig(ProviderMode, Result<ConfigView, String>),
    ByokModels(u64, Result<Vec<String>, String>),
    ByokTest(u64, &'static str, Result<(), String>),
    ByokModelLists(u64, Result<Vec<skillissue_core::ByokModelList>, String>),
    ProviderConnection(Result<ConfigView, String>),
    Catalog(u64, Result<Vec<ProviderModel>, String>),
    PalepaleAccount(u64, Result<skillissue_core::PalepaleAccountStatus, String>),
    /// Browser pairing metadata contains only the public approval code and URL. The private
    /// device code is retained by the core transport and never crosses into the UI.
    PairingStarted(u64, Result<(String, String, u64), String>),
    PairingReady(u64, Result<ConfigView, String>),
    Status(Result<(), String>),
    Exported(Result<PathBuf, String>),
    HistoryBackup(Result<PathBuf, String>),
    HistoryRestored(Result<usize, String>),
    HistoryTransferCancelled,
    ExportRevealed(Result<(), String>),
    Quit(Result<(), String>),
    Snapshot(Result<SessionSnapshot, String>),
    Notes(Result<SessionRecord, String>),
    Stopped(Result<Option<SessionRecord>, String>),
    Context(Result<skillissue_core::ContextStatus, String>),
    Latency(skillissue_core::LatencySummary),
    Assist(u64, Result<(), String>),
    SavedAnswer(Result<SessionRecord, String>),
    Captured(u64, Result<String, String>),
    Cleared(Result<(), String>),
}

#[derive(Default)]
struct WindowRegistry {
    handles: std::sync::Mutex<Vec<isize>>,
    capture: std::sync::Mutex<()>,
    requests: std::sync::Mutex<Vec<(uuid::Uuid, tokio_util::sync::CancellationToken)>>,
    closing: std::sync::atomic::AtomicBool,
}

struct RequestScope {
    windows: Arc<WindowRegistry>,
    id: uuid::Uuid,
}
impl Drop for RequestScope {
    fn drop(&mut self) {
        if let Ok(mut requests) = self.windows.requests.lock() {
            requests.retain(|(id, _)| *id != self.id);
        }
    }
}

pub struct NativeView {
    preference_queue: VecDeque<serde_json::Map<String, serde_json::Value>>,
    preference_saving: bool,
    quit_after_preferences: bool,
    runtime: Arc<tokio::runtime::Runtime>,
    data_dir: PathBuf,
    engine: Option<Arc<NativeEngine>>,
    events: Option<tokio::sync::broadcast::Receiver<EngineEvent>>,
    sender: mpsc::Sender<Reply>,
    receiver: mpsc::Receiver<Reply>,
    settings: Settings,
    snapshot: SessionSnapshot,
    sessions: Vec<SessionListEntry>,
    selected: Option<SessionRecord>,
    page: Page,
    search_query: String,
    status: String,
    error: Option<String>,
    busy: bool,
    stream: String,
    is_overlay: bool,
    expanded: bool,
    shown: bool,
    overlay: Option<(AnyWindowHandle, Entity<NativeView>)>,
    native: Option<NativeWindow>,
    own_windows: Arc<WindowRegistry>,
    hotkeys: Option<GlobalShortcuts>,
    shortcut_state: Option<(BTreeMap<String, String>, ShortcutContext)>,
    shortcut_retry: runtime::ShortcutRegistrationRetry,
    tray: Option<NativeTray>,
    has_key: bool,
    smoke: bool,
    smoke_gallery: bool,
    rendered_screens: BTreeSet<String>,
    #[cfg(feature = "ui-review")]
    review: Option<review::ReviewRun>,
    ticks: usize,
    closing: bool,
    opening: bool,
    root_focus: FocusHandle,
    question: Entity<TextareaState>,
    search: Entity<InputState>,
    endpoint: Entity<InputState>,
    model: Entity<InputState>,
    stt_model: Entity<InputState>,
    api_key: Entity<InputState>,
    title: Entity<InputState>,
    notes: Entity<TextareaState>,
    shortcuts: BTreeMap<String, Entity<InputState>>,
    scroll: ScrollHandle,
    subscriptions: Vec<Subscription>,
    recording_key: Option<String>,
    request_cancel: Option<tokio_util::sync::CancellationToken>,
    request_draft: session::RequestDraft,
    last_submitted_prompt: Option<String>,
    history_transfer_pending: bool,
    active_assistant_id: Option<String>,
    audio_retry_prompt: Option<String>,
    capture_generation: u64,
    mode_editor: Option<modes::ModeEditor>,
    notes_busy: bool,
    typing: bool,
    context: skillissue_core::ContextStatus,
    latency: runtime::LatencyCache,
    request_generation: u64,
    request_started_at: Option<(u64, Instant)>,
    dashboard: dashboard::DashboardState,
    settings_ui: settings::SettingsUi,
    overlay_ui: overlay::OverlayState,
    updates: updates::UpdateState,
    gallery: bool,
    preview_hidden: bool,
    review_id: String,
    saved_question: Entity<InputState>,
    saved_question_busy: bool,
    parent: Option<(AnyWindowHandle, WeakEntity<NativeView>)>,
    window_handle: AnyWindowHandle,
    captured_screen: Option<String>,
    gallery_scroll: ScrollHandle,
    modal_backdrop: Option<Arc<Image>>,
    modal_backdrop_size: gpui_kit::Size<Pixels>,
    backdrop_attempted: bool,
    backdrop_key: String,
    backdrop_generation: u64,
    capturing_backdrop: bool,
}

impl NativeView {
    pub fn new(
        runtime: Arc<tokio::runtime::Runtime>,
        data_dir: PathBuf,
        smoke: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_inner(
            runtime,
            data_dir,
            smoke,
            None,
            None,
            #[cfg(test)]
            true,
            window,
            cx,
        )
    }

    #[cfg(test)]
    fn new_for_test(
        runtime: Arc<tokio::runtime::Runtime>,
        data_dir: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_inner(runtime, data_dir, true, None, None, false, window, cx)
    }

    #[cfg_attr(
        test,
        allow(
            clippy::too_many_arguments,
            reason = "The test-only flag isolates the mock constructor from all OS and engine startup."
        )
    )]
    fn new_inner(
        runtime: Arc<tokio::runtime::Runtime>,
        data_dir: PathBuf,
        smoke: bool,
        engine: Option<Arc<NativeEngine>>,
        own_windows: Option<Arc<WindowRegistry>>,
        #[cfg(test)] interactive: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        window.set_window_title(if engine.is_some() {
            "skillissue · Assistant"
        } else {
            skillissue_domain::identity::PRODUCT_NAME
        });
        let (sender, receiver) = mpsc::channel();
        let settings = Settings::default();
        let input = |value: String,
                     placeholder: &'static str,
                     window: &mut Window,
                     cx: &mut Context<Self>| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .default_value(value)
            })
        };
        let question = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 2)
                .submit_on_enter(true)
        });
        let saved_question = input(String::new(), "Ask about this meeting…", window, cx);
        let search = input(String::new(), "Search sessions", window, cx);
        let endpoint = input(
            settings.api_base_url.clone(),
            "https://provider.example/v1",
            window,
            cx,
        );
        let model = input(settings.chat_model.clone(), "Assistant model", window, cx);
        let stt_model = input(
            settings.transcription_model.clone(),
            "Transcription model",
            window,
            cx,
        );
        let api_key = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Leave blank to keep the saved key")
                .masked(true)
        });
        let title = input(String::new(), "Session title", window, cx);
        let notes = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(12)
                .placeholder("Write meeting notes…")
        });
        let shortcuts = settings
            .shortcuts
            .iter()
            .map(|(action, value)| {
                (
                    action.clone(),
                    input(value.clone(), "Unassigned", window, cx),
                )
            })
            .collect();
        let root_focus = cx.focus_handle().tab_stop(false);
        window.focus(&root_focus, cx);
        #[cfg(not(test))]
        let native = native_window(window);
        #[cfg(test)]
        let native = interactive.then(|| native_window(window)).flatten();
        let own_windows = own_windows.unwrap_or_default();
        if let Some(native) = &native
            && let Ok(mut handles) = own_windows.handles.lock()
        {
            handles.push(native.handle());
        }
        let mut result = Self {
            preference_queue: VecDeque::new(),
            preference_saving: false,
            quit_after_preferences: false,
            runtime,
            data_dir,
            engine: None,
            events: None,
            sender,
            receiver,
            settings,
            snapshot: SessionSnapshot {
                session_id: None,
                active: false,
                transcript: vec![],
                messages: vec![],
            },
            sessions: vec![],
            selected: None,
            page: Page::Sessions,
            search_query: String::new(),
            status: "Opening your local workspace…".into(),
            error: None,
            busy: false,
            stream: String::new(),
            is_overlay: false,
            expanded: true,
            shown: true,
            overlay: None,
            native,
            own_windows,
            hotkeys: None,
            shortcut_state: None,
            shortcut_retry: Default::default(),
            tray: None,
            has_key: false,
            smoke,
            smoke_gallery: std::env::args().any(|arg| arg == "--smoke-gallery"),
            rendered_screens: BTreeSet::new(),
            #[cfg(feature = "ui-review")]
            review: review::ReviewRun::from_args(),
            ticks: 0,
            closing: false,
            opening: false,
            root_focus,
            question,
            search,
            endpoint,
            model,
            stt_model,
            api_key,
            title,
            notes,
            shortcuts,
            scroll: ScrollHandle::new(),
            subscriptions: vec![],
            recording_key: None,
            request_cancel: None,
            request_draft: Default::default(),
            last_submitted_prompt: None,
            history_transfer_pending: false,
            active_assistant_id: None,
            audio_retry_prompt: None,
            capture_generation: 0,
            mode_editor: None,
            notes_busy: false,
            typing: false,
            context: Default::default(),
            latency: Default::default(),
            request_generation: 0,
            request_started_at: None,
            dashboard: Default::default(),
            settings_ui: settings::SettingsUi::new(window, cx),
            overlay_ui: Default::default(),
            gallery: std::env::args().any(|arg| {
                arg == "--gallery"
                    || arg == "--smoke-gallery"
                    || arg.starts_with("--review-output=")
            }),
            preview_hidden: false,
            review_id: "dashboard".into(),
            saved_question,
            saved_question_busy: false,
            parent: None,
            updates: Default::default(),
            window_handle: Window::window_handle(window),
            captured_screen: None,
            gallery_scroll: ScrollHandle::new(),
            modal_backdrop: None,
            modal_backdrop_size: size(px(1100.), px(800.)),
            backdrop_attempted: false,
            backdrop_key: String::new(),
            backdrop_generation: 0,
            capturing_backdrop: false,
        };
        result.subscriptions.push(cx.subscribe_in(
            &result.question,
            window,
            |this, _, event, window, cx| {
                match event {
                    EditorEvent::Focus => {
                        this.overlay_ui.focused = true;
                        cx.notify();
                    }
                    EditorEvent::Blur => {
                        this.overlay_ui.focused = false;
                        cx.notify();
                    }
                    // TextareaState emits Change for keyboard, paste, and delete edits. The
                    // textarea paints itself, but the parent owns the conditional shortcut
                    // placeholder and must be invalidated when that value changes.
                    EditorEvent::Change => cx.notify(),
                    _ => {}
                }
                if matches!(
                    event,
                    EditorEvent::PressEnter {
                        secondary: false,
                        shift: false
                    }
                ) {
                    this.ask(window, cx);
                }
            },
        ));
        // `TextareaState::set_value` intentionally suppresses InputEvent::Change. Observe the
        // entity as well so programmatic draft restore/clear follows the same parent render path.
        result
            .subscriptions
            .push(cx.observe(&result.question, |_, _, cx| cx.notify()));
        result.subscriptions.push(cx.subscribe_in(
            &result.search,
            window,
            |this, input, event, _, cx| {
                if matches!(event, EditorEvent::Change) {
                    this.search_query = input.read(cx).value().to_string();
                    this.refresh_sessions();
                    cx.notify();
                }
            },
        ));
        result
            .subscriptions
            .push(cx.on_focus_lost(window, |this, window, cx| {
                let focus = this
                    .dashboard_keyboard_focus(cx)
                    .or_else(|| this.settings_keyboard_focus())
                    .or_else(|| window.focus_lost_restore_target(cx))
                    .unwrap_or_else(|| this.root_focus.clone());
                window.focus(&focus, cx);
            }));
        let weak = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            weak.update(cx, |this, cx| {
                if this.is_overlay || this.tray.is_some() {
                    this.shown = false;
                    if let Some(native) = &this.native {
                        let _ = native.hide();
                    }
                } else {
                    this.quit(cx);
                }
                false
            })
            .unwrap_or(true)
        });
        #[cfg(test)]
        if !interactive {
            return result;
        }
        if let Some(engine) = engine {
            let _ = result.sender.send(Reply::Open(Ok(engine)));
        } else {
            result.boot();
        }
        if result.gallery
            && let Some(id) =
                std::env::args().find_map(|arg| arg.strip_prefix("--screen=").map(str::to_owned))
        {
            result.select_review(&id, window, cx);
        }
        let timer = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            loop {
                timer.timer(Duration::from_millis(80)).await;
                if this
                    .update_in(cx, |this, window, cx| this.tick(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        result
    }
}
