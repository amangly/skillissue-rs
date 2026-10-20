//! Nova-3 segment transport for explicit Audio tests and encoded fixtures.
//! Capture uses the continuous transport in `live`.
pub(super) mod benchmark;
pub(crate) mod live;
use super::http::{bounded_body, check_status, endpoint, network_error};
use crate::models::Settings;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::{io::Cursor, time::Duration};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message, client::IntoClientRequest, protocol::WebSocketConfig},
};
use tokio_util::sync::CancellationToken;

pub(super) fn recognizes(settings: &Settings) -> bool {
    !settings.is_palepale()
        && skillissue_domain::byok::byok_provider(settings.provider_base_url())
            .is_some_and(|provider| provider.id == "deepgram")
}

fn supported_model(model: &str) -> bool {
    matches!(model, "nova-3" | "nova-3-general")
}

fn listen_url(settings: &Settings, key: &str) -> Result<String, String> {
    let mut url = url::Url::parse(&endpoint(settings, key, "listen")?)
        .map_err(|_| "Invalid Deepgram URL.")?;
    url.set_scheme("wss").map_err(|_| "Invalid Deepgram URL.")?;
    let model = settings.transcription_model.trim();
    if !supported_model(model) {
        return Err("Choose nova-3 or nova-3-general for Deepgram Audio.".into());
    }
    let language = settings.language.trim();
    if language.len() > 32
        || !language
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("Enter a language code such as en, id, zh, or multi.".into());
    }
    url.query_pairs_mut()
        .append_pair("model", model)
        .append_pair(
            "language",
            if language.is_empty() { "en" } else { language },
        )
        .append_pair("encoding", "linear16")
        .append_pair("sample_rate", "16000")
        .append_pair("channels", "1")
        .append_pair("endpointing", "false")
        .append_pair("interim_results", "false")
        .append_pair("punctuate", "true");
    Ok(url.into())
}

fn pcm(wav: Vec<u8>) -> Result<Vec<u8>, String> {
    let mut reader = hound::WavReader::new(Cursor::new(wav))
        .map_err(|_| "Deepgram requires valid WAV audio.")?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.sample_rate != 16_000
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
        || reader.len() == 0
        || reader.len() > 16_000 * 30
    {
        return Err("Deepgram requires mono 16 kHz PCM16 audio, at most 30 seconds.".into());
    }
    let mut pcm = Vec::with_capacity(reader.len() as usize * 2);
    for sample in reader.samples::<i16>() {
        pcm.extend_from_slice(
            &sample
                .map_err(|_| "Could not decode Deepgram audio.")?
                .to_le_bytes(),
        );
    }
    Ok(pcm)
}

pub(super) async fn transcribe(
    settings: &Settings,
    key: &str,
    wav: Vec<u8>,
) -> Result<String, String> {
    let url = listen_url(settings, key)?;
    let pcm = pcm(wav)?;
    tokio::time::timeout(Duration::from_secs(30), exchange(&url, key, pcm))
        .await
        .map_err(|_| "Deepgram transcription timed out. It was not retried.".to_string())?
}

