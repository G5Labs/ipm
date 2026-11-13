//! Content-addressed layer cache.
//!
//! Layout: `<root>/sha256/<hex>/` per layer. Each layer directory holds the
//! extracted tar contents of the corresponding gzipped blob plus a sentinel
//! `.complete` marker written after a successful extract.
//!
//! Two complementary locks coordinate concurrent populations:
//!
//! 1. **In-process**: a short-held mutex protects per-digest completion
//!    channels, deduplicating pulls and waking waiters even on cancellation.
//! 2. **Cross-process**: a per-digest `.lock` directory adjacent to the
//!    final cache entry is claimed via the atomic `fs::create_dir` mkdir.
//!    Losers poll with `tokio::time::sleep` (NOT `std::thread::sleep` —
//!    blocking the tokio reactor would stall every other daemon future
//!    while a contended pull completes) and then reuse the `.complete`
//!    layer once the winner releases the lock.
//!
//! The `.complete` marker exists so an interrupted extract — or a manually
//! mkdir'd empty layer dir — is not mistaken for a cache hit. Cache lookup
//! checks both that the layer dir exists AND that the marker file is present.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::sync::watch;

use super::error::OciError;
use super::extract::{encode_explicit_dirs, extract_gzipped_tar, EXPLICIT_DIRS_MARKER};
use super::registry::{Descriptor, ImageRef, RegistryClient};
use super::temporary::TemporaryDirectory;

pub const DEFAULT_CACHE_ROOT: &str = "/var/cache/g5d/oci-layers";
const ENV_CACHE_DIR: &str = "G5D_OCI_CACHE_DIR";
const COMPLETE_MARKER: &str = ".complete";
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Maximum time to wait for a cross-process cache lock before declaring
/// it stale and returning `OciError::CacheLockTimeout`. Codex final-review
/// B2 of PR #122: without this, a crash mid-extract would cause every
/// future pull to loop forever waiting for `.complete` to appear.
///
/// 10 minutes is generous for typical layer sizes (~100 MB → seconds to
/// extract) but bounded enough that a wedged supervisor surfaces quickly
/// in operator logs instead of silently hanging microVM launches.
const LOCK_MAX_WAIT: Duration = Duration::from_secs(600);

/// Stale-lock recovery threshold. If the lock dir has not been touched
/// (mtime) in this long AND no `.complete` marker has appeared, the lock
/// is considered abandoned and we attempt to take it over. Conservative
/// (5 min) to avoid racing live extractors of large layers.
const LOCK_STALE_AGE: Duration = Duration::from_secs(300);

pub struct LayerCache {
    root: PathBuf,
    inflight: Mutex<HashMap<String, watch::Sender<()>>>,
}

struct InflightPull<'a> {
    cache: &'a LayerCache,
    digest: &'a str,
}

impl Drop for InflightPull<'_> {
    fn drop(&mut self) {
        // Dropping the sender wakes every subscriber, including a waiter
        // that has not been polled yet. Never hold this mutex across I/O.
        self.cache
            .inflight
            .lock()
            .expect("inflight cache lock")
            .remove(self.digest);
    }
}

