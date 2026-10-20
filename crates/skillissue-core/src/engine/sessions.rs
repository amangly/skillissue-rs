//! Live session snapshots, durable history and guarded context replacement.

use super::{
    ContextStatus, EngineEvent, MESSAGE_LIMIT, NativeEngine, Session, TRANSCRIPT_LIMIT, lock,
};
use crate::models::{SessionListEntry, SessionRecord, SessionSnapshot};
use std::sync::atomic::Ordering;

impl NativeEngine {
    pub fn restore_history(&self, source: &std::path::Path) -> Result<usize, String> {
        let _transition = lock(&self.transition)?;
        self.ensure_idle()?;
        if lock(&self.session)?.id.is_some() {
            return Err("Start an empty session before restoring history.".into());
        }
        let count = lock(&self.history)?.restore_empty(source)?;
        self.emit(EngineEvent::HistoryChanged);
        Ok(count)
    }

    pub fn backup_history(&self, destination: &std::path::Path) -> Result<(), String> {
        lock(&self.history)?.backup(destination)
    }

    pub fn snapshot(&self) -> Result<SessionSnapshot, String> {
        let session = lock(&self.session)?;
        Ok(SessionSnapshot {
            session_id: session.id.clone(),
            active: self.active.load(Ordering::SeqCst),
            transcript: session.transcript.iter().cloned().collect(),
            messages: session.messages.iter().cloned().collect(),
        })
    }
    pub fn context_status(&self) -> Result<ContextStatus, String> {
        let session = lock(&self.session)?;
        let jobs = lock(&self.jobs)?;
        Ok(ContextStatus {
            capturing: self.active.load(Ordering::SeqCst),
            pending_transcriptions: jobs.pending_count(),
            latest_transcript_at: session.transcript.back().map(|entry| entry.created_at),
            transcript_entries: session.transcript.len(),
            transcriptions: jobs.progress(),
        })
    }
    pub fn list_sessions(
        &self,
        query: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<SessionListEntry>, String> {
        lock(&self.history)?.list(query, limit, offset)
    }
    pub fn saved_session(&self, id: &str) -> Result<SessionRecord, String> {
        lock(&self.history)?.get(id)
    }
    pub fn update_notes(&self, id: &str, title: &str, notes: &str) -> Result<(), String> {
        lock(&self.history)?.update(id, Some(title), Some(notes))?;
        self.emit(EngineEvent::HistoryChanged);
        Ok(())
    }
    pub fn delete_saved_session(&self, id: &str) -> Result<(), String> {
        let _questions = self
            .saved_questions
            .try_acquire()
            .map_err(|_| "Wait for the saved-session answer before deleting a session.")?;
        let _notes = self
            .note_jobs
            .try_acquire()
            .map_err(|_| "Wait for note generation before deleting a session.")?;
        let _transition = lock(&self.transition)?;
        self.ensure_idle()?;
        if lock(&self.session)?.id.as_deref() == Some(id) {
            return Err("The current session must be replaced before it can be deleted.".into());
        }
        lock(&self.history)?.delete(id)?;
        self.emit(EngineEvent::HistoryChanged);
        Ok(())
    }

    pub(super) fn ensure_idle(&self) -> Result<(), String> {
        if self.active.load(Ordering::SeqCst) || self.stopping.load(Ordering::SeqCst) {
            return Err(
                "Stop capture and wait for transcription before changing the session.".into(),
            );
        }
        if lock(&self.assistant)?.is_some() {
            return Err("Finish or cancel the current answer before changing the session.".into());
        }
        let jobs = lock(&self.jobs)?;
        if !jobs.finished_through(jobs.watermark()) {
            return Err("Transcription is still pending. Wait before changing the session.".into());
        }
        Ok(())
    }
    pub fn new_session(&self) -> Result<(), String> {
        self.replace_session(None)
    }
    pub fn resume_session(&self, id: &str) -> Result<(), String> {
        self.replace_session(Some(id))
    }
    fn replace_session(&self, id: Option<&str>) -> Result<(), String> {
        let _transition = lock(&self.transition)?;
        self.ensure_idle()?;
        // Load under the same transition lock used by deletion so a deleted
        // record cannot be installed from a stale, pre-lock snapshot.
        let record = id.map(|id| self.saved_session(id)).transpose()?;
        let mut session = lock(&self.session)?;
        if let Some(id) = &session.id {
            lock(&self.history)?.invalidate_notes(id)?;
        }
        if let Some(record) = record {
            lock(&self.history)?.invalidate_notes(&record.id)?;
            *session = Session {
                id: Some(record.id),
                transcript: record
                    .transcript
                    .into_iter()
                    .rev()
                    .take(TRANSCRIPT_LIMIT)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect(),
                messages: record
                    .messages
                    .into_iter()
                    .rev()
                    .take(MESSAGE_LIMIT)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect(),
            };
        } else {
            *session = Session::default();
        }
        self.epoch.fetch_add(1, Ordering::SeqCst);
        lock(&self.jobs)?.clear_progress();
        drop(session);
        self.publish_snapshot();
        self.emit(EngineEvent::HistoryChanged);
        Ok(())
    }
}
