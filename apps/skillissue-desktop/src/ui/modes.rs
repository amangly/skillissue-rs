// Rust | apps/skillissue-desktop/src/ui/modes.rs | GPUI Kit | native mode and document editing
use super::*;
use skillissue_core::documents::{DocumentMetadata, MAX_FILE_BYTES};
use std::io::Read;

pub(super) struct SectionEditor {
    pub(super) id: String,
    pub(super) title: Entity<InputState>,
    pub(super) prompt: Entity<TextareaState>,
}

enum EditorReply {
    Status(Vec<(String, Result<DocumentMetadata, String>)>),
    Imported(Result<Option<DocumentMetadata>, String>),
    Saved {
        delete: bool,
        fingerprint: String,
        result: Result<Box<ConfigView>, String>,
    },
}

pub(super) struct ModeEditor {
    pub(super) id: String,
    pub(super) name: Entity<InputState>,
    pub(super) prompt: Entity<TextareaState>,
    pub(super) sections: Vec<SectionEditor>,
    pub(super) files: Vec<ModeFile>,
    original_files: Vec<String>,
    imported_files: Vec<String>,
    pub(super) statuses: BTreeMap<String, Result<DocumentMetadata, String>>,
    sender: mpsc::Sender<EditorReply>,
    receiver: mpsc::Receiver<EditorReply>,
    pub(super) pending: bool,
    pub(super) saving: bool,
    pub(super) existing: bool,
    pub(super) delete_confirm: bool,
    pub(super) saved: bool,
    fingerprint: String,
    saved_fingerprint: String,
    edited_at: u64,
    save_failed: bool,
}

impl SectionEditor {
    pub(super) fn new(
        section: ModeSection,
        window: &mut Window,
        cx: &mut Context<NativeView>,
    ) -> Self {
        Self {
            id: section.id,
            title: cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(section.title)
                    .placeholder("Section title")
            }),
            prompt: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .rows(3)
                    .default_value(section.prompt)
                    .placeholder("What should notes capture in this section?")
            }),
        }
    }
}

fn editor_fingerprint(editor: &ModeEditor, cx: &App) -> String {
    serde_json::json!({"name":editor.name.read(cx).value().as_ref(),"prompt":editor.prompt.read(cx).value().as_ref(),
        "sections":editor.sections.iter().map(|section|(section.id.clone(),section.title.read(cx).value().to_string(),section.prompt.read(cx).value().to_string())).collect::<Vec<_>>(),
        "files":editor.files}).to_string()
}

fn commit_mode_change(
    engine: &NativeEngine,
    mode: AssistantMode,
    delete: bool,
) -> Result<ConfigView, String> {
    engine.update_preferences(move |settings| {
        if mode.id == "general" {
            if delete {
                return Err("The built-in mode cannot be deleted.".into());
            }
            settings.system_prompt = mode.prompt;
        } else if delete {
            settings.modes.retain(|current| current.id != mode.id);
            if settings.active_mode_id == mode.id {
                settings.active_mode_id = "general".into();
            }
        } else if let Some(current) = settings
            .modes
            .iter_mut()
            .find(|current| current.id == mode.id)
        {
            *current = mode;
        } else {
            settings.modes.push(mode);
        }
        Ok(())
    })
}

fn import_reference(
    engine: &NativeEngine,
    path: std::path::PathBuf,
) -> Result<Option<DocumentMetadata>, String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Choose a file with a valid Unicode filename.")?
        .to_owned();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "txt" | "pdf" | "docx") {
        return Err("Reference files must be TXT, PDF, or DOCX.".into());
    }
    let file = std::fs::File::open(&path).map_err(|error| format!("Open reference: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("Read reference metadata: {error}"))?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_FILE_BYTES as u64 {
        return Err("Choose a nonempty reference no larger than 20 MiB.".into());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Read reference: {error}"))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err("Reference grew beyond 20 MiB while reading.".into());
    }
    engine
        .import_document(&uuid::Uuid::new_v4().to_string(), &name, &bytes)
        .map(Some)
}

