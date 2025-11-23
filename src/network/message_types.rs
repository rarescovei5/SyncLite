use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::models::SyncState;

// Messages sent by the server to peers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    // Single Messages
    ConnectionAck {
        peer_id: String,
        leader_id: String,
    },
    FileContentRequestWithVersion {
        my_winning_files: HashMap<String, String>, // files with their content that server is sending
        request_files: Vec<String>,                // files that server wants from peer
    },

    // Broadcast Messages
    PeerListUpdate {
        peers: Vec<String>,
    },
    FileUpdatePush {
        version: HashMap<String, String>,
    },
    FileDeletionPush {
        files: Vec<String>,
    },
}

// Messages sent by peers to the server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PeerMessage {
    // Single Messages
    VersionPush { sync_state: SyncState }, // Step 1: peer sends their version
    FileContentResponse { files: HashMap<String, String> }, // Step 4: peer sends their winning files
}
