use crate::models::{FileEntry, PeersState};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::Path};

/// Generic JSON write operation
pub fn write_json<T: Serialize>(path: &Path, data: &T) -> Result<(), String> {
    let json_string = serde_json::to_string_pretty(data)
        .map_err(|e| format!("Failed to serialize JSON: {}", e))?;

    fs::write(path, json_string)
        .map_err(|e| format!("Failed to write file {}: {}", path.display(), e))
}

/// Generic JSON read operation
pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file {}: {}", path.display(), e))?;

    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse JSON from {}: {}", path.display(), e))
}

/// Helper to read peers config from the standard location
pub fn read_peers_state(storage_dir: &Path) -> PeersState {
    let peers_file = storage_dir.join("peers.json");
    match read_json(&peers_file) {
        Ok(state) => state,
        Err(_) => PeersState::new(),
    }
}

/// Helper to read sync state from the standard location
pub fn read_sync_state(storage_dir: &Path) -> HashMap<String, FileEntry> {
    let state_file = storage_dir.join("state.json");
    match read_json(&state_file) {
        Ok(state) => state,
        Err(_) => HashMap::new(),
    }
}
