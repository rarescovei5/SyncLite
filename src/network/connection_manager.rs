use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::Mutex;

use crate::utils::output::CliOutput;

use super::peer_messaging::{PeerMessage, send_message_to_peer};

/// Manages active connections to all peers
#[derive(Clone)]
pub struct PeerConnectionManager {
    /// Map of peer_id -> write stream for sending messages
    connections: Arc<Mutex<HashMap<String, OwnedWriteHalf>>>,
}

/// Connection manager for peer-to-peer communication
/// Includes
/// - Adding and removing connections: `add_connection` and `remove_connection`
/// - Sending messages to specific peers: `send_to_peer`
/// - Broadcasting messages to all peers: `broadcast_message`
/// - Broadcasting messages to all peers except the ones in the exclude list: `broadcast_except`
/// - Getting the number of active connections: `connection_count`
/// - Getting the list of connected peer IDs: `get_connected_peers`
/// - Checking if a specific peer is connected: `is_peer_connected`
impl PeerConnectionManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Add a new peer connection
    pub async fn add_connection(&self, peer_id: String, writer: OwnedWriteHalf) {
        let mut connections = self.connections.lock().await;
        connections.insert(peer_id, writer);
    }

    /// Remove a peer connection
    pub async fn remove_connection(&self, peer_id: &str) {
        let mut connections = self.connections.lock().await;
        connections.remove(peer_id);
    }

    /// Send a message to a specific peer
    pub async fn send_to_peer(&self, peer_id: &str, message: &PeerMessage) -> Result<(), String> {
        let mut connections = self.connections.lock().await;

        if let Some(writer) = connections.get_mut(peer_id) {
            send_message_to_peer(writer, message)
                .await
                .map_err(|e| format!("Failed to send message to peer {}: {}", peer_id, e))
        } else {
            Err(format!("No active connection to peer {}", peer_id))
        }
    }

    /// Broadcast a message to all connected peers
    pub async fn broadcast_message(&self, message: &PeerMessage) -> Vec<String> {
        let connection_count = self.connection_count().await;
        if connection_count == 0 {
            return Vec::new();
        }

        let mut connections = self.connections.lock().await;
        let mut failed_peers = Vec::new();

        // Collect peer IDs to avoid borrowing issues
        let peer_ids: Vec<String> = connections.keys().cloned().collect();

        for peer_id in peer_ids {
            if let Some(writer) = connections.get_mut(&peer_id) {
                if let Err(e) = send_message_to_peer(writer, message).await {
                    eprintln!("Failed to send message to peer {}: {}", peer_id, e);
                    failed_peers.push(peer_id.clone());
                }
            }
        }

        // Remove failed connections
        for peer_id in &failed_peers {
            connections.remove(peer_id);
        }

        if !failed_peers.is_empty() {
            CliOutput::warning(
                &format!("Failed to notify {} peers", failed_peers.len()),
                None,
            );
        }

        failed_peers
    }

    /// Broadcast a message to all peers except the ones in the exclude list (useful for forwarding)
    pub async fn broadcast_except(
        &self,
        message: &PeerMessage,
        exclude_peer: Vec<String>,
    ) -> Vec<String> {
        let connection_count = self.connection_count().await;
        if connection_count == 0 {
            return Vec::new();
        }

        let mut connections = self.connections.lock().await;
        let mut failed_peers = Vec::new();

        // Collect peer IDs to avoid borrowing issues
        let peer_ids: Vec<String> = connections
            .keys()
            .filter(|&id| !exclude_peer.contains(id))
            .cloned()
            .collect();

        for peer_id in peer_ids {
            if let Some(writer) = connections.get_mut(&peer_id) {
                if let Err(e) = send_message_to_peer(writer, message).await {
                    eprintln!("Failed to send message to peer {}: {}", peer_id, e);
                    failed_peers.push(peer_id.clone());
                }
            }
        }

        // Remove failed connections
        for peer_id in &failed_peers {
            connections.remove(peer_id);
        }

        if !failed_peers.is_empty() {
            CliOutput::warning(
                &format!("Failed to notify {} peers", failed_peers.len()),
                None,
            );
        }

        failed_peers
    }

    /// Get the number of active connections
    pub async fn connection_count(&self) -> usize {
        let connections = self.connections.lock().await;
        connections.len()
    }

    /// Get list of connected peer IDs
    pub async fn get_connected_peers(&self) -> Vec<String> {
        let connections = self.connections.lock().await;
        connections.keys().cloned().collect()
    }

    /// Check if a specific peer is connected
    pub async fn is_peer_connected(&self, peer_id: &str) -> bool {
        let connections = self.connections.lock().await;
        connections.contains_key(peer_id)
    }
}

impl Default for PeerConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}
