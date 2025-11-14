use crate::storage::json_utils::read_sync_state;
use crate::storage::types::SyncState;
use crate::utils::handle_result_or_exit;
use std::path::Path;

pub fn initialise_state(path: &str) {
    let storage_dir = Path::new(path).join(".synclite");
    let stored_sync_state = handle_result_or_exit(read_sync_state(&storage_dir));
    let current_sync_state = handle_result_or_exit(SyncState::from_directory(path));

    println!("Stored sync state:");
    println!("{:#?}", stored_sync_state);
    println!("\nCurrent directory state:");
    println!("{:#?}", current_sync_state);
}
