//! Bounded, cancellation-aware transport for configured assistant providers.
//!
//! Request policy and response decoding stay independent of engine state and desktop UI.

mod account;
mod byok;
mod chat;
mod completion;
mod deepgram;
mod http;
mod media;
mod notes;
mod pairing;
mod palepale;
mod smart;
mod sse;
mod transcription;

#[cfg(test)]
mod tests;

pub use account::{PalepaleAccountStatus, palepale_account_status};
pub use byok::{byok_models, byok_role_models, test_byok};
pub use chat::stream_assist;
pub use http::{is_loopback, validate_base_url};
pub use notes::{GeneratedNotes, generate_session_notes};
pub use pairing::{
    PairingCredential, PairingPoll, PairingStarted, PalepalePairing, pairing_browser_url,
    poll_pairing, start_pairing, wait_for_pairing, wait_pairing,
};
pub use palepale::palepale_models;
pub use transcription::transcribe;

const MAX_AUDIO_BYTES: usize = 25_000_000;
const PALEPALE_OUTPUT_TOKENS: u32 = 8192;
const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
const MAX_CONTEXT_BYTES: usize = 512 * 1024;
const MAX_EVENT_BYTES: usize = 2 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_JSON_BYTES: usize = 4 * 1024 * 1024;
const MAX_NOTES_CONTEXT_BYTES: usize = 384 * 1024;
const MAX_NOTES_RESPONSE_BYTES: usize = 256 * 1024;
const MAX_NOTES_SUMMARY_BYTES: usize = 128 * 1024;
const MAX_NOTES_TITLE_CHARS: usize = 160;