impl LayerCache {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            inflight: Mutex::new(HashMap::new()),
        }
    }

    pub fn from_env() -> Self {
        let root = std::env::var(ENV_CACHE_DIR)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_CACHE_ROOT));
        Self::new(root)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Layer cache dir for a raw digest string.
    ///
    /// **Trust expectation**: the caller has already validated the digest via
    /// `Digest::parse`. This method is preserved for back-compat with
    /// internal call sites that already validate; prefer `layer_dir_for`
    /// which takes a typed `Digest`.
    ///
    /// Codex final-review B1 of PR #122: the previous splash-on-':' fallback
    /// (`unwrap_or(("sha256", digest))`) would have created a cache path of
    /// `<root>/sha256:../etc/passwd` for a malformed digest. Now path
    /// construction requires either a typed `Digest` (preferred) or a
    /// digest string that has been parsed upstream.
    pub fn layer_dir(&self, digest: &str) -> PathBuf {
        let (algo, hex) = digest.split_once(':').unwrap_or(("sha256", digest));
        self.root.join(algo).join(hex)
    }

    /// Layer cache dir for a typed digest. Path component is the validated
    /// hex only (never `..`, `/`, NUL, etc.) — safe under any operator
    /// input.
    pub fn layer_dir_for(&self, digest: &super::registry::Digest) -> PathBuf {
        self.root.join("sha256").join(digest.hex())
    }

    fn is_complete(layer_dir: &Path) -> bool {
        if !std::fs::symlink_metadata(layer_dir).is_ok_and(|meta| meta.is_dir()) {
            return false;
        }
        let Ok(marker) = std::fs::symlink_metadata(layer_dir.join(COMPLETE_MARKER)) else {
            return false;
        };
        if !marker.is_file() {
            return false;
        }
        // Older extractors could overwrite an image file through a hardlinked
        // marker. Re-extract those layers as well as symlink-marker entries.
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if marker.nlink() != 1 {
                return false;
            }
        }
        // Layers cached before the explicit-directory sidecar existed cannot
        // tell a recorded directory from a synthesized parent; extract them
        // again (one blob fetch per layer, once per host).
        std::fs::symlink_metadata(layer_dir.join(EXPLICIT_DIRS_MARKER))
            .is_ok_and(|meta| meta.is_file())
    }

    pub async fn ensure_layer(
        &self,
        client: &dyn RegistryClient,
        image: &ImageRef,
        descriptor: &Descriptor,
    ) -> Result<PathBuf, OciError> {
        // Codex final-review B1 of PR #122: validate the descriptor digest
        // BEFORE deriving any cache path from it. A malformed/malicious
        // descriptor (e.g., a compromised registry returning a manifest
        // with digest "sha256:../etc/passwd") would otherwise create a
        // cache path outside `self.root`.
        let parsed = super::registry::Digest::parse(&descriptor.digest)?;
        let dest = self.layer_dir_for(&parsed);
        if Self::is_complete(&dest) {
            tracing::debug!(digest = %parsed, "OCI layer cache hit");
            return Ok(dest);
        }

        loop {
            let existing = {
                let mut map = self.inflight.lock().expect("inflight cache lock");
                if let Some(sender) = map.get(&descriptor.digest) {
                    Some(sender.subscribe())
                } else {
                    map.insert(descriptor.digest.clone(), watch::channel(()).0);
                    None
                }
            };
            match existing {
                None => break,
                Some(mut completion) => {
                    let _ = completion.changed().await;
                    if Self::is_complete(&dest) {
                        return Ok(dest);
                    }
                    // A failed/cancelled owner releases ownership so one
                    // waiter can retry while the others subscribe again.
                }
            }
        }
        let _inflight = InflightPull {
            cache: self,
            digest: &descriptor.digest,
        };
        self.do_fetch_and_extract(client, image, descriptor, &dest)
            .await
    }

    async fn do_fetch_and_extract(
        &self,
        client: &dyn RegistryClient,
        image: &ImageRef,
        descriptor: &Descriptor,
        dest: &Path,
    ) -> Result<PathBuf, OciError> {
        let (algo, hex) = descriptor
            .digest
            .split_once(':')
            .unwrap_or(("sha256", descriptor.digest.as_str()));
        let algo_dir = self.root.join(algo);
        tokio::fs::create_dir_all(&algo_dir)
            .await
            .map_err(|e| OciError::CacheIo {
                path: algo_dir.display().to_string(),
                source: e,
            })?;

        // Cross-process lock: a `.<hex>.lock` directory adjacent to the
        // layer dir. `fs::create_dir` is atomic across processes. If
        // another process holds the lock, poll the `.complete` marker
        // with `tokio::time::sleep` (NOT std::thread::sleep — that would
        // block the tokio reactor for every other future on this worker)
        // until either the marker shows up (cache hit, winner finished)
        // or the lock dir disappears (winner errored, try ourselves).
        let lock_dir = algo_dir.join(format!(".{}.lock", hex));
        // Codex final-review B2 of PR #122: bound the wait + recover from
        // stale locks. Without this, a process crash after creating the
        // lock dir but before writing `.complete` would cause every future
        // pull to spin forever.
        let lock_start = std::time::Instant::now();
        let lock = loop {
            match tokio::task::spawn_blocking({
                let lock_dir = lock_dir.clone();
                move || TemporaryDirectory::create(lock_dir)
            })
            .await
            .map_err(|join_err| OciError::CacheIo {
                path: lock_dir.display().to_string(),
                source: std::io::Error::other(join_err.to_string()),
            })? {
                Ok(directory) => break Arc::new(directory),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if Self::is_complete(dest) {
                        return Ok(dest.to_path_buf());
                    }
                    // Bounded total wait.
                    if lock_start.elapsed() >= LOCK_MAX_WAIT {
                        return Err(OciError::CacheLockTimeout {
                            path: lock_dir.display().to_string(),
                            waited_secs: LOCK_MAX_WAIT.as_secs(),
                        });
                    }
                    // Stale-lock recovery: if the lock dir's mtime is older
                    // than LOCK_STALE_AGE and `.complete` still hasn't
                    // appeared, the holder almost certainly crashed.
                    // Remove the abandoned lock dir + retry the create.
                    if let Ok(meta) = tokio::fs::metadata(&lock_dir).await {
                        if let Ok(modified) = meta.modified() {
                            if let Ok(age) = modified.elapsed() {
                                if age >= LOCK_STALE_AGE && !Self::is_complete(dest) {
                                    tracing::warn!(
                                        path = %lock_dir.display(),
                                        age_secs = age.as_secs(),
                                        "Stale OCI cache lock detected; the previous \
                                         holder appears to have crashed mid-extract. \
                                         Removing + retrying."
                                    );
                                    let _ = tokio::fs::remove_dir(&lock_dir).await;
                                    continue;
                                }
                            }
                        }
                    }
                    tokio::time::sleep(LOCK_POLL_INTERVAL).await;
                    // Re-check after sleep: another process may have just
                    // released the lock dir.
                    if !lock_dir.exists() {
                        // Lock released; loop and try to claim it.
                        continue;
                    }
                }
                Err(source) => {
                    return Err(OciError::CacheIo {
                        path: lock_dir.display().to_string(),
                        source,
                    });
                }
            }
        };

        self.do_locked_fetch_and_extract(client, image, descriptor, dest, &algo_dir, lock)
            .await
    }

    async fn do_locked_fetch_and_extract(
        &self,
        client: &dyn RegistryClient,
        image: &ImageRef,
        descriptor: &Descriptor,
        dest: &Path,
        algo_dir: &Path,
        lock: Arc<TemporaryDirectory>,
    ) -> Result<PathBuf, OciError> {
        // Another process may have completed while we were waiting for
        // the lock. Re-check before doing the network fetch.
        if Self::is_complete(dest) {
            return Ok(dest.to_path_buf());
        }

        // We own the digest lock and rechecked completeness above. Remove an
        // incomplete/old link-marker entry so promotion cannot fail forever
        // against its nonempty directory. Never follow a cache-root symlink.
        remove_cache_entry(dest).await?;

        let work_path = algo_dir.join(format!(".tmp-{}", uuid::Uuid::now_v7()));
        let work = Arc::new(
            TemporaryDirectory::create(work_path.clone()).map_err(|source| OciError::CacheIo {
                path: work_path.display().to_string(),
                source,
            })?,
        );
        let blob_tmp = work.path.join("blob");
        tracing::debug!(
            digest = %descriptor.digest,
            blob_tmp = %blob_tmp.display(),
            "Streaming OCI layer blob to disk"
        );
        client
            .fetch_layer_blob(image, descriptor, &blob_tmp)
            .await?;

        let extract_tmp = work.path.join("rootfs");
        std::fs::create_dir(&extract_tmp).map_err(|e| OciError::CacheIo {
            path: extract_tmp.display().to_string(),
            source: e,
        })?;

        let media = descriptor.media_type.clone();
        let blob_for_extract = blob_tmp.clone();
        let dir_for_extract = extract_tmp.clone();
        let digest_for_err = descriptor.digest.clone();
        let extraction_resources = (work.clone(), lock.clone());
        let extract_res = tokio::task::spawn_blocking(move || {
            // A dropped async caller cannot stop blocking extraction. Keep
            // its unpublished directory and lock alive until it finishes.
            let _resources = extraction_resources;
            extract_gzipped_tar(&blob_for_extract, &dir_for_extract, &media).map_err(|e| {
                OciError::LayerExtract {
                    digest: digest_for_err,
                    source: e,
                }
            })
        })
        .await
        .map_err(|join_err| OciError::CacheIo {
            path: extract_tmp.display().to_string(),
            source: std::io::Error::other(join_err.to_string()),
        })?;

        let explicit_dirs = match extract_res {
            Ok(explicit_dirs) => explicit_dirs,
            Err(e) => {
                let _ = tokio::fs::remove_dir_all(&extract_tmp).await;
                let _ = tokio::fs::remove_file(&blob_tmp).await;
                return Err(e);
            }
        };

        let _ = tokio::fs::remove_file(&blob_tmp).await;

        // Write the explicit-directory sidecar and then the `.complete`
        // sentinel inside the temp dir BEFORE the rename so both are part of
        // the atomic promotion. Readers that observe the layer dir under the
        // final path are guaranteed to see them too.
        write_reserved_file(
            &extract_tmp.join(EXPLICIT_DIRS_MARKER),
            &encode_explicit_dirs(&explicit_dirs),
        )
        .await?;
        write_reserved_file(&extract_tmp.join(COMPLETE_MARKER), b"ok").await?;

        match tokio::fs::rename(&extract_tmp, dest).await {
            Ok(()) => {
                tracing::info!(
                    digest = %descriptor.digest,
                    dest = %dest.display(),
                    "OCI layer cached"
                );
                Ok(dest.to_path_buf())
            }
            Err(e) => {
                // Another process won the race: their layer is already
                // promoted. Content-addressed safety means their dir is
                // equivalent to ours.
                if Self::is_complete(dest) {
                    let _ = tokio::fs::remove_dir_all(&extract_tmp).await;
                    Ok(dest.to_path_buf())
                } else {
                    let _ = tokio::fs::remove_dir_all(&extract_tmp).await;
                    Err(OciError::CacheIo {
                        path: dest.display().to_string(),
                        source: e,
                    })
                }
            }
        }
    }
}

