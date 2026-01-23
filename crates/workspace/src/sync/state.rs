use std::{collections::HashMap, fs, ops::Deref, path::Path};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    WORKSPACE_DIR,
    sync::{FileEntry, STATE_PATH},
};
use common::{fs::calculate_file_hash, json::read_json};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncState(pub HashMap<String, FileEntry>);

impl SyncState {
    pub fn load() -> anyhow::Result<Self> {
        let state = match read_json(&*STATE_PATH) {
            Ok(state) => state,
            Err(_) => Self(HashMap::new()),
        };

        Ok(state)
    }
    pub fn from_workspace() -> anyhow::Result<Self> {
        let workspace_dir = &*WORKSPACE_DIR;
        let mut state = HashMap::new();

        compute_sync_state(workspace_dir, workspace_dir, &mut state)?;

        Ok(Self(state))
    }
}

impl SyncState {
    pub fn add_file(&mut self, path: String, file_entry: FileEntry) -> anyhow::Result<()> {
        self.0.insert(path, file_entry);
        Ok(())
    }
    pub fn delete_file(&mut self, path: String) -> anyhow::Result<()> {
        self.0.insert(
            path,
            FileEntry {
                hash: None,
                is_deleted: true,
                last_modified: Utc::now(),
            },
        );
        Ok(())
    }
    pub fn update_file(&mut self, path: String, new_hash: String) -> anyhow::Result<()> {
        self.0.insert(
            path,
            FileEntry {
                hash: Some(new_hash),
                is_deleted: false,
                last_modified: Utc::now(),
            },
        );
        Ok(())
    }
}

fn compute_sync_state(
    workspace_path: &Path,
    current_path: &Path,
    sync_state: &mut HashMap<String, FileEntry>,
) -> anyhow::Result<()> {
    let entries = fs::read_dir(current_path).map_err(anyhow::Error::msg)?;

    for entry in entries {
        let entry = entry.map_err(anyhow::Error::msg)?;

        let entry_path = entry.path();

        // Skip .synclite directory
        if entry_path.file_name().unwrap() == ".synclite" {
            continue;
        }

        if entry_path.is_dir() {
            compute_sync_state(workspace_path, &entry_path, sync_state)
                .map_err(anyhow::Error::msg)?;
        } else {
            // Calculate relative path from base directory
            let relative_path = entry_path
                .strip_prefix(workspace_path)
                .map_err(anyhow::Error::msg)?
                .to_string_lossy()
                .to_string();

            let hash = calculate_file_hash(&entry_path)?;

            let last_modified = if let Ok(time) = fs::metadata(&entry_path)
                .map_err(anyhow::Error::msg)?
                .modified()
            {
                time
            } else {
                return Err(anyhow::anyhow!(
                    "Not supported on this platform, consider using a different operating system :)"
                ));
            };

            sync_state.insert(
                relative_path,
                FileEntry {
                    hash: Some(hash),
                    is_deleted: false,
                    last_modified: DateTime::from(last_modified),
                },
            );
        }
    }

    Ok(())
}

impl Deref for SyncState {
    type Target = HashMap<String, FileEntry>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
