// Rust | history.rs | SQLite/rusqlite | durable local session journal
use crate::models::{
    Message, ModeSection, SessionListEntry, SessionRecord, Settings, TranscriptEntry, now_ms,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::path::Path;

mod recovery;

pub struct History {
    connection: Connection,
}

fn database_error(error: impl std::fmt::Display) -> String {
    format!("Session history: {error}")
}

fn load(connection: &Connection, id: &str) -> Result<SessionRecord, String> {
    let row = connection.query_row(
        "SELECT id,title,created_at,updated_at,ended_at,state,summary,summary_error,mode_name,mode_sections FROM sessions WHERE id=?1",
        [id],
        |row| Ok(SessionRecord {
            id: row.get(0)?, title: row.get(1)?, created_at: row.get::<_,i64>(2)? as u64, updated_at: row.get::<_,i64>(3)? as u64,
            ended_at: row.get::<_,Option<i64>>(4)?.map(|value|value as u64), state: row.get(5)?, summary: row.get(6)?, summary_error: row.get(7)?,
            mode_name: row.get(8)?, mode_sections: Vec::new(), transcript: Vec::new(), messages: Vec::new(),
        }),
    ).optional().map_err(database_error)?.ok_or("Saved session not found.")?;
    let sections: String = connection
        .query_row(
            "SELECT mode_sections FROM sessions WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    let mut record = row;
    record.mode_sections = serde_json::from_str(&sections).map_err(database_error)?;
    let mut transcript = connection
        .prepare("SELECT data FROM transcript WHERE session_id=?1 ORDER BY created_at,id")
        .map_err(database_error)?;
    let rows = transcript
        .query_map([id], |row| row.get::<_, String>(0))
        .map_err(database_error)?;
    for row in rows {
        record
            .transcript
            .push(serde_json::from_str(&row.map_err(database_error)?).map_err(database_error)?);
    }
    let mut messages = connection
        .prepare("SELECT data FROM messages WHERE session_id=?1 ORDER BY sequence")
        .map_err(database_error)?;
    let rows = messages
        .query_map([id], |row| row.get::<_, String>(0))
        .map_err(database_error)?;
    for row in rows {
        record
            .messages
            .push(serde_json::from_str(&row.map_err(database_error)?).map_err(database_error)?);
    }
    Ok(record)
}

fn ensure_session(
    transaction: &Transaction<'_>,
    id: &str,
    settings: &Settings,
    active: bool,
) -> Result<(), String> {
    let mode = settings
        .modes
        .iter()
        .find(|mode| mode.id == settings.active_mode_id);
    let mode_name = mode.map(|mode| mode.name.as_str()).unwrap_or("General");
    let sections: &[ModeSection] = mode.map(|mode| mode.sections.as_slice()).unwrap_or(&[]);
    let sections = serde_json::to_string(sections).map_err(database_error)?;
    let now = now_ms() as i64;
    transaction.execute("INSERT OR IGNORE INTO sessions(id,title,created_at,updated_at,state,summary,mode_name,mode_sections,revision) VALUES(?1,'Untitled session',?2,?2,?3,'',?4,?5,0)", params![id, now, if active {"active"} else {"ready"}, mode_name, sections]).map_err(database_error)?;
    Ok(())
}

impl History {
    pub fn open(path: &Path) -> Result<Self, String> {
        Self::open_with_recovery(path, true)
    }

    pub fn open_without_recovery(path: &Path) -> Result<Self, String> {
        Self::open_with_recovery(path, false)
    }

    fn open_with_recovery(path: &Path, recover_interrupted: bool) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(database_error)?;
        }
        let connection = Connection::open(path).map_err(database_error)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(database_error)?;
        connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS sessions(id TEXT PRIMARY KEY,title TEXT NOT NULL,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,ended_at INTEGER,state TEXT NOT NULL,summary TEXT NOT NULL,summary_error TEXT,mode_name TEXT,mode_sections TEXT NOT NULL,revision INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS transcript(id TEXT PRIMARY KEY,session_id TEXT NOT NULL REFERENCES sessions(id),created_at INTEGER NOT NULL,data TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS transcript_session ON transcript(session_id,created_at,id);
            CREATE TABLE IF NOT EXISTS messages(sequence INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT NOT NULL UNIQUE,session_id TEXT NOT NULL REFERENCES sessions(id),data TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS messages_session ON messages(session_id,sequence);
            CREATE INDEX IF NOT EXISTS session_recent ON sessions(updated_at DESC,id);
            PRAGMA user_version=1;").map_err(database_error)?;
        if recover_interrupted {
            let now = now_ms() as i64;
            connection.execute("UPDATE sessions SET state='interrupted',ended_at=COALESCE(ended_at,?1),updated_at=?1,revision=revision+1 WHERE state='active'", [now]).map_err(database_error)?;
            connection.execute("UPDATE sessions SET state='error',summary_error='Notes generation was interrupted when the app closed.',updated_at=?1,revision=revision+1 WHERE state='analyzing'", [now]).map_err(database_error)?;
        }
        Ok(Self { connection })
    }

    pub fn get(&self, id: &str) -> Result<SessionRecord, String> {
        load(&self.connection, id)
    }

    pub fn list(
        &self,
        query: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<SessionListEntry>, String> {
        if query.chars().count() > 200 {
            return Err("Search is limited to 200 characters.".into());
        }
        let mut statement = self.connection.prepare("SELECT id,title,created_at,updated_at,ended_at,state,mode_name,(SELECT count(*) FROM transcript t WHERE t.session_id=s.id),(SELECT count(*) FROM messages m WHERE m.session_id=s.id) FROM sessions s WHERE ?1='' OR instr(lower(title),lower(?1))>0 OR instr(lower(summary),lower(?1))>0 OR EXISTS(SELECT 1 FROM transcript t WHERE t.session_id=s.id AND instr(lower(json_extract(t.data,'$.text')),lower(?1))>0) OR EXISTS(SELECT 1 FROM messages m WHERE m.session_id=s.id AND instr(lower(json_extract(m.data,'$.content')),lower(?1))>0) ORDER BY updated_at DESC,id LIMIT ?2 OFFSET ?3").map_err(database_error)?;
        let rows = statement
            .query_map(
                params![query.trim(), limit.clamp(1, 200), offset.min(100_000)],
                |row| {
                    Ok(SessionListEntry {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        created_at: row.get::<_, i64>(2)? as u64,
                        updated_at: row.get::<_, i64>(3)? as u64,
                        ended_at: row.get::<_, Option<i64>>(4)?.map(|value| value as u64),
                        state: row.get(5)?,
                        mode_name: row.get(6)?,
                        transcript_count: row.get::<_, i64>(7)? as u64,
                        message_count: row.get::<_, i64>(8)? as u64,
                    })
                },
            )
            .map_err(database_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
    }

    pub fn append_transcript(
        &mut self,
        id: &str,
        settings: &Settings,
        active: bool,
        entry: &TranscriptEntry,
    ) -> Result<(), String> {
        let data = serde_json::to_string(entry).map_err(database_error)?;
        let transaction = self.connection.transaction().map_err(database_error)?;
        ensure_session(&transaction, id, settings, active)?;
        transaction
            .execute(
                "INSERT INTO transcript(id,session_id,created_at,data) VALUES(?1,?2,?3,?4)",
                params![entry.id, id, entry.created_at as i64, data],
            )
            .map_err(database_error)?;
        transaction.execute("UPDATE sessions SET updated_at=?2,state=?3,ended_at=CASE WHEN ?3='active' THEN NULL ELSE ended_at END,summary_error=NULL,revision=revision+1 WHERE id=?1", params![id,now_ms() as i64,if active {"active"} else {"ready"}]).map_err(database_error)?;
        transaction.commit().map_err(database_error)
    }

    pub fn append_messages(
        &mut self,
        id: &str,
        settings: &Settings,
        active: bool,
        messages: &[Message],
    ) -> Result<(), String> {
        let data = messages
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .map_err(database_error)?;
        let transaction = self.connection.transaction().map_err(database_error)?;
        ensure_session(&transaction, id, settings, active)?;
        for (message, data) in messages.iter().zip(data) {
            transaction
                .execute(
                    "INSERT INTO messages(id,session_id,data) VALUES(?1,?2,?3)",
                    params![message.id, id, data],
                )
                .map_err(database_error)?;
        }
        transaction.execute("UPDATE sessions SET updated_at=?2,state=?3,ended_at=CASE WHEN ?3='active' THEN NULL ELSE ended_at END,summary_error=NULL,revision=revision+1 WHERE id=?1", params![id,now_ms() as i64,if active {"active"} else {"ready"}]).map_err(database_error)?;
        transaction.commit().map_err(database_error)
    }

    pub fn mark_active(&mut self, id: &str) -> Result<(), String> {
        self.connection.execute("UPDATE sessions SET state='active',ended_at=NULL,updated_at=?2,revision=revision+1 WHERE id=?1", params![id,now_ms() as i64]).map_err(database_error)?;
        Ok(())
    }

    pub fn finish(&mut self, id: &str) -> Result<(), String> {
        let now = now_ms() as i64;
        self.connection.execute("UPDATE sessions SET state='ready',ended_at=?2,updated_at=?2,revision=revision+1 WHERE id=?1", params![id,now]).map_err(database_error)?;
        Ok(())
    }

    pub fn update(
        &mut self,
        id: &str,
        title: Option<&str>,
        summary: Option<&str>,
    ) -> Result<SessionRecord, String> {
        if let Some(title) = title
            && (title.trim().is_empty() || title.chars().count() > 160)
        {
            return Err("Session title must contain 1–160 characters.".into());
        }
        if summary.is_some_and(|summary| summary.len() > 131_072) {
            return Err("Notes are limited to 128 KiB.".into());
        }
        if title.is_none() && summary.is_none() {
            return self.get(id);
        }
        let changed = self.connection.execute("UPDATE sessions SET title=COALESCE(?2,title),summary=COALESCE(?3,summary),state=CASE WHEN state='active' THEN state ELSE 'ready' END,summary_error=NULL,updated_at=?4,revision=revision+1 WHERE id=?1", params![id,title.map(str::trim),summary,now_ms() as i64]).map_err(database_error)?;
        if changed == 0 {
            return Err("Saved session not found.".into());
        }
        self.get(id)
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        let transaction = self.connection.transaction().map_err(database_error)?;
        transaction
            .execute("DELETE FROM transcript WHERE session_id=?1", [id])
            .map_err(database_error)?;
        transaction
            .execute("DELETE FROM messages WHERE session_id=?1", [id])
            .map_err(database_error)?;
        let changed = transaction
            .execute("DELETE FROM sessions WHERE id=?1", [id])
            .map_err(database_error)?;
        if changed == 0 {
            return Err("Saved session not found.".into());
        }
        transaction.commit().map_err(database_error)
    }

    pub fn begin_notes(&mut self, id: &str) -> Result<(SessionRecord, i64), String> {
        let transaction = self.connection.transaction().map_err(database_error)?;
        let record = load(&transaction, id)?;
        if record.transcript.is_empty() && record.messages.is_empty() {
            return Err("There is no conversation to summarize.".into());
        }
        if record.state == "active" {
            return Err("Stop the session before generating notes.".into());
        }
        transaction.execute("UPDATE sessions SET state='analyzing',summary_error=NULL,updated_at=?2,revision=revision+1 WHERE id=?1", params![id,now_ms() as i64]).map_err(database_error)?;
        let revision = transaction
            .query_row("SELECT revision FROM sessions WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .map_err(database_error)?;
        transaction.commit().map_err(database_error)?;
        Ok((record, revision))
    }

    pub fn finish_notes(
        &mut self,
        id: &str,
        revision: i64,
        result: Result<(&str, &str), &str>,
    ) -> Result<SessionRecord, String> {
        match result {
            Ok((title, summary)) => {
                self.connection.execute("UPDATE sessions SET title=CASE WHEN title='Untitled session' THEN ?3 ELSE title END,summary=?4,state='ready',summary_error=NULL,updated_at=?5,revision=revision+1 WHERE id=?1 AND revision=?2 AND state='analyzing'", params![id,revision,title,summary,now_ms() as i64]).map_err(database_error)?;
            }
            Err(error) => {
                self.connection.execute("UPDATE sessions SET state='error',summary_error=?3,updated_at=?4,revision=revision+1 WHERE id=?1 AND revision=?2 AND state='analyzing'",params![id,revision,error,now_ms() as i64]).map_err(database_error)?;
            }
        }
        self.get(id)
    }

    pub fn invalidate_notes(&mut self, id: &str) -> Result<(), String> {
        self.connection.execute("UPDATE sessions SET state='ready',summary_error='Notes generation was cancelled because the live context changed.',updated_at=?2,revision=revision+1 WHERE id=?1 AND state='analyzing'",params![id,now_ms() as i64]).map_err(database_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn database() -> (std::path::PathBuf, History) {
        let directory =
            std::env::temp_dir().join(format!("skillissue-history-{}", uuid::Uuid::new_v4()));
        let history = History::open(&directory.join("sessions.sqlite3")).unwrap();
        (directory, history)
    }
    fn entry(id: &str, text: &str) -> TranscriptEntry {
        TranscriptEntry {
            id: id.into(),
            source: "microphone".into(),
            text: text.into(),
            created_at: now_ms(),
        }
    }
    fn message(id: &str) -> Message {
        Message {
            id: id.into(),
            role: "assistant".into(),
            content: "answer evidence".into(),
            created_at: now_ms(),
        }
    }
    fn cleanup(directory: std::path::PathBuf, history: History) {
        drop(history);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn backup_recovers_committed_wal_content_without_replacing_existing_files() {
        let (directory, mut history) = database();
        history
            .append_transcript(
                "one",
                &Settings::default(),
                true,
                &entry("speech", "Recorded words"),
            )
            .unwrap();
        history
            .append_messages("one", &Settings::default(), true, &[message("answer")])
            .unwrap();
        history
            .update("one", Some("Meeting"), Some("Saved notes"))
            .unwrap();
        let backup = directory.join("backup, 验证.sqlite3");
        history.backup(&backup).unwrap();
        let original_bytes = std::fs::read(&backup).unwrap();
        history.update("one", None, Some("Later notes")).unwrap();
        assert!(history.backup(&backup).is_err());
        assert_eq!(std::fs::read(&backup).unwrap(), original_bytes);
        let restored = History::open(&backup).unwrap();
        let record = restored.get("one").unwrap();
        assert_eq!(record.title, "Meeting");
        assert_eq!(record.summary, "Saved notes");
        assert_eq!(record.transcript[0].text, "Recorded words");
        assert_eq!(record.messages[0].content, "answer evidence");
        assert_eq!(history.get("one").unwrap().summary, "Later notes");
        assert!(history.backup(Path::new("relative.sqlite3")).is_err());
        assert!(!std::fs::read_dir(&directory).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".skillissue-backup-")
        }));
        drop(restored);
        cleanup(directory, history);
    }

    #[test]
    fn restore_is_atomic_refuses_occupied_history_and_recovers_interrupted_sessions() {
        let (directory, mut original) = database();
        original
            .append_messages("one", &Settings::default(), true, &[message("answer")])
            .unwrap();
        let backup = directory.join("backup.sqlite3");
        original.backup(&backup).unwrap();
        let mut restored = History::open(&directory.join("restored.sqlite3")).unwrap();
        assert_eq!(restored.restore_empty(&backup).unwrap(), 1);
        assert_eq!(restored.get("one").unwrap().state, "interrupted");
        assert_eq!(
            restored.get("one").unwrap().messages[0].content,
            "answer evidence"
        );
        assert!(restored.restore_empty(&backup).is_err());
        assert_eq!(restored.list("", 100, 0).unwrap().len(), 1);
        let source = Connection::open(&backup).unwrap();
        source
            .execute("UPDATE messages SET data='invalid JSON'", [])
            .unwrap();
        drop(source);
        let mut empty = History::open(&directory.join("empty.sqlite3")).unwrap();
        assert!(empty.restore_empty(&backup).is_err());
        assert!(empty.list("", 100, 0).unwrap().is_empty());
        let count: i64 = empty
            .connection
            .query_row("SELECT count(*) FROM messages", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
        assert_eq!(original.get("one").unwrap().state, "active");
        drop(empty);
        drop(restored);
        cleanup(directory, original);
    }

    #[test]
    fn journal_survives_reopen_without_restarting_capture() {
        let (directory, mut history) = database();
        history
            .append_transcript(
                "one",
                &Settings::default(),
                true,
                &entry("speech", "important evidence"),
            )
            .unwrap();
        history
            .append_messages("one", &Settings::default(), true, &[message("reply")])
            .unwrap();
        drop(history);
        let history = History::open(&directory.join("sessions.sqlite3")).unwrap();
        let saved = history.get("one").unwrap();
        assert_eq!(saved.state, "interrupted");
        assert!(saved.ended_at.is_some());
        assert_eq!(saved.transcript[0].text, "important evidence");
        assert_eq!(saved.messages.len(), 1);
        cleanup(directory, history);
    }

    #[test]
    fn sessions_are_separate_and_failed_pair_is_atomic() {
        let (directory, mut history) = database();
        history
            .append_messages("one", &Settings::default(), false, &[message("existing")])
            .unwrap();
        let duplicate = message("existing");
        assert!(
            history
                .append_messages(
                    "two",
                    &Settings::default(),
                    false,
                    &[message("first"), duplicate]
                )
                .is_err()
        );
        assert!(history.get("two").is_err());
        assert_eq!(history.get("one").unwrap().messages.len(), 1);
        cleanup(directory, history);
    }

    #[test]
    fn delete_removes_the_session_and_its_journal_atomically() {
        let (directory, mut history) = database();
        history
            .append_transcript("one", &Settings::default(), false, &entry("a", "speech"))
            .unwrap();
        history
            .append_messages("one", &Settings::default(), false, &[message("reply")])
            .unwrap();
        history.delete("one").unwrap();
        assert!(history.get("one").is_err());
        assert!(history.list("", 20, 0).unwrap().is_empty());
        cleanup(directory, history);
    }

    #[test]
    fn edits_and_reruns_prevent_stale_notes_overwriting() {
        let (directory, mut history) = database();
        history
            .append_messages("one", &Settings::default(), false, &[message("reply")])
            .unwrap();
        let (_, old) = history.begin_notes("one").unwrap();
        history
            .update("one", Some("Manual title"), Some("My notes"))
            .unwrap();
        let record = history
            .finish_notes("one", old, Ok(("Stale title", "Stale notes")))
            .unwrap();
        assert_eq!(record.title, "Manual title");
        assert_eq!(record.summary, "My notes");
        let (_, old) = history.begin_notes("one").unwrap();
        let (_, new) = history.begin_notes("one").unwrap();
        assert_eq!(
            history
                .finish_notes("one", old, Err("old error"))
                .unwrap()
                .state,
            "analyzing"
        );
        let record = history
            .finish_notes("one", new, Err("provider failed"))
            .unwrap();
        assert_eq!(record.summary, "My notes");
        assert_eq!(record.state, "error");
        assert_eq!(record.summary_error.as_deref(), Some("provider failed"));
        cleanup(directory, history);
    }

    #[test]
    fn queries_are_literal_bounded_and_search_recorded_content() {
        let (directory, mut history) = database();
        history
            .append_transcript(
                "one",
                &Settings::default(),
                false,
                &entry("a", "Contains % and underscore_ literal"),
            )
            .unwrap();
        history
            .append_messages("two", &Settings::default(), false, &[message("b")])
            .unwrap();
        assert_eq!(history.list("%", 100, 0).unwrap().len(), 1);
        assert_eq!(history.list("EVIDENCE", 100, 0).unwrap().len(), 1);
        assert_eq!(history.list("", 1, 0).unwrap().len(), 1);
        assert_eq!(history.list("", 200, 1).unwrap().len(), 1);
        assert!(history.list(&"a".repeat(201), 20, 0).is_err());
        for metadata in ["createdAt", "microphone", "assistant", "\"role\""] {
            assert!(
                history.list(metadata, 100, 0).unwrap().is_empty(),
                "JSON metadata must not be searchable content: {metadata}"
            );
        }
        history
            .append_transcript(
                "three",
                &Settings::default(),
                false,
                &entry("c", "Discussed the createdAt field migration."),
            )
            .unwrap();
        let matching = history.list("createdAt", 100, 0).unwrap();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].id, "three");
        cleanup(directory, history);
    }

    #[test]
    fn interrupted_notes_preserve_previous_summary_and_mode_snapshot() {
        let (directory, mut history) = database();
        let mut settings = Settings::default();
        settings.modes.push(crate::models::AssistantMode {
            id: "custom".into(),
            name: "My mode".into(),
            prompt: "prompt".into(),
            sections: vec![ModeSection {
                id: "s".into(),
                title: "Decisions".into(),
                prompt: "decisions".into(),
            }],
            files: vec![],
        });
        settings.active_mode_id = "custom".into();
        history
            .append_messages("one", &settings, false, &[message("reply")])
            .unwrap();
        history.update("one", None, Some("Keep me")).unwrap();
        history.begin_notes("one").unwrap();
        drop(history);
        let history = History::open(&directory.join("sessions.sqlite3")).unwrap();
        let record = history.get("one").unwrap();
        assert_eq!(record.state, "error");
        assert_eq!(record.summary, "Keep me");
        assert_eq!(record.mode_name.as_deref(), Some("My mode"));
        assert_eq!(record.mode_sections[0].title, "Decisions");
        cleanup(directory, history);
    }

    #[test]
    fn full_journal_exceeds_live_context_bounds_and_invalidates_notes() {
        let (directory, mut history) = database();
        for n in 0..405 {
            history
                .append_transcript(
                    "one",
                    &Settings::default(),
                    false,
                    &entry(&format!("speech-{n}"), "evidence"),
                )
                .unwrap();
        }
        let (_, revision) = history.begin_notes("one").unwrap();
        history.invalidate_notes("one").unwrap();
        let record = history
            .finish_notes("one", revision, Ok(("stale", "stale")))
            .unwrap();
        assert_eq!(record.transcript.len(), 405);
        assert!(record.summary.is_empty());
        history
            .append_messages(
                "one",
                &Settings::default(),
                false,
                &[message("reply-1"), message("reply-2")],
            )
            .unwrap();
        history
            .update("one", None, Some(&"large notes ".repeat(10_000)))
            .unwrap();
        let entries = history.list("", 50, 0).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].transcript_count, 405);
        assert_eq!(entries[0].message_count, 2);
        let payload = serde_json::to_value(&entries).unwrap();
        for field in ["transcript", "messages", "summary", "modeSections"] {
            assert!(
                payload[0].get(field).is_none(),
                "History list must omit large session detail field {field}"
            );
        }
        assert!(
            serde_json::to_vec(&entries).unwrap().len() < 1024,
            "History list payload must remain small with 405 transcript entries and 120,000-byte notes"
        );
        let full_record = history.get("one").unwrap();
        assert_eq!(full_record.transcript.len(), 405);
        assert_eq!(full_record.messages.len(), 2);
        assert_eq!(full_record.summary.len(), 120_000);
        cleanup(directory, history);
    }
}
