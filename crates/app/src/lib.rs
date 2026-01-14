pub mod registry;

//

use common::Lazy;
use std::path::PathBuf;
static APP_DATA_DIR: Lazy<PathBuf> = Lazy::new(Some(|| {
    dirs::data_dir().map(|dir| dir.join("synclite")).unwrap()
}));
