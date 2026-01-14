pub mod peers;
pub mod setup;
pub mod sync;

//

use common::Lazy;
use std::path::PathBuf;
pub static WORKSPACE_DIR: Lazy<PathBuf> = Lazy::new(None);
pub static STORAGE_DIR: Lazy<PathBuf> = Lazy::new(Some(|| WORKSPACE_DIR.join(".synclite")));
