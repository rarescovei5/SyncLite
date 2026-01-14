use std::{
    collections::HashMap,
    fs,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use log::{elog, log};
use p2p::ConnectionManager;
use workspace::{
    WORKSPACE_DIR,
    sync::{SyncManager, SyncState, determine_winning_files},
};

use crate::network::messages::ServerMessage;

pub async fn handle_initial_sync_push(
    connection_manager: &ConnectionManager,
    sync_manager: &SyncManager,
    //
    ignore_file_events: &AtomicBool,
    //
    peer_id: &str,
    //
    peer_sync_state: &SyncState,
) -> anyhow::Result<()> {
    log!(log, "📥 Received initial sync from {}", peer_id);

    // STEP 2: Check winning files for both sides
    let server_sync_state = sync_manager.state().await;
    let (
        our_winning_files,
        their_winning_files,
        files_to_delete_from_server,
        files_to_delete_from_peer,
    ) = determine_winning_files(&server_sync_state, &peer_sync_state);

    log!(
        log,
        "Sync result: {} to send, {} to receive, {} to delete locally, {} to delete remotely",
        our_winning_files.len(),
        their_winning_files.len(),
        files_to_delete_from_server.len(),
        files_to_delete_from_peer.len()
    );

    // Handle file deletions first
    if !files_to_delete_from_server.is_empty() {
        ignore_file_events.store(true, Ordering::Relaxed);

        for path in &files_to_delete_from_server {
            log!(log, "  🗑️  Deleting: {}", path);
        }

        // Unified filesystem + state delete operation
        if let Err(e) = sync_manager
            .sync_batch_delete_files(
                &*WORKSPACE_DIR,
                &files_to_delete_from_server,
                Some(&peer_sync_state),
            )
            .await
        {
            elog!(log, "Failed to delete files: {}", e);
        }

        // Small delay to ensure file watcher events are processed
        tokio::time::sleep(Duration::from_millis(100)).await;
        ignore_file_events.store(false, Ordering::Relaxed);
    }

    // STEP 3: Send our winning files with content and request their winning files
    if !our_winning_files.is_empty() || !their_winning_files.is_empty() {
        // Read content of our winning files (paths are relative to workspace)
        let mut my_winning_files_with_content: HashMap<String, Vec<u8>> = HashMap::new();
        for file_path in &our_winning_files {
            // Convert relative path to absolute path for file operations
            let full_path = WORKSPACE_DIR.join(file_path);
            match fs::read(&full_path) {
                Ok(content) => {
                    // Store with relative path as key
                    my_winning_files_with_content.insert(file_path.clone(), content);
                }
                Err(e) => {
                    elog!(log, "Failed to read workspace file {}: {}", file_path, e);
                }
            }
        }

        if !my_winning_files_with_content.is_empty() {
            log!(
                log,
                "Sending {} winning files to peer: {}",
                my_winning_files_with_content.len(),
                peer_id
            );
        }

        if !their_winning_files.is_empty() {
            log!(
                log,
                "Requesting {} winning files from peer: {}",
                their_winning_files.len(),
                peer_id
            );
        }

        if let Err(e) = connection_manager
            .send_to_peer(
                &peer_id,
                &ServerMessage::InitialSyncPushResponse {
                    files_to_update: my_winning_files_with_content,
                    files_to_delete: files_to_delete_from_peer,
                    files_to_send_back: their_winning_files,
                },
            )
            .await
        {
            elog!(
                log,
                "Failed to send initial sync push response to peer {}: {}",
                peer_id,
                e
            );
        }
    }
    Ok(())
}
