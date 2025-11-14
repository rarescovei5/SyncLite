use super::types::{PeersConfig, WorkspaceConfig};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

/// General Operations
pub fn write_json<T: Serialize>(path: &Path, data: &T) -> Result<(), String> {
    let json_string = serde_json::to_string_pretty(data)
        .map_err(|e| format!("Failed to serialize JSON: {}", e))?;

    fs::write(path, json_string)
        .map_err(|e| format!("Failed to write file {}: {}", path.display(), e))
}
pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file {}: {}", path.display(), e))?;

    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse JSON from {}: {}", path.display(), e))
}

/// Helpers for workspace file
pub fn write_workspace_config(storage_dir: &Path, config: &WorkspaceConfig) -> Result<(), String> {
    let workspace_file = storage_dir.join("workspace.json");
    write_json(&workspace_file, config)
}
pub fn read_workspace_config(storage_dir: &Path) -> Result<WorkspaceConfig, String> {
    let workspace_file = storage_dir.join("workspace.json");
    read_json(&workspace_file)
}

/// Helpers for peers file
pub fn write_peers_config(storage_dir: &Path, config: &PeersConfig) -> Result<(), String> {
    let peers_file = storage_dir.join("peers.json");
    write_json(&peers_file, config)
}
pub fn read_peers_config(storage_dir: &Path) -> Result<PeersConfig, String> {
    let peers_file = storage_dir.join("peers.json");
    read_json(&peers_file)
}
