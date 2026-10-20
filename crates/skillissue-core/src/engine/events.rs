//! Engine notifications shared with desktop clients.

use crate::{
    audio::AudioLevel,
    models::{ConfigView, SessionSnapshot, TranscriptEntry},
};

#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextStatus {
    pub capturing: bool,
    pub pending_transcriptions: usize,
    pub latest_transcript_at: Option<u64>,
    pub transcript_entries: usize,
    pub transcriptions: Vec<crate::jobs::TranscriptionProgress>,
}

#[derive(Clone)]
pub enum EngineEvent {
    ConfigChanged(Box<ConfigView>),
    Snapshot(SessionSnapshot),
    Transcript(TranscriptEntry),
    Assistant {
        id: String,
        kind: String,
        /// For `start`, the submitted prompt shared with every observing window.
        /// Other kinds carry an answer delta, context warning, or error text.
        text: String,
    },
    AudioLevel(AudioLevel),
    HistoryChanged,
    Error(String),
    CaptureGap {
        source: String,
        created_at: u64,
        message: String,
    },
}
