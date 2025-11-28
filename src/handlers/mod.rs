use std::path::Path;

use crate::{
    cli::{Args, ParseArgsError, parse_args},
    storage::{InitialiseStorageError, initialise_storage},
    utils::Log,
};

pub fn handle_parse_args() -> Args {
    match parse_args() {
        Ok(args) => args,
        Err(e) => {
            match e {
                ParseArgsError::InvalidArguments => {
                    Log::error("Invalid arguments", None);
                }
                ParseArgsError::InvalidCommand(command) => {
                    Log::error(&format!("Invalid command: {}", command), None);
                }
                ParseArgsError::InvalidPath(path) => {
                    Log::error(&format!("Invalid path: {}", path), None);
                }
                ParseArgsError::InvalidPortNumber(port) => {
                    Log::error(&format!("Invalid port number: {}", port), None);
                }
            };
            std::process::exit(1);
        }
    }
}

pub fn handle_initialise_storage(abs_workspace_path: &Path) {
    match initialise_storage(abs_workspace_path) {
        Ok(_) => {}
        Err(e) => {
            match e {
                InitialiseStorageError::FailedToCheckConflicts(e) => {
                    Log::error(&format!("Failed to check conflicts: {}", e), None);
                }
                InitialiseStorageError::PromptDirectoryCreation(e) => {
                    Log::error(&format!("Failed to prompt directory creation: {}", e), None);
                }
                InitialiseStorageError::FailedToCreateStorageDirectory(e) => {
                    Log::error(&format!("Failed to create storage directory: {}", e), None);
                }
                InitialiseStorageError::FailedToCreateStorageFiles(e) => {
                    Log::error(&format!("Failed to create storage files: {}", e), None);
                }
                InitialiseStorageError::FailedToAddDirectoryToRegistry(e) => {
                    Log::error(&format!("Failed to add directory to registry: {}", e), None);
                }
            };
            std::process::exit(1);
        }
    }
}
