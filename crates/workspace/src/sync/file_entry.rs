use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileEntry {
    pub hash: Option<String>,
    pub is_deleted: bool,
    pub last_modified: DateTime<Utc>,
}

impl FileEntry {
    pub fn delete(self) -> Self {
        Self {
            hash: None,
            is_deleted: true,
            last_modified: self.last_modified,
        }
    }
}
