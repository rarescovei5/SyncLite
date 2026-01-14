mod file_entry;
mod manager;
mod merge;
mod state;

pub use file_entry::*;
pub use manager::*;
pub use merge::*;
pub use state::*;

//

use crate::STORAGE_DIR;
use common::Lazy;
use std::path::PathBuf;
static STATE_PATH: Lazy<PathBuf> = Lazy::new(Some(|| STORAGE_DIR.join("state.json")));
