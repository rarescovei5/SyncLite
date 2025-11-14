use super::{
    json_utils,
    types::{PeersConfig, SyncState, WorkspaceConfig},
};
use crate::cli::output::CliOutput;
use crate::utils::confirm;
use std::{fs, path::Path};

pub fn initialise_storage(path: &str, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let base_path = Path::new(path);

    // Validate that the base path exists and is a directory
    if !base_path.exists() {
        return Err(format!("Path does not exist: {}", path).into());
    }
    if !base_path.is_dir() {
        return Err(format!("Path is not a directory: {}", path).into());
    }

    // Check if the directory is already initialized
    let storage_dir = base_path.join(".synclite");
    if storage_dir.exists() {
        CliOutput::success("Directory is initialized", None);
        return Ok(());
    }

    CliOutput::step(1, 3, "Checking directory status");

    // Prompt the user to initialize the directory
    let answer = confirm(
        "This directory is not initialized. Do you want to initialize it? (y/n): ",
        Some(5),
    )?;
    if !["y", "yes"].contains(&answer.trim().to_lowercase().as_str()) {
        CliOutput::cancelled("Initialization cancelled", Some(5));
        std::process::exit(1);
    }

    CliOutput::step(2, 3, "Creating .synclite directory");

    // Create the directory
    fs::create_dir_all(&storage_dir)
        .map_err(|e| format!("Failed to create .synclite directory: {}", e))?;
    #[cfg(windows)] // Make it hidden on Windows
    make_hidden_windows(&storage_dir)?;

    CliOutput::step(3, 3, "Finalizing setup");

    // Create the workspace file
    json_utils::write_workspace_config(&storage_dir, &WorkspaceConfig::new())?;
    CliOutput::info(
        &format!(
            "Created file: {}",
            storage_dir.join("workspace.json").display()
        ),
        Some(5),
    );

    // Create the peers file
    json_utils::write_peers_config(&storage_dir, &PeersConfig::new())?;
    CliOutput::info(
        &format!("Created file: {}", storage_dir.join("peers.json").display()),
        Some(5),
    );

    // Create the state file
    json_utils::write_sync_state(&storage_dir, &SyncState::new())?;
    CliOutput::info(
        &format!("Created file: {}", storage_dir.join("state.json").display()),
        Some(5),
    );

    Ok(())
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
        return Err("Failed to set hidden attribute on Windows".to_string());
    }

    Ok(())
}
