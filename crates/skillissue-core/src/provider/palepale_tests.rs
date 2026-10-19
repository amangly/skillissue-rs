//! Hosted provider contracts exercised without contacting the hosted service.

use super::*;
use crate::provider::{
    http::endpoint,
    palepale::{fetch_models, parse_models},
};
use skillissue_domain::{
    models::{PalepaleSettings, ProviderMode},
    provider::PALEPALE_BASE_URL,
};

fn hosted_settings() -> Settings {
    Settings {
        provider_mode: ProviderMode::Palepale,
        palepale: PalepaleSettings {
            chat_model: "published-model".into(),
            transcription_model: "published-model".into(),
            ..Default::default()
        },
        api_base_url: "http://127.0.0.1:1/wrong-provider".into(),
        chat_model: "byok-model-must-not-be-used".into(),
        ..Settings::default()
    }
}

#[test]
fn palepale_chat_uses_text_content_and_a_bounded_metered_stream() {
    let settings = hosted_settings();
    let history = [ChatMessage {
        role: "assistant".into(),
        content: "Earlier answer".into(),
    }];
    let body = assist_body(
        &settings,
        "Current question",
        "Captured words",
        &history,
        None,
    )
    .unwrap();
    assert_eq!(body["model"], "published-model");
    assert_eq!(body["stream"], true);
    assert_eq!(body["stream_options"], json!({"include_usage":true}));
    assert_eq!(body["max_completion_tokens"], 8192);
    assert!(body.get("reasoning_effort").is_none());
    assert!(
        body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|message| message["content"].is_string())
    );
    assert_eq!(
        body["messages"][1],
        json!({"role":"assistant", "content":"Earlier answer"})
    );
    assert_eq!(
        body["messages"][2],
        json!({
            "role":"user", "content":"Recent transcript (captured context):\nCaptured words\n\nUser request:\nCurrent question"
        })
    );
    assert_eq!(
        endpoint(&settings, "hosted-token", "chat/completions").unwrap(),
        format!("{PALEPALE_BASE_URL}/chat/completions")
    );
    assert!(!body.to_string().contains("byok-model-must-not-be-used"));

    let byok = assist_body(&Settings::default(), "Question", "", &[], None).unwrap();
    assert!(byok["messages"][1]["content"].is_array());
    assert!(byok.get("max_completion_tokens").is_none());
    assert!(byok.get("stream_options").is_none());
}

#[test]
fn hosted_reasoning_is_explicit_and_never_aliased_or_added_to_byok() {
    let mut settings = hosted_settings();
    for effort in skillissue_domain::provider::REASONING_EFFORTS {
        settings.palepale.reasoning_effort = Some(effort.into());
        let body = assist_body(&settings, "Question", "", &[], None).unwrap();
        assert_eq!(body["reasoning_effort"], effort);
    }
    settings.provider_mode = ProviderMode::Byok;
    let body = assist_body(&settings, "Question", "", &[], None).unwrap();
    assert!(body.get("reasoning_effort").is_none());
    settings.provider_mode = ProviderMode::Palepale;
    settings.palepale.reasoning_effort = Some("automatic".into());
    assert!(
        assist_body(&settings, "Question", "", &[], None)
            .unwrap_err()
            .contains("reasoning effort")
    );
}

