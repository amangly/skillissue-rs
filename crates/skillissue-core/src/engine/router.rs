//! Runtime model routing from observed provider latency and health.

use super::{NativeEngine, lock};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
struct ModelPerformance {
    ewma_ms: f64,
    samples: u32,
    failures: u32,
}

#[derive(Default)]
pub(super) struct ModelPerformanceTable {
    entries: HashMap<(String, bool), ModelPerformance>,
}

const EWMA_ALPHA: f64 = 0.35;

impl ModelPerformanceTable {
    pub(super) fn load(path: &std::path::Path) -> Self {
        use std::io::{Read, Seek, SeekFrom};
        let mut table = Self::default();
        let Ok(mut file) = std::fs::File::open(path) else {
            return table;
        };
        let length = file.metadata().map(|m| m.len()).unwrap_or(0);
        let offset = length.saturating_sub(256 * 1024);
        if file.seek(SeekFrom::Start(offset)).is_err() {
            return table;
        }
        let mut bytes = Vec::new();
        if file.take(256 * 1024).read_to_end(&mut bytes).is_err() {
            return table;
        }
        let now = crate::models::now_ms();
        for (index, line) in bytes.split(|b| *b == b'\n').enumerate() {
            if offset > 0 && index == 0 {
                continue;
            }
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else {
                continue;
            };
            let Some(timestamp) = value["timestampMs"].as_u64() else {
                continue;
            };
            if timestamp > now
                || now - timestamp > 60 * 60 * 1000
                || value["hosted"] != true
                || value["ok"] != true
                || value["cancelled"] != false
            {
                continue;
            }
            let (Some(model), Some(screen), Some(ms)) = (
                value["model"].as_str(),
                value["screen"].as_bool(),
                value["firstTokenMs"].as_u64(),
            ) else {
                continue;
            };
            if model.len() <= 128 && ms <= 120_000 {
                table.observe(model, screen, Duration::from_millis(ms), true);
            }
        }
        table
    }

    fn observe(&mut self, model: &str, needs_image: bool, elapsed: Duration, success: bool) {
        let entry = self
            .entries
            .entry((model.to_owned(), needs_image))
            .or_insert(ModelPerformance {
                ewma_ms: elapsed.as_secs_f64() * 1_000.0,
                samples: 0,
                failures: 0,
            });
        if success {
            let sample = elapsed.as_secs_f64() * 1_000.0;
            entry.ewma_ms = if entry.samples == 0 {
                sample
            } else {
                EWMA_ALPHA.mul_add(sample, (1.0 - EWMA_ALPHA) * entry.ewma_ms)
            };
            entry.samples = entry.samples.saturating_add(1);
            entry.failures = entry.failures.saturating_sub(1);
        } else {
            entry.failures = entry.failures.saturating_add(1);
        }
    }

    fn score(&self, model: &str, needs_image: bool) -> Option<f64> {
        self.entries
            .get(&(model.to_owned(), needs_image))
            .map(|entry| {
                let penalty = 1.0 + (4.0 * f64::from(entry.failures.min(4)));
                entry.ewma_ms * penalty
            })
    }
}

impl NativeEngine {
    pub(super) fn fast_reasoning(
        model: &skillissue_domain::provider::ProviderModel,
    ) -> Option<String> {
        skillissue_domain::provider::REASONING_EFFORTS
            .iter()
            .find(|effort| model.supports_reasoning_effort(effort))
            .map(|effort| (*effort).to_owned())
    }