impl NativeView {
    pub(super) fn import_mode_path(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        let Some(engine) = self.engine.clone() else {
            return;
        };
        let Some(editor) = &mut self.mode_editor else {
            return;
        };
        if editor.pending || editor.saving {
            self.error = Some("Wait for the current reference operation to finish.".into());
            cx.notify();
            return;
        }
        if editor.files.len() >= 3 {
            self.error = Some("You've reached the file limit (3 files).".into());
            cx.notify();
            return;
        }
        editor.pending = true;
        let sender = editor.sender.clone();
        self.runtime.spawn_blocking(move || {
            let _ = sender.send(EditorReply::Imported(import_reference(&engine, path)));
        });
        cx.notify();
    }
    pub(super) fn mode_save_failed(&self) -> bool {
        self.mode_editor
            .as_ref()
            .is_some_and(|editor| editor.save_failed)
    }
    pub(super) fn sync_mode_draft(&mut self, cx: &App) {
        if let Some(editor) = &mut self.mode_editor {
            let fingerprint = editor_fingerprint(editor, cx);
            if fingerprint != editor.fingerprint {
                editor.fingerprint = fingerprint;
                editor.saved = false;
                editor.edited_at = now_ms();
                editor.save_failed = false;
            }
        }
    }
    pub(super) fn prepare_mode_shutdown(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(editor) = &mut self.mode_editor else {
            return true;
        };
        let fingerprint = editor_fingerprint(editor, cx);
        if fingerprint != editor.fingerprint {
            editor.fingerprint = fingerprint;
            editor.saved = false;
        }
        if editor.saved && !editor.pending && !editor.saving {
            return true;
        }
        let save = !editor.pending && !editor.saving;
        if save {
            self.save_mode_editor(false, cx);
        }
        false
    }
    pub(super) fn begin_mode_editor(
        &mut self,
        id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let existing = id.is_some();
        let mode = if id.as_deref() == Some("general") {
            AssistantMode {
                id: "general".into(),
                name: "General assistant".into(),
                prompt: self.settings.system_prompt.clone(),
                sections: vec![],
                files: vec![],
            }
        } else if let Some(id) = id {
            let Some(mode) = self
                .settings
                .modes
                .iter()
                .find(|mode| mode.id == id)
                .cloned()
            else {
                return;
            };
            mode
        } else {
            AssistantMode {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Untitled mode".into(),
                prompt: String::new(),
                sections: vec![],
                files: vec![],
            }
        };
        let (sender, receiver) = mpsc::channel();
        let files = mode.files;
        self.mode_editor = Some(ModeEditor {
            id: mode.id,
            name: cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(mode.name)
                    .placeholder("Mode name")
            }),
            prompt: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .rows(8)
                    .default_value(mode.prompt)
                    .placeholder("How should skillissue help in this conversation?")
            }),
            sections: mode
                .sections
                .into_iter()
                .map(|section| SectionEditor::new(section, window, cx))
                .collect(),
            original_files: files.iter().map(|file| file.id.clone()).collect(),
            files,
            imported_files: vec![],
            statuses: BTreeMap::new(),
            sender: sender.clone(),
            receiver,
            pending: false,
            saving: false,
            existing,
            delete_confirm: false,
            saved: existing,
            fingerprint: String::new(),
            saved_fingerprint: String::new(),
            edited_at: now_ms(),
            save_failed: false,
        });
        if let Some(editor) = &mut self.mode_editor {
            let fingerprint = editor_fingerprint(editor, cx);
            editor.fingerprint = fingerprint.clone();
            if existing {
                editor.saved_fingerprint = fingerprint;
            }
        }
        if let Some(engine) = self.engine.clone() {
            let ids = self.mode_editor.as_ref().unwrap().original_files.clone();
            self.runtime.spawn_blocking(move || {
                let results = ids
                    .into_iter()
                    .map(|id| {
                        let status = engine.document_status(&id);
                        (id, status)
                    })
                    .collect();
                let _ = sender.send(EditorReply::Status(results));
            });
        }
        self.error = None;
        cx.notify();
    }

    pub(super) fn poll_mode_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let replies: Vec<_> = self
            .mode_editor
            .as_ref()
            .map(|editor| editor.receiver.try_iter().collect())
            .unwrap_or_default();
        let mut dirty = !replies.is_empty();
        for reply in replies {
            match reply {
                EditorReply::Status(statuses) => {
                    if let Some(editor) = &mut self.mode_editor {
                        editor.statuses.extend(statuses);
                    }
                }
                EditorReply::Imported(result) => {
                    if let Some(editor) = &mut self.mode_editor {
                        editor.pending = false;
                        match result {
                            Ok(Some(metadata)) => {
                                editor.imported_files.push(metadata.id.clone());
                                editor.files.push(ModeFile {
                                    id: metadata.id.clone(),
                                    name: metadata.name.clone(),
                                    size: metadata.size,
                                });
                                editor.statuses.insert(metadata.id.clone(), Ok(metadata));
                            }
                            Ok(None) => {}
                            Err(error) => self.error = Some(error),
                        }
                    }
                }
                EditorReply::Saved {
                    delete,
                    fingerprint,
                    result: Ok(config),
                } => {
                    if delete {
                        self.mode_editor = None;
                        self.dashboard.modes_tab = "general".into();
                    } else if let Some(editor) = &mut self.mode_editor {
                        editor.saving = false;
                        editor.existing = true;
                        editor.saved_fingerprint = fingerprint;
                        editor.saved = editor.fingerprint == editor.saved_fingerprint;
                        editor.save_failed = false;
                        editor.original_files = config
                            .settings
                            .modes
                            .iter()
                            .find(|mode| mode.id == editor.id)
                            .map(|mode| mode.files.iter().map(|file| file.id.clone()).collect())
                            .unwrap_or_default();
                        editor.imported_files.clear();
                    }
                    self.set_config(*config, window, cx);
                    self.status = "Mode saved locally".into();
                }
                EditorReply::Saved {
                    result: Err(error), ..
                } => {
                    if let Some(editor) = &mut self.mode_editor {
                        editor.saving = false;
                        editor.save_failed = true;
                    }
                    self.error = Some(error);
                }
            }
        }
        let mut save = false;
        if let Some(editor) = &mut self.mode_editor {
            let fingerprint = editor_fingerprint(editor, cx);
            if fingerprint != editor.fingerprint {
                editor.fingerprint = fingerprint;
                editor.edited_at = now_ms();
                editor.saved = false;
                editor.save_failed = false;
                dirty = true;
            }
            save = !editor.pending
                && !editor.saving
                && !editor.save_failed
                && editor.fingerprint != editor.saved_fingerprint
                && now_ms().saturating_sub(editor.edited_at) >= 250;
        }
        if save {
            self.save_mode_editor(false, cx);
            dirty = true;
        }
        dirty
    }

    pub(super) fn import_mode_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(engine) = self.engine.clone() else {
            return;
        };
        let Some(editor) = &mut self.mode_editor else {
            return;
        };
        if editor.pending || editor.saving || editor.files.len() >= 3 {
            return;
        }
        editor.pending = true;
        let sender = editor.sender.clone();
        let runtime = self.runtime.clone();
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose a TXT, PDF or DOCX reference (up to 20 MiB)".into()),
        });
        cx.spawn_in(window, async move |_, _| {
            let selected = match selection.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => {
                    let _ = sender.send(EditorReply::Imported(Ok(None)));
                    return;
                }
                Ok(Err(error)) => {
                    let _ = sender.send(EditorReply::Imported(Err(format!(
                        "Open file picker: {error}"
                    ))));
                    return;
                }
                Err(error) => {
                    let _ = sender.send(EditorReply::Imported(Err(format!(
                        "File picker closed: {error}"
                    ))));
                    return;
                }
            };
            runtime.spawn_blocking(move || {
                let result = match selected {
                    Some(path) => import_reference(&engine, path),
                    None => Ok(None),
                };
                let _ = sender.send(EditorReply::Imported(result));
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn save_mode_editor(&mut self, delete: bool, cx: &mut Context<Self>) {
        let Some(engine) = self.engine.clone() else {
            return;
        };
        let Some(editor) = &mut self.mode_editor else {
            return;
        };
        if editor.pending || editor.saving {
            return;
        }
        let id = editor.id.clone();
        let name = editor.name.read(cx).value().to_string();
        let prompt = editor.prompt.read(cx).value().to_string();
        let files = editor.files.clone();
        let sections: Vec<_> = editor
            .sections
            .iter()
            .map(|section| ModeSection {
                id: section.id.clone(),
                title: section.title.read(cx).value().to_string(),
                prompt: section.prompt.read(cx).value().to_string(),
            })
            .collect();
        let cleanup: Vec<_> = editor
            .original_files
            .iter()
            .chain(&editor.imported_files)
            .filter(|id| delete || !files.iter().any(|file| &file.id == *id))
            .cloned()
            .collect();
        let sender = editor.sender.clone();
        let fingerprint = editor_fingerprint(editor, cx);
        editor.saving = true;
        editor.save_failed = false;
        self.error = None;
        self.runtime.spawn_blocking(move || {
            let result = (|| {
                let updated = AssistantMode {
                    id,
                    name,
                    prompt,
                    sections,
                    files,
                };
                let mut config = commit_mode_change(&engine, updated, delete)?;
                // Detach references durably before deleting originals from the separate library.
                for id in cleanup {
                    if config
                        .settings
                        .modes
                        .iter()
                        .any(|mode| mode.files.iter().any(|file| file.id == id))
                    {
                        continue;
                    }
                    if let Err(error) = engine.remove_document(&id) {
                        config.warnings.push(format!(
                            "Mode saved, but unused reference cleanup failed: {error}"
                        ));
                    }
                }
                Ok(config)
            })();
            let _ = sender.send(EditorReply::Saved {
                delete,
                fingerprint,
                result: result.map(Box::new),
            });
        });
        cx.notify();
    }

    pub(super) fn discard_mode_editor(&mut self, cx: &mut Context<Self>) {
        if self
            .mode_editor
            .as_ref()
            .is_some_and(|editor| editor.pending || editor.saving)
        {
            return;
        }
        let Some(editor) = self.mode_editor.take() else {
            return;
        };
        if !editor.imported_files.is_empty() {
            self.background(move |engine| {
                Reply::Status(
                    editor
                        .imported_files
                        .iter()
                        .try_for_each(|id| engine.remove_document(id)),
                )
            });
        }
        self.error = None;
        cx.notify();
    }
}

#[cfg(test)]
mod transaction_tests {
    use super::commit_mode_change;
    use skillissue_core::NativeEngine;
    use skillissue_domain::models::{AssistantMode, Settings};
    use std::path::PathBuf;

    struct Profile(PathBuf);
    impl Profile {
        fn new() -> Self {
            let directory =
                std::env::temp_dir().join(format!("skillissue-mode-edit-{}", uuid::Uuid::new_v4()));
            let settings = Settings {
                api_base_url: "http://127.0.0.1:9/v1".into(),
                microphone: false,
                system_audio: false,
                include_screenshot: false,
                ..Default::default()
            };
            skillissue_core::config::save(&directory.join("settings.json"), &settings).unwrap();
            Self(directory)
        }
    }
    impl Drop for Profile {
        fn drop(&mut self) {
            assert!(self.0.is_absolute());
            assert_eq!(self.0.parent(), Some(std::env::temp_dir().as_path()));
            assert!(
                self.0
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("skillissue-mode-edit-")
            );
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn mode(id: &str, prompt: &str) -> AssistantMode {
        AssistantMode {
            id: id.into(),
            name: id.into(),
            prompt: prompt.into(),
            sections: vec![],
            files: vec![],
        }
    }

    #[tokio::test]
    async fn saving_a_captured_mode_draft_keeps_newer_preferences_and_other_modes() {
        let profile = Profile::new();
        let engine = NativeEngine::open(profile.0.clone()).unwrap();
        engine
            .update_preferences(|settings| {
                settings.modes = vec![mode("edited", "Initial prompt"), mode("other", "Other")];
                settings.active_mode_id = "edited".into();
                Ok(())
            })
            .unwrap();
        let mut draft = engine.config().unwrap().settings.modes[0].clone();
        draft.prompt = "Draft prepared before the other changes".into();
        engine
            .update_preferences(|settings| {
                settings.theme = "dark".into();
                settings.active_mode_id = "other".into();
                settings
                    .shortcuts
                    .insert("ask".into(), "Ctrl+Alt+F20".into());
                settings.modes[1].prompt = "A newer change to another mode".into();
                Ok(())
            })
            .unwrap();

        let saved = commit_mode_change(&engine, draft, false).unwrap().settings;
        assert_eq!(saved.theme, "dark");
        assert_eq!(saved.active_mode_id, "other");
        assert_eq!(saved.shortcuts["ask"], "Ctrl+Alt+F20");
        assert_eq!(
            saved.modes[0].prompt,
            "Draft prepared before the other changes"
        );
        assert_eq!(saved.modes[1].prompt, "A newer change to another mode");
        let disk = skillissue_core::config::load(&profile.0.join("settings.json")).unwrap();
        assert_eq!(
            serde_json::to_value(saved).unwrap(),
            serde_json::to_value(disk).unwrap()
        );
        engine.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn mode_deletion_respects_the_latest_active_selection_and_builtin_rejection() {
        let profile = Profile::new();
        let engine = NativeEngine::open(profile.0.clone()).unwrap();
        engine
            .update_preferences(|settings| {
                settings.modes = vec![mode("deleted", "Old"), mode("active", "Keep")];
                settings.active_mode_id = "active".into();
                settings.theme = "dark".into();
                Ok(())
            })
            .unwrap();
        let saved = commit_mode_change(&engine, mode("deleted", ""), true).unwrap();
        assert_eq!(saved.settings.active_mode_id, "active");
        assert_eq!(saved.settings.modes.len(), 1);
        let saved = commit_mode_change(&engine, mode("active", ""), true).unwrap();
        assert_eq!(saved.settings.active_mode_id, "general");
        assert!(saved.settings.modes.is_empty());
        assert_eq!(saved.settings.theme, "dark");

        let before = std::fs::read(profile.0.join("settings.json")).unwrap();
        assert!(commit_mode_change(&engine, mode("general", ""), true).is_err());
        assert_eq!(
            std::fs::read(profile.0.join("settings.json")).unwrap(),
            before
        );
        assert_eq!(engine.config().unwrap().settings.theme, "dark");
        engine.shutdown().await.unwrap();
    }
}
