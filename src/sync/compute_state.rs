use std::{fs, path::Path};

use chrono::DateTime;
use sha2::{Digest, Sha256};

use crate::models::{FileEntry, SyncState};

pub fn compute_sync_state(
    workspace_path: &Path,
    current_path: &Path,
    sync_state: &mut SyncState,
) -> Result<(), String> {
    let entries = fs::read_dir(current_path).map_err(|e| {
        format!(
            "Failed to read directory {}: {}",
            workspace_path.display(),
            e
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;

        let entry_path = entry.path();

        // Skip .synclite directory
        if entry_path.file_name().unwrap() == ".synclite" {
            continue;
        }

        if entry_path.is_dir() {
            compute_sync_state(workspace_path, &entry_path, sync_state)?;
        } else {
            // Calculate relative path from base directory
            let relative_path = entry_path
                .strip_prefix(workspace_path)
                .map_err(|e| format!("Failed to calculate relative path: {}", e))?
                .to_string_lossy()
                .to_string();

            let hash = calculate_file_hash(&entry_path)?;

            let last_modified = if let Ok(time) = fs::metadata(&entry_path)
                .map_err(|e| {
                    format!(
                        "Failed to get metadata for file {}: {}",
                        entry_path.display(),
                        e
                    )
                })?
                .modified()
            {
                time
            } else {
                return Err(format!(
                    "Not supported on this platform, consider using a different operating system :)"
                ));
            };

            sync_state.insert(
                relative_path,
                FileEntry {
                    hash: Some(hash),
                    is_deleted: false,
                    last_modified: DateTime::from(last_modified),
                },
            );
        }
    }

    Ok(())
}

pub fn calculate_file_hash(path: &Path) -> Result<String, String> {
    let contents =
        fs::read(path).map_err(|e| format!("Failed to read file {}: {}", path.display(), e))?;

    let mut hasher = Sha256::new();
    hasher.update(&contents);
    let result = hasher.finalize();

    Ok(format!("{:x}", result))
}
