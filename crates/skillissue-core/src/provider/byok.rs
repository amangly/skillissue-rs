//! Explicit read-only BYOK access checks. Model listing does not prove inference support.
use super::http::{authenticate, bounded_body, check_status, endpoint, network_error};
use crate::models::Settings;
use tokio_util::sync::CancellationToken;

/// Explicit user-triggered sample request; never uses captured context.
pub async fn test_byok(
    client: &reqwest::Client,
    settings: &Settings,
    key: &str,
    audio: bool,
    image: bool,
    cancel: CancellationToken,
) -> Result<(), String> {
    let request = async {
        if audio {
            let mut cursor = std::io::Cursor::new(Vec::new());
            {
                let spec = hound::WavSpec {
                    channels: 1,
                    sample_rate: 16000,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                };
                let mut wav = hound::WavWriter::new(&mut cursor, spec)
                    .map_err(|_| "Could not prepare test audio.")?;
                for _ in 0..8000 {
                    wav.write_sample(0i16)
                        .map_err(|_| "Could not prepare test audio.")?;
                }
                wav.finalize()
                    .map_err(|_| "Could not prepare test audio.")?;
            }
            super::transcribe(client, settings, key, cursor.into_inner()).await?;
            return Ok(());
        }
        let screenshot = if image {
            use base64::Engine;
            let mut png = std::io::Cursor::new(Vec::new());
            image::DynamicImage::new_rgb8(32, 32)
                .write_to(&mut png, image::ImageFormat::Png)
                .map_err(|_| "Could not prepare test image.")?;
            Some(format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png.into_inner())
            ))
        } else {
            None
        };
        let mut sample = settings.clone();
        sample.system_prompt.clear();
        sample.active_mode_id = "general".into();
        sample.smart_mode = false;
        if sample.is_palepale() {
            super::stream_assist(
                client,
                &sample,
                key,
                "Reply with OK.",
                "",
                &[],
                screenshot.as_deref(),
                cancel.clone(),
                |_| {},
            )
            .await?;
            return Ok(());
        }
        let mut body =
            super::chat::assist_body(&sample, "Reply with OK.", "", &[], screenshot.as_deref())?;
        body["stream"] = false.into();
        let budget_field = if skillissue_domain::byok::byok_provider(sample.provider_base_url())
            .is_some_and(|p| p.id == "openai")
        {
            "max_completion_tokens"
        } else {
            "max_tokens"
        };
        body[budget_field] = 32.into();
        let url = endpoint(&sample, key, "chat/completions")?;
        let response = authenticate(client.post(url), key)
            .json(&body)
            .send()
            .await
            .map_err(network_error)?;
        let bytes = bounded_body(check_status(response)?, 64 * 1024, &cancel).await?;
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| "The provider returned invalid JSON.")?;
        if let Some(error) = super::http::provider_error(&value, key) {
            return Err(error);
        }
        if !value.pointer("/choices/0/message").is_some_and(|m| {
            m.is_object()
                && (m.get("content").is_some_and(|c| c.is_string())
                    || m.get("reasoning_content").is_some_and(|c| c.is_string()))
        }) {
            return Err("The provider did not return a chat response.".into());
        }
        Ok(())
    };
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err("Test cancelled.".into()),
        result = tokio::time::timeout(std::time::Duration::from_secs(30), request) => result.map_err(|_| "Test timed out. Try again.".to_string())?,
    }
}

pub async fn byok_models(
    client: &reqwest::Client,
    settings: &Settings,
    key: &str,
    cancel: CancellationToken,
) -> Result<Vec<String>, String> {
    model_list(client, settings, key, None, cancel).await
}

pub async fn byok_role_models(
    client: &reqwest::Client,
    settings: &Settings,
    key: &str,
    role: u8,
    cancel: CancellationToken,
) -> Result<Vec<String>, String> {
    if settings.is_palepale() || Settings::hosted_endpoint(settings.provider_base_url()) {
        return Ok(super::palepale_models(client, cancel)
            .await?
            .into_iter()
            .filter(|model| {
                model.capabilities.streaming
                    && match role {
                        1 => model.capabilities.audio_input,
                        2 => model.capabilities.image_input,
                        _ => true,
                    }
            })
            .map(|model| model.id)
            .collect());
    }
    model_list(client, settings, key, Some(role), cancel).await
}

