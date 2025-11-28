use crate::{
    app,
    models::{FileEntry, PeersState},
    utils::{Log, write_json},
};

use colored::Colorize;
use std::{collections::HashMap, fs, path::Path};

pub enum InitialiseStorageError {
    FailedToCheckConflicts(String),
    PromptDirectoryCreation(String),
    FailedToCreateStorageDirectory(String),
    FailedToCreateStorageFiles(String),
    FailedToAddDirectoryToRegistry(String),
}

/// Initialize storage in the specified directory
pub fn initialise_storage(abs_workspace_path: &Path) -> Result<(), InitialiseStorageError> {
    Log::wrench("Checking Directory Status:".bright_yellow(), None);

    let abs_storage_dir = abs_workspace_path.join(".synclite");

    if abs_storage_dir.exists() {
        Log::info(".synclite folder detected", Some(5));
    } else {
        Log::info("No .synclite folder detected", Some(5));
        check_conflicts_and_cleanup(abs_workspace_path)
            .map_err(|e| InitialiseStorageError::FailedToCheckConflicts(e))?;
        prompt_directory_creation()
            .map_err(|e| InitialiseStorageError::PromptDirectoryCreation(e))?;
        create_storage_directory(&abs_storage_dir)
            .map_err(|e| InitialiseStorageError::FailedToCreateStorageDirectory(e))?;
    }

    print!("\n");
    Log::wrench(
        "Checking contents of .synclite folder:".bright_yellow(),
        None,
    );

    create_storage_files(&abs_storage_dir)
        .map_err(|e| InitialiseStorageError::FailedToCreateStorageFiles(e))?;

    // Register the directory in the global registry
    app::add_directory(abs_workspace_path)
        .map_err(|e| InitialiseStorageError::FailedToAddDirectoryToRegistry(e))?;

    Ok(())
}

/// Check for conflicts with existing synclite directories and cleanup registry
fn check_conflicts_and_cleanup(abs_workspace_path: &Path) -> Result<(), String> {
    // Clean up the registry first (remove stale entries)
    app::cleanup_registry()?;

    // Check for conflicts with existing synclite directories using the registry
    let (has_conflict, conflicting_dir) = app::check_path_conflicts(abs_workspace_path)
        .map_err(|e| format!("Failed to check path conflicts: {}", e))?;

    if has_conflict {
        if let Some(conflicting_path) = conflicting_dir {
            Log::error(
                "Cannot initialize synclite here due to a conflict with an existing synclite directory.",
                None,
            );
            Log::info(
                &format!("Conflicting synclite directory: {}", conflicting_path),
                Some(5),
            );
            Log::info("Please either:", Some(5));
            Log::info("  1. Use the existing synclite directory, or", Some(6));
            Log::info("  2. Remove the existing .synclite directory, or", Some(6));
            Log::info(
                "  3. Choose a different directory outside of the synclite workspace",
                Some(6),
            );
            return Err("Path conflict detected".into());
        }
    }

    Log::success("Directory is valid for initialization", Some(5));

    Ok(())
}

/// Get user confirmation to proceed with initialization
/// Returns Ok(()) if the user confirms, Err(String) if the user does not confirm
fn prompt_directory_creation() -> Result<(), String> {
    Log::question("Do you want to initialize it? (y/n): ", Some(5));
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|e| format!("Failed to read line: {}", e))?;
    if answer.trim().to_lowercase() == "y" || answer.trim().to_lowercase() == "yes" {
        return Ok(());
    } else {
        return Err("User did not confirm initialization".into());
    }
}

/// Create the .synclite storage directory
fn create_storage_directory(abs_storage_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(abs_storage_dir)
        .map_err(|e| format!("Failed to create .synclite directory: {}", e))?;

    Log::info(
        &format!("Created .synclite directory: {}", abs_storage_dir.display()),
        Some(5),
    );

    #[cfg(windows)]
    make_hidden_windows(abs_storage_dir)?;

    Ok(())
}

/// Create configuration files (peers.json and state.json) if they do not exist
fn create_storage_files(abs_storage_dir: &Path) -> Result<(), String> {
    // Create the peers file
    if !abs_storage_dir.join("peers.json").exists() {
        Log::info(
            &format!(
                "Creating peers.json file at: {}",
                abs_storage_dir.join("peers.json").display()
            ),
            Some(5),
        );
        write_json(&abs_storage_dir.join("peers.json"), &PeersState::new())?;
    } else {
        Log::info(
            &format!(
                "File peers.json already exists: {}, correcting contents...",
                abs_storage_dir.join("peers.json").display()
            ),
            Some(5),
        );
    }

    // Create the state file
    if !abs_storage_dir.join("state.json").exists() {
        Log::info(
            &format!(
                "Creating state.json file at: {}",
                abs_storage_dir.join("state.json").display()
            ),
            Some(5),
        );
        write_json(
            &abs_storage_dir.join("state.json"),
            &HashMap::<String, FileEntry>::new(),
        )?;
    } else {
        Log::info(
            &format!(
                "state.json already exists: {}",
                abs_storage_dir.join("state.json").display()
            ),
            Some(5),
        );
    }

    Ok(())
}

#[cfg(windows)]
fn make_hidden_windows(abs_storage_dir: &Path) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::fileapi::SetFileAttributesW;
    use winapi::um::winnt::FILE_ATTRIBUTE_HIDDEN;

    let wide_path: Vec<u16> = OsStr::new(abs_storage_dir)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let result = unsafe { SetFileAttributesW(wide_path.as_ptr(), FILE_ATTRIBUTE_HIDDEN) };

    if result == 0 {
        return Err(format!(
            "Failed to set hidden attribute on Windows: {}",
            abs_storage_dir.display()
        ));
    }

    Ok(())
}
