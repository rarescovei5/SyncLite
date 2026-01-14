use std::{fs, path::Path};

use sha2::{Digest, Sha256};

/// Calculate the SHA-256 hash of a file
pub fn calculate_file_hash(path: &Path) -> anyhow::Result<String> {
    let contents = fs::read(path)?;

    let mut hasher = Sha256::new();
    hasher.update(&contents);
    let result = hasher.finalize();

    Ok(format!("{:x}", result))
}

#[cfg(windows)]
/// Make a file hidden on Windows
pub fn make_hidden_windows(path: &Path) -> anyhow::Result<()> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::fileapi::SetFileAttributesW;
    use winapi::um::winnt::FILE_ATTRIBUTE_HIDDEN;

    let wide_path: Vec<u16> = OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let result = unsafe { SetFileAttributesW(wide_path.as_ptr(), FILE_ATTRIBUTE_HIDDEN) };

    if result == 0 {
        anyhow::bail!(
            "Failed to set hidden attribute on Windows for path: {}",
            path.display()
        );
    }

    Ok(())
}
