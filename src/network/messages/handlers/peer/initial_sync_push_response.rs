use std::{
    collections::HashMap,
    fs,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use log::{elog, log};
use p2p::send_message_to_peer;
use tokio::net::tcp::OwnedWriteHalf;
use workspace::{WORKSPACE_DIR, sync::SyncManager};

use crate::network::messages::PeerMessage;

pub async fn handle_initial_sync_push_response(
    sync_manager: &SyncManager,
    //
    ignore_file_events: &AtomicBool,
    //
    writer: &mut OwnedWriteHalf,
    //
    files_to_update: HashMap<String, Vec<u8>>,
    files_to_delete: Vec<String>,
    files_to_send_back: Vec<String>,
) -> anyhow::Result<()> {
    log!(
        log,
        "📥 Initial sync response: {} to receive, {} to delete, {} to send",
        files_to_update.len(),
        files_to_delete.len(),
        files_to_send_back.len()
    );

    // Handle file deletions first
    if !files_to_delete.is_empty() {
        ignore_file_events.store(true, Ordering::Relaxed);
        for path in &files_to_delete {
            log!(log, "  🗑️  Deleting: {}", path);
        }

        // Unified filesystem + state delete operation
        if let Err(e) = sync_manager
            .sync_batch_delete_files(&*WORKSPACE_DIR, &files_to_delete, None)
            .await
        {
            elog!(log, "Failed to delete files: {}", e);
        }
    }

    // Apply server's winning files to our file system
    if !files_to_update.is_empty() {
        ignore_file_events.store(true, Ordering::Relaxed);
        for (path, _) in &files_to_update {
            log!(log, "  ✨ Creating/Updating: {}", path);
        }

        // Unified filesystem + state write operation
        if let Err(e) = sync_manager
            .sync_batch_write_files(&*WORKSPACE_DIR, &files_to_update)
            .await
        {
            elog!(log, "Failed to write files: {}", e);
        }
    }

    // Wait for file watcher events to be processed
    tokio::time::sleep(Duration::from_millis(100)).await;
    ignore_file_events.store(false, Ordering::Relaxed);

    // Read requested files and send back to server
    let mut our_winning_files: HashMap<String, Vec<u8>> = HashMap::new();
    for path in files_to_send_back {
        let full_path = WORKSPACE_DIR.join(&path);
        match fs::read(&full_path) {
            Ok(content) => {
                our_winning_files.insert(path, content);
            }
            Err(e) => {
                elog!(log, "Failed to read requested file {}: {}", path, e);
            }
        }
    }

    if !our_winning_files.is_empty() {
        log!(
            log,
            "📤 Sending {} files to server",
            our_winning_files.len()
        );
        let message = PeerMessage::FileUpdatePush {
            files_to_write: our_winning_files,
            paths_to_delete: Vec::new(),
        };

        if let Err(e) = send_message_to_peer(writer, &message).await {
            elog!(log, "Failed to send file content response: {}", e);
        }
    }

    Ok(())
}
