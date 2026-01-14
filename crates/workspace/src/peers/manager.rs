use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{STORAGE_DIR, peers::PeersState};
use common::json::write_json;

pub struct PeersManager(Arc<Mutex<PeersState>>);

impl PeersManager {
    pub fn new() -> anyhow::Result<Self> {
        let state = PeersState::load()?;

        Ok(Self(Arc::new(Mutex::new(state))))
    }

    /// Add a peer and automatically save
    pub async fn add_peer(&self, peer: String) -> anyhow::Result<()> {
        {
            let mut state = self.0.lock().await;
            state.peers.push(peer);
        }
        self.save().await
    }

    /// Set leader and automatically save
    pub async fn set_leader(&self, leader: String) -> anyhow::Result<()> {
        {
            let mut state = self.0.lock().await;
            state.leader = Some(leader);
        }
        self.save().await
    }

    /// Remove peer and automatically save
    pub async fn remove_peer(&self, peer: &str) -> anyhow::Result<()> {
        {
            let mut state = self.0.lock().await;
            state.peers.retain(|p| p != peer);
        }
        self.save().await
    }

    pub async fn set_peers(&self, peers: Vec<String>) -> anyhow::Result<()> {
        {
            let mut state = self.0.lock().await;
            state.peers = peers;
        }
        self.save().await
    }

    /// Get a clone of the config for read-only access
    pub async fn state(&self) -> PeersState {
        let state = self.0.lock().await;
        state.clone()
    }

    /// Execute multiple operations atomically
    pub async fn batch_operations<F>(&self, operations: F) -> anyhow::Result<()>
    where
        F: FnOnce(&mut PeersState),
    {
        {
            let mut state = self.0.lock().await;
            operations(&mut state);
        }
        self.save().await
    }

    /// Save the current config to disk
    pub async fn save(&self) -> anyhow::Result<()> {
        let state = self.0.lock().await;
        write_json(&STORAGE_DIR.join("peers.json"), &*state)
    }
}
