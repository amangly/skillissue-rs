//! Continuous speech connection. Ask/Stop drains CloseStream before acknowledging its boundary.
use super::*;
use crate::audio::LiveFrame;
use skillissue_domain::models::SpeechWord;
use tokio::sync::mpsc;

pub(crate) enum Command {
    Audio(LiveFrame),
    Boundary { job: u64, ending: bool },
}

pub(crate) struct ResultText {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub words: Vec<SpeechWord>,
    pub is_final: bool,
}

pub(crate) fn url(settings: &Settings, key: &str) -> Result<String, String> {
    let mut url =
        url::Url::parse(&super::listen_url(settings, key)?).map_err(|_| "Invalid audio URL.")?;
    let pairs: Vec<_> = url
        .query_pairs()
        .filter(|(key, _)| key != "endpointing" && key != "interim_results")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    url.set_query(None);
    url.query_pairs_mut()
        .extend_pairs(pairs)
        .append_pair("endpointing", "320")
        .append_pair("interim_results", "true")
        .append_pair("diarize_model", "v1");
    Ok(url.into())
}

fn millis(value: &Value, field: &str) -> Result<u64, String> {
    let seconds = value
        .get(field)
        .and_then(Value::as_f64)
        .ok_or("Deepgram omitted audio timing.")?;
    if !seconds.is_finite() || !(0.0..=604800.0).contains(&seconds) {
        return Err("Deepgram returned invalid audio timing.".into());
    }
    Ok((seconds * 1000.0).round() as u64)
}

pub(crate) fn parse(value: &Value, key: &str) -> Result<ResultText, String> {
    let alternative = value
        .pointer("/channel/alternatives/0")
        .ok_or("Deepgram omitted its transcript.")?;
    let text = alternative
        .get("transcript")
        .and_then(Value::as_str)
        .ok_or("Invalid Deepgram transcript.")?;
    if text.len() > 64 * 1024 || (!key.is_empty() && text.contains(key)) {
        return Err("Deepgram returned an invalid transcript.".into());
    }
    let start_ms = millis(value, "start")?;
    let end_ms = start_ms.saturating_add(millis(value, "duration")?);
    let mut words = Vec::new();
    if let Some(array) = alternative.get("words").and_then(Value::as_array) {
        if array.len() > 4096 {
            return Err("Deepgram returned too many words.".into());
        }
        for word in array {
            let text = word
                .get("punctuated_word")
                .or_else(|| word.get("word"))
                .and_then(Value::as_str)
                .ok_or("Deepgram returned an invalid word.")?;
            if text.len() > 1024 || (!key.is_empty() && text.contains(key)) {
                return Err("Deepgram returned an invalid word.".into());
            }
            let start = millis(word, "start")?;
            let end = millis(word, "end")?;
            if end < start || start + 2 < start_ms || end > end_ms + 2 {
                return Err("Deepgram returned inconsistent word timing.".into());
            }
            words.push(SpeechWord {
                text: text.into(),
                start_ms: start,
                end_ms: end,
                speaker: word
                    .get("speaker")
                    .and_then(Value::as_u64)
                    .filter(|id| *id < 1000)
                    .map(|id| id as u32),
                confidence: word
                    .get("confidence")
                    .and_then(Value::as_f64)
                    .filter(|score| (0.0..=1.0).contains(score)),
            });
        }
    }
    Ok(ResultText {
        text: text.trim().into(),
        start_ms,
        end_ms,
        words,
        is_final: value.get("is_final").and_then(Value::as_bool) == Some(true),
    })
}

