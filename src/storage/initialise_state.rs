use std::{collections::HashMap, path::Path};

use crate::{
    sync::{compute_sync_state, merge_sync_states},
    utils::{output::CliOutput, read_sync_state, write_json},
};
use colored::Colorize;

pub fn initialise_state(workspace_path: &Path) -> Result<(), String> {
    CliOutput::wrench("Initialising sync state".bright_yellow(), None);
    // Read saved sync state and compute new sync state from the files in the directory
    let saved_sync_state = read_sync_state(workspace_path.join(".synclite").as_path());
    let mut computed_sync_state = HashMap::new();
    compute_sync_state(workspace_path, workspace_path, &mut computed_sync_state)?;

    // Merge the saved and computed states
    let merged_sync_state = merge_sync_states(saved_sync_state, computed_sync_state)?;
    write_json(
        &workspace_path.join(".synclite").join("state.json"),
        &merged_sync_state,
    )?;
    Ok(())
}
