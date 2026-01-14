use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use log::{elog, log};
use workspace::{WORKSPACE_DIR, sync::SyncManager};

pub async fn handle_file_update_push(
    sync_manager: &SyncManager,
    //
    ignore_file_events: &AtomicBool,
    //
    files_to_write: std::collections::HashMap<String, Vec<u8>>,
    paths_to_delete: Vec<String>,
) -> anyhow::Result<()> {
    log!(
        log,
        "📥 Received from server: {} files, {} deletions",
        files_to_write.len(),
        paths_to_delete.len()
    );

    // Server is pushing updated files to us
    if !files_to_write.is_empty() {
        ignore_file_events.store(true, Ordering::Relaxed);
        for (path, _) in &files_to_write {
            log!(log, "  ✨ Creating/Updating: {}", path);
        }

        if let Err(e) = sync_manager
            .sync_batch_write_files(&*WORKSPACE_DIR, &files_to_write)
            .await
        {
            elog!(log, "Failed to write files: {}", e);
        }
    }

    // Handle directory deletions
    if !paths_to_delete.is_empty() {
        ignore_file_events.store(true, Ordering::Relaxed);
        for path in &paths_to_delete {
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

    // Wait for file watcher events to be processed
    tokio::time::sleep(Duration::from_millis(100)).await;
    ignore_file_events.store(false, Ordering::Relaxed);

    Ok(())
}

