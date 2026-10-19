//! Browser-approved Palepale device pairing.
//!
//! The native client creates a short-lived device code without credentials, opens the
//! public approval page with only the six-digit user code, and polls the private device
//! code until the server releases one revocable API key. Secrets are deliberately kept in
//! types without `Debug` or `Serialize` implementations.

use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use serde_json::Value;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

use skillissue_domain::provider::PALEPALE_BASE_URL;

const PALEPALE_CHAT_ORIGIN: &str = "https://palepale.tech";
const MAX_PAIRING_BODY_BYTES: usize = 32 * 1024;
const MIN_DEVICE_CODE_LEN: usize = 32;
const MAX_DEVICE_CODE_LEN: usize = 128;
const MAX_USER_CODE_LEN: usize = 6;
const MAX_TTL_SECONDS: u64 = 900;
const POLL_INTERVAL: Duration = Duration::from_secs(1);
const RETRY_INTERVAL: Duration = Duration::from_secs(2);
const POLL_RESPONSE_TIMEOUT: Duration = Duration::from_secs(20);

/// The native-only half of a pairing transaction.
///
/// This type intentionally has no `Debug`, `Serialize`, or `Deserialize` implementation:
/// the device code is a bearer secret and must never appear in logs or UI snapshots.
pub struct PairingStarted {
    device_code: String,
    user_code: String,
    expires_at: Instant,
}

impl PairingStarted {
    pub fn user_code(&self) -> &str {
        &self.user_code
    }

    pub fn ttl(&self) -> Duration {
        self.expires_at.saturating_duration_since(Instant::now())
    }

    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }

    pub fn browser_url(&self) -> String {
        pairing_browser_url(&self.user_code)
    }
}

/// A one-time API credential released by the pairing endpoint.
///
/// Keep the value private and move it into the credential store immediately. A caller can
/// consume it with [`PairingCredential::into_api_key`], but cannot accidentally format it.
pub struct PairingCredential(String);

impl PairingCredential {
    pub fn into_api_key(self) -> String {
        self.0
    }
}

pub enum PairingPoll {
    Pending,
    Ready(PairingCredential),
}

/// Hosted setup's opaque pairing handle. The device code remains private to this module.
pub type PalepalePairing = PairingStarted;

/// Starts a browser pairing transaction against the fixed Palepale API origin.
pub async fn start_pairing(
    client: &Client,
    cancel: CancellationToken,
) -> Result<PairingStarted, String> {
    start_at(client, api_origin(), cancel).await
}

/// Polls a started transaction once. `Pending` is expected until the browser approves the code.
pub async fn poll_pairing(
    client: &Client,
    started: &PairingStarted,
    cancel: CancellationToken,
) -> Result<PairingPoll, String> {
    poll_at(client, api_origin(), started, cancel).await
}

/// Opens no browser and performs no UI work; it only waits for an approval and returns the
/// one-time credential. The caller controls cancellation, including when the settings view
/// closes or the application shuts down.
pub async fn wait_for_pairing(
    client: &Client,
    started: &PairingStarted,
    cancel: CancellationToken,
) -> Result<PairingCredential, String> {
    wait_at(client, api_origin(), started, cancel).await
}

/// Waits for browser approval and moves the one-time credential out of its secret wrapper.
pub async fn wait_pairing(
    client: &Client,
    started: &PairingStarted,
    cancel: CancellationToken,
) -> Result<String, String> {
    wait_for_pairing(client, started, cancel)
        .await
        .map(PairingCredential::into_api_key)
}

/// Builds the official approval URL. Only the public six-digit code is placed in the URL;
/// the device code never leaves the native process.
pub fn pairing_browser_url(user_code: &str) -> String {
    let Ok(mut url) = Url::parse(PALEPALE_CHAT_ORIGIN) else {
        return format!("{PALEPALE_CHAT_ORIGIN}/pair");
    };
    url.set_path("/pair");
    url.query_pairs_mut().append_pair("code", user_code);
    url.to_string()
}

