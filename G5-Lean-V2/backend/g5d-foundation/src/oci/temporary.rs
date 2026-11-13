//! Owned unpublished directories, including cache locks. Cleanup must also
//! run when a preparation future is dropped by the host startup deadline.

use std::path::PathBuf;

pub(super) struct TemporaryDirectory {
    pub path: PathBuf,
}

impl TemporaryDirectory {
    pub fn create(path: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(path = %self.path.display(), %error, "OCI temporary directory cleanup failed");
            }
        }
    }
}
