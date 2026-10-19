//! Application orchestration: lifecycle, shared state and the public event stream.
//!
//! Feature modules implement the engine API against this state. `transition` serializes
//! session changes, capture transitions and durable preference commits; individual state
//! mutexes retain their original acquisition order inside each operation.

mod account;
mod assistance;
mod capture;
mod catalog;
mod configuration;
pub use configuration::ByokModelList;
mod context;
mod credentials;
mod documents;
mod events;
mod latency;
mod pairing;
mod router;
mod sessions;

#[cfg(test)]
mod hosted_request_tests;
#[cfg(test)]
mod tests;

pub use assistance::AssistTarget;
pub use events::{ContextStatus, EngineEvent};
pub use latency::LatencySummary;
pub use pairing::{PalepaleCredential, PalepalePairing};

use crate::{
    audio::{AudioCapture, AudioSegment},
    config,
    documents::DocumentStore,
    history::History,
    jobs::Jobs,
    models::{Message, Settings, TranscriptEntry},
};
use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64},
    },
    time::{Duration, Instant},
};
use tokio::sync::{Notify, Semaphore, broadcast, mpsc};
use tokio_util::sync::CancellationToken;

const TRANSCRIPT_LIMIT: usize = 400;
const MESSAGE_LIMIT: usize = 80;
const QUEUED_SEGMENTS: usize = 16;

struct Configuration {
    settings: Settings,
    key: String,
    transcription_key: String,
    image_key: String,
}
#[derive(Default)]
struct Session {
    id: Option<String>,
    transcript: VecDeque<TranscriptEntry>,
    messages: VecDeque<Message>,
}
struct QueuedSegment {
    epoch: u64,
    job: u64,
    segment: AudioSegment,
    created_at: u64,
    queued_at: Instant,
    settings: Settings,
    key: String,
}
pub struct NativeEngine {
    directory: PathBuf,
    _profile_lock: File,
    runtime: tokio::runtime::Handle,
    configuration: Mutex<Configuration>,
    credentials: Arc<dyn credentials::Credentials>,
    provider_revision: AtomicU64,
    pairing_generation: AtomicU64,
    pairing_work: Arc<pairing::PairingWork>,
    published_models: Mutex<Vec<skillissue_domain::provider::ProviderModel>>,
    model_performance: Mutex<router::ModelPerformanceTable>,
    transition: Mutex<()>,
    session: Mutex<Session>,
    history: Mutex<History>,
    documents: DocumentStore,
    audio: Mutex<Option<AudioCapture>>,
    active: AtomicBool,
    stopping: AtomicBool,
    stop_done: Notify,
    last_stop_error: Mutex<Option<String>>,
    epoch: AtomicU64,
    assistant: Mutex<Option<CancellationToken>>,
    assistant_done: Notify,
    saved_questions: Semaphore,
    note_jobs: Semaphore,
    jobs: Mutex<Jobs>,
    jobs_done: Notify,
    transcriptions: mpsc::Sender<QueuedSegment>,
    events: broadcast::Sender<EngineEvent>,
    client: reqwest::Client,
    warnings: Mutex<Vec<String>>,
}

