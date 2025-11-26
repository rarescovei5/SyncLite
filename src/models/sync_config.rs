use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::sandboxed::FileSystem;
use crate::sync::calculate_file_hash;
use crate::utils::write_json;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileEntry {
    pub hash: Option<String>,
    pub is_deleted: bool,
    pub last_modified: DateTime<Utc>,
}

impl FileEntry {
    pub fn deleted(&self) -> Self {
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

    // ===== UNIFIED FILESYSTEM + STATE METHODS =====
    // These methods keep filesystem and sync state in sync automatically

    /// Write a file to disk AND update sync state (unified operation)
    ///
    /// # Arguments
    /// * `fs` - FileSystem reference for sandboxed operations
    /// * `workspace_path` - Absolute path to workspace root
    /// * `relative_path` - Relative path from workspace root
    /// * `content` - File content to write
    pub async fn sync_write_file(
        &self,
        fs: &FileSystem,
        workspace_path: &Path,
        relative_path: &str,
        content: &str,
    ) -> Result<(), String> {
        let full_path = workspace_path.join(relative_path);

        // Create parent directory if needed
        if let Some(parent) = full_path.parent() {
            fs.create_directory(parent)
                .await
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }

        // Write file to filesystem
        fs.write_file(&full_path, content)
            .await
            .map_err(|e| format!("Failed to write file: {}", e))?;

        // Calculate hash and update state
        let hash = calculate_file_hash(&full_path)
            .map_err(|e| format!("Failed to calculate hash: {}", e))?;

        self.update_file(relative_path, hash).await
    }

    /// Delete a file from disk AND mark as deleted in sync state (unified operation)
    ///
    /// # Arguments
    /// * `fs` - FileSystem reference for sandboxed operations
    /// * `workspace_path` - Absolute path to workspace root
    /// * `relative_path` - Relative path from workspace root
    pub async fn sync_delete_file(
        &self,
        fs: &FileSystem,
        workspace_path: &Path,
        relative_path: &str,
    ) -> Result<(), String> {
        let full_path = workspace_path.join(relative_path);

        // Delete file from filesystem
        fs.delete_file(&full_path)
            .await
            .map_err(|e| format!("Failed to delete file: {}", e))?;

        // Mark as deleted in state
        self.delete_file(relative_path).await
    }

    /// Write multiple files to disk AND update sync state (batch unified operation)
    ///
    /// # Arguments
    /// * `fs` - FileSystem reference for sandboxed operations
    /// * `workspace_path` - Absolute path to workspace root
    /// * `files` - HashMap of relative paths to file contents
    pub async fn sync_batch_write_files(
        &self,
        fs: &FileSystem,
        workspace_path: &Path,
        files: &HashMap<String, String>,
    ) -> Result<(), String> {
        // Write all files to filesystem first
        for (relative_path, content) in files {
            let full_path = workspace_path.join(relative_path);

            // Create parent directory if needed
            if let Some(parent) = full_path.parent() {
                fs.create_directory(parent).await.map_err(|e| {
                    format!("Failed to create directory for {}: {}", relative_path, e)
                })?;
            }

            // Write file
            fs.write_file(&full_path, content)
                .await
                .map_err(|e| format!("Failed to write file {}: {}", relative_path, e))?;
        }

        // Update all state entries in a batch
        self.batch_operations(|state| {
            for (relative_path, _) in files {
                let full_path = workspace_path.join(relative_path);
                if let Ok(hash) = calculate_file_hash(&full_path) {
                    state.insert(
                        relative_path.clone(),
                        FileEntry {
                            hash: Some(hash),
                            is_deleted: false,
                            last_modified: Utc::now(),
                        },
                    );
                }
            }
        })
        .await
    }

    /// Delete multiple files from disk AND mark as deleted in sync state (batch unified operation)
    ///
    /// # Arguments
    /// * `fs` - FileSystem reference for sandboxed operations
    /// * `workspace_path` - Absolute path to workspace root
    /// * `relative_paths` - List of relative paths to delete
    /// * `peer_sync_state` - Optional peer state to copy timestamps from
    pub async fn sync_batch_delete_files(
        &self,
        fs: &FileSystem,
        workspace_path: &Path,
        relative_paths: &[String],
        peer_sync_state: Option<&SyncState>,
    ) -> Result<(), String> {
        // Delete all files from filesystem first
        for relative_path in relative_paths {
            let full_path = workspace_path.join(relative_path);
            // Ignore errors if file doesn't exist
            let _ = fs.delete_file(&full_path).await;
        }

        // Update all state entries in a batch
        self.batch_operations(|state| {
            for relative_path in relative_paths {
                let last_modified = if let Some(peer_state) = peer_sync_state {
                    peer_state
                        .get(relative_path)
                        .map(|e| e.last_modified)
                        .unwrap_or_else(Utc::now)
                } else {
                    Utc::now()
                };

                state.insert(
                    relative_path.clone(),
                    FileEntry {
                        hash: None,
                        is_deleted: true,
                        last_modified,
                    },
                );
            }
        })
        .await
    }
}
