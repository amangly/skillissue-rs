//! Queued settings patches, applied against the latest committed configuration.
use super::*;

pub(super) fn apply_preference_patch(
    settings: &mut Settings,
    patch: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    let mut value = serde_json::to_value(&*settings).map_err(|error| error.to_string())?;
    let object = value.as_object_mut().ok_or("Settings must be an object")?;
    for (name, value) in patch {
        object.insert(name.clone(), value.clone());
    }
    *settings = serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(())
}

impl NativeView {
    pub(super) fn change_preference(&mut self, change: impl FnOnce(&mut Settings)) {
        let before = self.settings.clone();
        let mut next = before.clone();
        change(&mut next);
        let patch = (|| -> Result<serde_json::Map<String, serde_json::Value>, String> {
            let old = serde_json::to_value(&before).map_err(|error| error.to_string())?;
            let serde_json::Value::Object(values) =
                serde_json::to_value(&next).map_err(|error| error.to_string())?
            else {
                return Err("Settings must be an object".into());
            };
            Ok(values
                .into_iter()
                .filter(|(name, value)| old.get(name) != Some(value))
                .collect())
        })();
        match patch {
            Ok(patch) if !patch.is_empty() => {
                self.settings = next;
                self.preference_queue.push_back(patch);
                self.flush_preferences();
            }
            Ok(_) => {}
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn flush_preferences(&mut self) {
        if self.preference_saving || self.engine.is_none() {
            return;
        }
        let Some(patch) = self.preference_queue.front().cloned() else {
            return;
        };
        self.preference_saving = true;
        self.background(move |engine| {
            Reply::Preference(
                engine.update_preferences(move |settings| apply_preference_patch(settings, &patch)),
            )
        });
    }
}
