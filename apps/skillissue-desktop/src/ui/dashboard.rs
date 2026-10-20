// Workspace state, shared visual primitives and the native session workflow.
use super::*;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::base::FocusTrapElement;

mod capture_button;
mod history;
mod modes;
mod setup;
mod workspace;

const INK: u32 = 0x263246;
const SECONDARY: u32 = 0x5c6a7d;
const BLUE: u32 = 0x2879d8;
const LINE: u32 = 0x6074952b;

// The stateless element builders share the palette selected for this render;
// GPUI constructs a window's elements synchronously on its foreground thread.
thread_local! { static DASHBOARD_DARK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
fn ink(value: u32) -> Hsla {
    let value = if DASHBOARD_DARK.get() {
        match value {
            INK => 0xe5edf9,
            SECONDARY => 0xb9c7dc,
            BLUE => 0x8cbaff,
            0x677588 => 0xacbdd5,
            0x64758e | 0x718096 | 0x7188a8 => 0xa8bddb,
            0x377b5d => 0x8adbb1,
            0x405c53 => 0xd0ebdf,
            0x0369a1 => 0x8ab8f5,
            0xc34b4b | 0xb91c1c | 0xdc2626 => 0xffa5b4,
            _ => value,
        }
    } else {
        value
    };
    gpui_kit::rgb(value).into()
}
fn glass(value: u32) -> Hsla {
    let value = if DASHBOARD_DARK.get() {
        match value {
            LINE => 0xd7e7ff24,
            0xffffffaa => 0xd7e8ff12,
            0xffffffb8 | 0xffffffd9 | 0xffffffe6 => 0xdbeaff29,
            0xf8fbffdd => 0x213047ed,
            0xeef4fbdd => 0x172236aa,
            0xffffff66 | 0xe7effb33 | 0xe5eefb70 | 0xe5eefb44 => 0xb8d0ff08,
            0xf9fbffad => 0x213047dd,
            0xffffffbb | 0xffffffdf => 0xbad5ff20,
            0xd0dcf057 | 0xd3e1f55e => 0xbdcff218,
            0xffffff3d | 0xffffff26 => 0xa9c9ff06,
            0xf7fbffdc => 0x253b58ee,
            0xffffffee => 0x345444ee,
            0xcbf2ddcc => 0x24533b88,
            0xe0f2feff => 0x1f466a88,
            0xe4eefc88 => 0xadcfff12,
            0x53668b22 => 0x060c1a70,
            0xf2f7ff7a => 0xd7e8ff12,
            0x778daf19 => 0xe0eeff12,
            _ => value,
        }
    } else {
        value
    };
    gpui_kit::rgba(value).into()
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Screen {
    #[default]
    Home,
    Session,
}
#[derive(Clone, Copy, Default, PartialEq)]
enum DetailTab {
    #[default]
    Summary,
    Transcript,
    Usage,
}

enum ModeDestination {
    General,
    Templates,
    Existing(String),
    New,
    Template(usize),
    Close,
}
enum SessionDestination {
    Home,
    Session(String),
}

enum DashboardReply {
    History {
        generation: u64,
        result: Result<Vec<SessionListEntry>, String>,
    },
    Loaded {
        id: String,
        result: Result<SessionRecord, String>,
    },
    Saved {
        id: String,
        revision: u64,
        result: Result<SessionRecord, String>,
    },
    Started(Result<(), String>),
    Resumed(Result<(), String>),
    Deleted {
        id: String,
        result: Result<(), String>,
    },
    Export(Result<PathBuf, String>),
}

pub(super) struct DashboardState {
    pub(super) setup_step: usize,
    pub(super) setup_dismissed: bool,
    screen: Screen,
    menu_open: bool,
    search_open: bool,
    detail_tab: DetailTab,
    note_evidence: Option<String>,
    pub(super) modes_tab: String,
    notice: String,
    notice_at: u64,
    sender: mpsc::Sender<DashboardReply>,
    receiver: mpsc::Receiver<DashboardReply>,
    subscribed: bool,
    dirty: bool,
    save_revision: u64,
    saving: bool,
    save_error: Option<String>,
    session_action: bool,
    delete_confirm: bool,
    mode_destination: Option<ModeDestination>,
    quit_requested: bool,
    requested_session: Option<String>,
    history_rows: Vec<SessionListEntry>,
    history_offset: u32,
    history_has_next: bool,
    history_query: String,
    history_signature: String,
    history_generation: u64,
    history_loading: bool,
    history_loaded: bool,
    history_error: Option<String>,
    notes_edited_at: u64,
    session_destination: Option<SessionDestination>,
    modal_kind: &'static str,
    modal_focus: Option<FocusHandle>,
    modal_initial: Option<FocusHandle>,
    modal_restore: Option<FocusHandle>,
}

impl Default for DashboardState {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            setup_step: 0,
            setup_dismissed: false,
            screen: Screen::Home,
            menu_open: false,
            search_open: false,
            detail_tab: DetailTab::Summary,
            note_evidence: None,
            modes_tab: "general".into(),
            notice: String::new(),
            notice_at: 0,
            sender,
            receiver,
            subscribed: false,
            dirty: false,
            save_revision: 0,
            saving: false,
            save_error: None,
            session_action: false,
            delete_confirm: false,
            mode_destination: None,
            quit_requested: false,
            requested_session: None,
            history_rows: vec![],
            history_offset: 0,
            history_has_next: false,
            history_query: String::new(),
            history_signature: String::new(),
            history_generation: 0,
            history_loading: false,
            history_loaded: false,
            history_error: None,
            notes_edited_at: 0,
            session_destination: None,
            modal_kind: "",
            modal_focus: None,
            modal_initial: None,
            modal_restore: None,
        }
    }
}