#[tokio::test]
async fn palepale_unsupported_capabilities_fail_before_network_or_audio_upload() {
    let mut settings = hosted_settings();
    for (smart, screenshot, expected) in [(
        false,
        Some("data:image/png;base64,AAAA"),
        "valid inline PNG or JPEG",
    )] {
        settings.smart_mode = smart;
        let error = stream_assist(
            &Client::new(),
            &settings,
            "",
            "Question",
            "",
            &[],
            screenshot,
            CancellationToken::new(),
            |_| {},
        )
        .await
        .unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
    let error = transcribe(&Client::new(), &settings, "", vec![])
        .await
        .unwrap_err();
    assert!(error.contains("complete PCM16 WAV"));
}

#[test]
fn palepale_notes_keep_the_selected_hosted_model_and_do_not_enable_search() {
    let settings = hosted_settings();
    let (transcript, messages) = note_evidence();
    let (job, prompt) = notes_request(&settings, &transcript, &messages, &[]).unwrap();
    let body = assist_body(&job, &prompt, "", &[], None).unwrap();
    assert_eq!(body["model"], "published-model");
    assert!(!job.smart_mode);
    assert!(!job.include_screenshot);
    assert!(body["messages"][1]["content"].is_string());
}

#[tokio::test]
async fn palepale_catalogue_is_public_and_keeps_only_safe_streaming_models() {
    let response = json!({"object":"list", "data":[
        {"id":"provider/model-1", "display_name":"Available model", "capabilities":{"streaming":true,"reasoning":true,"tool_calling":true,"temperature":false}, "reasoning_efforts":["low","medium","high"], "default_reasoning_effort":"high"},
        {"id":"", "capabilities":{"streaming":true}},
        {"id":"unsafe\nmodel", "capabilities":{"streaming":true}},
        {"id":"non-streaming", "capabilities":{"streaming":false}},
        {"id":"disabled", "available":false, "capabilities":{"streaming":true}},
        {"id":"unpublished", "published":false, "capabilities":{"streaming":true}},
        {"id":"x".repeat(129), "capabilities":{"streaming":true}},
        {"id":"unknown-capabilities"},
        {"id":"fallback-name", "display_name":"\n", "capabilities":{"streaming":true}}
    ]});
    let (settings, server) = local_response(
        "application/json",
        vec![serde_json::to_vec(&response).unwrap()],
    )
    .await;
    let models = fetch_models(
        &Client::new(),
        &format!("{}/models?app=skillissue-rs", settings.api_base_url),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "provider/model-1");
    assert_eq!(models[0].display_name, "Available model");
    assert!(models[0].capabilities.reasoning);
    assert!(models[0].capabilities.tool_calling);
    assert!(!models[0].capabilities.temperature);
    assert_eq!(models[0].reasoning_efforts, ["low", "medium", "high"]);
    assert_eq!(models[0].default_reasoning_effort.as_deref(), Some("high"));
    assert!(models[0].supports_reasoning_effort("high"));
    assert!(!models[0].supports_reasoning_effort("max"));
    assert_eq!(models[1].display_name, "fallback-name");
    assert!(models[1].reasoning_efforts.is_empty());
    assert!(models[1].default_reasoning_effort.is_none());
    let request = String::from_utf8(server.await.unwrap()).unwrap();
    assert!(request.starts_with("GET /v1/models?app=skillissue-rs HTTP/1.1\r\n"));
    assert!(!request.to_ascii_lowercase().contains("authorization:"));
    assert!(!request.to_ascii_lowercase().contains("cookie:"));
}

#[test]
fn catalog_reasoning_options_reject_contradictions_and_unknown_levels() {
    for (reasoning, efforts, default) in [
        (true, json!(["high", "high"]), json!("high")),
        (true, json!(["automatic"]), Value::Null),
        (true, json!(["high"]), json!("max")),
        (false, json!(["high"]), Value::Null),
        (false, json!([]), json!("high")),
        (true, json!(["high", 4]), Value::Null),
    ] {
        let bytes = serde_json::to_vec(&json!({"data":[{
            "id":"model", "capabilities":{"streaming":true,"reasoning":reasoning},
            "reasoning_efforts": efforts, "default_reasoning_effort": default
        }]}))
        .unwrap();
        assert!(parse_models(&bytes).is_err());
    }
}

#[test]
fn palepale_catalogue_rejects_duplicate_empty_malformed_and_oversized_results() {
    let model = json!({"id":"duplicate", "capabilities":{"streaming":true}});
    for response in [
        json!({"data":[]}),
        json!({"data":[model.clone(), model.clone()]}),
        json!({"data":[{"id":"incompatible"}]}),
        json!({"data":[{"id":123}]}),
        json!({"data": vec![model; 257]}),
        json!({"data":"not a list"}),
    ] {
        assert!(
            parse_models(&serde_json::to_vec(&response).unwrap()).is_err(),
            "{response}"
        );
    }
    assert!(
        parse_models(&vec![b' '; 512 * 1024 + 1])
            .unwrap_err()
            .contains("size limit")
    );
}

#[tokio::test]
async fn cancelled_catalogue_request_never_connects() {
    let token = CancellationToken::new();
    token.cancel();
    let error = fetch_models(&Client::new(), "http://127.0.0.1:1/v1/models", token)
        .await
        .unwrap_err();
    assert_eq!(error, "Request cancelled.");
}

#[tokio::test]
async fn done_without_an_explicit_finish_reason_does_not_complete_an_answer() {
    let (settings, server) = local_response(
        "text/event-stream",
        vec![
            b"data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\ndata: [DONE]\n\n"
                .to_vec(),
        ],
    )
    .await;
    let error = stream_assist(
        &Client::new(),
        &settings,
        "",
        "Question",
        "",
        &[],
        None,
        CancellationToken::new(),
        |_| {},
    )
    .await
    .unwrap_err();
    assert!(error.contains("before completion"));
    server.await.unwrap();
}

#[tokio::test]
async fn a_provider_error_after_finish_is_consumed_and_redacted() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"answer\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: {\"choices\":[],\"usage\":{\"total_tokens\":17}}\n\n",
        "event: error\ndata: {\"message\":\"Final metering failed for private-token\"}\n\n",
        "data: [DONE]\n\n"
    );
    let (settings, server) =
        local_response("text/event-stream", vec![body.as_bytes().to_vec()]).await;
    let error = stream_assist(
        &Client::new(),
        &settings,
        "private-token",
        "Question",
        "",
        &[],
        None,
        CancellationToken::new(),
        |_| {},
    )
    .await
    .unwrap_err();
    assert!(error.contains("Final metering failed"));
    assert!(error.contains("[redacted]"));
    assert!(!error.contains("private-token"));
    server.await.unwrap();
}

