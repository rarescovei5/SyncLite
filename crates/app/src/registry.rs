use std::{fs, path::Path};

use crate::APP_DATA_DIR;
use common::Lazy;
use common::json::read_json;
use std::path::PathBuf;

static REGISTRY_PATH: Lazy<PathBuf> = Lazy::new(Some(|| APP_DATA_DIR.join("registry.json")));

/// Load the directories registry from app data
fn load_registry() -> anyhow::Result<Vec<String>> {
    let registry_path = &*REGISTRY_PATH;

    if !registry_path.exists() {
        return Ok(vec![]);
    }

    match read_json(&registry_path) {
        Ok(registry) => Ok(registry),
        // File may exist, but the data may be corrupted
        Err(_) => Ok(vec![]),
    }
}

/// Save the directories registry to app data
fn save_registry(registry: &Vec<String>) -> anyhow::Result<()> {
    let app_data_dir = &*APP_DATA_DIR;

    // Ensure the app data directory exists
    fs::create_dir_all(&app_data_dir)?;

    let registry_path = &*REGISTRY_PATH;
    let json_string = serde_json::to_string_pretty(registry)?;

    fs::write(&registry_path, json_string)?;

    Ok(())
}

/// Add a directory to the registry
pub fn add_directory(path: &Path) -> anyhow::Result<()> {
    let canonical_path = path.canonicalize()?.to_string_lossy().to_string();

    let mut registry = load_registry()?;

    // Don't add duplicates
    if !registry.contains(&canonical_path) {
        registry.push(canonical_path);
        save_registry(&registry)?;
    }

    Ok(())
}

/// Check if a path conflicts with existing synclite directories
/// Returns (is_conflict, conflicting_directory_path)
pub fn check_path_conflicts(path: &Path) -> anyhow::Result<Option<String>> {
    let canonical_path = path.canonicalize()?;

    let registry = load_registry()?;

    for existing_dir in &registry {
        let existing_path = Path::new(existing_dir);

        // Check if the new path is a subdirectory of an existing synclite directory
        if canonical_path.starts_with(existing_path) {
            return Ok(Some(existing_dir.clone()));
        }

        // Check if an existing synclite directory is a subdirectory of the new path
        if existing_path.starts_with(&canonical_path) {
            return Ok(Some(existing_dir.clone()));
        }
    }

    Ok(None)
}

/// Clean up the registry by removing directories that no longer have .synclite folders
pub fn cleanup_registry() -> anyhow::Result<()> {
    let mut registry = load_registry()?;
    let original_count = registry.len();

    registry.retain(|dir| Path::new(dir).join(".synclite").exists());

    if registry.len() != original_count {
        save_registry(&registry)?;
    }

    Ok(())
}