fn icon(name: Glyph) -> Icon {
    Icon::new(name).size_4()
}
fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    let label = label.into();
    crate::ui::style::action(id).when(!label.is_empty(), |button| button.label(label))
}
fn material(
    angle: f32,
    light_top: u32,
    light_bottom: u32,
    dark_top: u32,
    dark_bottom: u32,
) -> Background {
    linear_gradient(
        angle,
        linear_color_stop(
            rgba(if DASHBOARD_DARK.get() {
                dark_top
            } else {
                light_top
            }),
            0.,
        ),
        linear_color_stop(
            rgba(if DASHBOARD_DARK.get() {
                dark_bottom
            } else {
                light_bottom
            }),
            1.,
        ),
    )
}
fn chrome(id: impl Into<ElementId>, glyph: Glyph, size: f32) -> Button {
    Button::new(id)
        .ghost()
        .small()
        .w(px(32.))
        .h(px(26.))
        .p_0()
        .rounded(px(7.))
        .bg(rgba(0xffffff00))
        .border_0()
        .shadow_none()
        .text_color(ink(0x64758e))
        .child(Icon::new(glyph).size(px(size)))
}
fn nav_label(glyph: Glyph, label: impl Into<SharedString>, active: bool) -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .items_center()
        .gap_2()
        .text_size(px(14.))
        .child(icon(glyph))
        .child(div().flex_1().min_w_0().text_ellipsis().child(label.into()))
        .when(active, |view| {
            view.child(div().size(px(6.)).rounded_full().bg(ink(BLUE)))
        })
}
fn panel() -> Div {
    div()
        .rounded(px(8.))
        .border_1()
        .border_color(ink(if DASHBOARD_DARK.get() {
            0x405064
        } else {
            0xd8dee8
        }))
        .bg(ink(if DASHBOARD_DARK.get() {
            0x213047
        } else {
            0xeaf0f8
        }))
}
fn muted(text: impl Into<SharedString>) -> Div {
    div()
        .text_sm()
        .text_color(ink(SECONDARY))
        .child(text.into())
}
fn state_label(state: &str) -> &'static str {
    match state {
        "active" => "Listening",
        "ready" => "Saved",
        "analyzing" => "Generating notes",
        "error" => "Needs attention",
        "interrupted" => "Interrupted",
        _ => "Saved",
    }
}

fn file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.)
    } else {
        format!("{:.1} MB", bytes as f64 / 1024. / 1024.)
    }
}

impl NativeView {
    pub(super) fn dashboard_keyboard_focus(&self, cx: &App) -> Option<FocusHandle> {
        if self.is_overlay {
            None
        } else if self.dashboard.search_open && matches!(self.page, Page::Sessions | Page::Modes) {
            Some(self.search.focus_handle(cx))
        } else if self.page == Page::Modes {
            self.dashboard.modal_initial.clone()
        } else {
            None
        }
    }

