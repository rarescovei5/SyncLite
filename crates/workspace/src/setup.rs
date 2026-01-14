use std::{fs, path::PathBuf};

use colored::Colorize;
#[cfg(windows)]
use common::fs::make_hidden_windows;
use log::{elog, log};

use crate::{STORAGE_DIR, WORKSPACE_DIR};
use app::registry;

pub fn initialise_workspace(path: PathBuf) {
    WORKSPACE_DIR
        .value
        .set(path)
        .expect("Failed to set workspace directory");

    log!(wrench, "Checking Directory Status:".bright_yellow());

    // Cleanup the registry first (remove stale entries)
    if let Err(e) = registry::cleanup_registry() {
        elog!(error, "Failed to cleanup registry: {}", e);
        std::process::exit(1);
    }

    if STORAGE_DIR.exists() {
        log!(info, 5, ".synclite folder detected");
    } else {
        log!(info, 5, ".synclite folder not detected");

        // Check for conflicts with existing synclite directories using the registry
        match check_conflicts() {
            Ok(_) => {
                log!(success, 5, "Directory is valid for initialization")
            }
            Err(conflicting_dir) => {
                elog!(
                    error,
                    "Cannot initialize synclite here due to a conflict with an existing synclite directory.",
                );
                log!(
                    info,
                    5,
                    "Conflicting synclite directory: {}",
                    conflicting_dir
                );
                log!(info, 6, "Please either:");
                log!(info, 6, "  1. Use the existing synclite directory");
                log!(info, 6, "  2. Remove the existing .synclite directory");
                log!(
                    info,
                    6,
                    "  3. Choose a different directory outside of the synclite workspace"
                );
                std::process::exit(1);
            }
        }

        // Ask if the user wants to create the .synclite folder
        match prompt_directory_creation() {
            Ok(false) => {
                elog!(error, 5, "User rejected directory creation, exiting...");
                std::process::exit(1);
            }
            Err(e) => {
                elog!(error, 5, "Failed to prompt directory creation: {}", e);
                std::process::exit(1);
            }
            _ => {}
        }

        match create_storage_directory() {
            Ok(_) => {
                log!(
                    info,
                    5,
                    "Created .synclite directory: {}",
                    &STORAGE_DIR.display()
                );
            }
            Err(e) => {
                elog!(error, "Failed to create storage directory: {}", e);
                std::process::exit(1);
            }
        }
    }
}

fn check_conflicts() -> anyhow::Result<()> {
    if let Some(conflicting_dir) = registry::check_path_conflicts(&WORKSPACE_DIR)? {
        anyhow::bail!(conflicting_dir);
    };
    Ok(())
}

fn prompt_directory_creation() -> anyhow::Result<bool> {
    log!(question, 5, "Do you want to initialize it? (y/n): ");

    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(anyhow::Error::msg)?;

    if answer.trim().to_lowercase() == "y" || answer.trim().to_lowercase() == "yes" {
        Ok(true)
    } else {
        Ok(false)
    }
}

fn create_storage_directory() -> anyhow::Result<()> {
    fs::create_dir_all(&*STORAGE_DIR).map_err(anyhow::Error::msg)?;

    #[cfg(windows)]
    make_hidden_windows(&*STORAGE_DIR)?;

    Ok(())
}
