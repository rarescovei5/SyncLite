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
}

impl FileEntry {
    /// Create a new file entry for an existing file
    pub fn new_file(hash: String) -> Self {
        Self {
            hash: Some(hash),
            is_deleted: false,
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
    pub fn from_directory(path: &str) -> Result<Self, String> {
        let mut sync_state = Self::new();

        let base_path = Path::new(path);
        if !base_path.exists() {
            return Err(format!("Path does not exist: {}", path));
        }
        if !base_path.is_dir() {
            return Err(format!("Path is not a directory: {}", path));
        }

        scan_directory_recursive(&base_path, &base_path, &mut sync_state.files)?;

        Ok(sync_state)
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
                    files.insert(relative_path, FileEntry::new_file(hash));
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