fn api_origin() -> Url {
    let mut url = Url::parse(PALEPALE_BASE_URL).expect("PALEPALE_BASE_URL is a valid URL");
    url.set_path("");
    url.set_query(None);
    url.set_fragment(None);
    url
}

async fn start_at(
    client: &Client,
    origin: Url,
    cancel: CancellationToken,
) -> Result<PairingStarted, String> {
    if cancel.is_cancelled() {
        return Err("Pairing cancelled.".into());
    }
    let url = origin
        .join("/api/pair/start")
        .map_err(|_| "Palepale pairing is unavailable.".to_string())?;
    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err("Pairing cancelled.".into()),
        response = client
            .post(url)
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&serde_json::json!({}))
            .send() => response.map_err(transport_error)?,
    };
    let status = response.status();
    let body = collect_body(response, &cancel).await?;
    if !status.is_success() {
        return Err(status_error(status));
    }
    parse_started(&body)
}

async fn poll_at(
    client: &Client,
    origin: Url,
    started: &PairingStarted,
    cancel: CancellationToken,
) -> Result<PairingPoll, String> {
    poll_inner(client, origin, started, cancel)
        .await
        .map_err(|failure| failure.message())
}

async fn wait_at(
    client: &Client,
    origin: Url,
    started: &PairingStarted,
    cancel: CancellationToken,
) -> Result<PairingCredential, String> {
    if started.is_expired() {
        return Err("The Palepale pairing code expired. Start pairing again.".into());
    }
    loop {
        match poll_inner(client, origin.clone(), started, cancel.clone()).await {
            Ok(PairingPoll::Ready(credential)) => return Ok(credential),
            Ok(PairingPoll::Pending) => sleep_or_cancel(POLL_INTERVAL, started, &cancel).await?,
            Err(PairingFailure::Retryable(_)) => {
                sleep_or_cancel(RETRY_INTERVAL, started, &cancel).await?
            }
            Err(error) => return Err(error.message()),
        }
    }
}

async fn poll_inner(
    client: &Client,
    origin: Url,
    started: &PairingStarted,
    cancel: CancellationToken,
) -> Result<PairingPoll, PairingFailure> {
    if cancel.is_cancelled() {
        return Err(PairingFailure::Cancelled);
    }
    if started.is_expired() {
        return Err(PairingFailure::Expired);
    }
    let path = format!("/api/pair/{}", started.device_code);
    let url = origin
        .join(&path)
        .map_err(|_| PairingFailure::Fatal("Palepale pairing is unavailable.".into()))?;
    // Polling can consume the only copy of an approved credential on the server.
    // Cancellation prevents another poll, but must not drop a dispatched reply.
    let (status, body) = tokio::time::timeout(POLL_RESPONSE_TIMEOUT, async {
        let response = client
            .get(url)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|error| PairingFailure::Retryable(transport_error(error)))?;
        let status = response.status();
        let body = collect_body(response, &CancellationToken::new())
            .await
            .map_err(PairingFailure::from_body_error)?;
        Ok::<_, PairingFailure>((status, body))
    })
    .await
    .map_err(|_| PairingFailure::Retryable("Palepale pairing response timed out.".into()))??;
    if status == StatusCode::ACCEPTED {
        return parse_pending(&body);
    }
    if status == StatusCode::GONE {
        return Err(PairingFailure::Expired);
    }
    if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
        return Err(PairingFailure::Retryable(status_error(status)));
    }
    if !status.is_success() {
        return Err(PairingFailure::Fatal(status_error(status)));
    }
    parse_ready(&body)
}

async fn sleep_or_cancel(
    delay: Duration,
    started: &PairingStarted,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let remaining = started.ttl();
    if remaining.is_zero() {
        return Err("The Palepale pairing code expired. Start pairing again.".into());
    }
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err("Pairing cancelled.".into()),
        _ = tokio::time::sleep(delay.min(remaining)) => Ok(()),
    }
}

