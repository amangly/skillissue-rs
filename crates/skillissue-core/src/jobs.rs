// Rust | jobs.rs | Tokio | bounded transcription completion tracking
use std::collections::BTreeSet;

#[derive(Default)]
pub struct Jobs {
    submitted: u64,
    pending: BTreeSet<u64>,
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
}
