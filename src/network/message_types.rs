use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::models::SyncState;

// Messages sent by the **server** to peers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    // Step 1: Server acknowledges connection
    ConnectionAck {
        peer_id: String,
        leader_id: String,
    },
    // Step 3: Server sends the files to update and the files to delete and requests files it needs from the peer
    InitialSyncPushResponse {
        files_to_update: HashMap<String, String>,
        files_to_delete: Vec<String>,
        files_to_send_back: Vec<String>,
    },

    // Other
    PeerListUpdate {
        peers: Vec<String>,
    },
    FileUpdatePush {
        files_to_write: HashMap<String, String>,
        paths_to_delete: Vec<String>,
    },
}

// Messages sent by **peers** to the server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PeerMessage {
    // Step 2: Peer sends their initial sync state
    InitialSyncPush {
        sync_state: SyncState,
    },

    // step 4: Peer sends the files back to the server and initial sync is complete
    FileUpdatePush {
        files_to_write: HashMap<String, String>,
        paths_to_delete: Vec<String>,
    },
}