async fn exchange(url: &str, key: &str, pcm: Vec<u8>) -> Result<String, String> {
    let mut request = url
        .into_client_request()
        .map_err(|_| "Could not prepare the Deepgram connection.")?;
    let mut auth = format!("Token {}", key.trim())
        .parse::<tokio_tungstenite::tungstenite::http::HeaderValue>()
        .map_err(|_| "The Deepgram API key has an invalid format.")?;
    auth.set_sensitive(true);
    request.headers_mut().insert("Authorization", auth);
    let config = WebSocketConfig::default()
        .max_message_size(Some(256 * 1024))
        .max_frame_size(Some(256 * 1024));
    // No redirects, reconnects or request replay; transport errors must never echo keys.
    let (socket, _) = connect_async_with_config(request, Some(config), false)
        .await
        .map_err(|error| match error {
            tokio_tungstenite::tungstenite::Error::Http(response) => format!(
                "Deepgram returned HTTP {}. Check the Audio key, model, credit and quota.",
                response.status().as_u16()
            ),
            _ => "Could not connect to Deepgram. Check your network and Audio settings.".into(),
        })?;
    let (mut sender, mut receiver) = socket.split();
    let send = async {
        for chunk in pcm.chunks(3200) {
            sender
                .send(Message::Binary(chunk.to_vec().into()))
                .await
                .map_err(|_| "Deepgram audio upload failed; it was not retried.")?;
        }
        sender
            .send(Message::Text(r#"{"type":"CloseStream"}"#.into()))
            .await
            .map_err(|_| "Could not finalize Deepgram audio.")?;
        Ok::<(), String>(())
    };
    let receive = async {
        let mut output = String::new();
        let mut bytes = 0usize;
        let mut messages = 0usize;
        let mut complete = false;
        while let Some(message) = receiver.next().await {
            let message = message.map_err(|_| "Deepgram disconnected before completion.")?;
            messages += 1;
            bytes = bytes.saturating_add(message.len());
            if messages > 4096 || bytes > super::MAX_JSON_BYTES {
                return Err("Deepgram exceeded the response size limit.".into());
            }
            match message {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(&text)
                        .map_err(|_| "Deepgram returned invalid JSON.")?;
                    match value.get("type").and_then(Value::as_str) {
                        Some("Results") => append_final(&mut output, &value, key)?,
                        Some("Metadata") => complete = true,
                        Some("Error") => return Err("Deepgram rejected the audio request. Check the model and language settings.".into()),
                        _ => {}
                    }
                }
                Message::Close(frame) => {
                    if frame.is_some_and(|frame| {
                        frame.code != tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Normal
                    }) {
                        return Err("Deepgram closed the connection with an error.".into());
                    }
                    break;
                }
                _ => {}
            }
        }
        if !complete {
            return Err(
                "Deepgram ended without confirming completion; the segment was not saved.".into(),
            );
        }
        Ok(output)
    };
    let (_, output) = tokio::try_join!(send, receive)?;
    Ok(output)
}

fn append_final(output: &mut String, value: &Value, key: &str) -> Result<(), String> {
    if value.get("is_final").and_then(Value::as_bool) != Some(true) {
        return Ok(());
    }
    let text = value
        .pointer("/channel/alternatives/0/transcript")
        .and_then(Value::as_str)
        .ok_or("Deepgram returned a final result without a transcript.")?
        .trim();
    if !key.trim().is_empty() && text.contains(key.trim()) {
        return Err("Deepgram returned an invalid transcript.".into());
    }
    if output.len() + text.len() + 1 > 64 * 1024 {
        return Err("Deepgram exceeded the transcript size limit.".into());
    }
    if !text.is_empty() {
        if !output.is_empty() {
            output.push(' ');
        }
        output.push_str(text);
    }
    Ok(())
}

pub(super) async fn models(
    client: &reqwest::Client,
    settings: &Settings,
    key: &str,
    cancel: CancellationToken,
) -> Result<Vec<String>, String> {
    let url = endpoint(settings, key, "models")?;
    let request = client
        .get(url)
        .header("Authorization", format!("Token {}", key.trim()))
        .timeout(Duration::from_secs(15))
        .send();
    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err("Connection check cancelled.".into()),
        response = request => response.map_err(network_error)?,
    };
    let body = bounded_body(check_status(response)?, 1024 * 1024, &cancel).await?;
    let value: Value =
        serde_json::from_slice(&body).map_err(|_| "Deepgram returned invalid model JSON.")?;
    parse_models(&value)
}

