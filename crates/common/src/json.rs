use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

/// Write JSON to a file
pub fn write_json<T: Serialize>(path: &Path, data: &T) -> anyhow::Result<()> {
    let json_string = serde_json::to_string_pretty(data)?;
    fs::write(path, json_string)?;
    Ok(())
}

/// Read JSON from a file
pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> anyhow::Result<T> {
    let content = fs::read_to_string(path)?;
    let data = serde_json::from_str(&content)?;
    Ok(data)
}
