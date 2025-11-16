use std::collections::HashMap;
use std::{fs, path::Path};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// -------------------------- File Entry --------------------------
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub hash: Option<String>, // None if file is deleted
    pub is_deleted: bool,     // Tombstone marker
    pub clock: u64,
}

impl FileEntry {
    /// Create a new file entry for an existing file
    pub fn new(hash: String) -> Self {
        Self {
            hash: Some(hash),
            is_deleted: false,
            clock: 0,
        }
    }

    pub fn new_deleted() -> Self {
        Self {
            hash: None,
            is_deleted: true,
            clock: 0,
        }
    }

    /// Check if this file entry represents an existing file
    pub fn exists(&self) -> bool {
        !self.is_deleted && self.hash.is_some()
    }
}

// -------------------------- Sync State --------------------------
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub files: HashMap<String, FileEntry>, // path -> file entry
    pub last_sync: DateTime<Utc>,
}

impl SyncState {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            last_sync: Utc::now(),
        }
    }

    /// Create a SyncState by scanning all files in the given directory recursively
    pub fn from_directory(workspace_path: &Path) -> Result<Self, String> {
        let mut sync_state = Self::new();

        let base_path = workspace_path;
        if !base_path.exists() {
            return Err(format!("Path does not exist: {}", workspace_path.display()));
        }
        if !base_path.is_dir() {
            return Err(format!(
                "Path is not a directory: {}",
                workspace_path.display()
            ));
        }

        scan_directory_recursive(&base_path, &base_path, &mut sync_state.files)?;

        Ok(sync_state)
    }
}

impl Default for SyncState {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncState {
    /// Add or update a file entry
    pub fn add_file(&mut self, path: String, hash: String) {
        self.files.insert(path, FileEntry::new(hash));
        self.last_sync = Utc::now();
    }

    /// Mark a file as deleted (tombstone)
    pub fn delete_file(&mut self, path: &str) {
        if let Some(entry) = self.files.get_mut(path) {
            entry.is_deleted = true;
            entry.hash = None;
            entry.clock += 1;
        } else {
            // File wasn't tracked, but we still need a tombstone
            self.files
                .insert(path.to_string(), FileEntry::new_deleted());
        }
        self.last_sync = Utc::now();
    }

    /// Update file hash (when file content changes)
    pub fn update_file(&mut self, path: &str, new_hash: String) {
        if let Some(entry) = self.files.get_mut(path) {
            entry.hash = Some(new_hash);
            entry.is_deleted = false;
            entry.clock += 1;
        } else {
            // New file
            self.files
                .insert(path.to_string(), FileEntry::new(new_hash));
        }
        self.last_sync = Utc::now();
    }

    /// Remove a file entry completely (not just mark as deleted)
    pub fn remove_file(&mut self, path: &str) {
        self.files.remove(path);
        self.last_sync = Utc::now();
    }

    /// Get file entry by path
    pub fn get_file(&self, path: &str) -> Option<&FileEntry> {
        self.files.get(path)
    }

    /// Check if a file exists and is not deleted
    pub fn file_exists(&self, path: &str) -> bool {
        self.files.get(path).map_or(false, |entry| entry.exists())
    }
}

/// Recursively scan a directory and populate the files HashMap
fn scan_directory_recursive(
    current_path: &Path,
    base_path: &Path,
    files: &mut HashMap<String, FileEntry>,
) -> Result<(), String> {
    let entries = fs::read_dir(current_path)
        .map_err(|e| format!("Failed to read directory {}: {}", current_path.display(), e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;

        let entry_path = entry.path();

        // Skip .synclite directory
        if entry_path.file_name().unwrap() == ".synclite" {
            continue;
        }

        if entry_path.is_dir() {
            // Recursively scan subdirectories
            scan_directory_recursive(&entry_path, base_path, files)?;
        } else if entry_path.is_file() {
            // Calculate relative path from base directory
            let relative_path = entry_path
                .strip_prefix(base_path)
                .map_err(|e| format!("Failed to calculate relative path: {}", e))?
                .to_string_lossy()
                .to_string();

            // Calculate file hash
            match calculate_file_hash(&entry_path) {
                Ok(hash) => {
                    files.insert(relative_path, FileEntry::new(hash));
                }
                Err(e) => {
                    // Log warning but continue processing other files
                    eprintln!(
                        "Warning: Failed to hash file {}: {}",
                        entry_path.display(),
                        e
                    );
                }
            }
        }
    }

    Ok(())
}

/// Calculate SHA-256 hash of a file
fn calculate_file_hash(path: &Path) -> Result<String, String> {
    let contents =
        fs::read(path).map_err(|e| format!("Failed to read file {}: {}", path.display(), e))?;

    let mut hasher = Sha256::new();
    hasher.update(&contents);
    let result = hasher.finalize();

    Ok(format!("{:x}", result))
}
