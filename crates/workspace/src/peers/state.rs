use common::json::read_json;
use serde::{Deserialize, Serialize};

use crate::STORAGE_DIR;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeersState {
    pub leader: Option<String>,
    pub peers: Vec<String>,
}

impl PeersState {
    pub fn load() -> anyhow::Result<Self> {
        let state = match read_json(&*STORAGE_DIR.join("peers.json")) {
            Ok(state) => state,
            Err(_) => Self {
                leader: None,
                peers: vec![],
            },
        };
        Ok(state)
    }
}
