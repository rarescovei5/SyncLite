use crate::models::SyncState;
use crate::storage::{read_sync_state, write_sync_state};
use crate::utils::{output::CliOutput, unwrap_or_exit};
use colored::Colorize;
use std::path::Path;

pub fn initialise_state(workspace_path: &Path) {
    let result_sync_state = unwrap_or_exit(compute_hashes(workspace_path));

    let storage_dir = workspace_path.join(".synclite");
    unwrap_or_exit(write_sync_state(&storage_dir, &result_sync_state));

    CliOutput::success(
        "Successfully computed hashes for all files".bright_green(),
        None,
    );
}

pub fn compute_hashes(workspace_path: &Path) -> Result<SyncState, String> {
    let storage_dir = workspace_path.join(".synclite");
    let stored_sync_state = read_sync_state(&storage_dir);
    let mut result_sync_state = unwrap_or_exit(SyncState::from_directory(workspace_path));

    // Add tombstone markers for files that are deleted
    for (file_path, file_entry) in &stored_sync_state.files {
        if !result_sync_state.files.contains_key(file_path) {
            result_sync_state.files.insert(file_path.clone(), {
                let mut file_entry = file_entry.clone();
                file_entry.is_deleted = true;
                file_entry.hash = None;
                file_entry
            });
        }
    }

    // Increment clock for files that have been modified
    let files_to_update: Vec<_> = result_sync_state
        .files
        .iter()
        .map(|(path, entry)| {
            if let Some(stored_file_entry) = stored_sync_state.files.get(path) {
                // File exists in both states - check if it has been modified
                let mut needs_clock_increment = false;

                // Check if deletion status changed
                if entry.is_deleted != stored_file_entry.is_deleted {
                    needs_clock_increment = true;
                }
                // Check if hash is different (content changed)
                else if entry.hash != stored_file_entry.hash {
                    needs_clock_increment = true;
                }

                let mut updated_entry = entry.clone();
                if needs_clock_increment {
                    updated_entry.clock = stored_file_entry.clock + 1;
                } else {
                    // No changes detected - preserve the stored clock value
                    updated_entry.clock = stored_file_entry.clock;
                }
                (path.clone(), updated_entry)
            } else {
                // File is new (doesn't exist in stored state) - start with clock = 1
                let mut updated_entry = entry.clone();
                updated_entry.clock = 1;
                (path.clone(), updated_entry)
            }
        })
        .collect();

    // Apply the updates
    for (file_path, updated_entry) in files_to_update {
        result_sync_state.files.insert(file_path, updated_entry);
    }

    Ok(result_sync_state)
}
