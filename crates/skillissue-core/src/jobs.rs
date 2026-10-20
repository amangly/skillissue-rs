// Rust | jobs.rs | Tokio | bounded transcription completion tracking
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TranscriptionState {
    Queued,
    Transcribing,
    Ready,
    NoSpeech,
    Failed,
}

impl TranscriptionState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::Transcribing => "Transcribing",
            Self::Ready => "Saved",
            Self::NoSpeech => "No speech returned",
            Self::Failed => "Could not transcribe",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionProgress {
    pub id: u64,
    pub source: String,
    pub created_at: u64,
    pub state: TranscriptionState,
}

#[derive(Default)]
pub struct Jobs {
    submitted: u64,
    pending: BTreeSet<u64>,
    progress: VecDeque<TranscriptionProgress>,
}

impl Jobs {
    pub fn submit(&mut self) -> u64 {
        self.submitted += 1;
        self.pending.insert(self.submitted);
        self.submitted
    }

    pub fn complete(&mut self, id: u64) {
        self.pending.remove(&id);
    }
    pub fn submit_segment(&mut self, source: String, created_at: u64) -> u64 {
        let id = self.submit();
        self.progress.push_back(TranscriptionProgress {
            id,
            source,
            created_at,
            state: TranscriptionState::Queued,
        });
        self.prune();
        id
    }
    pub fn transcribing(&mut self, id: u64) {
        if let Some(job) = self.progress.iter_mut().find(|job| job.id == id) {
            job.state = TranscriptionState::Transcribing;
        }
    }
    pub fn finish(&mut self, id: u64, state: TranscriptionState) {
        if let Some(job) = self.progress.iter_mut().find(|job| job.id == id) {
            job.state = state;
        }
        self.complete(id);
        self.prune();
    }
    fn prune(&mut self) {
        // Keep every accepted pending job and at most 32 completed jobs.
        while self
            .progress
            .iter()
            .filter(|job| !self.pending.contains(&job.id))
            .count()
            > 32
        {
            if let Some(index) = self
                .progress
                .iter()
                .position(|job| !self.pending.contains(&job.id))
            {
                self.progress.remove(index);
            }
        }
    }
    pub fn progress(&self) -> Vec<TranscriptionProgress> {
        self.progress.iter().cloned().collect()
    }
    pub fn clear_progress(&mut self) {
        self.progress.clear();
    }
    pub fn watermark(&self) -> u64 {
        self.submitted
    }
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
    pub fn pending_through(&self, watermark: u64) -> usize {
        self.pending.range(..=watermark).count()
    }
    pub fn finished_through(&self, watermark: u64) -> bool {
        self.pending.first().is_none_or(|id| *id > watermark)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_for_earlier_jobs_but_not_future_speech() {
        let mut jobs = Jobs::default();
        let microphone = jobs.submit();
        let system = jobs.submit();
        let watermark = jobs.watermark();
        let future = jobs.submit();
        jobs.complete(system);
        assert!(!jobs.finished_through(watermark));
        jobs.complete(microphone);
        assert!(jobs.finished_through(watermark));
        assert!(!jobs.finished_through(future));
        jobs.complete(future);
        assert!(jobs.finished_through(future));
    }

    #[test]
    fn progress_preserves_pending_jobs_and_bounds_completed_history() {
        let mut jobs = Jobs::default();
        let pending = jobs.submit_segment("microphone".into(), 1);
        jobs.transcribing(pending);
        for time in 2..100 {
            let job = jobs.submit_segment("system".into(), time);
            jobs.finish(job, TranscriptionState::Ready);
        }
        assert_eq!(jobs.progress().len(), 33);
        assert_eq!(jobs.progress()[0].state, TranscriptionState::Transcribing);
        jobs.finish(pending, TranscriptionState::Failed);
        assert_eq!(jobs.pending_count(), 0);
        assert_eq!(jobs.progress().len(), 32);
        jobs.clear_progress();
        assert!(jobs.progress().is_empty());
        assert_eq!(jobs.watermark(), 99);
    }
}
