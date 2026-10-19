//! Consistent history snapshots and transactional restoration into empty journals.
use super::{History, database_error, load};
use crate::models::now_ms;
use rusqlite::Connection;
use std::path::Path;

impl History {
    /// Import data into our own schema; never install schema objects from a backup.
    pub fn restore_empty(&mut self, source: &Path) -> Result<usize, String> {
        if !source.is_absolute() || !source.is_file() {
            return Err("Choose an existing absolute backup file.".into());
        }
        if source.metadata().map_err(database_error)?.len() > 256 * 1024 * 1024 {
            return Err("History backups are limited to 256 MiB for restore.".into());
        }
        let mut backup =
            Connection::open_with_flags(source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(database_error)?;
        backup
            .execute_batch("PRAGMA trusted_schema=OFF; PRAGMA query_only=ON;")
            .map_err(database_error)?;
        let read = backup.transaction().map_err(database_error)?;
        let version: i64 = read
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(database_error)?;
        if version != 1 {
            return Err("Unsupported history backup version.".into());
        }
        let integrity: String = read
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(database_error)?;
        if integrity != "ok" {
            return Err("History backup is damaged.".into());
        }
        let transaction = self.connection.transaction().map_err(database_error)?;
        let occupied: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM sessions) OR EXISTS(SELECT 1 FROM transcript) OR EXISTS(SELECT 1 FROM messages)", [], |row| row.get(0)).map_err(database_error)?;
        if occupied {
            return Err(
                "Restore requires an empty history. Existing sessions were preserved.".into(),
            );
        }
        for (table, columns, parameters) in [
            (
                "sessions",
                "id,title,created_at,updated_at,ended_at,state,summary,summary_error,mode_name,mode_sections,revision",
                "?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11",
            ),
            ("transcript", "id,session_id,created_at,data", "?1,?2,?3,?4"),
            ("messages", "sequence,id,session_id,data", "?1,?2,?3,?4"),
        ] {
            let kind: String = read
                .query_row(
                    "SELECT type FROM sqlite_schema WHERE name=?1",
                    [table],
                    |row| row.get(0),
                )
                .map_err(database_error)?;
            if kind != "table" {
                return Err("Backup contains an invalid history table.".into());
            }
            let mut select = read
                .prepare(&format!("SELECT {columns} FROM {table}"))
                .map_err(database_error)?;
            let width = select.column_count();
            let mut rows = select.query([]).map_err(database_error)?;
            let mut insert = transaction
                .prepare(&format!(
                    "INSERT INTO {table}({columns}) VALUES({parameters})"
                ))
                .map_err(database_error)?;
            while let Some(row) = rows.next().map_err(database_error)? {
                let values = (0..width)
                    .map(|column| row.get::<_, rusqlite::types::Value>(column))
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(database_error)?;
                insert
                    .execute(rusqlite::params_from_iter(values))
                    .map_err(database_error)?;
            }
        }
        let ids = {
            let mut statement = transaction
                .prepare("SELECT id FROM sessions")
                .map_err(database_error)?;
            statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(database_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(database_error)?
        };
        // Validate serialized records before committing any imported rows.
        for id in &ids {
            load(&transaction, id)?;
        }
        transaction.execute("UPDATE sessions SET state='interrupted',ended_at=COALESCE(ended_at,?1),revision=revision+1 WHERE state='active'", [now_ms() as i64]).map_err(database_error)?;
        transaction.execute("UPDATE sessions SET state='error',summary_error='Notes generation was interrupted before this backup.',revision=revision+1 WHERE state='analyzing'", []).map_err(database_error)?;
        transaction.commit().map_err(database_error)?;
        Ok(ids.len())
    }

    /// Publish a consistent standalone database without replacing an existing file.
    pub fn backup(&self, destination: &Path) -> Result<(), String> {
        if !destination.is_absolute() || destination.file_name().is_none() {
            return Err("Choose an absolute backup filename.".into());
        }
        let parent = destination
            .parent()
            .ok_or("Backup folder is unavailable.")?;
        let staging = parent.join(format!(".skillissue-backup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&staging).map_err(database_error)?;
        let temporary = staging.join("sessions.sqlite3");
        let result = (|| {
            let filename = temporary
                .to_str()
                .ok_or("Backup path must be valid Unicode.")?;
            self.connection
                .execute("VACUUM main INTO ?1", [filename])
                .map_err(database_error)?;
            let copy =
                Connection::open_with_flags(&temporary, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                    .map_err(database_error)?;
            let integrity: String = copy
                .query_row("PRAGMA quick_check", [], |row| row.get(0))
                .map_err(database_error)?;
            if integrity != "ok" {
                return Err("Backup database integrity check failed.".into());
            }
            drop(copy);
            std::fs::OpenOptions::new()
                .write(true)
                .open(&temporary)
                .and_then(|file| file.sync_all())
                .map_err(database_error)?;
            // Same-directory staging keeps this on one volume. A hard link publishes
            // the complete file atomically and fails if the destination already exists.
            std::fs::hard_link(&temporary, destination).map_err(database_error)
        })();
        let _ = std::fs::remove_file(&temporary);
        let _ = std::fs::remove_dir(&staging);
        result
    }
}
