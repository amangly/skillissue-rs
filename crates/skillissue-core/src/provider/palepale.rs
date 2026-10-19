//! Public Palepale catalogue discovery without inference or account credentials.

use super::http::{bounded_body, check_status, network_error};
use crate::models::Settings;
use reqwest::Client;
use serde_json::Value;
use skillissue_domain::provider::{PALEPALE_BASE_URL, ProviderModel, REASONING_EFFORTS};
use std::{
    collections::HashSet,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

const MAX_CATALOG_BYTES: usize = 512 * 1024;
const MAX_CATALOG_MODELS: usize = 256;
const MEDIA_CACHE_TTL: Duration = Duration::from_secs(30);

type MediaCache = Mutex<Option<(Instant, Vec<ProviderModel>)>>;
static MEDIA_CACHE: OnceLock<MediaCache> = OnceLock::new();

pub async fn palepale_models(
    client: &Client,
    cancel: CancellationToken,
) -> Result<Vec<ProviderModel>, String> {
    let result = fetch_models(
        client,
        &format!("{PALEPALE_BASE_URL}/models?app=skillissue-rs"),
        cancel,
    )
    .await;
    if let Ok(mut cache) = MEDIA_CACHE.get_or_init(|| Mutex::new(None)).lock() {
        *cache = result
            .as_ref()
            .ok()
            .map(|models| (Instant::now(), models.clone()));
    }
    result
}

pub(super) async fn fetch_models(
    client: &Client,
    url: &str,
    cancel: CancellationToken,
) -> Result<Vec<ProviderModel>, String> {
    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err("Request cancelled.".into()),
        response = client.get(url).header(reqwest::header::ACCEPT, "application/json").send() => {
            check_status(response.map_err(network_error)?)?
        },
    };
    let bytes = bounded_body(response, MAX_CATALOG_BYTES, &cancel).await?;
    parse_models(&bytes)
}

pub(super) fn parse_models(bytes: &[u8]) -> Result<Vec<ProviderModel>, String> {
    if bytes.len() > MAX_CATALOG_BYTES {
        return Err("The Palepale model catalogue exceeded the size limit.".into());
    }
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| "Palepale returned an invalid model catalogue.".to_string())?;
    let entries = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or("Palepale returned an invalid model catalogue.")?;
    if entries.len() > MAX_CATALOG_MODELS {
        return Err("The Palepale model catalogue contains too many entries.".into());
    }
    let mut ids = HashSet::new();
    let mut models = Vec::new();
    for entry in entries {
        if entry.get("published").and_then(Value::as_bool) == Some(false)
            || entry.get("available").and_then(Value::as_bool) == Some(false)
        {
            continue;
        }
        let mut model: ProviderModel = serde_json::from_value(entry.clone())
            .map_err(|_| "Palepale returned invalid model metadata.".to_string())?;
        if model.id.is_empty()
            || model.id.len() > 128
            || !model
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"/._:-".contains(&byte))
            || !model.capabilities.streaming
        {
            continue;
        }
        if !ids.insert(model.id.clone()) {
            return Err("The Palepale model catalogue contains duplicate model IDs.".into());
        }
        let mut efforts = HashSet::new();
        if model.reasoning_efforts.len() > REASONING_EFFORTS.len()
            || model.reasoning_efforts.iter().any(|effort| {
                !REASONING_EFFORTS.contains(&effort.as_str()) || !efforts.insert(effort)
            })
            || (!model.capabilities.reasoning
                && (!model.reasoning_efforts.is_empty()
                    || model.default_reasoning_effort.is_some()))
            || model
                .default_reasoning_effort
                .as_deref()
                .is_some_and(|effort| !model.supports_reasoning_effort(effort))
        {
            return Err(
                "Palepale returned invalid reasoning options. Refresh the model catalogue.".into(),
            );
        }
        model.display_name = model.display_name.trim().to_owned();
        if model.display_name.is_empty()
            || model.display_name.len() > 200
            || model.display_name.chars().any(char::is_control)
        {
            model.display_name.clone_from(&model.id);
        }
        models.push(model);
    }
    if models.is_empty() {
        return Err(
            "Palepale has no streaming chat models available. Refresh the model list or use BYOK."
                .into(),
        );
    }
    Ok(models)
}

pub(super) async fn verify_media_model(
    client: &Client,
    settings: &Settings,
    audio: bool,
    cancel: CancellationToken,
) -> Result<ProviderModel, String> {
    let cached = cached_media_models(MEDIA_CACHE.get_or_init(|| Mutex::new(None)));
    let models = match cached {
        Some(models) => models,
        None => palepale_models(client, cancel).await?,
    };
    require_media_model(
        &models,
        if audio {
            settings.provider_transcription_model()
        } else {
            settings.provider_chat_model()
        },
        audio,
    )
}

pub(super) fn cached_media_models(cache: &MediaCache) -> Option<Vec<ProviderModel>> {
    cache.lock().ok().and_then(|cache| {
        cache.as_ref().and_then(|(created, models)| {
            (created.elapsed() <= MEDIA_CACHE_TTL).then(|| models.clone())
        })
    })
}

pub(super) fn require_media_model(
    models: &[ProviderModel],
    model: &str,
    audio: bool,
) -> Result<ProviderModel, String> {
    if let Some(published) = models.iter().find(|entry| {
        entry.id == model
            && entry.capabilities.streaming
            && if audio {
                entry.capabilities.audio_input
            } else {
                entry.capabilities.image_input
            }
    }) {
        Ok(published.clone())
    } else {
        Err(format!(
            "The selected Palepale model does not currently publish {} input. Refresh the model catalogue before continuing.",
            if audio { "audio" } else { "image" }
        ))
    }
}
