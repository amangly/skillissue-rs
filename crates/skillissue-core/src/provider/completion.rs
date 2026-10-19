//! Chat JSON and event completion decoding with explicit finish statuses.

use super::{
    MAX_RESPONSE_BYTES,
    http::{provider_error, safe_error},
    sse::SseEvent,
};
use serde_json::Value;

pub(super) fn finish_reason(reason: &str) -> Result<(), String> {
    match reason {
        "stop" => Ok(()),
        "length" => Err("The provider reached its output limit. The answer is incomplete.".into()),
        "content_filter" => {
            Err("The provider stopped the answer because of its content filter.".into())
        }
        "tool_calls" | "function_call" => Err(
            "The provider requested a tool call, but this app has no model tools configured."
                .into(),
        ),
        _ => Err("The provider returned an unsupported completion status.".into()),
    }
}

fn content_text(content: &Value) -> Result<String, String> {
    match content {
        Value::Null => Ok(String::new()),
        Value::String(text) => Ok(text.clone()),
        Value::Array(parts) => {
            let mut text = String::new();
            for part in parts {
                if matches!(
                    part.get("type").and_then(Value::as_str),
                    Some("text" | "output_text")
                ) && let Some(value) = part.get("text").and_then(Value::as_str)
                {
                    text.push_str(value);
                }
            }
            Ok(text)
        }
        _ => Err("The provider returned an unsupported text format.".into()),
    }
}

pub(super) fn complete_json<F: Fn(&str)>(
    bytes: &[u8],
    api_key: &str,
    on_delta: &F,
    require_usage: bool,
) -> Result<String, String> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| "The provider returned invalid JSON.".to_string())?;
    if let Some(error) = provider_error(&value, api_key) {
        return Err(error);
    }
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .ok_or_else(|| "The provider response did not contain an answer.".to_string())?;
    let message = choice
        .get("message")
        .ok_or_else(|| "The provider response did not contain a message.".to_string())?;
    let mut text = content_text(message.get("content").unwrap_or(&Value::Null))?;
    if text.is_empty() {
        text = content_text(message.get("refusal").unwrap_or(&Value::Null))?;
    }
    if text.len() > MAX_RESPONSE_BYTES {
        return Err("The answer exceeded the response size limit.".into());
    }
    if require_usage {
        let reason = choice
            .get("finish_reason")
            .and_then(Value::as_str)
            .ok_or("Palepale did not confirm completion. The answer may be incomplete.")?;
        finish_reason(reason)?;
        validate_usage(value.get("usage"))?;
    }
    if !text.is_empty() {
        on_delta(&text);
    }
    if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
        finish_reason(reason)?;
    }
    if text.is_empty() {
        return Err("The provider returned an empty answer.".into());
    }
    Ok(text)
}

#[derive(Default)]
pub(super) struct Completion {
    text: String,
    pub(super) finished: bool,
    pub(super) usage: Option<Value>,
    require_usage: bool,
    done: bool,
}

impl Completion {
    pub(super) fn with_usage_requirement(require_usage: bool) -> Self {
        Self {
            require_usage,
            ..Self::default()
        }
    }

    pub(super) fn event<F: Fn(&str)>(
        &mut self,
        event: SseEvent,
        api_key: &str,
        on_delta: &F,
    ) -> Result<bool, String> {
        let data = std::str::from_utf8(&event.data)
            .map_err(|_| "The provider stream contained invalid UTF-8.".to_string())?
            .trim();
        if data.is_empty() {
            return Ok(false);
        }
        if data == "[DONE]" {
            self.done = true;
            return Ok(true);
        }
        let value: Value = serde_json::from_str(data)
            .map_err(|_| "The provider stream contained invalid JSON.".to_string())?;
        if let Some(error) = provider_error(&value, api_key) {
            return Err(error);
        }
        if event.kind.as_deref() == Some("error") {
            return Err(safe_error(
                value
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("The stream failed."),
                api_key,
            ));
        }
        let choice = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| {
                choices
                    .iter()
                    .find(|choice| choice.get("index").and_then(Value::as_u64).unwrap_or(0) == 0)
            });
        if let Some(delta) = choice.and_then(|choice| choice.get("delta")) {
            let mut text = content_text(delta.get("content").unwrap_or(&Value::Null))?;
            if text.is_empty() {
                text = content_text(delta.get("refusal").unwrap_or(&Value::Null))?;
            }
            if self.text.len().saturating_add(text.len()) > MAX_RESPONSE_BYTES {
                return Err("The answer exceeded the response size limit.".into());
            }
            if !text.is_empty() {
                if self.finished {
                    return Err("The provider sent answer text after its completion status.".into());
                }
                self.text.push_str(&text);
                on_delta(&text);
            }
        }
        if let Some(reason) = choice
            .and_then(|choice| choice.get("finish_reason"))
            .and_then(Value::as_str)
        {
            finish_reason(reason)?;
            self.finished = true;
        }
        if let Some(usage) = value.get("usage").filter(|usage| !usage.is_null()) {
            if self.require_usage {
                validate_usage(Some(usage))?;
            }
            // The gateway may observe preliminary supplier counts. Only counters at
            // or after the successful finish can confirm a hosted response.
            if usage.is_object() && (!self.require_usage || self.finished) {
                self.usage = Some(usage.clone());
            }
        }
        Ok(false)
    }

    pub(super) fn finish(self) -> Result<String, String> {
        if !self.finished {
            Err("The provider stream ended before completion. The answer may be incomplete.".into())
        } else if self.require_usage && !self.done {
            Err("Palepale stream ended before its final confirmation. Account usage may require reconciliation.".into())
        } else if self.require_usage && validate_usage(self.usage.as_ref()).is_err() {
            Err("Palepale did not confirm final usage. The response is incomplete; account usage may require reconciliation.".into())
        } else if self.text.is_empty() {
            Err("The provider returned an empty answer.".into())
        } else {
            Ok(self.text)
        }
    }
}

fn validate_usage(usage: Option<&Value>) -> Result<(), String> {
    let invalid = || {
        "Palepale did not provide valid final usage. Account usage may require reconciliation."
            .to_string()
    };
    let usage = usage.and_then(Value::as_object).ok_or_else(invalid)?;
    let prompt = usage
        .get("prompt_tokens")
        .and_then(Value::as_u64)
        .ok_or_else(invalid)?;
    let completion = usage
        .get("completion_tokens")
        .and_then(Value::as_u64)
        .ok_or_else(invalid)?;
    let total = prompt.checked_add(completion).ok_or_else(invalid)?;
    if let Some(supplied) = usage.get("total_tokens")
        && supplied.as_u64() != Some(total)
    {
        return Err(invalid());
    }
    Ok(())
}