#[test]
fn fragmented_stream_tracks_final_usage_after_finish_without_requiring_it() {
    let bytes = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"你🙂\"},\"finish_reason\":\"stop\"}]}\r\n\r\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":8,\"completion_tokens\":9,\"total_tokens\":17}}\r\n\r\n",
        "data: [DONE]\r\n\r\n"
    ).as_bytes();
    let mut decoder = SseDecoder::default();
    let mut completion = Completion::default();
    let mut done = false;
    for byte in bytes {
        for event in decoder.push(&[*byte]).unwrap() {
            done |= completion.event(event, "", &|_| {}).unwrap();
        }
    }
    assert!(done);
    assert_eq!(completion.usage.as_ref().unwrap()["total_tokens"], 17);
    assert_eq!(completion.finish().unwrap(), "你🙂");
}

#[tokio::test]
async fn eof_during_a_trailing_frame_does_not_hide_it_after_finish() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"answer\"},\"finish_reason\":\"stop\"}]}\n\n",
        "event: error\ndata: {\"message\":\"Unfinished error frame\"}"
    );
    let (settings, server) =
        local_response("text/event-stream", vec![body.as_bytes().to_vec()]).await;
    let error = stream_assist(
        &Client::new(),
        &settings,
        "",
        "Question",
        "",
        &[],
        None,
        CancellationToken::new(),
        |_| {},
    )
    .await
    .unwrap_err();
    assert!(error.contains("stream event was truncated"));
    server.await.unwrap();
}

fn decode_hosted_stream(stream: &str) -> Result<String, String> {
    let mut decoder = SseDecoder::default();
    let mut completion = Completion::with_usage_requirement(true);
    for byte in stream.as_bytes() {
        for event in decoder.push(&[*byte])? {
            if completion.event(event, "", &|_| {})? {
                return completion.finish();
            }
        }
    }
    decoder.finish()?;
    completion.finish()
}

#[test]
fn hosted_stream_requires_confirmed_usage_and_done_after_successful_finish() {
    let answer = "data: {\"choices\":[{\"delta\":{\"content\":\"Confirmed answer\"},\"finish_reason\":\"stop\"}]}\n\n";
    let usage = "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":8,\"completion_tokens\":9,\"total_tokens\":17}}\n\n";
    let done = "data: [DONE]\n\n";
    assert!(
        decode_hosted_stream(answer)
            .unwrap_err()
            .contains("final confirmation")
    );
    assert!(
        decode_hosted_stream(&format!("{answer}{done}"))
            .unwrap_err()
            .contains("final usage")
    );
    assert!(
        decode_hosted_stream(&format!("{answer}{usage}"))
            .unwrap_err()
            .contains("final confirmation")
    );
    assert_eq!(
        decode_hosted_stream(&format!("{answer}{usage}{done}")).unwrap(),
        "Confirmed answer"
    );
    assert!(
        decode_hosted_stream(&format!("{usage}{answer}{done}"))
            .unwrap_err()
            .contains("final usage")
    );

    let same_frame = "data: {\"choices\":[{\"delta\":{\"content\":\"Confirmed answer\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":0,\"completion_tokens\":0}}\n\ndata: [DONE]\n\n";
    assert_eq!(
        decode_hosted_stream(same_frame).unwrap(),
        "Confirmed answer"
    );
}

