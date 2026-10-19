//! Bounded aggregation of the redaction-safe local timing logs.

use super::NativeEngine;
use serde::Deserialize;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

const MAX_LOG_BYTES: u64 = 256 * 1024;

fn recent_lines(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let offset = file.metadata().ok()?.len().saturating_sub(MAX_LOG_BYTES);
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_LOG_BYTES).read_to_end(&mut bytes).ok()?;
    let start = if offset > 0 {
        bytes.iter().position(|byte| *byte == b'\n')? + 1
    } else {
        0
    };
    // Ignore an incomplete final record while a writer is appending.
    let end = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(start, |index| index + 1);
    Some(String::from_utf8_lossy(&bytes[start..end]).into_owned())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LatencySummary {
    pub requests: u64,
    pub transcriptions: u64,
    pub capture_ms: Option<u64>,
    pub transcription_ms: Option<u64>,
    pub first_token_ms: Option<u64>,
    pub display_ms: Option<u64>,
}

#[derive(Deserialize)]
struct UiEvent {
    #[serde(rename = "captureMs")]
    capture_ms: Option<u64>,
    #[serde(rename = "displayMs")]
    display_ms: Option<u64>,
}

#[derive(Deserialize)]
struct ProviderEvent {
    #[serde(rename = "firstTokenMs")]
    first_token_ms: Option<u64>,
}

#[derive(Deserialize)]
struct TranscriptionEvent {
    #[serde(rename = "totalMs")]
    total_ms: Option<u64>,
}

fn average(path: &Path, field: impl Fn(&str) -> Option<u64>) -> Option<u64> {
    let contents = recent_lines(path)?;
    let mut total = 0u128;
    let mut count = 0u128;
    for line in contents.lines().rev().take(256) {
        if let Some(value) = field(line) {
            total += value as u128;
            count += 1;
        }
    }
    (count > 0).then(|| (total / count) as u64)
}

fn count(path: &Path) -> u64 {
    recent_lines(path)
        .map(|contents| {
            contents
                .lines()
                .rev()
                .take(256)
                .filter(|line| {
                    serde_json::from_str::<serde_json::Value>(line)
                        .is_ok_and(|value| value.is_object())
                })
                .count() as u64
        })
        .unwrap_or_default()
}

impl NativeEngine {
    /// Return averages from the latest bounded window of local telemetry.
    pub fn latency_summary(&self) -> LatencySummary {
        let ui = self.directory.join("ui-latency.log");
        let provider = self.directory.join("provider-requests.log");
        let transcription = self.directory.join("transcription-latency.log");
        LatencySummary {
            requests: count(&provider),
            transcriptions: count(&transcription),
            capture_ms: average(&ui, |line| {
                serde_json::from_str::<UiEvent>(line).ok()?.capture_ms
            }),
            display_ms: average(&ui, |line| {
                serde_json::from_str::<UiEvent>(line).ok()?.display_ms
            }),
            first_token_ms: average(&provider, |line| {
                serde_json::from_str::<ProviderEvent>(line)
                    .ok()?
                    .first_token_ms
            }),
            transcription_ms: average(&transcription, |line| {
                serde_json::from_str::<TranscriptionEvent>(line)
                    .ok()?
                    .total_ms
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::average;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn average_uses_recent_valid_events_and_ignores_malformed_lines() {
        let path = std::env::temp_dir().join(format!(
            "skillissue-latency-{}.log",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, "bad\n{\"value\":10}\n{\"value\":20}\n").unwrap();
        assert_eq!(
            average(&path, |line| serde_json::from_str::<serde_json::Value>(
                line
            )
            .ok()?
            .get("value")?
            .as_u64()),
            Some(15)
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn empty_invalid_and_partial_logs_have_no_average() {
        let path = std::env::temp_dir().join(format!("latency-{}", uuid::Uuid::new_v4()));
        for contents in ["", "bad\n", "{\"value\":20}"] {
            fs::write(&path, contents).unwrap();
            assert_eq!(
                average(&path, |line| serde_json::from_str::<serde_json::Value>(
                    line
                )
                .ok()?["value"]
                    .as_u64()),
                None
            );
        }
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn reader_bounds_bytes_and_discards_cut_records() {
        let path = std::env::temp_dir().join(format!("latency-{}", uuid::Uuid::new_v4()));
        let contents = format!(
            "{}\n{{\"value\":7}}\npartial",
            "x".repeat(super::MAX_LOG_BYTES as usize)
        );
        fs::write(&path, contents).unwrap();
        assert_eq!(super::recent_lines(&path).unwrap(), "{\"value\":7}\n");
        fs::remove_file(path).unwrap();
    }
}
