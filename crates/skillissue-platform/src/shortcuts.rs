// Rust | shortcuts.rs | native desktop global shortcuts
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
pub use skillissue_domain::shortcuts::{
    Action, SETTINGS_ACCELERATOR, ShortcutContext, default_shortcuts, parse, validate,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
struct Binding {
    action: Action,
    accelerator: String,
    hotkey: HotKey,
}

fn desired_bindings(
    values: &BTreeMap<String, String>,
    context: ShortcutContext,
) -> Result<BTreeMap<u32, Binding>, String> {
    let mut values = values.clone();
    validate(&mut values)?;
    let mut result = BTreeMap::new();
    for (name, accelerator) in values {
        let action = Action::from_name(&name).ok_or_else(|| format!("Unknown action {name}."))?;
        if accelerator.is_empty() || !context.enabled(action) {
            continue;
        }
        let hotkey = parse(&accelerator)?;
        result.insert(
            hotkey.id(),
            Binding {
                action,
                accelerator,
                hotkey,
            },
        );
    }
    Ok(result)
}

/// Construct, reconcile, poll, and drop this manager on the UI message-loop thread.
/// Only one consumer should drain the process-wide global-hotkey event receiver.
pub struct GlobalShortcuts {
    manager: GlobalHotKeyManager,
    bindings: BTreeMap<u32, Binding>,
    probes: BTreeMap<u32, HotKey>,
    context: ShortcutContext,
}

impl GlobalShortcuts {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            manager: GlobalHotKeyManager::new()
                .map_err(|error| format!("Create global shortcut manager: {error}"))?,
            bindings: BTreeMap::new(),
            probes: BTreeMap::new(),
            context: ShortcutContext::default(),
        })
    }

    /// Roll back registrations if another process owns one of the requested keys.
    pub fn reconcile(
        &mut self,
        values: &BTreeMap<String, String>,
        context: ShortcutContext,
    ) -> Result<(), String> {
        self.release_probes()?;
        let desired = desired_bindings(values, context)?;
        let previous = self.bindings.clone();
        if let Err(error) = self.replace(&desired) {
            return match self.replace(&previous) {
                Ok(()) => Err(error),
                Err(rollback) => Err(format!("{error} Restore prior shortcuts: {rollback}")),
            };
        }
        self.context = context;
        Ok(())
    }

    /// Probe all proposed bindings before saving, even while recording is paused.
    /// The OS may still assign an accelerator to another process after this probe ends.
    pub fn validate_availability(
        &mut self,
        values: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        let desired = desired_bindings(values, ShortcutContext::default())?;
        self.probe_availability(desired)
    }

    /// Probe newly introduced accelerators, allowing existing conflicts to be
    /// repaired one command at a time without blocking removals or other edits.
    pub fn validate_changes(
        &mut self,
        previous: &BTreeMap<String, String>,
        proposed: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        let previous = desired_bindings(previous, ShortcutContext::default())?;
        let mut desired = desired_bindings(proposed, ShortcutContext::default())?;
        desired.retain(|id, _| !previous.contains_key(id));
        self.probe_availability(desired)
    }

    fn probe_availability(&mut self, desired: BTreeMap<u32, Binding>) -> Result<(), String> {
        self.release_probes()?;
        let mut failure = None;
        for (id, binding) in desired {
            if self.bindings.contains_key(&id) {
                continue;
            }
            match self.manager.register(binding.hotkey) {
                Ok(()) => {
                    self.probes.insert(id, binding.hotkey);
                }
                Err(error) => {
                    failure = Some(format!(
                        "Could not register {} ({}). Another app may be using it. {error}",
                        binding.accelerator,
                        binding.action.as_str()
                    ));
                    break;
                }
            }
        }
        match (failure, self.release_probes()) {
            (None, result) => result,
            (Some(error), Ok(())) => Err(error),
            (Some(error), Err(cleanup)) => Err(format!("{error} {cleanup}")),
        }
    }

    fn release_probes(&mut self) -> Result<(), String> {
        let mut failures = Vec::new();
        self.probes
            .retain(|_, key| match self.manager.unregister(*key) {
                Ok(()) => false,
                Err(error) => {
                    failures.push(format!("Release shortcut validation key {key}: {error}"));
                    true
                }
            });
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }

    fn replace(&mut self, desired: &BTreeMap<u32, Binding>) -> Result<(), String> {
        let obsolete: Vec<_> = self
            .bindings
            .iter()
            .filter(|(id, _)| !desired.contains_key(id))
            .map(|(id, binding)| (*id, binding.clone()))
            .collect();
        for (id, binding) in obsolete {
            self.manager.unregister(binding.hotkey).map_err(|error| {
                format!(
                    "Release {} ({}): {error}",
                    binding.accelerator,
                    binding.action.as_str()
                )
            })?;
            self.bindings.remove(&id);
        }
        for (id, binding) in desired {
            if !self.bindings.contains_key(id) {
                self.manager.register(binding.hotkey).map_err(|error| {
                    format!(
                        "Could not register {} ({}). Another app may be using it. {error}",
                        binding.accelerator,
                        binding.action.as_str()
                    )
                })?;
            }
            self.bindings.insert(*id, binding.clone());
        }
        Ok(())
    }

    pub fn poll(&self) -> Vec<Action> {
        GlobalHotKeyEvent::receiver()
            .try_iter()
            .filter(|event| event.state == HotKeyState::Pressed)
            .filter_map(|event| self.bindings.get(&event.id))
            .filter(|binding| self.context.enabled(binding.action))
            .map(|binding| binding.action)
            .collect()
    }

    pub fn registered(&self) -> Vec<(Action, String)> {
        self.bindings
            .values()
            .map(|entry| (entry.action, entry.accelerator.clone()))
            .collect()
    }
}