    pub(super) fn reset_dashboard_review(&mut self) {
        self.dashboard.screen = Screen::Home;
        self.dashboard.search_open = false;
        self.dashboard.menu_open = false;
    }
    pub(super) fn dashboard_modal_open(&self) -> bool {
        !self.is_overlay && (self.page == Page::Modes || self.dashboard.search_open)
    }
    fn dashboard_focus_trap(&self, id: &'static str, panel: Div) -> AnyElement {
        match &self.dashboard.modal_focus {
            Some(handle) => panel
                .id(id)
                .role(gpui_kit::accesskit::Role::Dialog)
                .focus_trap(id, handle)
                .into_any_element(),
            None => panel.into_any_element(),
        }
    }
    fn dashboard_dialog_close(
        &self,
        id: &'static str,
        label: &'static str,
    ) -> gpui_kit::base::Button {
        gpui_kit::base::Button::new(id)
            .accessibility_label(label)
            .w(px(28.))
            .h(px(28.))
            .p_0()
            .rounded(px(7.))
            .bg(rgba(0xffffff00))
            .text_color(ink(0x64758e))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .child(Icon::new(Glyph::X).size(px(18.)))
            .when_some(self.dashboard.modal_initial.as_ref(), |button, handle| {
                button.track_focus(handle)
            })
    }
    fn sync_dashboard_modal_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let modal = if self.page == Page::Modes {
            "modes"
        } else if self.dashboard.search_open {
            "search"
        } else {
            ""
        };
        if self.dashboard.modal_kind == modal {
            return false;
        }
        if let Some(previous) = self.dashboard.modal_restore.take() {
            window.focus(&previous, cx);
        }
        self.dashboard.modal_focus = None;
        self.dashboard.modal_initial = None;
        self.dashboard.modal_kind = modal;
        self.search.update(cx, |input, cx| {
            input.set_placeholder(
                if modal == "search" {
                    "Search saved sessions and actions..."
                } else {
                    "Search sessions..."
                },
                window,
                cx,
            );
        });
        if !modal.is_empty() {
            self.dashboard.modal_restore = window.focused(cx);
            self.dashboard.modal_focus = Some(cx.focus_handle());
            if modal == "search" {
                self.search
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.search.focus_handle(cx).focus(window, cx);
            } else {
                // The first button owns this handle even while the material capture
                // temporarily omits the panel from the element tree.
                let initial = cx.focus_handle();
                window.focus(&initial, cx);
                self.dashboard.modal_initial = Some(initial);
            }
        }
        true
    }
    pub(super) fn dashboard_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.is_overlay || !matches!(self.page, Page::Sessions | Page::Modes) {
            return false;
        }
        let key = &event.keystroke;
        if (key.modifiers.control || key.modifiers.platform) && !key.modifiers.alt && key.key == "k"
        {
            self.dashboard.search_open = !self.dashboard.search_open;
            if self.dashboard.search_open {
                self.search.focus_handle(cx).focus(window, cx);
            }
        } else if key.key == "escape" {
            if self.dashboard.search_open {
                self.dashboard.search_open = false;
            } else if self.page == Page::Modes {
                self.navigate_dashboard_mode(ModeDestination::Close, window, cx);
            } else if self.dashboard.menu_open {
                self.dashboard.menu_open = false;
            } else {
                return false;
            }
        } else {
            return false;
        }
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
        true
    }
    pub(super) fn select_dashboard_page(
        &mut self,
        page: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.page = if page == "modes" {
            Page::Modes
        } else {
            Page::Sessions
        };
        self.dashboard.search_open = page == "search";
        self.dashboard.screen = match page {
            "session" => {
                self.setup_dashboard_detail(window, cx);
                Screen::Session
            }
            _ => Screen::Home,
        };
        if page == "modes" {
            self.dashboard.modes_tab = "general".into();
        }
        cx.notify();
    }
    fn dashboard_notice(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.dashboard.notice = text.into();
        self.dashboard.notice_at = now_ms();
        cx.notify();
    }
}