fn parse_started(bytes: &[u8]) -> Result<PairingStarted, String> {
    if bytes.len() > MAX_PAIRING_BODY_BYTES {
        return Err("Palepale returned an oversized pairing response.".into());
    }
    let wire: StartedWire = serde_json::from_slice(bytes)
        .map_err(|_| "Palepale returned an invalid pairing response.".to_string())?;
    if !valid_device_code(&wire.device_code)
        || !valid_user_code(&wire.user_code)
        || !(1..=MAX_TTL_SECONDS).contains(&wire.ttl)
    {
        return Err("Palepale returned an invalid pairing response.".into());
    }
    Ok(PairingStarted {
        device_code: wire.device_code,
        user_code: wire.user_code,
        expires_at: Instant::now() + Duration::from_secs(wire.ttl),
    })
}

fn parse_pending(bytes: &[u8]) -> Result<PairingPoll, PairingFailure> {
    // The status code itself is the contract.  The current server includes a small JSON
    // marker, but accepting an empty 202 keeps the native client compatible with proxies that
    // strip response bodies while still enforcing the body bound in `collect_body`.
    if bytes.is_empty() {
        return Ok(PairingPoll::Pending);
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| {
        PairingFailure::Fatal("Palepale returned an invalid pairing response.".into())
    })?;
    if value.get("status").and_then(Value::as_str) == Some("pending") {
        Ok(PairingPoll::Pending)
    } else {
        Err(PairingFailure::Fatal(
            "Palepale returned an invalid pairing response.".into(),
        ))
    }
}

fn parse_ready(bytes: &[u8]) -> Result<PairingPoll, PairingFailure> {
    let wire: ReadyWire = serde_json::from_slice(bytes).map_err(|_| {
        PairingFailure::Fatal("Palepale returned an invalid pairing response.".into())
    })?;
    if wire.status != "ready" || !valid_api_key(&wire.api_key) {
        return Err(PairingFailure::Fatal(
            "Palepale returned an invalid pairing response.".into(),
        ));
    }
    Ok(PairingPoll::Ready(PairingCredential(wire.api_key)))
}

