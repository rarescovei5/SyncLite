use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::utils::write_json;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileEntry {
    pub hash: Option<String>,
    pub is_deleted: bool,
    pub last_modified: DateTime<Utc>,
}

impl FileEntry {
    pub fn delete(self) -> Self {
        Self {
            hash: None,
            is_deleted: true,
            last_modified: self.last_modified,
        }
    }
}

pub type SyncState = HashMap<String, FileEntry>;

pub struct SyncConfig {
    state: Arc<Mutex<SyncState>>,
    storage_path: std::path::PathBuf,
    auto_save: Arc<Mutex<bool>>,
}

impl SyncConfig {
    pub fn new(storage_path: impl AsRef<Path>, state: SyncState) -> Self {
        Self {
            state: Arc::new(Mutex::new(state)),
            storage_path: storage_path.as_ref().to_path_buf(),
            auto_save: Arc::new(Mutex::new(true)),
        }
    }

    /// Create with auto-save disabled (useful for batch operations)
    pub fn new_no_auto_save(storage_path: impl AsRef<Path>, state: SyncState) -> Self {
        Self {
            state: Arc::new(Mutex::new(state)),
            storage_path: storage_path.as_ref().to_path_buf(),
            auto_save: Arc::new(Mutex::new(false)),
        }
    }

    /// Enable or disable auto-save
    pub async fn set_auto_save(&self, enabled: bool) {
        let mut auto_save = self.auto_save.lock().await;
        *auto_save = enabled;
    }

    /// Add or update a file entry and auto-save
    pub async fn add_file(&self, path: String, file_entry: FileEntry) -> Result<(), String> {
        {
            let mut state = self.state.lock().await;
            state.insert(path, file_entry);
        }
        let auto_save = *self.auto_save.lock().await;
        if auto_save { self.save().await } else { Ok(()) }
    }

    /// Mark a file as deleted and auto-save
    pub async fn delete_file(&self, path: &str) -> Result<(), String> {
        {
            let mut state = self.state.lock().await;
            state.insert(
                path.to_string(),
                FileEntry {
                    hash: None,
                    is_deleted: true,
                    last_modified: Utc::now(),
                },
            );
        }
        let auto_save = *self.auto_save.lock().await;
        if auto_save { self.save().await } else { Ok(()) }
    }

    /// Update file hash and auto-save
    pub async fn update_file(&self, path: &str, new_hash: String) -> Result<(), String> {
        {
            let mut state = self.state.lock().await;
            state.insert(
                path.to_string(),
                FileEntry {
                    hash: Some(new_hash),
                    is_deleted: false,
                    last_modified: Utc::now(),
                },
            );
        }
        let auto_save = *self.auto_save.lock().await;
        if auto_save { self.save().await } else { Ok(()) }
    }

    /// Batch operations: disable auto-save, run operations, then save once
    pub async fn batch_operations<F>(&self, operations: F) -> Result<(), String>
    where
        F: FnOnce(&mut SyncState),
    {
        let original_auto_save = {
            let auto_save = self.auto_save.lock().await;
            *auto_save
        };

        // Temporarily disable auto-save
        {
            let mut auto_save = self.auto_save.lock().await;
            *auto_save = false;
        }

        // Execute operations
        {
            let mut state = self.state.lock().await;
            operations(&mut state);
        }

        // Restore auto-save setting
        {
            let mut auto_save = self.auto_save.lock().await;
            *auto_save = original_auto_save;
        }

        // Save if auto-save was originally enabled
        if original_auto_save {
            self.save().await
        } else {
            Ok(())
        }
    }

    /// Get a clone of the state for read-only access
    pub async fn state(&self) -> SyncState {
        let state = self.state.lock().await;
        state.clone()
    }

    /// Get specific file entry
    pub async fn get_file(&self, path: &str) -> Option<FileEntry> {
        let state = self.state.lock().await;
        state.get(path).cloned()
    }

    /// Check if a file exists and is not deleted
    pub async fn file_exists(&self, path: &str) -> bool {
        let state = self.state.lock().await;
        state
            .get(path)
            .map(|entry| !entry.is_deleted)
            .unwrap_or(false)
    }

    /// Get the number of files in the state
    pub async fn file_count(&self) -> usize {
        let state = self.state.lock().await;
        state.len()
    }

    /// Manually save the current state to disk
    pub async fn save(&self) -> Result<(), String> {
        let state = self.state.lock().await;
        write_json(&self.storage_path.join("state.json"), &*state)
    }

    /// Force a refresh of last_sync timestamp and save
    pub async fn touch(&self) -> Result<(), String> {
        {
            let mut state = self.state.lock().await;
            for entry in state.values_mut() {
                entry.last_modified = Utc::now();
            }
        }
        let auto_save = *self.auto_save.lock().await;
        if auto_save { self.save().await } else { Ok(()) }
    }
}
