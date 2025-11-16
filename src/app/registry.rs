use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoriesRegistry {
    pub directories: Vec<String>,
}

impl DirectoriesRegistry {
    pub fn new() -> Self {
        Self {
            directories: Vec::new(),
        }
    }
}

impl Default for DirectoriesRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Get the path to the synclite app data directory
pub fn get_app_data_dir() -> Result<PathBuf, String> {
    dirs::data_dir()
        .map(|dir| dir.join("synclite"))
        .ok_or_else(|| "Failed to get app data directory".to_string())
}

/// Get the path to the directories registry file
pub fn get_registry_path() -> Result<PathBuf, String> {
    Ok(get_app_data_dir()?.join("directories.json"))
}

/// Load the directories registry from app data
pub fn load_registry() -> Result<DirectoriesRegistry, String> {
    let registry_path = get_registry_path()?;

    if !registry_path.exists() {
        return Ok(DirectoriesRegistry::new());
    }

    let content = fs::read_to_string(&registry_path).map_err(|e| {
        format!(
            "Failed to read existing synclite directories from registry file: {}",
            e
        )
    })?;

    serde_json::from_str(&content).map_err(|e| {
        format!(
            "Failed to parse synclite directories from registry JSON: {}",
            e
        )
    })
}

/// Save the directories registry to app data
pub fn save_registry(registry: &DirectoriesRegistry) -> Result<(), String> {
    let app_data_dir = get_app_data_dir()?;

    // Ensure the app data directory exists
    fs::create_dir_all(&app_data_dir)
        .map_err(|e| format!("Failed to create synclite directories registry file: {}", e))?;

    let registry_path = get_registry_path()?;
    let json_string = serde_json::to_string_pretty(registry).map_err(|e| {
        format!(
            "Failed to serialize synclite directories to registry JSON: {}",
            e
        )
    })?;

    fs::write(&registry_path, json_string)
        .map_err(|e| format!("Failed to write synclite directories registry file: {}", e))
}

/// Add a directory to the registry
pub fn add_directory(path: &Path) -> Result<(), String> {
    let canonical_path = path
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize synclite directory path: {}", e))?
        .to_string_lossy()
        .to_string();

    let mut registry = load_registry()?;

    // Don't add duplicates
    if !registry.directories.contains(&canonical_path) {
        registry.directories.push(canonical_path);
        save_registry(&registry)?;
    }

    Ok(())
}

/// Remove a directory from the registry
pub fn remove_directory(path: &Path) -> Result<(), String> {
    let canonical_path = path
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize synclite directory path: {}", e))?
        .to_string_lossy()
        .to_string();

    let mut registry = load_registry()?;
    registry.directories.retain(|dir| dir != &canonical_path);
    save_registry(&registry)?;

    Ok(())
}

/// Check if a path conflicts with existing synclite directories
/// Returns (is_conflict, conflicting_directory_path)
pub fn check_path_conflicts(path: &Path) -> Result<(bool, Option<String>), String> {
    let canonical_path = path
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize synclite directory path: {}", e))?;

    let registry = load_registry()?;

    for existing_dir in &registry.directories {
        let existing_path = Path::new(existing_dir);

        // Check if the new path is a subdirectory of an existing synclite directory
        if canonical_path.starts_with(existing_path) {
            return Ok((true, Some(existing_dir.clone())));
        }

        // Check if an existing synclite directory is a subdirectory of the new path
        if existing_path.starts_with(&canonical_path) {
            return Ok((true, Some(existing_dir.clone())));
        }
    }

    Ok((false, None))
}

/// Clean up the registry by removing directories that no longer have .synclite folders
pub fn cleanup_registry() -> Result<(), String> {
    let mut registry = load_registry()?;
    let original_count = registry.directories.len();

    registry
        .directories
        .retain(|dir| Path::new(dir).join(".synclite").exists());

    if registry.directories.len() != original_count {
        save_registry(&registry)?;
    }

    Ok(())
}