fn valid_device_code(value: &str) -> bool {
    (MIN_DEVICE_CODE_LEN..=MAX_DEVICE_CODE_LEN).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_user_code(value: &str) -> bool {
    value.len() == MAX_USER_CODE_LEN && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_api_key(value: &str) -> bool {
    (16..=4096).contains(&value.len())
        && !value.trim().is_empty()
        && !value.chars().any(|character| character.is_control())
}

async fn collect_body(
    mut response: reqwest::Response,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    loop {
        let next = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err("Pairing cancelled.".into()),
            next = response.chunk() => next.map_err(transport_error)?,
        };
        let Some(chunk) = next else { return Ok(bytes) };
        if bytes.len().saturating_add(chunk.len()) > MAX_PAIRING_BODY_BYTES {
            return Err("Palepale returned an oversized pairing response.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
}

fn transport_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "Palepale pairing timed out. Check your network connection.".into()
    } else if error.is_connect() {
        "Could not connect to Palepale. Check your network connection.".into()
    } else {
        "Palepale pairing failed while contacting the service.".into()
    }
}

fn status_error(status: StatusCode) -> String {
    format!("Palepale pairing failed (HTTP {}).", status.as_u16())
}

#[derive(Deserialize)]
struct StartedWire {
    device_code: String,
    user_code: String,
    ttl: u64,
}

#[derive(Deserialize)]
struct ReadyWire {
    status: String,
    api_key: String,
}

enum PairingFailure {
    Cancelled,
    Expired,
    Retryable(String),
    Fatal(String),
}

impl PairingFailure {
    fn message(&self) -> String {
        match self {
            Self::Cancelled => "Pairing cancelled.".into(),
            Self::Expired => "The Palepale pairing code expired. Start pairing again.".into(),
            Self::Retryable(message) | Self::Fatal(message) => message.clone(),
        }
    }

    fn from_body_error(message: String) -> Self {
        if message == "Pairing cancelled." {
            Self::Cancelled
        } else if message.contains("oversized") {
            Self::Fatal(message)
        } else {
            Self::Retryable(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn client() -> Client {
        Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
    }

    fn origin(address: std::net::SocketAddr) -> Url {
        Url::parse(&format!("http://{address}")).unwrap()
    }

    #[test]
    fn browser_url_contains_only_public_code() {
        let url = pairing_browser_url("012345&device_code=secret");
        let parsed = Url::parse(&url).unwrap();
        let pairs: Vec<_> = parsed.query_pairs().collect();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "code");
        assert_eq!(pairs[0].1, "012345&device_code=secret");
        assert!(!url.contains("device_code=secret&"));
    }

    #[test]
    fn parser_rejects_invalid_or_oversized_start_response() {
        let invalid = serde_json::json!({
            "device_code": "short",
            "user_code": "12",
            "ttl": 900
        });
        assert!(parse_started(&serde_json::to_vec(&invalid).unwrap()).is_err());
        assert!(parse_started(&vec![b'x'; MAX_PAIRING_BODY_BYTES + 1]).is_err());
    }

    #[test]
    fn accepted_without_a_body_is_still_pending() {
        assert!(matches!(parse_pending(&[]), Ok(PairingPoll::Pending)));
    }

    #[tokio::test]
    async fn loopback_pairing_start_and_one_time_ready_poll() {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0_u8; 8192];
            let read = stream.read(&mut request).await.unwrap();
            let text = String::from_utf8_lossy(&request[..read]);
            assert!(text.starts_with("POST /api/pair/start HTTP/1.1"));
            let body = serde_json::json!({
                "device_code": "d".repeat(40),
                "user_code": "123456",
                "ttl": 900
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0_u8; 8192];
            let read = stream.read(&mut request).await.unwrap();
            let text = String::from_utf8_lossy(&request[..read]);
            assert!(text.starts_with("GET /api/pair/"));
            let key = "pk_".to_string() + &"k".repeat(32);
            let body = serde_json::json!({ "status": "ready", "api_key": key }).to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });

        let cancel = CancellationToken::new();
        let started = start_at(&client(), origin(address), cancel.clone())
            .await
            .unwrap();
        assert_eq!(started.user_code(), "123456");
        let ready = poll_at(&client(), origin(address), &started, cancel)
            .await
            .unwrap();
        let PairingPoll::Ready(key) = ready else {
            panic!("expected ready pairing");
        };
        assert!(key.into_api_key().starts_with("pk_"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn cancelled_start_does_not_open_a_request() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = start_pairing(&client(), cancel).await;
        assert!(matches!(result, Err(error) if error == "Pairing cancelled."));
    }

    #[tokio::test]
    async fn cancellation_after_dispatch_preserves_a_consumed_ready_response() {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let cancel = CancellationToken::new();
        let server_cancel = cancel.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            assert!(stream.read(&mut request).await.unwrap() > 0);
            let body = serde_json::json!({
                "status": "ready", "api_key": "synthetic-consumed-credential"
            })
            .to_string();
            stream.write_all(format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            ).as_bytes()).await.unwrap();
            server_cancel.cancel();
            tokio::task::yield_now().await;
            stream.write_all(body.as_bytes()).await.unwrap();
        });
        let started = parse_started(
            &serde_json::to_vec(&serde_json::json!({
                "device_code":"d".repeat(40), "user_code":"123456", "ttl":900
            }))
            .unwrap(),
        )
        .unwrap();
        let credential = wait_at(&client(), origin(address), &started, cancel.clone())
            .await
            .unwrap();
        assert!(cancel.is_cancelled());
        assert_eq!(credential.into_api_key(), "synthetic-consumed-credential");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn cancellation_after_pending_stops_before_another_poll() {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let cancel = CancellationToken::new();
        let server_cancel = cancel.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            assert!(stream.read(&mut request).await.unwrap() > 0);
            server_cancel.cancel();
            stream
                .write_all(
                    b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(50), listener.accept())
                    .await
                    .is_err()
            );
        });
        let started = parse_started(
            &serde_json::to_vec(&serde_json::json!({
                "device_code":"d".repeat(40), "user_code":"123456", "ttl":900
            }))
            .unwrap(),
        )
        .unwrap();
        let result = wait_at(&client(), origin(address), &started, cancel).await;
        assert!(matches!(result, Err(error) if error == "Pairing cancelled."));
        server.await.unwrap();
    }
}
