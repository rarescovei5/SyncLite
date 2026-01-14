use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use log::{elog, log};
use p2p::ConnectionManager;
use workspace::{WORKSPACE_DIR, sync::SyncManager};

use crate::network::messages::ServerMessage;

pub async fn handle_file_update_push(
    connection_manager: &ConnectionManager,
    sync_manager: &SyncManager,
    //
    ignore_file_events: &AtomicBool,
    //
    peer_id: &str,
    //
    files_to_write: &std::collections::HashMap<String, Vec<u8>>,
    paths_to_delete: &Vec<String>,
) -> anyhow::Result<()> {
    log!(
        log,
        "📥 Received from {}: {} files, {} deletions",
        peer_id,
        files_to_write.len(),
        paths_to_delete.len()
    );

    // Handle directory deletions
    if !paths_to_delete.is_empty() {
        ignore_file_events.store(true, Ordering::Relaxed);
        for path in paths_to_delete {
            log!(log, "  🗑️  Deleting: {}", path);

            // Recursively delete files in this directory (also updates sync state)
            let _ = sync_manager.delete_directory_recursive(path).await;

            // Delete from filesystem (file or directory)
            let path_buf = PathBuf::from(&*WORKSPACE_DIR).join(path);
            if path_buf.is_dir() {
                let _ = fs::remove_dir_all(&path_buf);
            } else {
                let _ = fs::remove_file(&path_buf);
            }
        }
    }

    if !files_to_write.is_empty() {
        ignore_file_events.store(true, Ordering::Relaxed);
        for (path, _) in files_to_write {
            log!(log, "  ✨ Creating/Updating: {}", path);
        }

        if let Err(e) = sync_manager
            .sync_batch_write_files(&*WORKSPACE_DIR, files_to_write)
            .await
        {
            elog!(log, "Failed to write files: {}", e);
        }
    }

    // Small delay to ensure file watcher events are processed
    tokio::time::sleep(Duration::from_millis(100)).await;
    ignore_file_events.store(false, Ordering::Relaxed);

    // Forward to other peers
    if !files_to_write.is_empty() || !paths_to_delete.is_empty() {
        log!(log, "📡 Forwarding to other peers");
    }

    let failed_peers = connection_manager
        .broadcast_except(
            &ServerMessage::FileUpdatePush {
                files_to_write: files_to_write.clone(),
                paths_to_delete: paths_to_delete.clone(),
            },
            vec![peer_id.to_owned()],
        )
        .await;

    if !failed_peers.is_empty() {
        elog!(
            log,
            "Failed to broadcast file updates to {} peers",
            failed_peers.len()
        );
    }

    Ok(())
}
