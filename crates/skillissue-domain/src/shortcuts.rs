// Rust | shortcuts.rs | global-hotkey | validated desktop accelerators
use crate::models::Settings;
pub use crate::models::default_shortcuts;
use global_hotkey::hotkey::HotKey as Shortcut;
use std::collections::{BTreeMap, HashMap};

pub const SETTINGS_ACCELERATOR: &str = "Ctrl+Shift+,";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Action {
    ToggleVisibility,
    Ask,
    Clear,
    ToggleSession,
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    ScrollUp,
    ScrollDown,
    Settings,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ToggleVisibility => "toggleVisibility",
            Self::Ask => "ask",
            Self::Clear => "clear",
            Self::ToggleSession => "toggleSession",
            Self::MoveUp => "moveUp",
            Self::MoveDown => "moveDown",
            Self::MoveLeft => "moveLeft",
            Self::MoveRight => "moveRight",
            Self::ScrollUp => "scrollUp",
            Self::ScrollDown => "scrollDown",
            Self::Settings => "settings",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "toggleVisibility" => Self::ToggleVisibility,
            "ask" => Self::Ask,
            "clear" => Self::Clear,
            "toggleSession" => Self::ToggleSession,
            "moveUp" => Self::MoveUp,
            "moveDown" => Self::MoveDown,
            "moveLeft" => Self::MoveLeft,
            "moveRight" => Self::MoveRight,
            "scrollUp" => Self::ScrollUp,
            "scrollDown" => Self::ScrollDown,
            "settings" => Self::Settings,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShortcutContext {
    pub active: bool,
    pub ambient_enabled: bool,
    pub chat_shown: bool,
    pub recording: bool,
}

impl ShortcutContext {
    pub fn enabled(self, _action: Action) -> bool {
        !self.recording
    }
}

pub fn clamp_movement(position: i32, delta: i32, origin: i32, available: u32, extent: u32) -> i32 {
    let low = i64::from(origin);
    let high = (low + i64::from(available) - i64::from(extent))
        .max(low)
        .min(i64::from(i32::MAX));
    (i64::from(position) + i64::from(delta)).clamp(low, high) as i32
}

pub fn parse(value: &str) -> Result<Shortcut, String> {
    if value.len() > 96 || value.chars().any(char::is_control) {
        return Err("Keyboard shortcuts must contain at most 96 printable characters.".into());
    }
    value
        .trim()
        .parse::<Shortcut>()
        .map_err(|e| format!("Invalid keyboard shortcut {value:?}: {e}"))
}

pub fn validate(values: &mut BTreeMap<String, String>) -> Result<(), String> {
    let defaults = default_shortcuts();
    if let Some(action) = values.keys().find(|key| !defaults.contains_key(*key)) {
        return Err(format!("Unknown shortcut action: {action}."));
    }
    for (action, accelerator) in defaults {
        values.entry(action).or_insert(accelerator);
    }
    let mut seen = HashMap::new();
    for (action, value) in values.iter_mut() {
        if value.len() > 96 || value.chars().any(char::is_control) {
            return Err("Keyboard shortcuts must contain at most 96 printable characters.".into());
        }
        *value = value.trim().to_owned();
        if value.is_empty() {
            continue;
        }
        let key = parse(value)?;
        if let Some(other) = seen.insert(key.id(), action) {
            return Err(format!(
                "{action} and {other} use the same keyboard shortcut."
            ));
        }
    }
    Ok(())
}

pub fn enabled(
    action: &str,
    settings: &Settings,
    active: bool,
    chat_shown: bool,
    recording: bool,
) -> bool {
    Action::from_name(action).is_some_and(|action| {
        ShortcutContext {
            active,
            ambient_enabled: settings.ambient_enabled,
            chat_shown,
            recording,
        }
        .enabled(action)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_preserves_offscreen_monitor_coordinates_and_stays_in_work_area() {
        assert_eq!(clamp_movement(-1800, -80, -1920, 1920, 560), -1880);
        assert_eq!(clamp_movement(-1880, -80, -1920, 1920, 560), -1920);
        assert_eq!(clamp_movement(1300, 80, 0, 1920, 560), 1360);
        assert_eq!(clamp_movement(0, 80, 0, 300, 560), 0);
    }
    #[test]
    fn detects_equivalent_accelerators_including_settings() {
        let mut values = default_shortcuts();
        values.insert("ask".into(), "CommandOrControl+R".into());
        assert!(validate(&mut values).unwrap_err().contains("same"));
        values.insert("ask".into(), SETTINGS_ACCELERATOR.into());
        assert!(validate(&mut values).unwrap_err().contains("settings"));
    }
    #[test]
    fn supports_partial_legacy_maps_but_rejects_unsafe_syntax() {
        let mut values = BTreeMap::from([("ask".into(), "Ctrl+Alt+K".into())]);
        validate(&mut values).unwrap();
        assert_eq!(values.len(), 11);
        assert_eq!(values["settings"], SETTINGS_ACCELERATOR);
        values.insert("ask".into(), "Ctrl+K\n".into());
        assert!(validate(&mut values).is_err());
        values.insert("ask".into(), "Ctrl+K+A".into());
        assert!(validate(&mut values).is_err());
        values.insert("ask".into(), String::new());
        assert!(validate(&mut values).is_ok());
    }
    #[test]
    fn every_action_is_global_except_while_recording_a_shortcut() {
        for ambient_enabled in [false, true] {
            let settings = Settings {
                ambient_enabled,
                ..Default::default()
            };
            for active in [false, true] {
                for chat_shown in [false, true] {
                    for recording in [false, true] {
                        for action in default_shortcuts().keys() {
                            assert_eq!(
                                enabled(action, &settings, active, chat_shown, recording),
                                !recording,
                                "{action}: ambient={ambient_enabled}, active={active}, chat={chat_shown}, recording={recording}"
                            );
                        }
                    }
                }
            }
        }
        assert!(!enabled("unknown", &Settings::default(), true, true, false));
    }

    #[test]
    fn settings_binding_can_move_or_be_unassigned_without_reserving_its_default() {
        let mut values = default_shortcuts();
        values.insert("settings".into(), "Ctrl+Alt+S".into());
        values.insert("ask".into(), SETTINGS_ACCELERATOR.into());
        validate(&mut values).unwrap();
        assert_eq!(values["settings"], "Ctrl+Alt+S");
        assert_eq!(values["ask"], SETTINGS_ACCELERATOR);

        values.insert("settings".into(), String::new());
        validate(&mut values).unwrap();
        assert!(values["settings"].is_empty());

        values.insert("settings".into(), "CommandOrControl+Shift+,".into());
        assert!(validate(&mut values).unwrap_err().contains("same keyboard"));
    }

    #[test]
    fn legacy_ten_action_map_gains_settings_and_keeps_every_override() {
        let mut values = default_shortcuts();
        values.remove("settings");
        values.insert("ask".into(), "Ctrl+Alt+K".into());
        values.insert("clear".into(), String::new());
        let legacy = values.clone();
        validate(&mut values).unwrap();
        assert_eq!(values.len(), 11);
        assert_eq!(values["settings"], SETTINGS_ACCELERATOR);
        for (action, accelerator) in legacy {
            assert_eq!(values[&action], accelerator);
        }
    }
}