impl Drop for GlobalShortcuts {
    fn drop(&mut self) {
        for binding in self.bindings.values() {
            let _ = self.manager.unregister(binding.hotkey);
        }
        for key in self.probes.values() {
            let _ = self.manager.unregister(*key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    static HOTKEY_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn legacy_partial_maps_keep_all_eleven_actions() {
        let mut values = BTreeMap::from([("ask".into(), " Ctrl+Alt+K ".into())]);
        validate(&mut values).unwrap();
        assert_eq!(values.len(), 11);
        assert_eq!(values["ask"], "Ctrl+Alt+K");
        assert_eq!(values["toggleVisibility"], "Ctrl+\\");
        assert_eq!(values["settings"], SETTINGS_ACCELERATOR);
    }

    #[test]
    fn rejects_alias_collisions_including_settings_key() {
        let mut values = default_shortcuts();
        values.insert("ask".into(), "CommandOrControl+R".into());
        assert!(validate(&mut values).unwrap_err().contains("same keyboard"));
        values.insert("ask".into(), SETTINGS_ACCELERATOR.into());
        assert!(validate(&mut values).unwrap_err().contains("settings"));
    }

    #[test]
    fn rejects_unknown_actions_control_characters_and_invalid_chords() {
        let mut values = default_shortcuts();
        values.insert("other".into(), "Ctrl+A".into());
        assert!(validate(&mut values).unwrap_err().contains("Unknown"));
        values.remove("other");
        for invalid in ["Ctrl+K\n", "Ctrl+K+A", "\tCtrl+K", &"a".repeat(97)] {
            values.insert("ask".into(), invalid.into());
            assert!(validate(&mut values).is_err(), "accepted {invalid:?}");
        }
        values.insert("ask".into(), String::new());
        assert!(validate(&mut values).is_ok());
    }

    #[test]
    fn all_actions_remain_registered_without_a_session_or_visible_chat() {
        let values = default_shortcuts();
        assert_eq!(
            desired_bindings(&values, ShortcutContext::default())
                .unwrap()
                .len(),
            11
        );
        let context = ShortcutContext {
            ambient_enabled: true,
            chat_shown: true,
            ..Default::default()
        };
        assert_eq!(desired_bindings(&values, context).unwrap().len(), 11);
        assert_eq!(
            desired_bindings(
                &values,
                ShortcutContext {
                    chat_shown: false,
                    ..context
                }
            )
            .unwrap()
            .len(),
            11
        );
        assert!(
            desired_bindings(
                &values,
                ShortcutContext {
                    recording: true,
                    ..context
                }
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn registry_uses_the_custom_settings_binding_and_honors_unassigning() {
        let mut values = default_shortcuts();
        values.insert("settings".into(), "Ctrl+Alt+S".into());
        values.insert("ask".into(), SETTINGS_ACCELERATOR.into());
        let desired = desired_bindings(&values, ShortcutContext::default()).unwrap();
        assert_eq!(desired.len(), 11);
        assert_eq!(
            desired[&parse("Ctrl+Alt+S").unwrap().id()].action,
            Action::Settings
        );
        assert_eq!(
            desired[&parse(SETTINGS_ACCELERATOR).unwrap().id()].action,
            Action::Ask
        );
        values.insert("settings".into(), String::new());
        let desired = desired_bindings(&values, ShortcutContext::default()).unwrap();
        assert_eq!(desired.len(), 10);
        assert!(
            desired
                .values()
                .all(|binding| binding.action != Action::Settings)
        );
    }

    #[test]
    fn rebinding_swaps_actions_without_duplicate_ids() {
        let mut values = default_shortcuts();
        values.insert("ask".into(), "Ctrl+R".into());
        values.insert("clear".into(), "Ctrl+Enter".into());
        let context = ShortcutContext {
            active: true,
            chat_shown: true,
            ..Default::default()
        };
        let desired = desired_bindings(&values, context).unwrap();
        assert_eq!(desired[&parse("Ctrl+R").unwrap().id()].action, Action::Ask);
        assert_eq!(
            desired[&parse("Ctrl+Enter").unwrap().id()].action,
            Action::Clear
        );
    }

    #[cfg(windows)]
    #[test]
    fn edits_can_repair_multiple_existing_conflicts_one_binding_at_a_time() {
        let _guard = HOTKEY_TEST_LOCK.lock().unwrap();
        use global_hotkey::hotkey::{Code, Modifiers};
        let owner = GlobalHotKeyManager::new().unwrap();
        let modifiers = Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT;
        let keys = [
            Code::F24,
            Code::F23,
            Code::F22,
            Code::F21,
            Code::F20,
            Code::F19,
        ]
        .into_iter()
        .filter_map(|code| {
            let key = HotKey::new(Some(modifiers), code);
            owner.register(key).ok().map(|()| key)
        })
        .take(3)
        .collect::<Vec<_>>();
        assert_eq!(keys.len(), 3, "three uncommon test keys are available");
        let (first, second, available) = (keys[0], keys[1], keys[2]);
        owner.unregister(available).unwrap();

        let mut previous = default_shortcuts();
        previous.values_mut().for_each(String::clear);
        previous.insert("ask".into(), first.to_string());
        previous.insert("clear".into(), second.to_string());
        let mut shortcuts = GlobalShortcuts::new().unwrap();
        assert!(shortcuts.validate_availability(&previous).is_err());

        let mut proposed = previous.clone();
        proposed.insert("ask".into(), String::new());
        shortcuts.validate_changes(&previous, &proposed).unwrap();
        assert!(shortcuts.registered().is_empty());
        assert!(shortcuts.probes.is_empty());

        proposed.insert("ask".into(), available.to_string());
        shortcuts.validate_changes(&previous, &proposed).unwrap();
        assert!(shortcuts.registered().is_empty());
        assert!(shortcuts.probes.is_empty());
        owner.register(available).unwrap();
        assert!(shortcuts.validate_changes(&previous, &proposed).is_err());
        owner.unregister(available).unwrap();

        let mut repaired = proposed.clone();
        repaired.insert("clear".into(), String::new());
        shortcuts.validate_changes(&proposed, &repaired).unwrap();
        shortcuts
            .reconcile(&repaired, ShortcutContext::default())
            .unwrap();
        assert_eq!(
            shortcuts.registered(),
            vec![(Action::Ask, available.to_string())]
        );
        assert!(owner.register(first).is_err());
        assert!(owner.register(second).is_err());
        owner.unregister(first).unwrap();
        owner.unregister(second).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn availability_probe_does_not_release_owned_bindings_on_conflict() {
        let _guard = HOTKEY_TEST_LOCK.lock().unwrap();
        use global_hotkey::hotkey::{Code, Modifiers};
        let manager = GlobalHotKeyManager::new().unwrap();
        let modifiers = Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT;
        let keys = [Code::F24, Code::F23, Code::F22, Code::F21]
            .into_iter()
            .filter_map(|code| {
                let key = HotKey::new(Some(modifiers), code);
                manager.register(key).ok().map(|()| key)
            })
            .take(2)
            .collect::<Vec<_>>();
        assert_eq!(
            keys.len(),
            2,
            "two uncommon accelerators are available for the Windows registration test"
        );
        let (owned, conflict) = (keys[0], keys[1]);
        manager.unregister(owned).unwrap();
        let mut shortcuts = GlobalShortcuts::new().unwrap();
        shortcuts.manager.register(owned).unwrap();
        shortcuts.bindings.insert(
            owned.id(),
            Binding {
                action: Action::Ask,
                accelerator: owned.to_string(),
                hotkey: owned,
            },
        );
        let desired = BTreeMap::from([(
            conflict.id(),
            Binding {
                action: Action::Ask,
                accelerator: conflict.to_string(),
                hotkey: conflict,
            },
        )]);
        assert!(
            shortcuts
                .probe_availability(desired.clone())
                .unwrap_err()
                .contains("ask")
        );
        assert_eq!(
            shortcuts.registered(),
            vec![(Action::Ask, owned.to_string())]
        );
        assert!(manager.register(owned).is_err());
        assert!(shortcuts.probes.is_empty());
        manager.unregister(conflict).unwrap();
        shortcuts.probe_availability(desired).unwrap();
        assert!(shortcuts.probes.is_empty());
        manager.register(conflict).unwrap();
        manager.unregister(conflict).unwrap();
        drop(shortcuts);
        manager.register(owned).unwrap();
        manager.unregister(owned).unwrap();
    }
}
