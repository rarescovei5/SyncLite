use std::path::{Path, PathBuf};

// Sandboxed version of file system operations that checks if the path is within the sandbox
pub struct FileSystem {
    path: PathBuf,
}

impl FileSystem {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl FileSystem {
    pub async fn read_file(&self, path: &Path) -> Result<String, Box<dyn std::error::Error>> {
        if !path.starts_with(&self.path) {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Path is not within the sandbox",
            )));
        }
        let contents = tokio::fs::read_to_string(path).await?;
        Ok(contents)
    }
    pub async fn write_file(
        &self,
        path: &Path,
        contents: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !path.starts_with(&self.path) {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Path is not within the sandbox",
            )));
        }
        tokio::fs::write(path, contents).await?;
        Ok(())
    }
    pub async fn delete_file(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if !path.starts_with(&self.path) {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Path is not within the sandbox",
            )));
        }
        tokio::fs::remove_file(path).await?;
        Ok(())
    }
    pub async fn create_directory(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if !path.starts_with(&self.path) {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Path is not within the sandbox",
            )));
        }
        tokio::fs::create_dir_all(path).await?;
        Ok(())
    }
    pub async fn delete_directory(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if !path.starts_with(&self.path) {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Path is not within the sandbox",
            )));
        }
        tokio::fs::remove_dir_all(path).await?;
        Ok(())
    }
}