#[test]
fn hosted_usage_rejects_missing_negative_noninteger_and_inconsistent_counters() {
    for usage in [
        json!({}),
        json!({"prompt_tokens":1}),
        json!({"prompt_tokens":-1,"completion_tokens":2}),
        json!({"prompt_tokens":true,"completion_tokens":2}),
        json!({"prompt_tokens":1.5,"completion_tokens":2}),
        json!({"prompt_tokens":1,"completion_tokens":2,"total_tokens":4}),
        json!({"prompt_tokens":u64::MAX,"completion_tokens":1}),
        json!("not an object"),
    ] {
        let stream = format!(
            "data: {{\"choices\":[{{\"delta\":{{\"content\":\"Answer\"}},\"finish_reason\":\"stop\"}}]}}\n\ndata: {{\"choices\":[],\"usage\":{usage}}}\n\ndata: [DONE]\n\n"
        );
        assert!(
            decode_hosted_stream(&stream)
                .unwrap_err()
                .contains("valid final usage"),
            "{usage}"
        );
    }
}

#[test]
fn hosted_settlement_error_overrides_valid_usage_and_a_finished_answer() {
    let stream = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Answer\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":8,\"completion_tokens\":9}}\n\n",
        "data: {\"error\":{\"type\":\"usage_unknown\",\"message\":\"Usage settlement requires reconciliation.\"}}\n\n",
        "data: [DONE]\n\n"
    );
    assert!(
        decode_hosted_stream(stream)
            .unwrap_err()
            .contains("Usage settlement requires reconciliation")
    );
}

#[test]
fn hosted_json_fallback_requires_usage_and_an_explicit_successful_finish() {
    use crate::provider::completion::complete_json;
    for (finish, usage, succeeds) in [
        (
            json!("stop"),
            json!({"prompt_tokens":4,"completion_tokens":5,"total_tokens":9}),
            true,
        ),
        (
            json!("stop"),
            json!({"prompt_tokens":0,"completion_tokens":0}),
            true,
        ),
        (
            Value::Null,
            json!({"prompt_tokens":4,"completion_tokens":5}),
            false,
        ),
        (json!("stop"), Value::Null, false),
        (
            json!("stop"),
            json!({"prompt_tokens":4,"completion_tokens":5,"total_tokens":1}),
            false,
        ),
    ] {
        let bytes = serde_json::to_vec(&json!({"choices":[{"message":{"content":"Answer"},"finish_reason":finish}],"usage":usage})).unwrap();
        assert_eq!(complete_json(&bytes, "", &|_| {}, true).is_ok(), succeeds);
    }
    let byok =
        br#"{"choices":[{"message":{"content":"BYOK without usage"},"finish_reason":"stop"}]}"#;
    assert_eq!(
        complete_json(byok, "", &|_| {}, false).unwrap(),
        "BYOK without usage"
    );
}

#[tokio::test]
async fn byok_can_end_after_finish_with_a_benign_unterminated_comment_and_no_usage() {
    let body = "data: {\"choices\":[{\"delta\":{\"content\":\"BYOK answer\"},\"finish_reason\":\"stop\"}]}\n\n: keepalive";
    let (settings, server) =
        local_response("text/event-stream", vec![body.as_bytes().to_vec()]).await;
    let answer = stream_assist(
        &Client::new(),
        &settings,
        "",
        "Question",
        "",
        &[],
        None,
        CancellationToken::new(),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(answer, "BYOK answer");
    server.await.unwrap();
}

fn hosted_test_wav(seconds: u32, rate: u32, channels: u16) -> Vec<u8> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(
            &mut cursor,
            hound::WavSpec {
                channels,
                sample_rate: rate,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for _ in 0..seconds * rate * channels as u32 {
            writer.write_sample(0_i16).unwrap();
        }
        writer.finalize().unwrap();
    }
    cursor.into_inner()
}

fn hosted_test_png(width: u32, height: u32) -> String {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::new(width, height))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    format!(
        "data:image/png;base64,{}",
        STANDARD.encode(png.into_inner())
    )
}