/// Create a cache-owned file at a reserved name inside an unpublished layer
/// directory. The image owns every entry there, including this name: unlink
/// its entry first (also breaking hardlinks), then create the file fresh
/// without following a link supplied by the image.
async fn write_reserved_file(path: &Path, bytes: &[u8]) -> Result<(), OciError> {
    remove_cache_entry(path).await?;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await
        .map_err(|source| OciError::CacheIo {
            path: path.display().to_string(),
            source,
        })?;
    file.write_all(bytes)
        .await
        .map_err(|source| OciError::CacheIo {
            path: path.display().to_string(),
            source,
        })?;
    Ok(())
}

// Both callers hold an unpublished cache entry or its digest lock. Parents
// belong to the daemon; the final component may come from an untrusted image.
async fn remove_cache_entry(path: &Path) -> Result<(), OciError> {
    let result = match tokio::fs::symlink_metadata(path).await {
        Ok(meta) if meta.is_dir() => tokio::fs::remove_dir_all(path).await,
        Ok(_) => tokio::fs::remove_file(path).await,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    };
    result.map_err(|source| OciError::CacheIo {
        path: path.display().to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_dir_splits_on_colon() {
        let cache = LayerCache::new(PathBuf::from("/tmp/oci"));
        let dir = cache.layer_dir("sha256:abc");
        assert_eq!(dir, PathBuf::from("/tmp/oci/sha256/abc"));
    }

    #[test]
    fn layer_dir_handles_missing_algo_prefix() {
        let cache = LayerCache::new(PathBuf::from("/tmp/oci"));
        let dir = cache.layer_dir("deadbeef");
        assert_eq!(dir, PathBuf::from("/tmp/oci/sha256/deadbeef"));
    }

    #[test]
    fn is_complete_requires_marker_file() {
        let tmp = tempfile::tempdir().unwrap();
        let layer = tmp.path().join("sha256").join("abcd");
        std::fs::create_dir_all(&layer).unwrap();
        // Layer dir exists but no marker: NOT a cache hit.
        assert!(!LayerCache::is_complete(&layer));
        // The marker alone is a layer cached before the explicit-directory
        // sidecar: still not a hit, so it is extracted again.
        std::fs::write(layer.join(COMPLETE_MARKER), b"ok").unwrap();
        assert!(!LayerCache::is_complete(&layer));
        // Marker and sidecar: cache hit.
        std::fs::write(layer.join(EXPLICIT_DIRS_MARKER), b"").unwrap();
        assert!(LayerCache::is_complete(&layer));
    }

    /// Codex final-review B1 of PR #122: `layer_dir_for` takes a typed
    /// digest and produces a path under the cache root with the validated
    /// hex as the only path component. Cannot escape via `..` because
    /// `Digest::parse` already rejected non-hex chars.
    #[test]
    fn layer_dir_for_uses_validated_hex_only() {
        use super::super::registry::Digest;
        let cache = LayerCache::new(PathBuf::from("/tmp/oci"));
        let d = Digest::parse(
            "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
        )
        .unwrap();
        let dir = cache.layer_dir_for(&d);
        assert_eq!(
            dir,
            PathBuf::from(
                "/tmp/oci/sha256/abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
            )
        );
        // Sanity: the path stays under the root.
        assert!(dir.starts_with("/tmp/oci"));
    }

    /// Codex final-review B2 of PR #122: stale-lock detection — a lock
    /// dir mtime older than LOCK_STALE_AGE is recoverable.
    ///
    /// We don't exercise the full `ensure_layer` flow here (that needs a
    /// `RegistryClient`); instead we confirm the constants are sensible
    /// + that the threshold ordering is correct.
    #[test]
    fn lock_stale_threshold_is_below_max_wait() {
        assert!(
            LOCK_STALE_AGE < LOCK_MAX_WAIT,
            "stale threshold ({:?}) must be < max wait ({:?}) so recovery \
             can fire before timeout",
            LOCK_STALE_AGE,
            LOCK_MAX_WAIT,
        );
        assert!(LOCK_MAX_WAIT >= Duration::from_secs(60));
    }
}
