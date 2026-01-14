pub mod network;

//

use log::elog;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use workspace::{peers::PeersManager, setup, sync::SyncManager};

pub fn setup(path: PathBuf) -> anyhow::Result<(Arc<SyncManager>, Arc<PeersManager>)> {
    let abs_workspace_path = match Path::new(&path).canonicalize() {
        Ok(mut canonical_workspace) => {
            let canonical_str = canonical_workspace.to_string_lossy();
            if canonical_str.starts_with(r"\\?\") {
                canonical_workspace = PathBuf::from(&canonical_str[4..]);
            }
            canonical_workspace
        }
        Err(_) => {
            elog!(error, "Invalid workspace path: {}", path.display());
            std::process::exit(1);
        }
    };

    setup::initialise_workspace(abs_workspace_path);

    let sync_config = Arc::new(SyncManager::new()?);
    let peers_config = Arc::new(PeersManager::new()?);

    Ok((sync_config, peers_config))
}
