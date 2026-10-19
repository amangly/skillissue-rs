//! Reference-file imports and safe removal after mode detachment.

use super::{NativeEngine, lock};
use crate::documents::DocumentMetadata;

impl NativeEngine {
    pub fn import_document(
        &self,
        id: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<DocumentMetadata, String> {
        self.documents.import(id, name, bytes)
    }
    pub fn document_status(&self, id: &str) -> Result<DocumentMetadata, String> {
        self.documents.status(id)
    }
    pub fn remove_document(&self, id: &str) -> Result<(), String> {
        let _transition = lock(&self.transition)?;
        if lock(&self.configuration)?
            .settings
            .modes
            .iter()
            .any(|mode| mode.files.iter().any(|file| file.id == id))
        {
            return Err(
                "Remove this file from its Modes before deleting the stored document.".into(),
            );
        }
        self.documents.remove(id)
    }
}
