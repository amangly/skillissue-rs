//! Explicit paid evaluation using supplied fixture audio, never capture/history.
use super::*;
use crate::audio::LiveFrame;
use tokio::sync::mpsc;

pub async fn benchmark_live_audio(
    settings: &Settings,
    key: &str,
    wav: Vec<u8>,
    reference: Option<&str>,
) -> Result<Value, String> {
    let url = live::url(settings, key)?;
    let pcm = super::pcm(wav)?;
    let audio_ms = pcm.len() as u64 / 32;
    let (sender, mut receiver) = mpsc::channel(100);
    let started = tokio::time::Instant::now();
    let producer = async move {
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        for chunk in pcm.chunks(3200) {
            interval.tick().await;
            sender
                .send(live::Command::Audio(LiveFrame {
                    source: "fixture".into(),
                    pcm: chunk.to_vec(),
                    created_at: 0,
                }))
                .await
                .map_err(|_| "Fixture connection closed.")?;
        }
        sender
            .send(live::Command::Boundary {
                job: 1,
                ending: true,
            })
            .await
            .map_err(|_| "Fixture connection closed.")?;
        Ok::<(), String>(())
    };
    let consumer = async {
        let first = match receiver.recv().await {
            Some(live::Command::Audio(frame)) => frame,
            _ => return Err("Fixture audio is empty.".into()),
        };
        let mut transcript = String::new();
        let mut interims = 0;
        let mut first_interim_ms = None;
        let mut lags = Vec::new();
        live::connection(
            &url,
            key,
            first,
            &mut receiver,
            &mut None,
            || {},
            |result| {
                if !result.text.is_empty() {
                    if result.is_final {
                        if !transcript.is_empty() {
                            transcript.push(' ');
                        }
                        transcript.push_str(&result.text);
                        lags.push(
                            (started.elapsed().as_millis() as u64).saturating_sub(result.end_ms),
                        );
                    } else {
                        interims += 1;
                        first_interim_ms.get_or_insert(started.elapsed().as_millis() as u64);
                    }
                }
                Ok(())
            },
        )
        .await?;
        lags.sort_unstable();
        let percentile = |percent: usize| {
            lags.get((lags.len() * percent).div_ceil(100).saturating_sub(1))
                .copied()
        };
        Ok(serde_json::json!({
            "audioMs":audio_ms, "totalMs":started.elapsed().as_millis(),
            "firstInterimMs":first_interim_ms,"interimResults":interims,"finalResults":lags.len(),
            "finalLagP50Ms":percentile(50),"finalLagP95Ms":percentile(95),
            "wordErrorRate":reference.and_then(|reference| word_error_rate(reference, &transcript)),
            "transcript":transcript,
            "latencyDefinition":"Elapsed since paced fixture start minus provider segment end; includes connection startup.",
        }))
    };
    let ((), report) = tokio::time::timeout(Duration::from_secs(75), async {
        tokio::try_join!(producer, consumer)
    })
    .await
    .map_err(|_| "Fixture benchmark timed out; audio was not replayed.")??;
    Ok(report)
}

fn word_error_rate(reference: &str, transcript: &str) -> Option<f64> {
    let words = |text: &str| {
        text.split_whitespace()
            .map(|word| {
                word.chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect::<String>()
                    .to_lowercase()
            })
            .filter(|word| !word.is_empty())
            .collect::<Vec<_>>()
    };
    let expected = words(reference);
    let actual = words(transcript);
    if expected.is_empty() || expected.len() > 4096 || actual.len() > 4096 {
        return None;
    }
    let mut previous: Vec<_> = (0..=actual.len()).collect();
    for (i, a) in expected.iter().enumerate() {
        let mut row = vec![i + 1; actual.len() + 1];
        for (j, b) in actual.iter().enumerate() {
            row[j + 1] = (previous[j] + usize::from(a != b))
                .min(previous[j + 1] + 1)
                .min(row[j] + 1);
        }
        previous = row;
    }
    Some(previous[actual.len()] as f64 / expected.len() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn errors_count_insertions_deletions_and_substitutions() {
        assert_eq!(word_error_rate("Hello, WORLD.", "hello world"), Some(0.0));
        assert_eq!(
            word_error_rate("one two three", "one four"),
            Some(2.0 / 3.0)
        );
        assert_eq!(word_error_rate("", "anything"), None);
    }
}
