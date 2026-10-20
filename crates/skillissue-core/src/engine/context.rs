//! Deterministic context selection; no extra model round trip is required.

use super::{NativeEngine, lock};
use crate::models::Settings;
use std::future::Future;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub(super) const TRANSCRIPT_BYTES: usize = 32 * 1024;
pub(super) const HISTORY_BYTES: usize = 32 * 1024;

/// Input is newest first; output is chronological. Labels and separators count
/// toward the budget. Only the oldest retained entry may be shortened.
pub(super) fn bounded_recent(
    entries: impl Iterator<Item = (String, String)>,
    mut budget: usize,
) -> Vec<(String, String)> {
    const OMITTED: &str = "[Earlier content omitted] ";
    let mut selected = Vec::new();
    for (label, content) in entries {
        let overhead = label.len() + 2;
        if overhead >= budget {
            break;
        }
        let available = budget - overhead;
        if content.len() <= available {
            budget -= overhead + content.len();
            selected.push((label, content));
        } else {
            if available > OMITTED.len() {
                let mut start = content.len() - (available - OMITTED.len());
                while !content.is_char_boundary(start) {
                    start += 1;
                }
                if start < content.len() {
                    selected.push((label, format!("{OMITTED}{}", &content[start..])));
                }
            }
            break;
        }
    }
    selected.reverse();
    selected
}

#[derive(Debug)]
pub(super) struct AudioWait {
    pub watermark: u64,
    pub pending: usize,
    pub elapsed: Duration,
}

impl NativeEngine {
    pub(super) async fn wait_for_transcript(
        &self,
        watermark: u64,
        budget: Duration,
    ) -> Result<AudioWait, String> {
        let started = tokio::time::Instant::now();
        let deadline = started + budget;
        loop {
            let notified = self.jobs_done.notified();
            let pending = lock(&self.jobs)?.pending_through(watermark);
            if pending == 0 || tokio::time::Instant::now() >= deadline {
                return Ok(AudioWait {
                    watermark,
                    pending,
                    elapsed: started.elapsed(),
                });
            }
            // Recheck on timeout too: completion can race the deadline.
            let _ = tokio::time::timeout_at(deadline, notified).await;
        }
    }
}

pub(super) struct ContextPlan {
    pub references: Vec<String>,
    pub transcript_entries: usize,
    pub history_messages: usize,
    pub needs_image: bool,
}

impl ContextPlan {
    pub fn new(settings: &Settings, has_screen: bool) -> Self {
        let references = settings
            .modes
            .iter()
            .find(|mode| mode.id == settings.active_mode_id)
            .map(|mode| mode.files.iter().map(|file| file.id.clone()).collect())
            .unwrap_or_default();
        Self {
            references,
            transcript_entries: 80,
            history_messages: 16,
            needs_image: has_screen,
        }
    }
}

pub(super) async fn collect<A, B, U, T>(
    cancel: &CancellationToken,
    audio: A,
    documents: B,
) -> Result<(U, T), String>
where
    A: Future<Output = Result<U, String>>,
    B: Future<Output = Result<T, String>>,
{
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err("Request cancelled.".into()),
        result = async {
            let documents = async {
                tokio::time::timeout(Duration::from_secs(10), documents)
                    .await
                    .map_err(|_| "Reference search timed out. Your question has not been sent to the answer provider.".to_string())?
            };
            tokio::try_join!(audio, documents)
        } => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_budget_keeps_recent_context_and_labels() {
        let entries = vec![
            ("user".into(), "new".into()),
            ("assistant".into(), "x".repeat(100)),
            ("user".into(), "old".into()),
        ];
        let output = bounded_recent(entries.into_iter(), 64);
        assert_eq!(output.last().unwrap().1, "new");
        assert_eq!(output[0].0, "assistant");
        assert!(output[0].1.starts_with("[Earlier content omitted]"));
        assert!(
            output
                .iter()
                .map(|(a, b)| a.len() + b.len() + 2)
                .sum::<usize>()
                <= 64
        );
        assert_eq!(output.len(), 2);
    }

    #[test]
    fn truncation_preserves_unicode_for_every_budget() {
        for budget in 0..100 {
            let output =
                bounded_recent(std::iter::once(("me".into(), "你好🙂".repeat(50))), budget);
            assert!(
                output
                    .iter()
                    .map(|(a, b)| a.len() + b.len() + 2)
                    .sum::<usize>()
                    <= budget
            );
            if let Some((_, text)) = output.first() {
                assert!(text.ends_with('🙂'));
            }
        }
    }

    #[tokio::test]
    async fn cancellation_interrupts_pending_collectors() {
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        let result = collect(
            &cancel,
            async move {
                trigger.cancel();
                std::future::pending::<Result<(), String>>().await
            },
            std::future::pending::<Result<(), String>>(),
        )
        .await;
        assert_eq!(result.unwrap_err(), "Request cancelled.");
    }

    #[tokio::test]
    async fn collectors_progress_concurrently() {
        let (send, receive) = tokio::sync::oneshot::channel();
        let result = collect(
            &CancellationToken::new(),
            async { receive.await.map_err(|error| error.to_string()) },
            async {
                send.send(()).unwrap();
                Ok("document context")
            },
        )
        .await
        .unwrap();
        assert_eq!(result, ((), "document context"));
    }

    #[tokio::test]
    async fn collector_failure_does_not_wait_for_other_collector() {
        let result = collect(
            &CancellationToken::new(),
            async { Err::<(), String>("Audio flush failed".into()) },
            std::future::pending::<Result<(), String>>(),
        )
        .await;
        assert_eq!(result.unwrap_err(), "Audio flush failed");
    }

    #[test]
    fn empty_plan_does_not_enable_screen_or_documents() {
        let plan = ContextPlan::new(&Settings::default(), false);
        assert!(!plan.needs_image);
        assert!(plan.references.is_empty());
    }
}
