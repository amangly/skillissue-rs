// Rust | config.rs | shared native runtime | Windows Credential Manager and local settings
use crate::models::Settings;
use skillissue_domain::identity;
use std::{collections::HashSet, path::Path};

const MAX_SETTINGS_BYTES: usize = 64 * 1024;
const MAX_MODE_PROMPT_BYTES: usize = 16_000;

fn bounded_label(label: &str, value: &str, limit: usize, allow_empty: bool) -> Result<(), String> {
    if (!allow_empty && value.trim().is_empty())
        || value.len() > limit
        || value.chars().any(char::is_control)
    {
        return Err(format!(
            "{label} must {}contain at most {limit} bytes without control characters.",
            if allow_empty { "" } else { "be nonempty and " }
        ));
    }
    Ok(())
}

fn mode_id<'a>(ids: &mut HashSet<&'a str>, id: &'a str) -> Result<(), String> {
    bounded_label("Mode, section, and file IDs", id, 128, false)?;
    if !ids.insert(id) {
        return Err("Mode, section, and file IDs must be unique.".into());
    }
    Ok(())
}

fn validate_modes(settings: &Settings) -> Result<(), String> {
    if settings.modes.len() > 32 {
        return Err("Save at most 32 custom modes.".into());
    }
    let mut ids = HashSet::new();
    for mode in &settings.modes {
        mode_id(&mut ids, &mode.id)?;
        if mode.id == "general" {
            return Err("General is reserved for the built-in mode.".into());
        }
        bounded_label("Mode names", &mode.name, 200, true)?;
        if mode.prompt.len() > MAX_MODE_PROMPT_BYTES {
            return Err("Mode prompts must contain at most 16,000 bytes.".into());
        }
        if mode.sections.len() > 32 {
            return Err("Save at most 32 notes sections per mode.".into());
        }
        let mut section_ids = HashSet::new();
        for section in &mode.sections {
            mode_id(&mut section_ids, &section.id)?;
            bounded_label("Notes section titles", &section.title, 200, true)?;
            if section.prompt.len() > 4_000 {
                return Err("Notes section prompts must contain at most 4,000 bytes.".into());
            }
        }
        if mode.files.len() > 3 {
            return Err("Save at most three reference file selections per mode.".into());
        }
        let mut file_ids = HashSet::new();
        for file in &mode.files {
            mode_id(&mut file_ids, &file.id)?;
            bounded_label("Reference file names", &file.name, 512, false)?;
            if file.size > 20 * 1024 * 1024 {
                return Err("Reference file selections must be 20 MiB or smaller.".into());
            }
        }
    }
    bounded_label("The active mode ID", &settings.active_mode_id, 128, false)?;
    if settings.active_mode_id != "general"
        && !settings
            .modes
            .iter()
            .any(|mode| mode.id == settings.active_mode_id)
    {
        return Err("The active mode must be General or a saved custom mode.".into());
    }
    Ok(())
}

fn encode_settings(settings: &Settings) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_SETTINGS_BYTES {
        return Err(
            "Settings exceed 64 KiB. Shorten the saved mode prompts or notes templates.".into(),
        );
    }
    Ok(bytes)
}