/// Processes one connection until an explicit boundary. Subsequent audio stays in
/// the bounded receiver while CloseStream drains; submitted bytes are never replayed.
pub(crate) async fn connection(
    url: &str,
    key: &str,
    first: LiveFrame,
    receiver: &mut mpsc::Receiver<Command>,
    boundary: &mut Option<(u64, bool)>,
    on_connected: impl FnOnce(),
    mut on_result: impl FnMut(ResultText) -> Result<(), String>,
) -> Result<(), String> {
    let mut request = url
        .into_client_request()
        .map_err(|_| "Invalid Deepgram request.")?;
    let mut auth = format!("Token {}", key.trim())
        .parse::<tokio_tungstenite::tungstenite::http::HeaderValue>()
        .map_err(|_| "Invalid Deepgram key.")?;
    auth.set_sensitive(true);
    request.headers_mut().insert("Authorization", auth);
    let config = WebSocketConfig::default()
        .max_message_size(Some(256 * 1024))
        .max_frame_size(Some(256 * 1024));
    let (mut socket, _) = tokio::time::timeout(
        Duration::from_secs(15),
        connect_async_with_config(request, Some(config), false),
    )
    .await
    .map_err(|_| "Deepgram connection timed out.")?
    .map_err(|_| "Deepgram connection failed. Check the Audio key, network and quota.")?;
    tokio::time::timeout(
        Duration::from_secs(5),
        socket.send(Message::Binary(first.pcm.into())),
    )
    .await
    .map_err(|_| "Deepgram upload stalled.")?
    .map_err(|_| "Deepgram upload failed.")?;
    on_connected();
    let mut closing = false;
    let mut close_deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut keepalive = tokio::time::interval(Duration::from_secs(3));
    keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_final = 0;
    loop {
        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(close_deadline), if closing => {
                return Err("Deepgram did not finish pending audio before the drain timeout.".into());
            }
            message = socket.next() => {
                match message.ok_or("Deepgram disconnected; pending audio was not replayed.")?.map_err(|_| "Deepgram connection failed; pending audio was not replayed.")? {
                    Message::Text(text) => {
                        let value: Value = serde_json::from_str(&text).map_err(|_| "Deepgram returned invalid JSON.")?;
                        match value.get("type").and_then(Value::as_str) {
                            Some("Results") => {
                                let result = parse(&value, key)?;
                                if result.end_ms <= last_final && last_final > 0 { continue; }
                                if result.start_ms + 2 < last_final { return Err("Deepgram returned overlapping finalized audio.".into()); }
                                if result.is_final { last_final = result.end_ms; }
                                on_result(result)?;
                            }
                            Some("Metadata") if closing => return Ok(()),
                            Some("Error") => return Err("Deepgram rejected the live audio. Check model, language and quota.".into()),
                            _ => {}
                        }
                    }
                    Message::Close(_) => return Err("Deepgram closed before confirming completion.".into()),
                    Message::Ping(data) => {
                        tokio::time::timeout(Duration::from_secs(5), socket.send(Message::Pong(data)))
                            .await.map_err(|_| "Deepgram keepalive timed out.")?
                            .map_err(|_| "Deepgram keepalive failed.")?;
                    }
                    _ => {}
                }
            }
            command = receiver.recv(), if !closing => {
                let message = match command {
                    Some(Command::Audio(frame)) => Message::Binary(frame.pcm.into()),
                    Some(Command::Boundary { job, ending }) => {
                        *boundary = Some((job, ending));
                        closing = true;
                        close_deadline = tokio::time::Instant::now() + Duration::from_secs(30);
                        Message::Text(r#"{"type":"CloseStream"}"#.into())
                    }
                    None => return Err("Capture ended without an audio drain boundary.".into()),
                };
                tokio::time::timeout(Duration::from_secs(5), socket.send(message)).await.map_err(|_| "Deepgram upload stalled.")?.map_err(|_| "Deepgram upload failed; audio was not replayed.")?;
            }
            _ = keepalive.tick() => {
                tokio::time::timeout(Duration::from_secs(5), socket.send(Message::Text(r#"{"type":"KeepAlive"}"#.into()))).await.map_err(|_| "Deepgram keepalive timed out.")?.map_err(|_| "Deepgram keepalive failed.")?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn result(finalized: bool) -> Value {
        json!({"type":"Results","start":0.0,"duration":0.1,"is_final":finalized,
            "channel":{"alternatives":[{"transcript":"Hello.","words":[{"word":"hello","punctuated_word":"Hello.","start":0.0,"end":0.1,"speaker":0,"confidence":0.98}]}]}})
    }
    fn frame() -> LiveFrame {
        LiveFrame {
            source: "microphone".into(),
            pcm: vec![0; 3200],
            created_at: 0,
        }
    }
    #[test]
    fn word_times_and_speakers_are_bounded_and_keys_are_rejected() {
        let parsed = parse(&result(true), "test-secret").unwrap();
        assert_eq!(parsed.words[0].speaker, Some(0));
        assert_eq!(parsed.end_ms, 100);
        let mut invalid = result(true);
        invalid["channel"]["alternatives"][0]["words"][0]["end"] = json!(9);
        assert!(parse(&invalid, "").is_err());
        invalid = result(false);
        invalid["channel"]["alternatives"][0]["transcript"] = json!("test-secret");
        assert!(parse(&invalid, "test-secret").is_err());
    }

    #[tokio::test]
    async fn interim_precedes_boundary_finals_deduplicate_and_future_audio_waits() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("ws://{}/v1/listen", listener.local_addr().unwrap());
            let (sender, mut receiver) = mpsc::channel(10);
            let server = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
                assert!(matches!(
                    socket.next().await.unwrap().unwrap(),
                    Message::Binary(_)
                ));
                socket
                    .send(Message::Text(result(false).to_string().into()))
                    .await
                    .unwrap();
                loop {
                    if let Message::Text(text) = socket.next().await.unwrap().unwrap()
                        && text.contains("CloseStream")
                    {
                        break;
                    }
                }
                for response in [result(true), result(true), json!({"type":"Metadata"})] {
                    socket
                        .send(Message::Text(response.to_string().into()))
                        .await
                        .unwrap();
                }
            });
            let mut boundary = None;
            let mut interims = 0;
            let mut finals = 0;
            connection(
                &url,
                "test-secret",
                frame(),
                &mut receiver,
                &mut boundary,
                || {},
                |result| {
                    if result.is_final {
                        finals += 1;
                    } else {
                        interims += 1;
                        sender
                            .try_send(Command::Boundary {
                                job: 5,
                                ending: false,
                            })
                            .unwrap();
                        sender.try_send(Command::Audio(frame())).unwrap();
                    }
                    Ok(())
                },
            )
            .await
            .unwrap();
            assert_eq!((interims, finals), (1, 1));
            assert_eq!(boundary, Some((5, false)));
            assert!(matches!(receiver.try_recv(), Ok(Command::Audio(_))));
            server.await.unwrap();
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn disconnect_does_not_acknowledge_or_replay_audio() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/v1/listen", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            socket.next().await.unwrap().unwrap();
            socket.close(None).await.unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let (_sender, mut receiver) = mpsc::channel(10);
        assert!(
            connection(
                &url,
                "test-secret",
                frame(),
                &mut receiver,
                &mut None,
                || {},
                |_| Ok(())
            )
            .await
            .is_err()
        );
        server.await.unwrap();
    }
}
