//! Hosted inference regressions use synthetic credentials and loopback TLS only.

use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use skillissue_domain::{
    models::ProviderMode,
    provider::{ModelCapabilities, ProviderModel},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use tokio_rustls::{TlsAcceptor, rustls};

// Synthetic test certificate and key; trusted only by this fixture's private client.
const CERT: &str = "MIIBTjCB9qADAgECAghVFPqqHS8U0TAKBggqhkjOPQQDAjAcMRowGAYDVQQDExFhcGkucGFsZXBhbGUudGVjaDAgFw0yMDAxMDEwMDAwMDBaGA8yMTIwMDEwMTAwMDAwMFowHDEaMBgGA1UEAxMRYXBpLnBhbGVwYWxlLnRlY2gwWTATBgcqhkjOPQIBBggqhkjOPQMBBwNCAAQdeXFOl0TiE9ngdPAtxTf3zJq0yiwZaGQ+xMGU/AXENf8+6limB6Cs2zLSttyC8TypY7h2lQArkOVKmW182zQLoyAwHjAcBgNVHREEFTATghFhcGkucGFsZXBhbGUudGVjaDAKBggqhkjOPQQDAgNHADBEAiBYtBkL8HDycZqTCFdIb66+iGO7FPLL+qh8uJY0ToC3bQIgQaCXMnO+yjr3r0fnE7SFfUZC1gBFjOx9/mrMTTgWvyY=";
const TEST_KEY: &str = "MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgWJiVBFXXnKZftZZTjTkLcUHjpnlocKgtTYe71tk77UqhRANCAAQdeXFOl0TiE9ngdPAtxTf3zJq0yiwZaGQ+xMGU/AXENf8+6limB6Cs2zLSttyC8TypY7h2lQArkOVKmW182zQL";
const TOKEN: &str = "synthetic-hosted-regression-key";

struct TestCredentials;
impl credentials::Credentials for TestCredentials {
    fn load_saved(&self, _: &str) -> Result<Option<String>, String> {
        Ok(Some(TOKEN.into()))
    }
    fn save(&self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn load_provider(&self, _: &Settings) -> Result<String, String> {
        Ok(TOKEN.into())
    }
}

struct TestDirectory(PathBuf);
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn accepted_reasoning_or_media_error_before_first_answer_sends_one_post() {
    for with_image in [false, true] {
        for late_error in [false, true] {
            let directory = TestDirectory(
                std::env::temp_dir()
                    .join(format!("skillissue-hosted-once-{}", uuid::Uuid::new_v4())),
            );
            let settings = Settings {
                provider_mode: ProviderMode::Palepale,
                palepale: skillissue_domain::models::PalepaleSettings {
                    chat_model: "selected-image".into(),
                    transcription_model: "independent-audio".into(),
                    reasoning_effort: Some("low".into()),
                },
                include_screenshot: false,
                ..Settings::default()
            };
            config::save(&directory.0.join("settings.json"), &settings).unwrap();
            let models: Vec<_> = ["selected-image", "eligible-alternate"]
                .into_iter()
                .map(|id| ProviderModel {
                    id: id.into(),
                    capabilities: ModelCapabilities {
                        streaming: true,
                        reasoning: true,
                        image_input: true,
                        ..Default::default()
                    },
                    reasoning_efforts: vec!["low".into()],
                    ..Default::default()
                })
                .collect();
            let catalog = serde_json::json!({"data": models}).to_string();
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let cert = STANDARD.decode(CERT).unwrap();
            let tls = rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(
                    vec![rustls::pki_types::CertificateDer::from(cert.clone())],
                    rustls::pki_types::PrivatePkcs8KeyDer::from(STANDARD.decode(TEST_KEY).unwrap())
                        .into(),
                )
                .unwrap();
            let acceptor = TlsAcceptor::from(Arc::new(tls));
            let stop = CancellationToken::new();
            let server_stop = stop.clone();
            let server = tokio::spawn(async move {
                let mut posts = Vec::new();
                loop {
                    let accepted = tokio::select! {
                        _ = server_stop.cancelled() => break,
                        accepted = listener.accept() => accepted.unwrap(),
                    };
                    let mut stream = acceptor.accept(accepted.0).await.unwrap();
                    let mut bytes = Vec::new();
                    let header_end = loop {
                        let mut buffer = [0; 4096];
                        let size = stream.read(&mut buffer).await.unwrap();
                        assert!(size > 0);
                        bytes.extend_from_slice(&buffer[..size]);
                        assert!(bytes.len() <= 128 * 1024);
                        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                            break end + 4;
                        }
                    };
                    let headers = String::from_utf8_lossy(&bytes[..header_end]).to_lowercase();
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("content-length:")
                                .map(|value| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or_default();
                    while bytes.len() < header_end + length {
                        let mut buffer = [0; 4096];
                        let size = stream.read(&mut buffer).await.unwrap();
                        assert!(size > 0);
                        bytes.extend_from_slice(&buffer[..size]);
                        assert!(bytes.len() <= 128 * 1024);
                    }
                    let (mime, body) = if headers.starts_with("get /v1/models?app=skillissue-rs ") {
                        assert!(!headers.contains("authorization:"));
                        ("application/json", catalog.clone())
                    } else {
                        assert!(headers.starts_with("post /v1/chat/completions "));
                        assert!(headers.contains(&format!("authorization: bearer {TOKEN}")));
                        let body: serde_json::Value =
                            serde_json::from_slice(&bytes[header_end..header_end + length])
                                .unwrap();
                        assert_eq!(body["model"], "selected-image");
                        assert_eq!(
                            body["messages"].as_array().unwrap().last().unwrap()["content"]
                                .is_array(),
                            with_image
                        );
                        posts.push(body);
                        let reasoning = "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"Reasoning already accepted\"},\"finish_reason\":null}]}\n\n";
                        let body = if late_error {
                            format!(
                                "{reasoning}event: error\ndata: {{\"error\":{{\"message\":\"Supplier usage unknown\"}}}}\n\n"
                            )
                        } else {
                            reasoning.into()
                        };
                        ("text/event-stream", body)
                    };
                    stream.write_all(format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    ).as_bytes()).await.unwrap();
                    stream.shutdown().await.unwrap();
                }
                posts
            });
            let client = reqwest::Client::builder()
                .no_proxy()
                .tls_built_in_root_certs(false)
                .add_root_certificate(reqwest::Certificate::from_der(&cert).unwrap())
                .resolve("api.palepale.tech", address)
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();
            let published = crate::provider::palepale_models(&client, CancellationToken::new())
                .await
                .unwrap();
            let engine = NativeEngine::open_with_transport(
                directory.0.clone(),
                Arc::new(TestCredentials),
                client,
            )
            .unwrap();
            *lock(&engine.published_models).unwrap() = published;
            let mut events = engine.subscribe();
            let screenshot = with_image.then(|| {
                let mut png = std::io::Cursor::new(Vec::new());
                image::DynamicImage::new_rgb8(1, 1)
                    .write_to(&mut png, image::ImageFormat::Png)
                    .unwrap();
                format!(
                    "data:image/png;base64,{}",
                    STANDARD.encode(png.into_inner())
                )
            });
            let result = engine.assist("Read this context".into(), screenshot).await;
            assert!(
                result.is_err(),
                "Unsettled upstream work cannot complete an answer"
            );
            assert!(engine.snapshot().unwrap().messages.is_empty());
            assert!(engine.list_sessions("", 10, 0).unwrap().is_empty());
            while let Ok(event) = events.try_recv() {
                if let EngineEvent::Assistant { kind, .. } = event {
                    assert!(!matches!(kind.as_str(), "delta" | "done"));
                }
            }
            engine.shutdown().await.unwrap();
            stop.cancel();
            assert_eq!(
                server.await.unwrap().len(),
                1,
                "An eligible alternate must not receive another paid request"
            );
        }
    }
}
