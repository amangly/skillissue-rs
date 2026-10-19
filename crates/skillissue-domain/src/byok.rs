//! Reviewed endpoint presets, not claims of account-specific or model-specific capability.
#[derive(Clone, Copy, Debug)]
pub struct ByokProvider {
    pub id: &'static str,
    pub name: &'static str,
    pub base_url: &'static str,
    pub setup_url: &'static str,
    pub docs_url: &'static str,
    pub guidance: &'static str,
    pub transcription_model: Option<&'static str>,
}
pub const BYOK_PROVIDERS: &[ByokProvider] = &[
    ByokProvider {
        id: "openai",
        name: "OpenAI",
        base_url: "https://api.openai.com/v1",
        setup_url: "https://platform.openai.com/api-keys",
        docs_url: "https://developers.openai.com/api/reference/resources/chat/subresources/completions",
        guidance: "Compatible chat; select a chat model. Screenshots need a vision model. Smart needs a supported reasoning model.",
        transcription_model: Some("gpt-4o-mini-transcribe"),
    },
    ByokProvider {
        id: "anthropic",
        name: "Anthropic (Claude)",
        base_url: "https://api.anthropic.com/v1",
        setup_url: "https://console.anthropic.com/settings/keys",
        docs_url: "https://platform.claude.com/docs/en/cli-sdks-libraries/libraries/openai-sdk",
        guidance: "Official chat compatibility layer. Images depend on model. Use a workspace-scoped API key; multi-workspace headers are not configured here. Smart uses thinking budgets on supported models. Native caching is not integrated.",
        transcription_model: None,
    },
    ByokProvider {
        id: "gemini",
        name: "Google Gemini",
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        setup_url: "https://aistudio.google.com/apikey",
        docs_url: "https://ai.google.dev/gemini-api/docs/openai",
        guidance: "Gemini Flash supports text, screenshots and audio transcription. Gemini grounding and native tools are not integrated.",
        transcription_model: Some("gemini-3.8-flash"),
    },
    ByokProvider {
        id: "deepseek",
        name: "DeepSeek",
        base_url: "https://api.deepseek.com",
        setup_url: "https://platform.deepseek.com/api_keys",
        docs_url: "https://api-docs.deepseek.com/",
        guidance: "DeepSeek Flash supports text and screenshots. Audio needs a separate provider. DeepSeek search tools are not integrated.",
        transcription_model: None,
    },
    ByokProvider {
        id: "groq",
        name: "Groq",
        base_url: "https://api.groq.com/openai/v1",
        setup_url: "https://console.groq.com/keys",
        docs_url: "https://console.groq.com/docs/openai",
        guidance: "Compatible chat and separate speech models. Choose a vision model for screenshots. Groq built-in tools are not integrated.",
        transcription_model: Some("whisper-large-v3-turbo"),
    },
    ByokProvider {
        id: "xai",
        name: "xAI (Grok)",
        base_url: "https://api.x.ai/v1",
        setup_url: "https://console.x.ai/",
        docs_url: "https://docs.x.ai/developers/model-capabilities/text/generate-text",
        guidance: "Compatible chat on supported models. Choose an image-capable model for screenshots. Native xAI search and tools are not integrated.",
        transcription_model: None,
    },
    ByokProvider {
        id: "cerebras",
        name: "Cerebras",
        base_url: "https://api.cerebras.ai/v1",
        setup_url: "https://cloud.cerebras.ai/",
        docs_url: "https://inference-docs.cerebras.ai/resources/openai",
        guidance: "Compatible chat. Use a shared model ID or your dedicated endpoint ID. Images depend on model and account access; native tools are not integrated.",
        transcription_model: None,
    },
    ByokProvider {
        id: "mistral",
        name: "Mistral",
        base_url: "https://api.mistral.ai/v1",
        setup_url: "https://console.mistral.ai/",
        docs_url: "https://docs.mistral.ai/api",
        guidance: "Mistral Small supports screenshots; Voxtral transcribes audio with the same key. Diarization and native agents are not integrated.",
        transcription_model: Some("voxtral-mini-latest"),
    },
    ByokProvider {
        id: "openrouter",
        name: "OpenRouter",
        base_url: "https://openrouter.ai/api/v1",
        setup_url: "https://openrouter.ai/settings/keys",
        docs_url: "https://openrouter.ai/docs/quickstart",
        guidance: "Use an OpenRouter key and full provider/model ID. Media and access depend on the routed model. A public model list does not verify your key or credit.",
        transcription_model: Some("openai/whisper-large-v3"),
    },
    ByokProvider {
        id: "ollama",
        name: "Ollama local",
        base_url: "http://localhost:11434/v1",
        setup_url: "https://docs.ollama.com/quickstart",
        docs_url: "https://docs.ollama.com/api/openai-compatibility",
        guidance: "Start Ollama and install a model first. Local keys are optional. Images require a vision model. Use a separate transcription service.",
        transcription_model: None,
    },
    ByokProvider {
        id: "lmstudio",
        name: "LM Studio local",
        base_url: "http://localhost:1234/v1",
        setup_url: "https://lmstudio.ai/docs/developer",
        docs_url: "https://lmstudio.ai/docs/developer/openai-compat",
        guidance: "Enable the local server and load a model first. A key may be required if server authentication is enabled. Images depend on the loaded model.",
        transcription_model: None,
    },
    ByokProvider {
        id: "mlx",
        name: "MLX server",
        base_url: "http://localhost:8080/v1",
        setup_url: "https://github.com/ml-explore/mlx-lm",
        docs_url: "https://github.com/ml-explore/mlx-lm/blob/main/mlx_lm/SERVER.md",
        guidance: "MLX runs on Apple silicon, not Windows. Use an explicitly configured HTTPS server or a localhost tunnel; install and serve the model on the Mac.",
        transcription_model: None,
    },
];
/// Exact normalized endpoint matching avoids granting provider behavior to lookalike hosts.
pub fn byok_provider(base: &str) -> Option<&'static ByokProvider> {
    let base = base.trim().trim_end_matches('/');
    BYOK_PROVIDERS
        .iter()
        .find(|provider| provider.base_url == base)
}

/// Starter models can be overridden in Advanced; availability depends on the account.
pub fn starter_model(id: &str) -> Option<&'static str> {
    match id {
        "openai" => Some("gpt-4.1-mini"),
        "anthropic" => Some("claude-sonnet-4-6"),
        "gemini" => Some("gemini-3.8-flash"),
        "deepseek" => Some("deepseek-flash"),
        "groq" => Some("llama-3.3-70b-versatile"),
        "cerebras" => Some("qwen-3.8-27b"),
        "mistral" => Some("mistral-small-latest"),
        "openrouter" => Some("openai/gpt-4.1-mini"),
        _ => None,
    }
}

/// Reviewed image-capable starter models, separate from text-only defaults.
pub fn starter_image_model(id: &str) -> Option<&'static str> {
    match id {
        "openai" | "anthropic" | "gemini" | "deepseek" | "mistral" | "openrouter" => {
            starter_model(id)
        }
        "groq" => Some("qwen/qwen3.8-27b"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_matching_does_not_recognize_lookalikes_or_unrelated_paths() {
        assert_eq!(
            byok_provider(" https://api.anthropic.com/v1/ ").unwrap().id,
            "anthropic"
        );
        assert!(byok_provider("https://api.anthropic.com.evil.example/v1").is_none());
        assert!(byok_provider("https://api.anthropic.com/v1/proxy").is_none());
        let ids: std::collections::BTreeSet<_> = BYOK_PROVIDERS.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), 12);
    }
}
