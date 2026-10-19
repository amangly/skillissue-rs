//! Smart controls reasoning on the selected Text or Images connection.
use crate::models::Settings;
use serde_json::{Value, json};

pub(super) fn apply(settings: &Settings, body: &mut Value) -> Result<(), String> {
    if !settings.smart_mode {
        return Ok(()); // Keep the provider default (or the explicit hosted setting).
    }
    let effort = settings.smart_effort.as_str();
    if !matches!(effort, "low" | "high") {
        return Err("Choose Low or High Smart effort.".into());
    }
    if settings.is_palepale() {
        // The engine checks this against the hosted model catalogue before sending.
        body["reasoning_effort"] = json!(effort);
        return Ok(());
    }
    let model = settings.chat_model.trim();
    let provider = skillissue_domain::byok::byok_provider(settings.provider_base_url())
        .map(|provider| provider.id);
    match provider {
        Some("openai")
            if ["o1", "o3", "o4-mini", "gpt-5", "gpt-6"]
                .iter()
                .any(|prefix| {
                    model == *prefix
                        || model.starts_with(&format!("{prefix}-"))
                        || model.starts_with(&format!("{prefix}."))
                })
                && !model.contains("chat")
                && !model.contains("pro")
                && !model.contains("preview") =>
        {
            body["reasoning_effort"] = json!(effort);
        }
        Some("gemini")
            if ["gemini-2.5-", "gemini-3", "gemini-4"]
                .iter()
                .any(|prefix| model.starts_with(prefix)) =>
        {
            body["reasoning_effort"] = json!(effort);
        }
        Some("deepseek")
            if matches!(
                model,
                "deepseek-flash" | "deepseek-v4-flash" | "deepseek-v4-pro"
            ) =>
        {
            body["thinking"] = json!({"type":"enabled"});
            body["reasoning_effort"] = json!(effort);
        }
        Some("anthropic")
            if [
                "claude-sonnet-4-6",
                "claude-sonnet-4-5",
                "claude-opus-4-6",
                "claude-opus-4-5",
            ]
            .iter()
            .any(|prefix| model == *prefix || model.starts_with(&format!("{prefix}-"))) =>
        {
            // Claude's compatibility layer ignores reasoning_effort. Use its documented
            // thinking budget instead; leave room for the final answer.
            body["thinking"] = json!({"type":"enabled", "budget_tokens": if effort == "low" { 2048 } else { 8192 }});
            body["max_tokens"] = json!(16384);
        }
        Some("openrouter") => {
            body["reasoning"] = json!({"effort":effort, "exclude":true});
            body["provider"] = json!({"require_parameters":true});
        }
        None => {
            // Custom OpenAI-compatible servers validate this explicit user request.
            body["reasoning_effort"] = json!(effort);
        }
        _ => {
            return Err(format!(
                "Smart effort is not supported for {model} on this connection. Choose a reasoning model or turn Smart off."
            ));
        }
    }
    Ok(())
}
