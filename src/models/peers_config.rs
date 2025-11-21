use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::utils::write_json;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeersState {
    pub leader: Option<String>,
    pub peers: Vec<String>,
}

impl PeersState {
    pub fn new() -> Self {
        Self {
            leader: None,
            peers: vec![],
        }
    }
}

pub struct PeersConfig {
    state: Arc<Mutex<PeersState>>,
    storage_path: std::path::PathBuf,
}

impl PeersConfig {
    pub fn new(storage_path: impl AsRef<Path>, state: PeersState) -> Self {
        Self {
            state: Arc::new(Mutex::new(state)),
            storage_path: storage_path.as_ref().to_path_buf(),
        }
    }

    /// Add a peer and automatically save
    pub async fn add_peer(&self, peer: String) -> Result<(), String> {
        {
            let mut state = self.state.lock().await;
            state.peers.push(peer);
        }
        self.save().await
    }

    /// Set leader and automatically save
    pub async fn set_leader(&self, leader: String) -> Result<(), String> {
        {
            let mut state = self.state.lock().await;
            state.leader = Some(leader);
        }
        self.save().await
    }

    /// Remove peer and automatically save
    pub async fn remove_peer(&self, peer: &str) -> Result<(), String> {
        {
            let mut state = self.state.lock().await;
            state.peers.retain(|p| p != peer);
        }
        self.save().await
    }

    pub async fn set_peers(&self, peers: Vec<String>) -> Result<(), String> {
        {
            let mut state = self.state.lock().await;
            state.peers = peers;
        }
        self.save().await
    }

    /// Get a clone of the config for read-only access
    pub async fn state(&self) -> PeersState {
        let state = self.state.lock().await;
        state.clone()
    }

    /// Execute multiple operations atomically
    pub async fn batch_operations<F>(&self, operations: F) -> Result<(), String>
    where
        F: FnOnce(&mut PeersState),
    {
        {
            let mut state = self.state.lock().await;
            operations(&mut state);
        }
        self.save().await
    }

    /// Save the current config to disk
    pub async fn save(&self) -> Result<(), String> {
        let state = self.state.lock().await;
        write_json(&self.storage_path.join("peers.json"), &*state)
    }
}
