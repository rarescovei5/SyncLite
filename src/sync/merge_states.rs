use crate::models::SyncState;

/// This function returns a tuple of four vectors which mean the following:
/// 1. The files that **sync_state_2** needs to get from the **sync_state_1**
/// 2. The files that **sync_state_1** needs to get from the **sync_state_2**
/// 3. The files that **sync_state_2** needs to delete from the **sync_state_1**
/// 4. The files that **sync_state_1** needs to delete from the **sync_state_2**
pub fn determine_winning_files(
    sync_state_1: &SyncState,
    sync_state_2: &SyncState,
) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    let mut winning_files_1 = Vec::new();
    let mut winning_files_2 = Vec::new();
    let mut files_to_delete_from_1 = Vec::new();
    let mut files_to_delete_from_2 = Vec::new();

    // Collect all unique file paths from both states
    let mut all_files = std::collections::HashSet::new();
    for path in sync_state_1.keys() {
        all_files.insert(path.clone());
    }
    for path in sync_state_2.keys() {
        all_files.insert(path.clone());
    }

    for path in all_files {
        match (sync_state_1.get(&path), sync_state_2.get(&path)) {
            (Some(file_1), Some(file_2)) => {
                // File exists in both states - apply last-writer-wins
                if file_1.hash != file_2.hash {
                    if file_2.last_modified > file_1.last_modified {
                        // Peer wins
                        if file_2.is_deleted {
                            files_to_delete_from_1.push(path);
                        } else {
                            winning_files_2.push(path);
                        }
                    } else if file_1.last_modified > file_2.last_modified {
                        // We win
                        if file_1.is_deleted {
                            files_to_delete_from_2.push(path);
                        } else {
                            winning_files_1.push(path);
                        }
                    }
                    // If timestamps are equal, no action needed (keep current state)
                }
            }
            (Some(file_1), None) => {
                // We have the file, peer doesn't - we win
                if !file_1.is_deleted {
                    winning_files_1.push(path);
                }
            }
            (None, Some(file_2)) => {
                // Peer has the file, we don't - peer wins
                if !file_2.is_deleted {
                    winning_files_2.push(path);
                }
            }
            (None, None) => {
                // Neither has the file - shouldn't happen but skip
            }
        }
    }

    (
        winning_files_1,
        winning_files_2,
        files_to_delete_from_1,
        files_to_delete_from_2,
    )
}