pub fn validate(mut settings: Settings) -> Result<Settings, String> {
    settings.api_base_url = crate::provider::validate_base_url(&settings.api_base_url)?;
    settings.transcription_base_url = settings.transcription_base_url.trim().to_owned();
    if !settings.transcription_base_url.is_empty() {
        settings.transcription_base_url =
            crate::provider::validate_base_url(&settings.transcription_base_url)?;
    }
    settings.image_base_url = settings.image_base_url.trim().to_owned();
    if !settings.image_base_url.is_empty() {
        settings.image_base_url = crate::provider::validate_base_url(&settings.image_base_url)?;
    }
    settings.image_model = settings.image_model.trim().to_owned();
    if settings.image_model.len() > 200 || settings.image_model.chars().any(char::is_control) {
        return Err("Image model must be at most 200 characters.".into());
    }
    settings.chat_model = settings.chat_model.trim().to_owned();
    settings.transcription_model = settings.transcription_model.trim().to_owned();
    for (label, value) in [
        ("Assistant model", &settings.chat_model),
        ("Transcription model", &settings.transcription_model),
    ] {
        if (label == "Transcription model" && value.trim().is_empty())
            || value.len() > 200
            || value.chars().any(char::is_control)
        {
            return Err(format!("{label} must contain 1–200 characters."));
        }
    }
    settings.palepale.chat_model = settings.palepale.chat_model.trim().to_owned();
    settings.palepale.transcription_model = settings.palepale.transcription_model.trim().to_owned();
    for model in [
        &settings.palepale.chat_model,
        &settings.palepale.transcription_model,
    ] {
        if model.len() > 128
            || !model
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"/._:-".contains(&byte))
        {
            return Err("The Palepale model must be an ID of at most 128 ASCII characters using letters, numbers, /, ., _, : or -.".into());
        }
    }
    if settings
        .palepale
        .reasoning_effort
        .as_deref()
        .is_some_and(|effort| !skillissue_domain::provider::REASONING_EFFORTS.contains(&effort))
    {
        return Err("Choose a published Palepale reasoning effort or Server default.".into());
    }
    if settings.system_prompt.len() > 16_000 {
        return Err("The system prompt must be shorter than 16,000 bytes.".into());
    }
    settings.language = settings.language.trim().to_owned();
    if settings.language.len() > 16
        || !settings
            .language
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c == '-')
    {
        return Err(
            "Language must be an ISO language code, or blank for automatic detection.".into(),
        );
    }
    if !(3..=20).contains(&settings.audio_chunk_seconds) {
        return Err("Audio chunks must be between 3 and 20 seconds.".into());
    }
    if !matches!(settings.theme.as_str(), "system" | "light" | "dark") {
        return Err("Choose system, light, or dark for the appearance.".into());
    }
    if !matches!(settings.smart_effort.as_str(), "low" | "high") {
        return Err("Choose Low or High Smart effort.".into());
    }
    settings.output_language = settings.output_language.trim().to_owned();
    if settings.output_language.is_empty()
        || settings.output_language.len() > 35
        || !settings
            .output_language
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("Output language must be a language tag such as en-US.".into());
    }
    crate::shortcuts::validate(&mut settings.shortcuts)?;
    validate_modes(&settings)?;
    encode_settings(&settings)?;
    Ok(settings)
}

pub fn load(path: &Path) -> Result<Settings, String> {
    if !path.exists() {
        let mut settings = Settings::default();
        if let Ok(base) = std::env::var(identity::API_BASE_ENV) {
            settings.api_base_url = base;
        }
        if let Ok(model) = std::env::var(identity::CHAT_MODEL_ENV) {
            settings.chat_model = model;
        }
        if let Ok(model) = std::env::var(identity::TRANSCRIPTION_MODEL_ENV) {
            settings.transcription_model = model;
        }
        return validate(settings);
    }
    let metadata = std::fs::metadata(path).map_err(|e| format!("Read settings: {e}"))?;
    if metadata.len() > MAX_SETTINGS_BYTES as u64 {
        return Err("Settings file exceeds 64 KiB.".into());
    }
    let bytes = std::fs::read(path).map_err(|e| format!("Read settings: {e}"))?;
    if bytes.len() > MAX_SETTINGS_BYTES {
        return Err("Settings file exceeds 64 KiB.".into());
    }
    let mut settings: Settings =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid settings: {e}"))?;
    let legacy_prompt = "You are a concise desktop assistant. Use the provided transcript and screen only as context. Answer the user directly. If no explicit question is supplied, identify the latest question in the conversation and help answer it. State uncertainty when context is insufficient.";
    if settings.system_prompt == legacy_prompt {
        settings.system_prompt = skillissue_domain::models::DEFAULT_SYSTEM_PROMPT.into();
    }
    validate(settings)
}

pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    let settings = validate(settings.clone())?;
    let bytes = encode_settings(&settings)?;
    let parent = path.parent().ok_or("Settings directory is unavailable.")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("Create settings directory: {e}"))?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, bytes).map_err(|e| format!("Write settings: {e}"))?;
    std::fs::rename(&temp, path).map_err(|e| format!("Commit settings: {e}"))
}

pub fn load_key(base: &str) -> Result<String, String> {
    if let Some(key) = load_saved_key(base)? {
        return Ok(key);
    }
    Ok(environment_key(base))
}

pub(crate) fn environment_key(base: &str) -> String {
    let env_base =
        std::env::var(identity::API_BASE_ENV).unwrap_or_else(|_| Settings::default().api_base_url);
    if env_base.trim_end_matches('/') == base.trim_end_matches('/') {
        return std::env::var(identity::API_KEY_ENV).unwrap_or_default();
    }
    String::new()
}

/// BYOK retains its historical URL scope; hosted credentials never share that scope.
pub fn provider_credential_id(settings: &Settings) -> String {
    if settings.is_palepale() || Settings::hosted_endpoint(&settings.api_base_url) {
        format!(
            "palepale:{}",
            skillissue_domain::provider::PALEPALE_BASE_URL
        )
    } else {
        settings.api_base_url.clone()
    }
}

pub fn load_provider_key(settings: &Settings) -> Result<String, String> {
    resolve_provider_key(settings, load_saved_key, load_key)
}

fn resolve_provider_key(
    settings: &Settings,
    saved: impl FnOnce(&str) -> Result<Option<String>, String>,
    byok: impl FnOnce(&str) -> Result<String, String>,
) -> Result<String, String> {
    if settings.is_palepale() || Settings::hosted_endpoint(&settings.api_base_url) {
        Ok(saved(&provider_credential_id(settings))?.unwrap_or_default())
    } else {
        byok(&settings.api_base_url)
    }
}

pub fn load_saved_key(base: &str) -> Result<Option<String>, String> {
    load_saved_key_for_service(identity::CREDENTIAL_SERVICE, base)
}

pub(crate) fn load_saved_key_for_service(
    service: &str,
    base: &str,
) -> Result<Option<String>, String> {
    #[cfg(windows)]
    {
        let entry = keyring::Entry::new(service, base)
            .map_err(|e| format!("Open credential store: {e}"))?;
        match entry.get_password() {
            Ok(key) => return Ok(Some(key)),
            Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(format!("Read credential store: {e}")),
        }
    }
    let _ = (service, base);
    Ok(None)
}

pub fn save_key(base: &str, key: &str) -> Result<(), String> {
    save_key_for_service(identity::CREDENTIAL_SERVICE, base, key)
}

pub(crate) fn save_key_for_service(service: &str, base: &str, key: &str) -> Result<(), String> {
    if key.len() > 4096 || key.chars().any(char::is_control) {
        return Err("API key contains invalid characters or exceeds 4096 bytes.".into());
    }
    #[cfg(windows)]
    {
        let entry = keyring::Entry::new(service, base)
            .map_err(|e| format!("Open credential store: {e}"))?;
        if key.is_empty() {
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(format!("Delete saved API key: {e}")),
            }
        } else {
            entry
                .set_password(key)
                .map_err(|e| format!("Save API key: {e}"))
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (service, base);
        Err(
            "Saved credentials currently require Windows. Use SKILLISSUE_API_KEY for this platform."
                .into(),
        )
    }
}

pub fn image_credential_id(settings: &Settings) -> String {
    if Settings::hosted_endpoint(&settings.image_base_url) {
        return format!(
            "palepale:{}",
            skillissue_domain::provider::PALEPALE_BASE_URL
        );
    }
    format!("image:{}", settings.image_base_url)
}

