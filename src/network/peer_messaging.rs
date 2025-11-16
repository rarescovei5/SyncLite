use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
};
use uuid::Uuid;

// Message types for peer communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PeerMessage {
    // Sent to acknowledge successful connection
    ConnectionAck { peer_id: String, leader_id: String },
    // Sent to update peer list
    PeerListUpdate { peers: Vec<String> },
}

// Generate unique peer ID
pub fn generate_peer_id() -> String {
    Uuid::new_v4().to_string()
}

// Send message to a peer
pub async fn send_message_to_peer(
    stream: &mut OwnedWriteHalf,
    message: &PeerMessage,
) -> Result<(), Box<dyn std::error::Error>> {
    let serialized = serde_json::to_string(message)?;
    let message_bytes = serialized.as_bytes();
    let length = message_bytes.len() as u32;

    // Send length prefix followed by message
    stream.write_u32(length).await?;
    stream.write_all(message_bytes).await?;
    stream.flush().await?;

    Ok(())
}

// Receive message from a peer
pub async fn receive_message_from_peer(
    stream: &mut OwnedReadHalf,
) -> Result<PeerMessage, Box<dyn std::error::Error>> {
    // Read length prefix
    let length = stream.read_u32().await?;

    // Read message
    let mut message_bytes = vec![0u8; length as usize];
    stream.read_exact(&mut message_bytes).await?;

    let message_str = String::from_utf8(message_bytes)?;
    let message: PeerMessage = serde_json::from_str(&message_str)?;

    Ok(message)
}
