use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Represents the workspace.json file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub workspace_id: Uuid,
    pub created_at: DateTime<Utc>,
}

impl WorkspaceConfig {
    /// Create a new workspace configuration with a unique ID and current timestamp
    pub fn new() -> Self {
        Self {
            workspace_id: Uuid::new_v4(),
            created_at: Utc::now(),
        }
    }
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents the peers.json file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeersConfig {
    #[serde(default)]
    pub leader: Option<String>,
    #[serde(default)]
    pub peers: Vec<String>,
}

impl PeersConfig {
    /// Create a new empty peers configuration
    pub fn new() -> Self {
        Self {
            leader: None,
            peers: Vec::new(),
        }
    }

    /// Add a peer to the list if not already present
    pub fn add_peer(&mut self, peer: String) {
        if !self.peers.contains(&peer) {
            self.peers.push(peer);
        }
    }

    /// Set the leader peer
    pub fn set_leader(&mut self, leader: String) {
        self.leader = Some(leader);
    }

    /// Remove a peer from the list
    pub fn remove_peer(&mut self, peer: &str) {
        self.peers.retain(|p| p != peer);
        // If the removed peer was the leader, clear the leader
        if let Some(ref current_leader) = self.leader {
            if current_leader == peer {
                self.leader = None;
            }
        }
    }
}

impl Default for PeersConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents a file entry in the sync state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub hash: Option<String>, // None if file is deleted
    pub last_modified: DateTime<Utc>,
    pub is_deleted: bool, // Tombstone marker
}

impl FileEntry {
    /// Create a new file entry for an existing file
    pub fn new_file(hash: String, last_modified: DateTime<Utc>) -> Self {
        Self {
            hash: Some(hash),
            last_modified,
            is_deleted: false,
        }
    }

    /// Check if this file entry represents an existing file
    pub fn exists(&self) -> bool {
        !self.is_deleted && self.hash.is_some()
    }
}

/// Represents the complete sync state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub files: HashMap<String, FileEntry>, // path -> file entry
    pub last_sync: DateTime<Utc>,
}

impl SyncState {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            last_sync: Utc::now(),
        }
    }

    /// Add or update a file
    pub fn add_file(&mut self, path: String, file: FileEntry) {
        self.files.insert(path.clone(), file);
        self.last_sync = Utc::now();
    }

    /// Mark a file as deleted (create tombstone)
    pub fn delete_file(&mut self, path: &str) {
        if self.files.contains_key(path) {
            let deleted_file = self.files.get_mut(path).unwrap();
            deleted_file.is_deleted = true;
            deleted_file.hash = None;
            deleted_file.last_modified = Utc::now();
            self.last_sync = Utc::now();
        }
    }
}
