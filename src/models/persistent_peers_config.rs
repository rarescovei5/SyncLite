use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::PeersConfig;

pub struct PersistentPeersConfig {
    config: Arc<Mutex<PeersConfig>>,
    storage_path: std::path::PathBuf,
}

impl PersistentPeersConfig {
    pub fn new(config: PeersConfig, storage_path: impl AsRef<Path>) -> Self {
        Self {
            config: Arc::new(Mutex::new(config)),
            storage_path: storage_path.as_ref().to_path_buf(),
        }
    }

    /// Add a peer and automatically save
    pub async fn add_peer(&self, peer: String) -> Result<(), String> {
        {
            let mut config = self.config.lock().await;
            config.add_peer(peer);
        }
        self.save().await
    }

    /// Set leader and automatically save
    pub async fn set_leader(&self, leader: String) -> Result<(), String> {
        {
            let mut config = self.config.lock().await;
            config.set_leader(leader);
        }
        self.save().await
    }

    /// Remove peer and automatically save
    pub async fn remove_peer(&self, peer: &str) -> Result<(), String> {
        {
            let mut config = self.config.lock().await;
            config.remove_peer(peer);
        }
        self.save().await
    }

    pub async fn set_peers(&self, peers: Vec<String>) -> Result<(), String> {
        {
            let mut config = self.config.lock().await;
            config.peers = peers;
        }
        self.save().await
    }

    /// Get a clone of the config for read-only access
    pub async fn config(&self) -> PeersConfig {
        let config = self.config.lock().await;
        config.clone()
    }

    /// Execute multiple operations atomically
    pub async fn batch_operations<F>(&self, operations: F) -> Result<(), String>
    where
        F: FnOnce(&mut PeersConfig),
    {
        {
            let mut config = self.config.lock().await;
            operations(&mut config);
        }
        self.save().await
    }

    /// Save the current config to disk
    async fn save(&self) -> Result<(), String> {
        let config = self.config.lock().await;
        crate::storage::write_json(&self.storage_path.join("peers.json"), &*config)
    }
}