    pub(super) fn route_model(
        &self,
        settings: &skillissue_domain::models::Settings,
        needs_image: bool,
    ) -> Option<String> {
        if !settings.is_palepale() {
            return None;
        }
        let models = lock(&self.published_models).ok()?;
        let preferred = "deepseek-v4.1-flash";
        if let Some(model) = models.iter().find(|model| {
            model.id == preferred
                && model.capabilities.streaming
                && (!needs_image || model.capabilities.image_input)
        }) {
            return Some(model.id.clone());
        }
        let eligible = models.iter().filter(|model| {
            model.capabilities.streaming && (!needs_image || model.capabilities.image_input)
        });
        let current = settings.provider_chat_model();
        let performance = lock(&self.model_performance).ok()?;
        let fastest = eligible
            .filter_map(|model| {
                performance
                    .score(&model.id, needs_image)
                    .map(|score| (score, model))
            })
            .min_by(|(left, _), (right, _)| left.total_cmp(right))
            .map(|(_, model)| model.id.clone());
        fastest.or_else(|| {
            models
                .iter()
                .find(|model| {
                    model.id == current
                        && model.capabilities.streaming
                        && (!needs_image || model.capabilities.image_input)
                })
                .or_else(|| {
                    models.iter().find(|model| {
                        model.capabilities.streaming
                            && (!needs_image || model.capabilities.image_input)
                    })
                })
                .map(|model| model.id.clone())
        })
    }

    pub(super) fn observe_model(
        &self,
        model: &str,
        needs_image: bool,
        elapsed: Duration,
        success: bool,
    ) {
        if let Ok(mut performance) = self.model_performance.lock() {
            performance.observe(model, needs_image, elapsed, success);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ModelPerformanceTable;
    use std::time::Duration;

    #[test]
    fn fast_reasoning_overrides_high_server_default_with_lowest_supported_effort() {
        use skillissue_domain::provider::{ModelCapabilities, ProviderModel};
        let mut model = ProviderModel {
            capabilities: ModelCapabilities {
                reasoning: true,
                ..Default::default()
            },
            reasoning_efforts: vec!["high".into(), "low".into(), "none".into()],
            default_reasoning_effort: Some("high".into()),
            ..Default::default()
        };
        assert_eq!(
            super::NativeEngine::fast_reasoning(&model).as_deref(),
            Some("none")
        );
        model.reasoning_efforts = vec!["high".into(), "low".into()];
        assert_eq!(
            super::NativeEngine::fast_reasoning(&model).as_deref(),
            Some("low")
        );
        model.capabilities.reasoning = false;
        assert_eq!(super::NativeEngine::fast_reasoning(&model), None);
    }

    #[test]
    fn restores_only_recent_successful_hosted_timings() {
        let path =
            std::env::temp_dir().join(format!("skillissue-routing-{}.log", uuid::Uuid::new_v4()));
        let now = crate::models::now_ms();
        let valid = serde_json::json!({
            "timestampMs": now, "hosted": true, "ok": true, "cancelled": false,
            "model": "valid", "screen": true, "firstTokenMs": 125
        });
        let mut records = vec![valid.clone()];
        for (model, key, value) in [
            ("stale", "timestampMs", serde_json::json!(now - 3_600_001)),
            ("byok", "hosted", serde_json::json!(false)),
            ("cancelled", "cancelled", serde_json::json!(true)),
            ("failed", "ok", serde_json::json!(false)),
        ] {
            let mut record = valid.clone();
            record["model"] = serde_json::json!(model);
            record[key] = value;
            records.push(record);
        }
        std::fs::write(
            &path,
            records
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .unwrap();
        let table = ModelPerformanceTable::load(&path);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(table.entries.len(), 1);
        assert_eq!(table.score("valid", true), Some(125.0));
        assert_eq!(table.score("valid", false), None);
    }

    #[test]
    fn ewma_prefers_fast_model_and_penalizes_failures() {
        let mut table = ModelPerformanceTable::default();
        table.observe("slow", false, Duration::from_millis(400), true);
        table.observe("fast", false, Duration::from_millis(100), true);
        assert!(table.score("fast", false) < table.score("slow", false));
        table.observe("fast", false, Duration::from_millis(1), false);
        assert!(table.score("fast", false) > table.score("slow", false));
    }

    #[test]
    fn screen_latency_does_not_change_text_ranking() {
        let mut table = ModelPerformanceTable::default();
        table.observe("model", false, Duration::from_millis(100), true);
        table.observe("model", true, Duration::from_millis(2000), true);
        assert_eq!(table.score("model", false), Some(100.0));
        assert_eq!(table.score("model", true), Some(2000.0));
        assert_eq!(table.score("unmeasured", false), None);
    }
}
