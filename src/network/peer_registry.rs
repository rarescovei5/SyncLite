use std::sync::Arc;

use crate::{
    network::{PeerConnectionManager, PeerMessage},
    utils::output::CliOutput,
};

/// Acknowledges a peer connection and registers it in the system
/// Saves the peer to the connection manager and the peers config
pub async fn acknowledge_peer(
    connection_manager: Arc<PeerConnectionManager>,
    peer_id: String,
    leader_id: String,
) -> Result<(), Box<dyn std::error::Error>> {
    // Send connection acknowledgment with peer ID
    CliOutput::log(
        &format!("Sending connection acknowledgment to peer: {}", peer_id),
        None,
    );

    connection_manager
        .send_to_peer(
            &peer_id,
            &PeerMessage::ConnectionAck {
                peer_id: peer_id.clone(),
                leader_id,
            },
        )
        .await?;

    CliOutput::log(&format!("Successfully registered peer: {}", peer_id), None);
    Ok(())
}

pub async fn broadcast_peer_list(
    connection_manager: Arc<PeerConnectionManager>,
    peers: Vec<String>,
) -> Result<(), Vec<String>> {
    let peers_changed_message = PeerMessage::PeerListUpdate { peers };

    // Broadcast to all existing peers (except the new one)
    CliOutput::log(
        &format!(
            "Notifying {} existing peers about new peer list update.",
            connection_manager.connection_count().await,
        ),
        None,
    );

    let failed_peers = connection_manager
        .broadcast_message(&peers_changed_message)
        .await;

    if failed_peers.is_empty() {
        Ok(())
    } else {
        Err(failed_peers)
    }
}
