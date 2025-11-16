use crate::{
    app,
    models::{PeersConfig, SyncState},
    storage::{write_peers_config, write_sync_state},
    utils::{confirm, output::CliOutput, unwrap_or_exit},
};

use colored::Colorize;
use std::{fs, path::Path};

/// Initialize storage in the specified directory
pub fn initialise_storage(path: &Path) {
    CliOutput::wrench("Checking Directory Status:".bright_yellow(), None);

    unwrap_or_exit(validate_path(path));

    let storage_dir = path.join(".synclite");

    if storage_dir.exists() {
        CliOutput::info(".synclite folder detected", Some(5));
    } else {
        CliOutput::info("No .synclite folder detected", Some(5));
        unwrap_or_exit(check_conflicts_and_cleanup(path));
        unwrap_or_exit(prompt_directory_creation());
        unwrap_or_exit(create_storage_directory(&storage_dir));
    }

    print!("\n");
    CliOutput::wrench(
        "Checking contents of .synclite folder:".bright_yellow(),
        None,
    );
    unwrap_or_exit(create_storage_files(&storage_dir));
    unwrap_or_exit(register_directory(path));
}

/// Validate that the provided path exists and is a directory
fn validate_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!(
            "Cannot initialize synclite here because the path does not exist: {}",
            path.display()
        ));
    }

    if !path.is_dir() {
        return Err(format!(
            "Cannot initialize synclite here because the path is not a directory, but a file: {}",
            path.display()
        ));
    }

    Ok(())
}

/// Check for conflicts with existing synclite directories and cleanup registry
fn check_conflicts_and_cleanup(path: &Path) -> Result<(), String> {
    // Clean up the registry first (remove stale entries)
    app::cleanup_registry()?;

    // Check for conflicts with existing synclite directories using the registry
    let (has_conflict, conflicting_dir) = app::check_path_conflicts(path)
        .map_err(|e| format!("Failed to check path conflicts: {}", e))?;

    if has_conflict {
        if let Some(conflicting_path) = conflicting_dir {
            CliOutput::error(
                "Cannot initialize synclite here due to a conflict with an existing synclite directory.",
                None,
            );
            CliOutput::info(
                &format!("Conflicting synclite directory: {}", conflicting_path),
                Some(5),
            );
            CliOutput::info("Please either:", Some(5));
            CliOutput::info("  1. Use the existing synclite directory, or", Some(6));
            CliOutput::info("  2. Remove the existing .synclite directory, or", Some(6));
            CliOutput::info(
                "  3. Choose a different directory outside of the synclite workspace",
                Some(6),
            );
            return Err("Path conflict detected".into());
        }
    }

    CliOutput::success("Directory is valid for initialization", Some(5));

    Ok(())
}

/// Get user confirmation to proceed with initialization
/// Returns Ok(()) if the user confirms, Err(String) if the user does not confirm
fn prompt_directory_creation() -> Result<(), String> {
    match confirm("Do you want to initialize it? (y/n): ", Some(5)) {
        Ok(answer) => {
            if ["y", "yes"].contains(&answer.trim().to_lowercase().as_str()) {
                return Ok(());
            } else {
                return Err("User did not confirm initialization".into());
            }
        }
        Err(e) => {
            return Err(format!("Failed to get user input: {}", e));
        }
    }
}

/// Create the .synclite storage directory
fn create_storage_directory(storage_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(storage_dir)
        .map_err(|e| format!("Failed to create .synclite directory: {}", e))?;

    CliOutput::info(
        &format!("Created .synclite directory: {}", storage_dir.display()),
        Some(5),
    );

    #[cfg(windows)]
    make_hidden_windows(storage_dir)?;

    Ok(())
}

/// Create configuration files (peers.json and state.json) if they do not exist
fn create_storage_files(storage_dir: &Path) -> Result<(), String> {
    // Create the peers file
    if !storage_dir.join("peers.json").exists() {
        CliOutput::info(
            &format!(
                "Creating peers.json file at: {}",
                storage_dir.join("peers.json").display()
            ),
            Some(5),
        );
    } else {
        CliOutput::info(
            &format!(
                "File peers.json already exists: {}, correcting contents...",
                storage_dir.join("peers.json").display()
            ),
            Some(5),
        );
    }

    let peers_config = PeersConfig::new();
    write_peers_config(storage_dir, &peers_config)?;

    // Create the state file
    if !storage_dir.join("state.json").exists() {
        let state_config = SyncState::new();
        write_sync_state(storage_dir, &state_config)?;

        CliOutput::info(
            &format!(
                "Creating state.json file at: {}",
                storage_dir.join("state.json").display()
            ),
            Some(5),
        );
    } else {
        CliOutput::info(
            &format!(
                "state.json already exists: {}",
                storage_dir.join("state.json").display()
            ),
            Some(5),
        );
    }

    Ok(())
}

/// Register the directory in the global registry
fn register_directory(path: &Path) -> Result<(), String> {
    app::add_directory(path).map_err(|e| format!("Failed to add directory to registry: {}", e))
}

#[cfg(windows)]
fn make_hidden_windows(path: &Path) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::fileapi::SetFileAttributesW;
    use winapi::um::winnt::FILE_ATTRIBUTE_HIDDEN;

    let wide_path: Vec<u16> = OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let result = unsafe { SetFileAttributesW(wide_path.as_ptr(), FILE_ATTRIBUTE_HIDDEN) };

    if result == 0 {
        return Err(format!(
            "Failed to set hidden attribute on Windows: {}",
            result
        ));
    }

    Ok(())
}
