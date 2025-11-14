use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
