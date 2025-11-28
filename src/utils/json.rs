use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

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
