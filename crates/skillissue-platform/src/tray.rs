// Rust | tray.rs | native desktop tray integration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Show,
    Settings,
    Quit,
}

#[cfg(windows)]
mod implementation {
    use super::TrayAction;
    use tray_icon::{
        Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
        menu::{Menu, MenuEvent, MenuId, MenuItem},
    };

    /// Own this on the UI message-loop thread and keep it alive for the application lifetime.
    pub struct NativeTray {
        icon: TrayIcon,
        show: MenuId,
        settings: MenuId,
        quit: MenuId,
    }

    impl NativeTray {
        pub fn new() -> Result<Self, String> {
            let menu = Menu::new();
            let show = MenuItem::with_id("skillissue.native.show", "Show skillissue", true, None);
            let settings = MenuItem::with_id("skillissue.native.settings", "Settings", true, None);
            let quit = MenuItem::with_id("skillissue.native.quit", "Quit", true, None);
            menu.append_items(&[&show, &settings, &quit])
                .map_err(|error| format!("Create tray menu: {error}"))?;
            let rgba = include_bytes!("../../../assets/branding/tray-32.rgba").to_vec();
            let image = Icon::from_rgba(rgba, 32, 32)
                .map_err(|error| format!("Create tray icon: {error}"))?;
            let icon = TrayIconBuilder::new()
                .with_id("skillissue.native")
                .with_icon(image)
                .with_tooltip("skillissue — local meeting assistant")
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(false)
                .build()
                .map_err(|error| format!("Create system tray icon: {error}"))?;
            Ok(Self {
                icon,
                show: show.id().clone(),
                settings: settings.id().clone(),
                quit: quit.id().clone(),
            })
        }

        pub fn poll(&self) -> Vec<TrayAction> {
            let mut actions = Vec::new();
            for event in TrayIconEvent::receiver().try_iter() {
                if matches!(event, TrayIconEvent::Click { ref id, button: MouseButton::Left, button_state: MouseButtonState::Up, .. } if id == self.icon.id())
                {
                    actions.push(TrayAction::Show);
                }
            }
            for event in MenuEvent::receiver().try_iter() {
                if event.id == self.show {
                    actions.push(TrayAction::Show);
                } else if event.id == self.settings {
                    actions.push(TrayAction::Settings);
                } else if event.id == self.quit {
                    actions.push(TrayAction::Quit);
                }
            }
            actions
        }
    }
}

#[cfg(windows)]
pub use implementation::NativeTray;

#[cfg(not(windows))]
pub struct NativeTray;

#[cfg(not(windows))]
impl NativeTray {
    pub fn new() -> Result<Self, String> {
        Err("Native tray currently supports Windows only.".into())
    }
    pub fn poll(&self) -> Vec<TrayAction> {
        Vec::new()
    }
}