async fn model_list(
    client: &reqwest::Client,
    settings: &Settings,
    key: &str,
    role: Option<u8>,
    cancel: CancellationToken,
) -> Result<Vec<String>, String> {
    let mut url = endpoint(settings, key, "models")?;
    if role == Some(1)
        && skillissue_domain::byok::byok_provider(settings.provider_base_url())
            .is_some_and(|p| p.id == "openrouter")
    {
        url.push_str("?output_modalities=transcription");
    }
    let mut request = authenticate(client.get(url), key);
    if skillissue_domain::byok::byok_provider(settings.provider_base_url())
        .is_some_and(|p| p.id == "anthropic")
    {
        // Model discovery uses Anthropic's native Models endpoint, unlike compatible chat.
        request = request
            .header("x-api-key", key.trim())
            .header("anthropic-version", "2023-06-01");
    }
    let request = request.timeout(std::time::Duration::from_secs(15)).send();
    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err("Connection check cancelled.".into()),
        response = request => response.map_err(network_error)?,
    };
    if matches!(response.status().as_u16(), 404 | 405 | 501) {
        return Err("Model listing is unavailable at this URL. You can enter model IDs manually; credentials and inference remain unverified.".into());
    }
    let body = bounded_body(check_status(response)?, 1024 * 1024, &cancel).await?;
    let value: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|_| "The model endpoint returned invalid JSON.".to_string())?;
    let data = value
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or("The model endpoint did not return an OpenAI-compatible model list.")?;
    if data.len() > 4096 {
        return Err("The provider returned too many models.".into());
    }
    let mut models: Vec<String> = data
        .iter()
        .filter(|model| {
            role.is_none_or(|role| supports_role(settings.provider_base_url(), model, role))
        })
        .filter_map(|m| m.get("id").and_then(|id| id.as_str()))
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 200
                && !id.chars().any(char::is_control)
                && (key.trim().is_empty() || !id.contains(key.trim()))
        })
        .map(str::to_owned)
        .collect();
    models.sort();
    models.dedup();
    Ok(models)
}

/// Unknown metadata is not evidence of support. Exact reviewed presets are a fallback.
pub(super) fn supports_role(base: &str, model: &serde_json::Value, role: u8) -> bool {
    use skillissue_domain::byok::{byok_provider, starter_image_model, starter_model};
    let provider = byok_provider(base);
    let id = model.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let inputs = model
        .get("input_modalities")
        .or_else(|| model.pointer("/architecture/input_modalities"))
        .and_then(|v| v.as_array());
    let outputs = model
        .get("output_modalities")
        .or_else(|| model.pointer("/architecture/output_modalities"))
        .and_then(|v| v.as_array());
    let contains =
        |values: &[serde_json::Value], name: &str| values.iter().any(|v| v.as_str() == Some(name));
    let capability = |name: &str| {
        model
            .get("capabilities")
            .and_then(|v| v.get(name))
            .and_then(|v| v.as_bool())
    };
    let known = provider.is_some_and(|p| match role {
        0 => starter_model(p.id) == Some(id) || starter_image_model(p.id) == Some(id),
        1 => p.transcription_model == Some(id),
        2 => starter_image_model(p.id) == Some(id),
        _ => false,
    });
    if role == 1 {
        // Audio chat and multipart transcription are different transports.
        if provider.is_some_and(|p| p.id == "gemini") {
            if let (Some(inputs), Some(outputs)) = (inputs, outputs) {
                return contains(inputs, "audio") && contains(outputs, "text");
            }
            return known;
        }
        if let Some(supported) = capability("audio_transcription") {
            return supported;
        }
        if let Some(outputs) = outputs {
            // OpenRouter advertises its dedicated transcription endpoint this way.
            return contains(outputs, "transcription");
        }
        return known;
    }
    if role > 2 {
        return false;
    }
    if capability("completion_chat") == Some(false) {
        return false;
    }
    if let Some(outputs) = outputs
        && !contains(outputs, "text")
    {
        return false;
    }
    if role == 2
        && let Some(vision) = capability("vision")
    {
        return vision && (capability("completion_chat") == Some(true) || known);
    }
    if let (Some(inputs), Some(outputs)) = (inputs, outputs) {
        return contains(inputs, if role == 2 { "image" } else { "text" })
            && contains(outputs, "text");
    }
    if role == 0 && capability("completion_chat") == Some(true) {
        return true;
    }
    if let Some(inputs) = inputs
        && !contains(inputs, if role == 2 { "image" } else { "text" })
    {
        return false;
    }
    known
}
