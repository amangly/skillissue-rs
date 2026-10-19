//! Read-only public account projection; credentials and account identifiers stay out of the UI.

use reqwest::Client;
use serde::Deserialize;
use skillissue_domain::provider::PALEPALE_BASE_URL;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const MAX_ACCOUNT_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct AccountDetails {
    pub available_microdollars: Option<i64>,
    pub reserved_microdollars: Option<u64>,
    pub wallet_spend_limit_microdollars: Option<u64>,
    pub wallet_spent_microdollars: Option<u64>,
    pub balance_fallback: Option<bool>,
    pub passes: Option<Vec<AccountPass>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct AccountPass {
    pub plan: String,
    pub ends_at: String,
    pub allowance_microdollars: u64,
    pub remaining_microdollars: u64,
    pub reserved_microdollars: u64,
    pub refills: Option<String>,
    pub refill_microdollars_per_day: Option<u64>,
    #[serde(default)]
    pub scope: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct AccountUsage {
    pub days: u32,
    pub requests: u64,
    pub confirmed_requests: u64,
    pub unknown_requests: u64,
    pub spent_microdollars: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cached_tokens: u64,
    pub reasoning_tokens: u64,
}

async fn usage_at(
    client: &Client,
    url: &str,
    key: &str,
    cancel: &CancellationToken,
) -> Result<AccountUsage, String> {
    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err("Usage refresh cancelled.".into()),
        response = client.get(url).bearer_auth(key).timeout(Duration::from_secs(15)).send() =>
            response.map_err(|_| "Usage unavailable. Refresh to retry.".to_string())?,
    };
    if response.status().as_u16() != 200 {
        return Err("Usage unavailable. Refresh to retry.".into());
    }
    let bytes = super::http::bounded_body(response, MAX_ACCOUNT_BYTES, cancel)
        .await
        .map_err(|_| "Usage could not be read.".to_string())?;
    let usage: AccountUsage =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid usage summary.".to_string())?;
    if usage.days != 0 {
        return Err("Unexpected usage period.".into());
    }
    Ok(usage)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalepaleAccountStatus {
    pub can_spend: bool,
    pub details: Option<AccountDetails>,
    pub usage: Option<AccountUsage>,
    pub usage_error: Option<String>,
}

pub async fn palepale_account_status(
    client: &Client,
    key: &str,
    cancel: CancellationToken,
) -> Result<PalepaleAccountStatus, String> {
    let origin = PALEPALE_BASE_URL
        .strip_suffix("/v1")
        .ok_or("Palepale account service is unavailable.")?;
    let mut status = account_at(client, &format!("{origin}/api/me"), key, cancel.clone()).await?;
    match usage_at(
        client,
        &format!("{origin}/api/me/overview?days=0"),
        key,
        &cancel,
    )
    .await
    {
        Ok(usage) => status.usage = Some(usage),
        Err(error) => status.usage_error = Some(error),
    }
    if cancel.is_cancelled() {
        return Err("Account check cancelled.".into());
    }
    Ok(status)
}

async fn account_at(
    client: &Client,
    url: &str,
    key: &str,
    cancel: CancellationToken,
) -> Result<PalepaleAccountStatus, String> {
    if key.is_empty() || key.len() > 4096 || key.chars().any(char::is_control) {
        return Err("Connect Palepale before checking this device.".into());
    }
    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err("Account check cancelled.".into()),
        response = client.get(url).bearer_auth(key)
            .header(reqwest::header::ACCEPT, "application/json")
            .timeout(Duration::from_secs(15)).send() => {
                response.map_err(|_| "Could not check Palepale. Check your network connection.".to_string())?
            },
    };
    match response.status().as_u16() {
        200 => {}
        401 | 403 => {
            return Err(
                "This Palepale connection is no longer authorized. Reconnect this device.".into(),
            );
        }
        402 => {
            return Ok(PalepaleAccountStatus {
                can_spend: false,
                ..Default::default()
            });
        }
        429 => return Err("Palepale is busy. Check the connection again shortly.".into()),
        _ => return Err("Palepale account status is unavailable. Try again.".into()),
    }
    let bytes = super::http::bounded_body(response, MAX_ACCOUNT_BYTES, &cancel)
        .await
        .map_err(|_| "Palepale account status could not be read. Try again.".to_string())?;
    let wire: AccountWire = serde_json::from_slice(&bytes)
        .map_err(|_| "Palepale returned an invalid account status.".to_string())?;
    Ok(PalepaleAccountStatus {
        can_spend: wire.enabled && wire.can_spend,
        details: Some(wire.details),
        ..Default::default()
    })
}

#[derive(Deserialize)]
struct AccountWire {
    enabled: bool,
    can_spend: bool,
    #[serde(flatten)]
    details: AccountDetails,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    const TEST_KEY: &str = "synthetic-test-device-key";

    async fn response(status: &str, body: Vec<u8>) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let status = status.to_owned();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            while !bytes.windows(4).any(|chunk| chunk == b"\r\n\r\n") {
                let mut buffer = [0; 1024];
                let read = socket.read(&mut buffer).await.unwrap();
                assert!(read > 0);
                bytes.extend_from_slice(&buffer[..read]);
                assert!(bytes.len() < 8192);
            }
            let header = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(header.as_bytes()).await.unwrap();
            let _ = socket.write_all(&body).await;
            String::from_utf8(bytes).unwrap()
        });
        (format!("http://{address}/api/me"), task)
    }

    #[tokio::test]
    async fn readiness_uses_authenticated_get_and_discards_profile_details() {
        for (enabled, can_spend, ready) in [
            (true, true, true),
            (true, false, false),
            (false, true, false),
        ] {
            let body = serde_json::json!({"enabled":enabled,"can_spend":can_spend,"account_id":"private-account","account_hint":"private-hint"});
            let (url, server) = response("200 OK", serde_json::to_vec(&body).unwrap()).await;
            let status = account_at(&Client::new(), &url, TEST_KEY, CancellationToken::new())
                .await
                .unwrap();
            assert_eq!(status.can_spend, ready);
            assert!(!format!("{status:?}").contains("private"));
            let request = server.await.unwrap().to_lowercase();
            assert!(request.starts_with("get /api/me http/1.1\r\n"));
            assert!(request.contains(&format!("authorization: bearer {TEST_KEY}")));
            assert!(!request.contains("cookie:"));
        }
    }

    #[tokio::test]
    async fn account_projects_wallet_and_pass_contract_without_private_identifiers() {
        let body = serde_json::json!({
            "enabled":true,"can_spend":true,"account_id":"private-account",
            "account_hint":"private-hint", "available_microdollars":4949716,
            "reserved_microdollars":41833, "wallet_spend_limit_microdollars":10000000,
            "wallet_spent_microdollars":8451,"balance_fallback":true,
            "passes":[{"id":"private-pass-id","plan":"monthly","ends_at":"2026-11-19T00:00:00Z",
                "allowance_microdollars":1000000,"remaining_microdollars":700000,
                "reserved_microdollars":100000,"scope":["text","image"],"refills":null}]
        });
        let (url, server) = response("200 OK", serde_json::to_vec(&body).unwrap()).await;
        let status = account_at(&Client::new(), &url, TEST_KEY, CancellationToken::new())
            .await
            .unwrap();
        assert!(status.can_spend);
        assert!(!format!("{status:?}").contains("private"));
        let details = status.details.unwrap();
        assert_eq!(details.available_microdollars, Some(4949716));
        assert_eq!(details.reserved_microdollars, Some(41833));
        let passes = details.passes.unwrap();
        assert_eq!(
            passes[0].remaining_microdollars - passes[0].reserved_microdollars,
            600000
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn usage_preserves_authoritative_counts_and_rejects_wrong_period_or_partial_data() {
        let valid = serde_json::json!({"days":0,"requests":26,"confirmed_requests":23,
            "unknown_requests":3,"spent_microdollars":8451,"prompt_tokens":122000,
            "completion_tokens":7800,"cached_tokens":31900,"reasoning_tokens":857});
        for body in [valid.clone(), serde_json::json!({"requests":0}), {
            let mut wrong = valid.clone();
            wrong["days"] = 30.into();
            wrong
        }] {
            let (url, server) = response("200 OK", serde_json::to_vec(&body).unwrap()).await;
            let result = usage_at(&Client::new(), &url, TEST_KEY, &CancellationToken::new()).await;
            if body == valid {
                let usage = result.unwrap();
                assert_eq!(usage.unknown_requests, 3);
                assert_eq!(usage.spent_microdollars, 8451);
                assert_eq!(usage.cached_tokens, 31900);
            } else {
                assert!(result.is_err());
            }
            assert!(
                server
                    .await
                    .unwrap()
                    .to_lowercase()
                    .contains(&format!("authorization: bearer {TEST_KEY}"))
            );
        }
    }

    #[test]
    fn absent_financial_fields_remain_unknown() {
        let wire: AccountWire =
            serde_json::from_str(r#"{"enabled":true,"can_spend":true}"#).unwrap();
        assert!(wire.details.available_microdollars.is_none());
        assert!(wire.details.passes.is_none());
    }

    #[tokio::test]
    async fn rejected_or_malformed_status_never_echoes_credentials_or_profile() {
        for (http, body) in [
            ("401 Unauthorized", TEST_KEY.as_bytes().to_vec()),
            ("302 Found", TEST_KEY.as_bytes().to_vec()),
            ("200 OK", b"{}".to_vec()),
            ("200 OK", vec![b'x'; MAX_ACCOUNT_BYTES + 1]),
        ] {
            let (url, server) = response(http, body).await;
            let error = account_at(&Client::new(), &url, TEST_KEY, CancellationToken::new())
                .await
                .unwrap_err();
            assert!(!error.contains(TEST_KEY));
            assert!(!error.contains(&url));
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn cancelled_account_check_never_sends_a_credential() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/api/me", listener.local_addr().unwrap());
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(
            account_at(&Client::new(), &url, TEST_KEY, cancel)
                .await
                .is_err()
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
    }
}
