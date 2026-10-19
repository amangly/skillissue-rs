//! Endpoint validation, authentication, bounded bodies and redacted errors.

use crate::models::Settings;
use reqwest::{RequestBuilder, Response, Url};
use serde_json::Value;
use std::net::IpAddr;
use tokio_util::sync::CancellationToken;

pub fn validate_base_url(input: &str) -> Result<String, String> {
    let input = input.trim();
    if input.chars().any(char::is_control) {
        return Err("The provider URL contains control characters.".into());
    }
    let url = Url::parse(input).map_err(|_| "Enter a complete provider URL.".to_string())?;
    let authority = input
        .split_once("://")
        .map(|(_, tail)| tail.split(['/', '?', '#']).next().unwrap_or_default())
        .unwrap_or_default();
    if !url.username().is_empty() || url.password().is_some() || authority.contains('@') {
        return Err("Provider URLs cannot contain credentials.".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("Provider URLs cannot contain a query or fragment.".into());
    }
    if url.host_str().is_none() || url.cannot_be_a_base() {
        return Err("The provider URL must have a host.".into());
    }
    let path = url.path().trim_end_matches('/');
    if [
        "/chat/completions",
        "/audio/transcriptions",
        "/responses",
        "/models",
    ]
    .iter()
    .any(|suffix| path.ends_with(suffix))
    {
        return Err("Enter the API base URL (for example https://api.openai.com/v1), without /chat/completions, /audio/transcriptions, /responses or /models.".into());
    }
    match url.scheme() {
        "https" => (),
        "http" if is_loopback(&url) => (),
        "http" => {
            return Err("Use HTTPS for remote providers; HTTP is allowed only on loopback.".into());
        }
        _ => return Err("Provider URLs must use HTTPS or loopback HTTP.".into()),
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

pub fn is_loopback(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    host.eq_ignore_ascii_case("localhost")
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

pub(super) fn endpoint(settings: &Settings, api_key: &str, path: &str) -> Result<String, String> {
    let base = validate_base_url(settings.provider_base_url())?;
    let url = Url::parse(&base).map_err(|_| "Invalid provider URL.".to_string())?;
    if api_key.trim().is_empty() && !is_loopback(&url) {
        return Err("Add an API key in Settings before contacting a remote provider.".into());
    }
    if api_key.contains(['\r', '\n']) || api_key.len() > 16_384 {
        return Err("The API key has an invalid format.".into());
    }
    Ok(format!("{base}/{path}"))
}

pub(super) fn authenticate(request: RequestBuilder, api_key: &str) -> RequestBuilder {
    if api_key.trim().is_empty() {
        request
    } else {
        request.bearer_auth(api_key.trim())
    }
}

pub(super) fn network_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "The provider request timed out.".into()
    } else if error.is_connect() {
        "Could not connect to the provider. Check the URL and network connection.".into()
    } else {
        format!("Provider transport failed: {}", error.without_url())
    }
}

pub(super) fn check_status(response: Response) -> Result<Response, String> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let help = match status.as_u16() {
        400 | 422 => "Check the model name and provider compatibility.",
        401 | 403 => "Check the API key and its permissions.",
        402 => "Check your account credit, spending limit, balance or plan before retrying.",
        404 => "Check the API base URL and model name.",
        413 => "The provider rejected the request size.",
        429 => "Check the provider's quota and rate limits before retrying.",
        500..=599 => "The provider is unavailable. Try again later.",
        300..=399 => "Configure the final API URL; redirects are not followed.",
        _ => "Check the provider configuration.",
    };
    // Error bodies can echo Authorization headers or submitted context.
    Err(format!(
        "Provider returned HTTP {}. {help}",
        status.as_u16()
    ))
}

pub(super) async fn bounded_body(
    mut response: Response,
    limit: usize,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    loop {
        let next = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err("Request cancelled.".into()),
            next = response.chunk() => next.map_err(network_error)?,
        };
        let Some(chunk) = next else { return Ok(bytes) };
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err("The provider response exceeded the size limit.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
}
pub(super) fn provider_error(value: &Value, api_key: &str) -> Option<String> {
    let error = value.get("error")?;
    let detail = error
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| error.as_str())
        .unwrap_or("The provider reported an error.");
    Some(safe_error(detail, api_key))
}

pub(super) fn safe_error(detail: &str, api_key: &str) -> String {
    let text = if api_key.trim().is_empty() {
        detail.to_string()
    } else {
        detail.replace(api_key.trim(), "[redacted]")
    };
    let text: String = text
        .chars()
        .filter(|c| !c.is_control() || c.is_whitespace())
        .take(512)
        .collect();
    format!(
        "Provider error: {}",
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    )
}
