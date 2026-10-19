//! Provider catalogue metadata shared by transport and native settings.

use serde::{Deserialize, Serialize};

pub const PALEPALE_BASE_URL: &str = "https://api.palepale.tech/v1";
pub const REASONING_EFFORTS: [&str; 7] =
    ["none", "minimal", "low", "medium", "high", "xhigh", "max"];

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct ModelCapabilities {
    pub streaming: bool,
    pub reasoning: bool,
    pub tool_calling: bool,
    pub temperature: bool,
    pub image_input: bool,
    pub audio_input: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProviderModel {
    pub id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub capabilities: ModelCapabilities,
    #[serde(default)]
    pub reasoning_efforts: Vec<String>,
    #[serde(default)]
    pub default_reasoning_effort: Option<String>,
}

impl ProviderModel {
    pub fn supports_reasoning_effort(&self, effort: &str) -> bool {
        self.capabilities.reasoning
            && self
                .reasoning_efforts
                .iter()
                .any(|published| published == effort)
    }
}