fn parse_models(value: &Value) -> Result<Vec<String>, String> {
    let entries = value
        .get("stt")
        .and_then(Value::as_array)
        .ok_or("Deepgram did not return a speech model catalog.")?;
    if entries.len() > 4096 {
        return Err("Deepgram returned too many models.".into());
    }
    let mut models: Vec<String> = entries
        .iter()
        .filter(|model| model.get("streaming").and_then(Value::as_bool) == Some(true))
        .filter_map(|model| model.get("canonical_name").and_then(Value::as_str))
        .filter(|id| supported_model(id))
        .map(str::to_owned)
        .collect();
    models.sort();
    models.dedup();
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_validates_model_language_key_and_pcm_format() {
        let mut settings = Settings::new_profile();
        settings.api_base_url = settings.transcription_base_url.clone();
        let url = listen_url(&settings, "synthetic-key").unwrap();
        assert!(url.starts_with("wss://api.deepgram.com/v1/listen?"));
        assert!(url.contains("language=en"));
        assert!(!url.contains("synthetic-key"));
        settings.language = "id".into();
        assert!(
            listen_url(&settings, "synthetic-key")
                .unwrap()
                .contains("language=id")
        );
        settings.language = "en&model=other".into();
        assert!(listen_url(&settings, "synthetic-key").is_err());
        settings.language.clear();
        settings.transcription_model = "flux-general-en".into();
        assert!(listen_url(&settings, "synthetic-key").is_err());
        settings.transcription_model = "nova-3".into();
        assert!(listen_url(&settings, "").is_err());
        assert!(listen_url(&settings, "bad\r\nkey").is_err());
        assert!(pcm(vec![0; 44]).is_err());
        let wav = crate::audio::encode_hosted_wav(&crate::audio::AudioSegment {
            source: "test".into(),
            sample_rate: 48_000,
            samples: vec![0.0; 4800],
        })
        .unwrap();
        assert_eq!(pcm(wav).unwrap().len(), 3200);
    }

    #[tokio::test]
    async fn disconnect_without_metadata_does_not_commit_partial_transcript() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/v1/listen", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            socket.next().await.unwrap().unwrap();
            socket.next().await.unwrap().unwrap();
            socket.send(Message::Text(json!({"type":"Results","is_final":true,"channel":{"alternatives":[{"transcript":"Partial"}]}}).to_string().into())).await.unwrap();
            socket.close(None).await.unwrap();
        });
        assert!(
            exchange(&url, "synthetic-key", vec![0, 0])
                .await
                .unwrap_err()
                .contains("without confirming")
        );
        server.await.unwrap();
    }

    #[test]
    fn only_final_text_is_committed_and_credentials_cannot_be_echoed() {
        let mut text = String::new();
        for (is_final, words) in [
            (false, "wrong"),
            (true, "Hello"),
            (true, ""),
            (true, "world."),
        ] {
            append_final(
                &mut text,
                &json!({"is_final":is_final,"channel":{"alternatives":[{"transcript":words}]}}),
                "test-secret",
            )
            .unwrap();
        }
        assert_eq!(text, "Hello world.");
        assert!(
            append_final(
                &mut text,
                &json!({"is_final":true,"channel":{"alternatives":[{"transcript":"test-secret"}]}}),
                "test-secret"
            )
            .is_err()
        );
        assert!(append_final(&mut text, &json!({"is_final":true}), "").is_err());
    }

    #[test]
    fn catalog_excludes_batch_only_and_other_protocols() {
        let models = parse_models(&json!({"stt":[
            {"canonical_name":"nova-3","streaming":true},
            {"canonical_name":"nova-3","streaming":true},
            {"canonical_name":"nova-3-general","streaming":false},
            {"canonical_name":"flux-general-en","streaming":true}
        ]}))
        .unwrap();
        assert_eq!(models, ["nova-3"]);
        assert!(parse_models(&json!({"data":[]})).is_err());
    }

    #[tokio::test]
    #[allow(clippy::result_large_err)] // Tungstenite's server callback fixes the error type.
    async fn websocket_sends_pcm_auth_and_close_then_collects_finals() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/v1/listen", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_hdr_async(
                stream,
                |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                    assert_eq!(request.headers()["Authorization"], "Token test-secret");
                    Ok(response)
                },
            )
            .await
            .unwrap();
            assert_eq!(
                socket.next().await.unwrap().unwrap(),
                Message::Binary(vec![0, 1, 2, 3].into())
            );
            assert_eq!(
                socket.next().await.unwrap().unwrap(),
                Message::Text(r#"{"type":"CloseStream"}"#.into())
            );
            for value in [
                json!({"type":"Results","is_final":false,"channel":{"alternatives":[{"transcript":"interim"}]}}),
                json!({"type":"Results","is_final":true,"channel":{"alternatives":[{"transcript":"Final words."}]}}),
                json!({"type":"Metadata"}),
            ] {
                socket
                    .send(Message::Text(value.to_string().into()))
                    .await
                    .unwrap();
            }
            socket.close(None).await.unwrap();
        });
        let text = exchange(&url, "test-secret", vec![0, 1, 2, 3])
            .await
            .unwrap();
        assert_eq!(text, "Final words.");
        server.await.unwrap();
    }
}
