use std::collections::HashMap;

use crate::models::SyncState;

pub fn merge_sync_states(
    sync_state_1: SyncState,
    sync_state_2: SyncState,
) -> Result<SyncState, String> {
    let mut merged_sync_state = sync_state_1.clone();
    let files_to_update = files_to_update(&sync_state_1, &sync_state_2);
    for (path, entry) in files_to_update {
        merged_sync_state.insert(path, entry);
    }
    Ok(merged_sync_state)
}

/// Returns a map of files to update in sync_state_1 to match sync_state_2
/// Applies Last Writer Wins conflict resolution strategy
pub fn files_to_update(sync_state_1: &SyncState, sync_state_2: &SyncState) -> SyncState {
    let mut updates = HashMap::new();

    for (path, entry2) in sync_state_2.iter() {
        match sync_state_1.get(path) {
            Some(entry1) => {
                // File exists in both states - apply last-writer-wins
                // Only update if entry2 is strictly newer OR if content/status differs with newer timestamp
                if entry1.hash != entry2.hash || entry1.is_deleted != entry2.is_deleted {
                    // There's a difference - use the newer one
                    if entry2.last_modified > entry1.last_modified {
                        updates.insert(path.clone(), entry2.clone());
                    }
                    // If entry1 is newer or equal, we keep entry1 (no update needed)
                }
                // If hash and is_deleted are the same, no update needed regardless of timestamp
            }
            None => {
                // File exists in state_2 but not in state_1 - it's a new file from state_2
                updates.insert(path.clone(), entry2.clone());
            }
        }
    }

    updates
}
