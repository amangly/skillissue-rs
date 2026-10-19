// Rust | models.rs | shared native runtime | persisted settings and session models
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const DEFAULT_SYSTEM_PROMPT: &str = include_str!("assistant_prompt.md");
pub const RESPONSE_STYLE: &str = include_str!("response_style.md");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderMode {
    #[default]
    Byok,
    Palepale,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PalepaleSettings {
    pub chat_model: String,
    pub transcription_model: String,
    pub reasoning_effort: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub provider_mode: ProviderMode,
    pub palepale: PalepaleSettings,
    pub api_base_url: String,
    /// Empty uses the chat connection; otherwise a separately scoped audio connection.
    pub transcription_base_url: String,
    pub image_base_url: String,
    pub image_model: String,
    pub byok_audio_enabled: bool,
    pub chat_model: String,
    pub transcription_model: String,
    pub language: String,
    pub system_prompt: String,
    pub capture_protection: bool,
    pub no_focus: bool,
    pub include_screenshot: bool,
    pub microphone: bool,
    pub system_audio: bool,
    pub audio_chunk_seconds: u32,
    pub ambient_enabled: bool,
    pub hide_widget: bool,
    pub open_at_login: bool,
    pub theme: String,
    pub output_language: String,
    pub smart_mode: bool,
    pub smart_effort: String,
    pub shortcuts: BTreeMap<String, String>,
    pub modes: Vec<AssistantMode>,
    pub active_mode_id: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            provider_mode: ProviderMode::Byok,
            palepale: PalepaleSettings::default(),
            api_base_url: "https://api.openai.com/v1".into(),
            transcription_base_url: String::new(),
            image_base_url: String::new(),
            image_model: String::new(),
            byok_audio_enabled: true,
            chat_model: "gpt-4.1-mini".into(),
            transcription_model: "gpt-4o-mini-transcribe".into(),
            language: String::new(),
            system_prompt: DEFAULT_SYSTEM_PROMPT.into(),
            capture_protection: false,
            no_focus: false,
            include_screenshot: true,
            microphone: true,
            system_audio: true,
            audio_chunk_seconds: 8,
            ambient_enabled: false,
            hide_widget: true,
            open_at_login: true,
            theme: "system".into(),
            output_language: "en-US".into(),
            smart_mode: false,
            smart_effort: "high".into(),
            shortcuts: default_shortcuts(),
            modes: Vec::new(),
            active_mode_id: "general".into(),
        }
    }
}

impl Settings {
    pub fn hosted_endpoint(endpoint: &str) -> bool {
        endpoint.trim_end_matches('/') == crate::provider::PALEPALE_BASE_URL.trim_end_matches('/')
    }

    /// Resolve a BYOK row to the hosted protocol without changing the saved global mode.
    pub fn resolve_hosted_role(&mut self, audio: bool) {
        if self.provider_mode == ProviderMode::Byok && Self::hosted_endpoint(&self.api_base_url) {
            self.provider_mode = ProviderMode::Palepale;
            self.palepale.chat_model = self.chat_model.clone();
            if audio {
                self.palepale.transcription_model = self.transcription_model.clone();
            }
        }
    }
    pub fn provider_base_url(&self) -> &str {
        match self.provider_mode {
            ProviderMode::Byok => &self.api_base_url,
            ProviderMode::Palepale => crate::provider::PALEPALE_BASE_URL,
        }
    }

    pub fn provider_chat_model(&self) -> &str {
        match self.provider_mode {
            ProviderMode::Byok => &self.chat_model,
            ProviderMode::Palepale => &self.palepale.chat_model,
        }
    }

    pub fn is_palepale(&self) -> bool {
        self.provider_mode == ProviderMode::Palepale
    }

    pub fn provider_transcription_model(&self) -> &str {
        match self.provider_mode {
            ProviderMode::Byok => &self.transcription_model,
            ProviderMode::Palepale => &self.palepale.transcription_model,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantMode {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub sections: Vec<ModeSection>,
    pub files: Vec<ModeFile>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeSection {
    pub id: String,
    pub title: String,
    pub prompt: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeFile {
    pub id: String,
    pub name: String,
    pub size: u64,
}

pub fn default_shortcuts() -> BTreeMap<String, String> {
    [
        ("toggleVisibility", "Ctrl+\\"),
        ("ask", "Ctrl+Enter"),
        ("clear", "Ctrl+R"),
        ("toggleSession", "Ctrl+Shift+\\"),
        ("moveUp", "Ctrl+Up"),
        ("moveDown", "Ctrl+Down"),
        ("moveLeft", "Ctrl+Left"),
        ("moveRight", "Ctrl+Right"),
        ("scrollUp", "Ctrl+Shift+Up"),
        ("scrollDown", "Ctrl+Shift+Down"),
        ("settings", crate::shortcuts::SETTINGS_ACCELERATOR),
    ]
    .into_iter()
    .map(|(action, key)| (action.into(), key.into()))
    .collect()
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigView {
    pub settings: Settings,
    pub has_api_key: bool,
    pub has_byok_key: bool,
    pub has_audio_key: bool,
    pub has_image_key: bool,
    pub has_palepale_key: bool,
    pub platform: String,
    pub shortcuts: Vec<ShortcutInfo>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Serialize)]
pub struct ShortcutInfo {
    pub action: String,
    pub accelerator: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptEntry {
    pub id: String,
    pub source: String,
    pub text: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub role: String,
    pub content: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub session_id: Option<String>,
    pub active: bool,
    pub transcript: Vec<TranscriptEntry>,
    pub messages: Vec<Message>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub ended_at: Option<u64>,
    pub state: String,
    pub summary: String,
    pub summary_error: Option<String>,
    pub mode_name: Option<String>,
    pub mode_sections: Vec<ModeSection>,
    pub transcript: Vec<TranscriptEntry>,
    pub messages: Vec<Message>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListEntry {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub ended_at: Option<u64>,
    pub state: String,
    pub mode_name: Option<String>,
    pub transcript_count: u64,
    pub message_count: u64,
}

#[derive(Deserialize)]
pub struct AssistRequest {
    pub id: String,
    pub prompt: String,
}

#[derive(Clone, Serialize)]
pub struct AssistantEvent {
    pub id: String,
    pub kind: String,
    pub text: String,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