/// Audio overrides never use the chat key or environment fallback.
pub fn transcription_credential_id(settings: &Settings) -> String {
    if Settings::hosted_endpoint(&settings.transcription_base_url) {
        return format!(
            "palepale:{}",
            skillissue_domain::provider::PALEPALE_BASE_URL
        );
    }
    format!("transcription:{}", settings.transcription_base_url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AssistantMode, ModeFile, ModeSection};

    fn custom_mode(id: &str) -> AssistantMode {
        AssistantMode {
            id: id.into(),
            name: "Team meeting".into(),
            prompt: "Help me identify decisions and open questions.".into(),
            sections: vec![ModeSection {
                id: "notes".into(),
                title: "Decisions".into(),
                prompt: "Decisions agreed during the meeting.".into(),
            }],
            files: vec![ModeFile {
                id: "reference".into(),
                name: "agenda.pdf".into(),
                size: 20 * 1024 * 1024,
            }],
        }
    }

    #[test]
    fn byok_model_ids_normalize_pasted_whitespace_and_reject_embedded_controls() {
        let mut settings = Settings {
            chat_model: "  provider/chat-model\r\n".into(),
            transcription_model: " speech-model \t".into(),
            ..Settings::default()
        };
        let normalized = validate(settings.clone()).unwrap();
        assert_eq!(normalized.chat_model, "provider/chat-model");
        assert_eq!(normalized.transcription_model, "speech-model");
        settings.chat_model = "chat\nmodel".into();
        assert!(validate(settings.clone()).is_err());
        settings.chat_model = " \t\r\n".into();
        assert!(validate(settings).unwrap().chat_model.is_empty());
    }

    #[test]
    fn settings_reject_invalid_resource_limits() {
        let mut settings = Settings {
            audio_chunk_seconds: 0,
            ..Settings::default()
        };
        assert!(validate(settings.clone()).is_err());
        settings.audio_chunk_seconds = 8;
        settings.language = "en\r\nX: injected".into();
        assert!(validate(settings).is_err());
    }

    #[test]
    fn settings_round_trip_does_not_include_credentials() {
        let json = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(json["apiBaseUrl"], "https://api.openai.com/v1");
        assert!(json.get("apiKey").is_none());
        assert!(validate(serde_json::from_value(json).unwrap()).is_ok());
    }

    #[test]
    fn hosted_reasoning_persists_and_old_settings_keep_server_default() {
        let old: Settings = serde_json::from_str(r#"{"palepale":{"chatModel":"hosted"}}"#).unwrap();
        assert!(old.palepale.reasoning_effort.is_none());
        for effort in skillissue_domain::provider::REASONING_EFFORTS {
            let mut settings = old.clone();
            settings.palepale.reasoning_effort = Some(effort.into());
            let encoded = serde_json::to_value(&settings).unwrap();
            assert_eq!(encoded["palepale"]["reasoningEffort"], effort);
            let restored: Settings = serde_json::from_value(encoded).unwrap();
            assert_eq!(
                validate(restored)
                    .unwrap()
                    .palepale
                    .reasoning_effort
                    .as_deref(),
                Some(effort)
            );
        }
        for invalid in ["", "High", "auto", " high", "high\n"] {
            let mut settings = old.clone();
            settings.palepale.reasoning_effort = Some(invalid.into());
            assert!(validate(settings).unwrap_err().contains("reasoning effort"));
        }
    }

    #[test]
    fn pre_ui_settings_receive_all_new_defaults() {
        let settings: Settings =
            serde_json::from_str(r#"{"chatModel":"local-model","captureProtection":true}"#)
                .unwrap();
        let settings = validate(settings).unwrap();
        assert_eq!(settings.chat_model, "local-model");
        assert!(settings.capture_protection);
        assert!(!settings.ambient_enabled);
        assert!(settings.hide_widget);
        assert!(settings.open_at_login);
        assert!(!settings.smart_mode);
        assert_eq!(settings.shortcuts.len(), 11);
        assert_eq!(settings.output_language, "en-US");
        assert!(settings.modes.is_empty());
        assert_eq!(settings.active_mode_id, "general");
        assert_eq!(settings.provider_mode, crate::models::ProviderMode::Byok);
        assert!(settings.palepale.chat_model.is_empty());
        assert_eq!(settings.provider_base_url(), settings.api_base_url);
        assert_eq!(settings.provider_chat_model(), "local-model");
    }

    #[test]
    fn provider_mode_migration_keeps_independent_byok_and_hosted_drafts() {
        let mut settings: Settings = serde_json::from_str(
            r#"{"apiBaseUrl":"https://example.test/v1","chatModel":"byok-model","transcriptionModel":"byok-speech"}"#,
        )
        .unwrap();
        settings = validate(settings).unwrap();
        assert!(!settings.is_palepale());
        assert!(settings.palepale.transcription_model.is_empty());
        settings.provider_mode = crate::models::ProviderMode::Palepale;
        assert!(
            validate(settings.clone()).is_ok(),
            "Saving incomplete hosted setup must be allowed"
        );
        settings.palepale.chat_model = "hosted-model".into();
        settings.palepale.transcription_model = "hosted-audio".into();
        let encoded = serde_json::to_value(&settings).unwrap();
        assert_eq!(encoded["providerMode"], "palepale");
        assert_eq!(encoded["palepale"]["chatModel"], "hosted-model");
        assert_eq!(encoded["palepale"]["transcriptionModel"], "hosted-audio");
        let mut restored = validate(serde_json::from_value(encoded).unwrap()).unwrap();
        assert_eq!(
            restored.provider_base_url(),
            skillissue_domain::provider::PALEPALE_BASE_URL
        );
        assert_eq!(restored.provider_chat_model(), "hosted-model");
        assert_eq!(restored.provider_transcription_model(), "hosted-audio");
        restored.provider_mode = crate::models::ProviderMode::Byok;
        assert_eq!(restored.provider_base_url(), "https://example.test/v1");
        assert_eq!(restored.provider_chat_model(), "byok-model");
        assert_eq!(restored.transcription_model, "byok-speech");
        assert_eq!(restored.provider_transcription_model(), "byok-speech");
        assert_eq!(restored.palepale.chat_model, "hosted-model");
        for invalid in [
            "bad\nmodel".into(),
            "bad model".into(),
            "模型".into(),
            "x".repeat(129),
        ] {
            restored.palepale.chat_model = invalid;
            assert!(
                validate(restored.clone())
                    .unwrap_err()
                    .contains("Palepale model")
            );
        }
        restored.palepale.chat_model = "x".repeat(128);
        assert!(validate(restored.clone()).is_ok());
        restored.palepale.chat_model = "  provider/model.v1_1:latest-test  ".into();
        assert_eq!(
            validate(restored).unwrap().palepale.chat_model,
            "provider/model.v1_1:latest-test"
        );
    }

    #[test]
    fn old_hosted_settings_require_discovery_for_audio_and_validate_independent_model_ids() {
        let settings: Settings = serde_json::from_str(
            r#"{"providerMode":"palepale","palepale":{"chatModel":"old-image"}}"#,
        )
        .unwrap();
        assert!(validate(settings.clone()).is_ok());
        assert!(settings.provider_transcription_model().is_empty());
        for invalid in ["has space".into(), "bad\nmodel".into(), "x".repeat(129)] {
            let mut invalid_settings = settings.clone();
            invalid_settings.palepale.transcription_model = invalid;
            assert!(validate(invalid_settings).is_err());
        }
    }

    #[test]
    fn hosted_credentials_never_use_byok_url_scope_or_environment_fallback() {
        let mut settings = Settings {
            api_base_url: "https://byok.example/v1".into(),
            ..Settings::default()
        };
        assert_eq!(provider_credential_id(&settings), settings.api_base_url);
        assert_eq!(
            resolve_provider_key(
                &settings,
                |_| panic!("BYOK resolver owns its legacy URL and environment policy"),
                |base| {
                    assert_eq!(base, settings.api_base_url);
                    Ok("BYOK-env-or-saved-key".into())
                },
            )
            .unwrap(),
            "BYOK-env-or-saved-key"
        );
        settings.provider_mode = crate::models::ProviderMode::Palepale;
        assert_eq!(
            provider_credential_id(&settings),
            "palepale:https://api.palepale.tech/v1"
        );
        for saved_key in [None, Some("hosted-only-key".to_string())] {
            let expected = saved_key.clone().unwrap_or_default();
            assert_eq!(
                resolve_provider_key(
                    &settings,
                    |id| {
                        assert_eq!(id, "palepale:https://api.palepale.tech/v1");
                        Ok(saved_key)
                    },
                    |_| panic!(
                        "Hosted mode must never inherit a BYOK saved key or environment key"
                    ),
                )
                .unwrap(),
                expected
            );
        }
        settings.provider_mode = crate::models::ProviderMode::Byok;
        settings.api_base_url = format!("{}/", skillissue_domain::provider::PALEPALE_BASE_URL);
        assert_eq!(
            provider_credential_id(&settings),
            "palepale:https://api.palepale.tech/v1"
        );
        assert_eq!(
            resolve_provider_key(
                &settings,
                |_| Ok(Some("account".into())),
                |_| panic!("Hosted rows must not use environment keys")
            )
            .unwrap(),
            "account"
        );
    }

    #[test]
    fn model_catalogue_defaults_capabilities_without_inventing_support() {
        let model: skillissue_domain::provider::ProviderModel =
            serde_json::from_value(serde_json::json!({"id":"provider/model"})).unwrap();
        assert_eq!(model.id, "provider/model");
        assert!(model.display_name.is_empty());
        assert_eq!(
            model.capabilities,
            skillissue_domain::provider::ModelCapabilities::default()
        );
        let encoded = serde_json::to_value(skillissue_domain::provider::ModelCapabilities {
            streaming: true,
            tool_calling: true,
            ..skillissue_domain::provider::ModelCapabilities::default()
        })
        .unwrap();
        assert_eq!(encoded["tool_calling"], true);
        assert_eq!(encoded["reasoning"], false);
        assert!(encoded.get("toolCalling").is_none());
    }

    #[test]
    fn modes_round_trip_and_rejected_save_preserves_the_previous_file() {
        let directory =
            std::env::temp_dir().join(format!("skillissue-modes-{}", uuid::Uuid::new_v4()));
        let path = directory.join("settings.json");
        let settings = Settings {
            modes: vec![custom_mode("team")],
            active_mode_id: "team".into(),
            ..Settings::default()
        };
        save(&path, &settings).unwrap();
        let saved_bytes = std::fs::read(&path).unwrap();
        let reloaded = load(&path).unwrap();
        assert_eq!(reloaded.active_mode_id, "team");
        assert_eq!(reloaded.modes[0].sections[0].title, "Decisions");
        assert_eq!(reloaded.modes[0].files[0].size, 20 * 1024 * 1024);
        assert_eq!(
            serde_json::to_value(&reloaded).unwrap(),
            serde_json::to_value(&settings).unwrap()
        );
        let mut invalid = settings;
        invalid.modes.clear();
        assert!(save(&path, &invalid).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), saved_bytes);
        assert!(!path.with_extension("json.tmp").exists());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn mode_selection_and_ids_are_validated_at_their_own_scope() {
        let mut settings = Settings {
            modes: vec![custom_mode("team"), custom_mode("other")],
            active_mode_id: "team".into(),
            ..Settings::default()
        };
        assert!(validate(settings.clone()).is_ok());
        settings.active_mode_id = "missing".into();
        assert!(validate(settings.clone()).is_err());
        settings.active_mode_id = "general".into();
        settings.modes[1].id = "team".into();
        assert!(validate(settings.clone()).is_err());
        settings.modes[1].id = "general".into();
        assert!(validate(settings.clone()).is_err());
        settings.modes.pop();
        let duplicate_section = settings.modes[0].sections[0].clone();
        settings.modes[0].sections.push(duplicate_section);
        assert!(validate(settings.clone()).is_err());
        settings.modes[0].sections.pop();
        let duplicate_file = settings.modes[0].files[0].clone();
        settings.modes[0].files.push(duplicate_file);
        assert!(validate(settings.clone()).is_err());
        settings.modes[0].files.pop();
        settings.modes[0].name.clear();
        settings.modes[0].sections[0].title.clear();
        assert!(validate(settings.clone()).is_ok());
        settings.modes[0].files[0].name = "file\nname.pdf".into();
        assert!(validate(settings.clone()).is_err());
        settings.modes[0].files[0].name = "agenda.pdf".into();
        settings.modes[0].id = "bad\u{0}id".into();
        assert!(validate(settings).is_err());
    }

    #[test]
    fn mode_limits_bound_prompt_bytes_and_collection_sizes() {
        let mut settings = Settings {
            modes: vec![custom_mode("team")],
            ..Settings::default()
        };
        settings.modes[0].prompt = "🙂".repeat(4_000);
        settings.modes[0].sections[0].prompt = "你".repeat(1_333) + "x";
        assert!(validate(settings.clone()).is_ok());
        settings.modes[0].prompt.push('x');
        assert!(validate(settings.clone()).is_err());
        settings.modes[0].prompt.clear();
        settings.modes[0].sections[0].prompt.push('x');
        assert!(validate(settings.clone()).is_err());
        settings.modes[0].sections[0].prompt.clear();
        settings.modes[0].files[0].size += 1;
        assert!(validate(settings.clone()).is_err());
        settings.modes[0].files.clear();
        settings.modes[0].files = (0..4)
            .map(|index| ModeFile {
                id: format!("file-{index}"),
                name: "reference.txt".into(),
                size: 0,
            })
            .collect();
        assert!(validate(settings.clone()).is_err());
        settings.modes[0].files.clear();
        settings.modes[0].sections = (0..33)
            .map(|index| ModeSection {
                id: format!("section-{index}"),
                title: String::new(),
                prompt: String::new(),
            })
            .collect();
        assert!(validate(settings.clone()).is_err());
        settings.modes = (0..33)
            .map(|index| custom_mode(&format!("mode-{index}")))
            .collect();
        assert!(validate(settings).is_err());
    }

    #[test]
    fn total_pretty_serialization_limit_matches_load_and_save() {
        let directory =
            std::env::temp_dir().join(format!("skillissue-limit-{}", uuid::Uuid::new_v4()));
        let path = directory.join("settings.json");
        let mut settings = Settings {
            modes: (0..4)
                .map(|index| {
                    let mut mode = custom_mode(&format!("mode-{index}"));
                    mode.prompt = "a".repeat(15_000);
                    mode
                })
                .collect(),
            ..Settings::default()
        };
        let available = MAX_SETTINGS_BYTES - serde_json::to_vec_pretty(&settings).unwrap().len();
        settings.modes[0]
            .prompt
            .push_str(&"a".repeat(available.min(1_000)));
        let remaining = MAX_SETTINGS_BYTES - serde_json::to_vec_pretty(&settings).unwrap().len();
        settings.modes[1].sections[0]
            .prompt
            .push_str(&"a".repeat(remaining));
        assert_eq!(
            serde_json::to_vec_pretty(&settings).unwrap().len(),
            MAX_SETTINGS_BYTES
        );
        save(&path, &settings).unwrap();
        assert!(load(&path).is_ok());
        settings.modes[1].sections[0].prompt.push('a');
        assert!(validate(settings.clone()).unwrap_err().contains("64 KiB"));
        let previous = std::fs::read(&path).unwrap();
        assert!(save(&path, &settings).unwrap_err().contains("64 KiB"));
        assert_eq!(std::fs::read(&path).unwrap(), previous);
        std::fs::write(&path, serde_json::to_vec_pretty(&settings).unwrap()).unwrap();
        assert!(load(&path).unwrap_err().contains("64 KiB"));
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
