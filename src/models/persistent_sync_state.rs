use super::{FileEntry, SyncState};
use chrono::Utc;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct PersistentSyncState {
    state: Arc<Mutex<SyncState>>,
    storage_path: std::path::PathBuf,
    auto_save: Arc<Mutex<bool>>,
}

impl PersistentSyncState {
    pub fn new(state: SyncState, storage_path: impl AsRef<Path>) -> Self {
        Self {
            state: Arc::new(Mutex::new(state)),
            storage_path: storage_path.as_ref().to_path_buf(),
            auto_save: Arc::new(Mutex::new(true)),
        }
    }

    /// Create with auto-save disabled (useful for batch operations)
    pub fn new_no_auto_save(state: SyncState, storage_path: impl AsRef<Path>) -> Self {
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
    pub async fn add_file(&self, path: String, hash: String) -> Result<(), String> {
        {
            let mut state = self.state.lock().await;
            state.add_file(path, hash);
        }
        let auto_save = *self.auto_save.lock().await;
        if auto_save { self.save().await } else { Ok(()) }
    }

    /// Mark a file as deleted and auto-save
    pub async fn delete_file(&self, path: &str) -> Result<(), String> {
        {
            let mut state = self.state.lock().await;
            state.delete_file(path);
        }
        let auto_save = *self.auto_save.lock().await;
        if auto_save { self.save().await } else { Ok(()) }
    }

    /// Update file hash and auto-save
    pub async fn update_file(&self, path: &str, new_hash: String) -> Result<(), String> {
        {
            let mut state = self.state.lock().await;
            state.update_file(path, new_hash);
        }
        let auto_save = *self.auto_save.lock().await;
        if auto_save { self.save().await } else { Ok(()) }
    }

    /// Remove a file entry completely and auto-save
    pub async fn remove_file(&self, path: &str) -> Result<(), String> {
        {
            let mut state = self.state.lock().await;
            state.remove_file(path);
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
        state.get_file(path).cloned()
    }

    /// Check if a file exists and is not deleted
    pub async fn file_exists(&self, path: &str) -> bool {
        let state = self.state.lock().await;
        state.file_exists(path)
    }

    /// Get the number of files in the state
    pub async fn file_count(&self) -> usize {
        let state = self.state.lock().await;
        state.files.len()
    }

    /// Manually save the current state to disk
    pub async fn save(&self) -> Result<(), String> {
        let state = self.state.lock().await;
        crate::storage::write_json(&self.storage_path.join("state.json"), &*state)
    }

    /// Force a refresh of last_sync timestamp and save
    pub async fn touch(&self) -> Result<(), String> {
        {
            let mut state = self.state.lock().await;
            state.last_sync = Utc::now();
        }
        let auto_save = *self.auto_save.lock().await;
        if auto_save { self.save().await } else { Ok(()) }
    }
}
