use crate::models::SyncState;

/// This function returns a tuple of four vectors which mean the following:
/// 1. The files that the peer needs to get from the server
/// 2. The files that the server needs to get from the peer
/// 3. The files that the server needs to delete from the peer
/// 4. The files that the peer needs to delete from the server
pub fn determine_winning_files(
    server_sync_state: &SyncState,
    peer_sync_state: &SyncState,
) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    let mut server_winning_files = Vec::new();
    let mut peer_winning_files = Vec::new();
    let mut files_to_delete_from_server = Vec::new();
    let mut files_to_delete_from_peer = Vec::new();

    // Collect all unique file paths from both states
    let mut all_files = std::collections::HashSet::new();
    for path in server_sync_state.keys() {
        all_files.insert(path.clone());
    }
    for path in peer_sync_state.keys() {
        all_files.insert(path.clone());
    }

    for path in all_files {
        match (server_sync_state.get(&path), peer_sync_state.get(&path)) {
            (Some(server_file), Some(peer_file)) => {
                // File exists in both states - apply last-writer-wins
                if server_file.hash != peer_file.hash {
                    if peer_file.last_modified > server_file.last_modified {
                        // Peer wins
                        if peer_file.is_deleted {
                            files_to_delete_from_server.push(path);
                        } else {
                            peer_winning_files.push(path);
                        }
                    } else if server_file.last_modified > peer_file.last_modified {
                        // We win
                        if server_file.is_deleted {
                            files_to_delete_from_server.push(path);
                        } else {
                            server_winning_files.push(path);
                        }
                    }
                    // If timestamps are equal, no action needed (keep current state)
                }
            }
            (Some(server_file), None) => {
                // We have the file, peer doesn't - we win
                if server_file.is_deleted {
                    files_to_delete_from_peer.push(path);
                }
            }
            (None, Some(peer_file)) => {
                // Peer has the file, we don't - peer wins
                if peer_file.is_deleted {
                    files_to_delete_from_server.push(path);
                }
            }
            (None, None) => {
                // Neither has the file - shouldn't happen but skip
            }
        }
    }

    (
        server_winning_files,
        peer_winning_files,
        files_to_delete_from_server,
        files_to_delete_from_peer,
    )
}