fn lock<T>(value: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, String> {
    value
        .lock()
        .map_err(|_| "An internal state lock failed. Restart skillissue.".into())
}
impl NativeEngine {
    /// Open inside an entered Tokio runtime. Existing settings, database and credentials are reused.
    pub fn open(directory: PathBuf) -> Result<Arc<Self>, String> {
        if !directory.is_absolute() {
            return Err("The skillissue data directory must be an absolute path.".into());
        }
        std::fs::create_dir_all(&directory).map_err(|e| format!("Create data directory: {e}"))?;
        let credentials = credentials::SystemCredentials::for_directory(&directory)?;
        Self::open_with_credentials(directory, Arc::new(credentials))
    }

    fn open_with_credentials(
        directory: PathBuf,
        credentials: Arc<dyn credentials::Credentials>,
    ) -> Result<Arc<Self>, String> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(120))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("skillissue-native/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| e.to_string())?;
        Self::open_with_transport(directory, credentials, client)
    }

    fn open_with_transport(
        directory: PathBuf,
        credentials: Arc<dyn credentials::Credentials>,
        client: reqwest::Client,
    ) -> Result<Arc<Self>, String> {
        if !directory.is_absolute() {
            return Err("The skillissue data directory must be an absolute path.".into());
        }
        std::fs::create_dir_all(&directory).map_err(|e| format!("Create data directory: {e}"))?;
        let profile_lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("native-profile.lock"))
            .map_err(|e| format!("Open profile lock: {e}"))?;
        profile_lock.try_lock().map_err(|_| "This profile is already open in another native skillissue process. Close it or use a separate SKILLISSUE_CONFIG_DIR.".to_string())?;
        let settings = config::load(&directory.join("settings.json"))?;
        let mut warnings = Vec::new();
        let key = credentials
            .load_provider(&settings)
            .unwrap_or_else(|error| {
                warnings.push(error);
                String::new()
            });
        let transcription_key = if settings.transcription_base_url.is_empty() {
            String::new()
        } else {
            credentials
                .load_saved(&config::transcription_credential_id(&settings))
                .unwrap_or_else(|error| {
                    warnings.push(error);
                    None
                })
                .unwrap_or_default()
        };
        let image_key = if settings.image_base_url.is_empty() {
            String::new()
        } else {
            credentials
                .load_saved(&config::image_credential_id(&settings))
                .unwrap_or_else(|error| {
                    warnings.push(error);
                    None
                })
                .unwrap_or_default()
        };
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|e| format!("skillissue requires an active Tokio runtime: {e}"))?;
        // Legacy processes do not share our instance lock. Opening the new UI must not
        // mutate their active/analyzing records as if they had crashed.
        let history = History::open_without_recovery(&directory.join("sessions.sqlite3"))?;
        let documents = DocumentStore::open(&directory.join("documents"))?;
        let (events, _) = broadcast::channel(4096);
        let (transcriptions, mut receiver) = mpsc::channel::<QueuedSegment>(QUEUED_SEGMENTS);
        let model_performance =
            router::ModelPerformanceTable::load(&directory.join("provider-requests.log"));
        let engine = Arc::new(Self {
            directory,
            _profile_lock: profile_lock,
            runtime: runtime.clone(),
            configuration: Mutex::new(Configuration {
                settings,
                key,
                transcription_key,
                image_key,
            }),
            credentials,
            provider_revision: AtomicU64::new(0),
            pairing_generation: AtomicU64::new(0),
            pairing_work: Arc::new(pairing::PairingWork::default()),
            published_models: Mutex::new(Vec::new()),
            model_performance: Mutex::new(model_performance),
            transition: Mutex::new(()),
            session: Mutex::new(Session::default()),
            history: Mutex::new(history),
            documents,
            audio: Mutex::new(None),
            active: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            stop_done: Notify::new(),
            last_stop_error: Mutex::new(None),
            epoch: AtomicU64::new(0),
            assistant: Mutex::new(None),
            assistant_done: Notify::new(),
            saved_questions: Semaphore::new(1),
            note_jobs: Semaphore::new(1),
            jobs: Mutex::new(Jobs::default()),
            jobs_done: Notify::new(),
            transcriptions,
            events,
            client,
            warnings: Mutex::new(warnings),
        });
        let weak = Arc::downgrade(&engine);
        runtime.spawn(async move {
            let slots = Arc::new(Semaphore::new(4));
            loop {
                let Ok(permit) = slots.clone().acquire_owned().await else {
                    break;
                };
                let Some(queued) = receiver.recv().await else {
                    break;
                };
                let Some(engine) = weak.upgrade() else {
                    break;
                };
                tokio::spawn(async move {
                    let _permit = permit;
                    engine.transcribe(queued).await;
                });
            }
        });
        Ok(engine)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.events.subscribe()
    }
    fn emit(&self, event: EngineEvent) {
        let _ = self.events.send(event);
    }
    fn publish_snapshot(&self) {
        match self.snapshot() {
            Ok(snapshot) => self.emit(EngineEvent::Snapshot(snapshot)),
            Err(error) => self.emit(EngineEvent::Error(error)),
        }
    }
    pub fn data_directory(&self) -> &std::path::Path {
        &self.directory
    }
    pub async fn shutdown(self: &Arc<Self>) -> Result<(), String> {
        self.drain_palepale_pairing().await?;
        self.cancel_assist();
        let assist_finished = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let done = self.assistant_done.notified();
                if lock(&self.assistant)?.is_none() {
                    return Ok::<(), String>(());
                }
                done.await;
            }
        })
        .await
        .map_err(|_| "The cancelled answer did not finish shutting down.".to_string());
        self.stop_capture().await?;
        assist_finished??;
        Ok(())
    }
}
