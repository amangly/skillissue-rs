// Native development previews map directly to the implementation modules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReviewScreen {
    pub id: &'static str,
    pub group: &'static str,
    pub label: &'static str,
    pub view: &'static str,
    pub page: Option<&'static str>,
    pub tab: Option<&'static str>,
    pub source: &'static str,
    pub service: &'static str,
}

pub const REVIEW_GROUPS: [&str; 4] = ["Workspace", "Overlay", "Settings", "Application"];

pub const REVIEW_SCREENS: [ReviewScreen; 14] = [
    ReviewScreen {
        id: "dashboard",
        group: "Workspace",
        label: "Dashboard",
        view: "dashboard",
        page: None,
        tab: None,
        source: "apps/skillissue-desktop/src/ui/dashboard/workspace.rs",
        service: "Local session history and capture controls",
    },
    ReviewScreen {
        id: "search",
        group: "Workspace",
        label: "Search",
        view: "dashboard",
        page: Some("search"),
        tab: None,
        source: "apps/skillissue-desktop/src/ui/dashboard/workspace.rs",
        service: "Search local sessions",
    },
    ReviewScreen {
        id: "modes",
        group: "Workspace",
        label: "Modes and documents",
        view: "dashboard",
        page: Some("modes"),
        tab: None,
        source: "apps/skillissue-desktop/src/ui/dashboard/modes.rs",
        service: "Prompt modes and local document context",
    },
    ReviewScreen {
        id: "session",
        group: "Workspace",
        label: "Session detail",
        view: "dashboard",
        page: Some("session"),
        tab: None,
        source: "apps/skillissue-desktop/src/ui/dashboard/history.rs",
        service: "Persisted transcripts, responses and notes",
    },
    ReviewScreen {
        id: "control",
        group: "Overlay",
        label: "Control widget",
        view: "control",
        page: None,
        tab: None,
        source: "apps/skillissue-desktop/src/ui/overlay/presentation.rs",
        service: "Start, stop, show and hide the native assistant",
    },
    ReviewScreen {
        id: "chat",
        group: "Overlay",
        label: "Chat",
        view: "overlay",
        page: None,
        tab: None,
        source: "apps/skillissue-desktop/src/ui/overlay/presentation.rs",
        service: "Streaming provider responses and session context",
    },
    ReviewScreen {
        id: "chat-populated",
        group: "Overlay",
        label: "Response preview",
        view: "overlay",
        page: Some("sample"),
        tab: None,
        source: "apps/skillissue-desktop/src/ui/overlay/presentation.rs",
        service: "Local review fixture; no provider request",
    },
    ReviewScreen {
        id: "chat-error",
        group: "Overlay",
        label: "Error preview",
        view: "overlay",
        page: Some("error"),
        tab: None,
        source: "apps/skillissue-desktop/src/ui/overlay/presentation.rs",
        service: "Local error review fixture",
    },
    ReviewScreen {
        id: "chat-viewed-context",
        group: "Overlay",
        label: "Context preview",
        view: "overlay",
        page: Some("metadata"),
        tab: None,
        source: "apps/skillissue-desktop/src/ui/overlay/presentation.rs",
        service: "Local context metadata review fixture",
    },
    ReviewScreen {
        id: "settings-general",
        group: "Settings",
        label: "General",
        view: "settings",
        page: None,
        tab: Some("general"),
        source: "apps/skillissue-desktop/src/ui/settings/general.rs",
        service: "Desktop, capture, theme and audio controls",
    },
    ReviewScreen {
        id: "settings-keybinds",
        group: "Settings",
        label: "Keybinds",
        view: "settings",
        page: None,
        tab: Some("keybinds"),
        source: "apps/skillissue-desktop/src/ui/settings/keybinds.rs",
        service: "Editable global keyboard shortcuts",
    },
    ReviewScreen {
        id: "settings-language",
        group: "Settings",
        label: "Language",
        view: "settings",
        page: None,
        tab: Some("language"),
        source: "apps/skillissue-desktop/src/ui/settings/general.rs",
        service: "Transcription and response languages",
    },
    ReviewScreen {
        id: "settings-provider",
        group: "Settings",
        label: "AI provider",
        view: "settings",
        page: None,
        tab: Some("provider"),
        source: "apps/skillissue-desktop/src/ui/settings/provider.rs",
        service: "Provider configuration and OS credential storage",
    },
    ReviewScreen {
        id: "splash",
        group: "Application",
        label: "Starting skillissue",
        view: "splash",
        page: None,
        tab: None,
        source: "apps/skillissue-desktop/src/ui/surfaces.rs",
        service: "Native startup state",
    },
];

pub fn screen(id: &str) -> Option<&'static ReviewScreen> {
    REVIEW_SCREENS.iter().find(|screen| screen.id == id)
}

pub fn settings_screen(tab: &str) -> &'static ReviewScreen {
    let tab = if tab == "shortcuts" { "keybinds" } else { tab };
    REVIEW_SCREENS
        .iter()
        .find(|screen| screen.view == "settings" && screen.tab == Some(tab))
        .unwrap_or(&REVIEW_SCREENS[9])
}

pub fn group_screens(group: &str) -> impl Iterator<Item = &'static ReviewScreen> + '_ {
    REVIEW_SCREENS
        .iter()
        .filter(move |screen| screen.group == group)
}
