use super::{
    json_utils, registry,
    types::{PeersConfig, SyncState},
};
use crate::utils::handle_result_or_exit;
use crate::utils::{confirm, output::CliOutput};
use std::{fs, path::Path};

/// Initialize storage in the specified directory
pub fn initialise_storage(path: &str) {
    // Validate the path first
    handle_result_or_exit(validate_path(path));

    let base_path = Path::new(path);
    let storage_dir = base_path.join(".synclite");

    // Check if already initialized
    if storage_dir.exists() {
        CliOutput::success("Directory is already initialized", None);
        return;
    }

    // Check for conflicts and clean registry
    handle_result_or_exit(check_conflicts_and_cleanup(path));

    // Get user confirmation
    if !get_user_confirmation() {
        CliOutput::cancelled("Initialization cancelled", Some(3));
        std::process::exit(1);
    }

    // Create the storage directory
    handle_result_or_exit(create_storage_directory(&storage_dir));

    // Initialize configuration files
    handle_result_or_exit(initialize_config_files(&storage_dir));

    // Register the directory
    handle_result_or_exit(register_directory(path));
}

/// Validate that the provided path exists and is a directory
fn validate_path(path: &str) -> Result<(), String> {
    let base_path = Path::new(path);

    if !base_path.exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    if !base_path.is_dir() {
        return Err(format!("Path is not a directory: {}", path));
    }

    Ok(())
}

/// Check for conflicts with existing synclite directories and cleanup registry
fn check_conflicts_and_cleanup(path: &str) -> Result<(), String> {
    // Clean up the registry first (remove stale entries)
    registry::cleanup_registry().map_err(|e| format!("Failed to cleanup registry: {}", e))?;

    // Check for conflicts with existing synclite directories using the registry
    let (has_conflict, conflicting_dir) = registry::check_path_conflicts(path)
        .map_err(|e| format!("Failed to check path conflicts: {}", e))?;

    if has_conflict {
        if let Some(conflicting_path) = conflicting_dir {
            CliOutput::error(
                "Cannot initialize synclite here due to a conflict with an existing synclite directory.",
                None,
            );
            CliOutput::info(
                &format!("Conflicting synclite directory: {}", conflicting_path),
                Some(3),
            );
            CliOutput::info("Please either:", Some(3));
            CliOutput::info("  1. Use the existing synclite directory, or", Some(6));
            CliOutput::info("  2. Remove the existing .synclite directory, or", Some(6));
            CliOutput::info(
                "  3. Choose a different directory outside of the synclite workspace",
                Some(6),
            );
            return Err("Path conflict detected".to_string());
        }
    }

    Ok(())
}

/// Get user confirmation to proceed with initialization
fn get_user_confirmation() -> bool {
    CliOutput::step(1, 3, "Checking directory status");

    match confirm(
        "This directory is not initialized. Do you want to initialize it? (y/n): ",
        Some(3),
    ) {
        Ok(answer) => ["y", "yes"].contains(&answer.trim().to_lowercase().as_str()),
        Err(e) => {
            CliOutput::error(&format!("Failed to get user input: {}", e), Some(3));
            false
        }
    }
}

/// Create the .synclite storage directory
fn create_storage_directory(storage_dir: &Path) -> Result<(), String> {
    CliOutput::step(2, 3, "Creating .synclite directory");

    fs::create_dir_all(storage_dir)
        .map_err(|e| format!("Failed to create .synclite directory: {}", e))?;

    #[cfg(windows)]
    make_hidden_windows(storage_dir)?;

    Ok(())
}

/// Initialize configuration files (peers.json and state.json)
fn initialize_config_files(storage_dir: &Path) -> Result<(), String> {
    CliOutput::step(3, 3, "Finalizing setup");

    // Create the peers file
    json_utils::write_peers_config(storage_dir, &PeersConfig::new())
        .map_err(|e| format!("Failed to create peers.json: {}", e))?;

    CliOutput::info(
        &format!("Created file: {}", storage_dir.join("peers.json").display()),
        Some(3),
    );

    // Create the state file
    json_utils::write_sync_state(storage_dir, &SyncState::new())
        .map_err(|e| format!("Failed to create state.json: {}", e))?;

    CliOutput::info(
        &format!("Created file: {}", storage_dir.join("state.json").display()),
        Some(3),
    );

    Ok(())
}

/// Register the directory in the global registry
fn register_directory(path: &str) -> Result<(), String> {
    registry::add_directory(path).map_err(|e| format!("Failed to add directory to registry: {}", e))
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
        CliOutput::error("Failed to set hidden attribute on Windows", None);
        std::process::exit(1);
    }

    Ok(())
}
