use log::{elog, log};
use workspace::peers::PeersManager;

pub async fn handle_peer_list_update(
    peers_config: &PeersManager,
    peers: Vec<String>,
) -> anyhow::Result<()> {
    log!(log, "Updated connected peers list");

    // Update local peers config with full peer list
    if let Err(e) = peers_config.set_peers(peers).await {
        elog!(log, "Failed to update peers config: {}", e);
    }

    Ok(())
}