#[test]
fn hosted_screenshots_are_inline_bounded_and_preserve_metering() {
    let settings = hosted_settings();
    let png = hosted_test_png(5, 3);
    let body = assist_body(&settings, "Describe the screen", "", &[], Some(&png)).unwrap();
    assert_eq!(
        body["messages"][1]["content"][0],
        json!({"type":"text", "text":"Describe the screen"})
    );
    assert_eq!(body["messages"][1]["content"][1]["image_url"]["url"], png);
    assert_eq!(body["stream_options"], json!({"include_usage":true}));
    assert_eq!(body["max_completion_tokens"], PALEPALE_OUTPUT_TOKENS);
    for invalid in [
        "https://example.com/image.png".to_owned(),
        png.replacen("image/png", "image/jpeg", 1),
        png.replacen("image/png", "image/webp", 1),
        hosted_test_png(4097, 1),
        format!(
            "data:image/png;base64,{}",
            "A".repeat(4 * 1024 * 1024 * 4 / 3 + 8)
        ),
    ] {
        assert!(assist_body(&settings, "Question", "", &[], Some(&invalid)).is_err());
    }
}

#[test]
fn hosted_media_capabilities_fail_closed_for_absent_stale_and_changed_models() {
    use crate::provider::palepale::require_media_model;
    let bytes = serde_json::to_vec(&json!({"data":[
        {"id":"vision", "capabilities":{"streaming":true,"image_input":true}},
        {"id":"audio", "capabilities":{"streaming":true,"audio_input":true}},
        {"id":"text", "capabilities":{"streaming":true}}
    ]}))
    .unwrap();
    let models = parse_models(&bytes).unwrap();
    assert!(require_media_model(&models, "vision", false).is_ok());
    assert!(require_media_model(&models, "audio", true).is_ok());
    for (model, audio) in [
        ("vision", true),
        ("audio", false),
        ("text", true),
        ("text", false),
        ("removed", false),
    ] {
        assert!(require_media_model(&models, model, audio).is_err());
    }
    assert!(require_media_model(&[], "vision", false).is_err());
    let mut stopped_audio = models[1].clone();
    stopped_audio.capabilities.streaming = false;
    assert!(require_media_model(&[stopped_audio], "audio", true).is_err());
}

