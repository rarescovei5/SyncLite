use serde::{Deserialize, Serialize};

// Message types for peer communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PeerMessage {
    // Sent to acknowledge successful connection
    ConnectionAck { peer_id: String, leader_id: String },
    // Sent to update peer list
    PeerListUpdate { peers: Vec<String> },
}
