//! Native backup and restore dialogs and background transfer dispatch.
use super::*;

impl NativeView {
    pub(super) fn restore_history(&mut self, cx: &mut Context<Self>) {
        if self.history_transfer_pending {
            return;
        }
        let Some(engine) = self.engine.clone() else {
            return;
        };
        self.history_transfer_pending = true;
        self.error = None;
        cx.notify();
        let chooser = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(
                "Restore saved history from a skillissue SQLite backup (empty history only)".into(),
            ),
        });
        let sender = self.sender.clone();
        self.runtime.spawn(async move {
            let result = match chooser.await {
                Ok(Ok(Some(paths))) => {
                    let Some(path) = paths.into_iter().next() else {
                        let _ = sender.send(Reply::HistoryTransferCancelled);
                        return;
                    };
                    tokio::task::spawn_blocking(move || engine.restore_history(&path))
                        .await
                        .map_err(|error| format!("Restore task: {error}"))
                        .and_then(|result| result)
                }
                Ok(Ok(None)) => {
                    let _ = sender.send(Reply::HistoryTransferCancelled);
                    return;
                }
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = sender.send(Reply::HistoryRestored(result));
        });
    }

    pub(super) fn backup_history(&mut self, cx: &mut Context<Self>) {
        if self.history_transfer_pending {
            return;
        }
        let Some(engine) = self.engine.clone() else {
            return;
        };
        self.history_transfer_pending = true;
        self.error = None;
        cx.notify();
        let directory = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.data_dir.clone());
        let chooser = cx.prompt_for_new_path(
            &directory,
            Some(&format!("skillissue-history-{}.sqlite3", now_ms())),
        );
        let sender = self.sender.clone();
        self.runtime.spawn(async move {
            let result = match chooser.await {
                Ok(Ok(Some(path))) => tokio::task::spawn_blocking(move || {
                    engine.backup_history(&path)?;
                    Ok(path)
                })
                .await
                .map_err(|error| format!("Backup task: {error}"))
                .and_then(|result| result),
                Ok(Ok(None)) => {
                    let _ = sender.send(Reply::HistoryTransferCancelled);
                    return;
                }
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = sender.send(Reply::HistoryBackup(result));
        });
    }
}