#[test]
fn expired_or_cleared_media_catalog_cannot_authorize_an_audio_model() {
    use crate::provider::palepale::{cached_media_models, require_media_model};
    use std::{
        sync::Mutex,
        time::{Duration, Instant},
    };
    let model = skillissue_domain::provider::ProviderModel {
        id: "audio".into(),
        capabilities: skillissue_domain::provider::ModelCapabilities {
            streaming: true,
            audio_input: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let cache = Mutex::new(Some((Instant::now(), vec![model.clone()])));
    assert!(require_media_model(&cached_media_models(&cache).unwrap(), "audio", true).is_ok());
    *cache.lock().unwrap() = Some((Instant::now() - Duration::from_secs(31), vec![model]));
    assert!(cached_media_models(&cache).is_none());
    *cache.lock().unwrap() = None;
    assert!(cached_media_models(&cache).is_none());
    assert!(require_media_model(&[], "audio", true).is_err());
}

#[test]
fn hosted_transcription_uses_independent_audio_model_and_only_audio_text_parts() {
    use crate::provider::transcription::hosted_audio_body;
    use base64::{Engine, engine::general_purpose::STANDARD};
    let mut settings = hosted_settings();
    settings.palepale.chat_model = "glm-vision".into();
    settings.language = "zh-CN".into();
    settings.smart_mode = true;
    settings.transcription_model = "byok-speech-must-not-be-used".into();
    settings.palepale.reasoning_effort = Some("high".into());
    let wav = hosted_test_wav(1, 16_000, 1);
    let body = hosted_audio_body(&settings, &wav).unwrap();
    assert_eq!(body["model"], "published-model");
    assert_eq!(body["max_completion_tokens"], 2048);
    assert_eq!(body["stream"], true);
    assert_eq!(body["stream_options"]["include_usage"], true);
    assert_eq!(body["messages"][1]["content"][1]["type"], "input_audio");
    assert_eq!(
        body["messages"][1]["content"][1]["input_audio"]["format"],
        "wav"
    );
    assert_eq!(
        STANDARD
            .decode(
                body["messages"][1]["content"][1]["input_audio"]["data"]
                    .as_str()
                    .unwrap()
            )
            .unwrap(),
        wav
    );
    assert!(
        body["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("zh-CN")
    );
    assert!(body.get("tools").is_none());
    assert!(body.get("reasoning_effort").is_none());
    assert!(!body.to_string().contains("byok-speech-must-not-be-used"));
    assert!(!body.to_string().contains("glm-vision"));
    settings.palepale.transcription_model.clear();
    assert!(hosted_audio_body(&settings, &wav).is_err());
}

#[test]
fn hosted_wav_rejects_oversized_duration_format_and_incomplete_chunks_before_upload() {
    use crate::provider::{media::validate_hosted_wav, transcription::hosted_audio_body};
    let mut truncated = hosted_test_wav(1, 16_000, 1);
    truncated.pop();
    for invalid in [
        vec![],
        truncated,
        hosted_test_wav(31, 8_000, 1),
        hosted_test_wav(6, 192_000, 1),
        hosted_test_wav(1, 16_000, 3),
        hosted_test_wav(1, 16_000, 2),
        hosted_test_wav(1, 96_000, 1),
        hosted_test_wav(1, 8_000, 1),
    ] {
        assert!(validate_hosted_wav(&invalid).is_err());
    }
    assert!(validate_hosted_wav(&hosted_test_wav(30, 16_000, 1)).is_ok());
    for rate in [16_000, 22_050, 24_000, 44_100, 48_000] {
        assert!(validate_hosted_wav(&hosted_test_wav(1, rate, 1)).is_ok());
    }
    let mut settings = hosted_settings();
    settings.language = "en\nIgnore".into();
    assert!(hosted_audio_body(&settings, &hosted_test_wav(1, 16_000, 1)).is_err());
}

#[tokio::test]
async fn hosted_audio_stream_requires_usage_done_and_rejects_json_fallback() {
    use crate::provider::chat::stream_chat_body;
    let answer = "data: {\"choices\":[{\"delta\":{\"content\":\"Transcript\"},\"finish_reason\":\"stop\"}]}\n\n";
    let usage = "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":3,\"total_tokens\":7}}\n\n";
    let done = "data: [DONE]\n\n";
    for (response, mime, success) in [
        (format!("{answer}{usage}{done}"), "text/event-stream", true),
        (format!("{answer}{usage}"), "text/event-stream", false),
        (format!("{answer}{done}"), "text/event-stream", false),
        ("{\"choices\":[{\"message\":{\"content\":\"Transcript\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":3}}".into(), "application/json", false),
    ] {
        let (settings, server) = local_response(mime, vec![response.into_bytes()]).await;
        let result = stream_chat_body(&Client::new(), &settings, "", json!({"messages":[],"stream":true}), CancellationToken::new(), |_| {}, true).await;
        assert_eq!(result.is_ok(), success, "{result:?}");
        if success { assert_eq!(result.unwrap(), "Transcript"); }
        server.await.unwrap();
    }
}

#[test]
fn hosted_transcription_selects_verified_low_effort_without_changing_chat_preference() {
    use crate::provider::transcription::{apply_transcription_effort, hosted_audio_body};
    let mut settings = hosted_settings();
    settings.palepale.reasoning_effort = Some("high".into());
    for (options, expected) in [
        (json!(["low", "medium", "high"]), "low"),
        (json!(["high", "minimal"]), "minimal"),
        (json!(["high", "none", "low"]), "low"),
    ] {
        let models = parse_models(&serde_json::to_vec(&json!({"data":[{
            "id":"published-model", "capabilities":{"streaming":true,"audio_input":true,"reasoning":true},
            "reasoning_efforts":options
        }]})).unwrap()).unwrap();
        let mut body = hosted_audio_body(&settings, &hosted_test_wav(1, 16_000, 1)).unwrap();
        apply_transcription_effort(&mut body, &models[0]).unwrap();
        assert_eq!(body["reasoning_effort"], expected);
        assert!(models[0].supports_reasoning_effort(expected));
        assert_eq!(settings.palepale.reasoning_effort.as_deref(), Some("high"));
    }
    let mut model = skillissue_domain::provider::ProviderModel {
        id: "published-model".into(),
        capabilities: skillissue_domain::provider::ModelCapabilities {
            streaming: true,
            audio_input: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut body = hosted_audio_body(&settings, &hosted_test_wav(1, 16_000, 1)).unwrap();
    apply_transcription_effort(&mut body, &model).unwrap();
    assert!(body.get("reasoning_effort").is_none());
    model.capabilities.reasoning = true;
    assert!(apply_transcription_effort(&mut body, &model).is_err());
    model.reasoning_efforts = vec!["automatic".into()];
    assert!(apply_transcription_effort(&mut body, &model).is_err());
    model.id = "different-model".into();
    model.reasoning_efforts = vec!["low".into()];
    assert!(apply_transcription_effort(&mut body, &model).is_err());
}
