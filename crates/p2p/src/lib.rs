use serde::{Serialize, de::DeserializeOwned};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::Mutex;

use log::elog;

/// Manages active connections to all peers
#[derive(Clone)]
pub struct ConnectionManager {
    /// Map of peer_id -> write stream for sending messages
    connections: Arc<Mutex<HashMap<String, OwnedWriteHalf>>>,
}

/// Connection manager for peer-to-peer communication
impl ConnectionManager {
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
    pub async fn send_to_peer<T: Serialize>(
        &self,
        peer_id: &str,
        message: &T,
    ) -> anyhow::Result<()> {
        let mut connections = self.connections.lock().await;

        if let Some(writer) = connections.get_mut(peer_id) {
            send_message_to_peer(writer, message).await?;
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("No active connection to peer {}", peer_id),
            )
            .into())
        }
    }

    /// Broadcast a message to all connected peers
    pub async fn broadcast_message<T: Serialize>(&self, message: &T) -> Vec<String> {
        let connection_count = self.connection_count().await;
        if connection_count == 0 {
            return Vec::new();
        }

        let mut connections = self.connections.lock().await;
        let mut failed_peers = Vec::new();

        // Collect peer IDs to avoid borrowing issues
        let peer_ids: Vec<String> = connections.keys().cloned().collect();

        // Send messages while holding the lock (avoid recursive lock)
        for peer_id in peer_ids {
            if let Some(writer) = connections.get_mut(&peer_id) {
                if let Err(e) = send_message_to_peer(writer, message).await {
                    elog!(log, "Failed to send message to peer {}: {}", peer_id, e);
                    failed_peers.push(peer_id.clone());
                }
            }
        }

        // Remove failed connections
        for peer_id in &failed_peers {
            connections.remove(peer_id);
        }

        if !failed_peers.is_empty() {
            elog!(log, "Failed to notify {} peers", failed_peers.len());
        }

        failed_peers
    }

    /// Broadcast a message to all peers except the ones in the exclude list (useful for forwarding)
    pub async fn broadcast_except<T: Serialize>(
        &self,
        message: &T,
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

        // Send messages while holding the lock (avoid recursive lock)
        for peer_id in peer_ids {
            if let Some(writer) = connections.get_mut(&peer_id) {
                if let Err(e) = send_message_to_peer(writer, message).await {
                    elog!(log, "Failed to send message to peer {}: {}", peer_id, e);
                    failed_peers.push(peer_id.clone());
                }
            }
        }

        // Remove failed connections
        for peer_id in &failed_peers {
            connections.remove(peer_id);
        }

        if !failed_peers.is_empty() {
            elog!(log, "Failed to notify {} peers", failed_peers.len());
        }

        failed_peers
    }

    /// Get the number of active connections
    pub async fn connection_count(&self) -> usize {
        let connections = self.connections.lock().await;
        connections.len()
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

// Helper functions for sending and receiving messages

// Send message to a peer
pub async fn send_message_to_peer<T: Serialize>(
    writer: &mut OwnedWriteHalf,
    message: &T,
) -> anyhow::Result<()> {
    let serialized = serde_json::to_string(message)?;
    let message_bytes = serialized.as_bytes();
    let length = message_bytes.len() as u32;

    // Send length prefix followed by message
    writer.write_u32(length).await?;
    writer.write_all(message_bytes).await?;
    writer.flush().await?;

    Ok(())
}

// Receive message from a peer
pub async fn receive_message_from_peer<T: DeserializeOwned>(
    reader: &mut OwnedReadHalf,
) -> anyhow::Result<T> {
    // Read length prefix
    let length = reader.read_u32().await?;

    // Read message
    let mut message_bytes = vec![0u8; length as usize];
    reader.read_exact(&mut message_bytes).await?;

    let message_str = String::from_utf8(message_bytes)?;
    let message: T = serde_json::from_str(&message_str)?;

    Ok(message)
}
