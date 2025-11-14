use serde::{Deserialize, Serialize};

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
