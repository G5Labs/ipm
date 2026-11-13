//! Attachment byte storage (`g5.issue.native.attachment-store`).
//!
//! Whole, write-once objects named by their organization and a random id
//! G5 chose. The interface exposes no paths, renames or listings, so an
//! object-store bucket can implement it later under the same names
//! ([`AttachmentKey::object_name`]). Tenancy is checked above it, by the
//! attachments' owner; this layer only guarantees that a name is built
//! from typed ids and nothing else.

use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use async_trait::async_trait;
use bytes::Bytes;
use futures_core::Stream;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use super::StorageError;

/// One stored object: its organization and the random id G5 chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AttachmentKey {
    pub org_id: Uuid,
    pub id: Uuid,
}

impl AttachmentKey {
    /// `<org>/<first two hex of id>/<id>`: a relative path for a directory
    /// store and an object key for a bucket.
    #[must_use]
    pub fn object_name(&self) -> String {
        let id = self.id.hyphenated().to_string();
        format!("{}/{}/{id}", self.org_id.hyphenated(), &id[..2])
    }
}

/// An object's bytes, in order.
pub type ByteStream = Pin<Box<dyn Stream<Item = io::Result<Bytes>> + Send>>;

/// An opened object.
pub struct StoredObject {
    pub length: u64,
    pub body: ByteStream,
}

impl std::fmt::Debug for StoredObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredObject")
            .field("length", &self.length)
            .finish_non_exhaustive()
    }
}

#[async_trait]
pub trait AttachmentStore: Send + Sync + 'static {
    /// Store a new object. It becomes readable whole or not at all.
    async fn put(&self, key: &AttachmentKey, bytes: Bytes) -> Result<(), StorageError>;

    /// Open an object; [`StorageError::NotFound`] when there is none.
    async fn get(&self, key: &AttachmentKey) -> Result<StoredObject, StorageError>;

    /// Delete an object; deleting one that is absent succeeds.
    async fn delete(&self, key: &AttachmentKey) -> Result<(), StorageError>;
}

/// Objects as files under one directory: a local disk, or a dedicated
/// Filestore mount on hosted deployments.
#[derive(Debug)]
pub struct DirectoryAttachmentStore {
    root: PathBuf,
}

impl DirectoryAttachmentStore {
    /// Use `root`, creating it when missing.
    pub fn new(root: PathBuf) -> Result<Self, StorageError> {
        std::fs::create_dir_all(&root).map_err(|e| {
            StorageError::Backend(format!(
                "cannot create attachment store {}: {e}",
                root.display()
            ))
        })?;
        Ok(Self { root })
    }

    fn path(&self, key: &AttachmentKey) -> PathBuf {
        self.root.join(key.object_name())
    }
}

fn not_found(path: &Path, error: io::Error) -> StorageError {
    if error.kind() == io::ErrorKind::NotFound {
        StorageError::NotFound(path.display().to_string())
    } else {
        StorageError::Io(error)
    }
}

#[async_trait]
impl AttachmentStore for DirectoryAttachmentStore {
    async fn put(&self, key: &AttachmentKey, bytes: Bytes) -> Result<(), StorageError> {
        let target = self.path(key);
        let dir = target
            .parent()
            .expect("an object name has a directory")
            .to_path_buf();
        tokio::fs::create_dir_all(&dir).await?;
        // Written whole and synced under a name no reader looks for, then
        // renamed into place, so a reader never sees a partial object.
        let tmp = dir.join(format!(".{}.{}.tmp", key.id, Uuid::new_v4().simple()));
        let written = async {
            let mut file = tokio::fs::File::create(&tmp).await?;
            file.write_all(&bytes).await?;
            file.sync_all().await?;
            drop(file);
            tokio::fs::rename(&tmp, &target).await?;
            // The rename itself survives a crash once its directory is synced.
            tokio::fs::File::open(&dir).await?.sync_all().await
        }
        .await;
        if let Err(error) = written {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(StorageError::Io(error));
        }
        Ok(())
    }

    async fn get(&self, key: &AttachmentKey) -> Result<StoredObject, StorageError> {
        let path = self.path(key);
        let file = tokio::fs::File::open(&path)
            .await
            .map_err(|e| not_found(&path, e))?;
        let length = file.metadata().await?.len();
        Ok(StoredObject {
            length,
            body: Box::pin(tokio_util::io::ReaderStream::with_capacity(file, 64 * 1024)),
        })
    }

    async fn delete(&self, key: &AttachmentKey) -> Result<(), StorageError> {
        let path = self.path(key);
        match tokio::fs::remove_file(&path).await {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(StorageError::Io(error)),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn read_all(mut object: StoredObject) -> Vec<u8> {
        use std::future::poll_fn;
        let mut out = Vec::new();
        while let Some(chunk) = poll_fn(|cx| object.body.as_mut().poll_next(cx)).await {
            out.extend_from_slice(&chunk.unwrap());
        }
        out
    }

    #[test]
    fn names_come_from_the_typed_ids_only() {
        let key = AttachmentKey {
            org_id: Uuid::parse_str("0b7f3c2e-1111-4222-8333-944455556666").unwrap(),
            id: Uuid::parse_str("af01c3d4-aaaa-4bbb-8ccc-ddddeeeeffff").unwrap(),
        };
        assert_eq!(
            key.object_name(),
            "0b7f3c2e-1111-4222-8333-944455556666/af/af01c3d4-aaaa-4bbb-8ccc-ddddeeeeffff"
        );
    }

    #[tokio::test]
    async fn objects_are_written_read_and_deleted_whole() {
        let dir = tempfile::tempdir().unwrap();
        let store = DirectoryAttachmentStore::new(dir.path().join("attachments")).unwrap();
        let key = AttachmentKey {
            org_id: Uuid::new_v4(),
            id: Uuid::new_v4(),
        };
        assert!(matches!(
            store.get(&key).await,
            Err(StorageError::NotFound(_))
        ));
        let bytes: Vec<u8> = (0..200_000u32).map(|n| (n % 251) as u8).collect();
        store.put(&key, Bytes::from(bytes.clone())).await.unwrap();
        let object = store.get(&key).await.unwrap();
        assert_eq!(object.length, bytes.len() as u64);
        assert_eq!(read_all(object).await, bytes);

        let stored = dir.path().join("attachments").join(key.object_name());
        assert!(stored.is_file());
        let siblings: Vec<_> = std::fs::read_dir(stored.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(siblings.len(), 1, "no temporary file is left behind");

        store.delete(&key).await.unwrap();
        assert!(!stored.exists());
        store.delete(&key).await.unwrap();
    }
}
