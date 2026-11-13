//! Assembled-rootfs template cache (ontology `g5/g5d/oci` D29).
//!
//! The layer cache (`cache.rs`) persists *extracted layers*; assembling them
//! into a bootable rootfs (whiteouts, ~13 GB of file copies for the fat
//! agent image) previously re-ran on EVERY microVM boot (issue #707). This
//! module caches the *assembled* rootfs once per image content identity and
//! lets each boot COW-clone it instead:
//!
//! ```text
//! <template_root>/sha256/<key>/
//!     rootfs/               ← assembled tree; the ONLY clone source
//!     image-config.json     ← decoded image config persisted at build time
//!     .complete             ← sentinel, written before the atomic rename
//!     .<key>.lock/          ← (sibling of <key>/) cross-process build lock
//! ```
//!
//! The markers are siblings of `rootfs/`, never inside it, so nothing
//! cache-internal can leak into a guest. A template's `rootfs/` is never
//! booted directly: it populates a pristine block template, and each VM gets a
//! private CoW disk before bootstrap writes source and per-VM configuration.
//!
//! Locking mirrors the layer cache's mkdir-lock + poll-for-`.complete`
//! pattern, with one addition: a multi-GB assembly legitimately runs far
//! longer than the layer cache's 5-minute stale threshold, so the builder
//! refreshes a `heartbeat` file inside the lock dir and waiters judge
//! staleness by that heartbeat, not by the lock dir's creation time.

use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest as _, Sha256};

use super::error::OciError;
use super::registry::ImageConfig;

/// Bumped whenever the assembly semantics change in a way that makes
/// previously-built templates non-equivalent (e.g. a change to the D6
/// whiteout or setid-strip rules). Part of the template key, so stale
/// templates are simply never matched again.
// Version 2 rebuilds templates with symlink-safe layer merge semantics (SEC-C03).
// Version 3 rebuilds templates whose assembly dropped a later layer's mode on
// an existing directory (`g5.runtime.boot.image.rootfs`): the g5dev image's
// baked cargo registry and sccache trees were root-only in every guest.
const TEMPLATE_FORMAT_VERSION: u32 = 3;

pub(crate) const COMPLETE_MARKER: &str = ".complete";
pub const ROOTFS_DIR: &str = "rootfs";
pub(crate) const IMAGE_CONFIG_FILE: &str = "image-config.json";
const HEARTBEAT_FILE: &str = "heartbeat";

const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(250);
/// Total waiter budget for a sibling's template build. Sized for a multi-GB
/// assembly on a pressured disk (observed ~15 min worst case on dev worker
/// hosts), not the layer cache's 10-minute budget.
const LOCK_MAX_WAIT: Duration = Duration::from_secs(1800);
/// A lock whose heartbeat is older than this with no `.complete` marker is
/// considered abandoned (holder crashed mid-build). The live builder
/// refreshes the heartbeat every [`HEARTBEAT_INTERVAL`], keeping a healthy
/// lock far fresher than this.
const LOCK_STALE_AGE: Duration = Duration::from_secs(300);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(60);
/// Upper bound on one `cp` clone/copy invocation (CodeRabbit review of
/// PR #758): the clone runs on every microVM boot, so a `cp` stalled on
/// degraded or network-backed storage must not hang the boot forever.
/// Generous — a plain-copy fallback of a ~13 GB rootfs on a pressured disk
/// legitimately takes many minutes; `kill_on_drop` reaps the child when the
/// timeout fires.
const CP_TIMEOUT: Duration = Duration::from_secs(1800);

/// Content-addressed template key (D29): sha256 hex over a versioned
/// identity string covering the format version, the effective-uid class
/// (the D6 setid-strip rule makes assembled bytes differ between a root and
/// a non-root daemon), the image-config digest, and the ordered layer
/// digests. Derived from *resolved* manifest content, never from tag text,
/// so a moved tag yields a new template.
pub(crate) fn template_key(
    root_daemon: bool,
    config_digest: &str,
    layer_digests: &[String],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("g5d-rootfs-template-v{}", TEMPLATE_FORMAT_VERSION));
    hasher.update([0u8]);
    hasher.update(if root_daemon {
        "uid=root"
    } else {
        "uid=nonroot"
    });
    hasher.update([0u8]);
    hasher.update(config_digest);
    for digest in layer_digests {
        hasher.update([0u8]);
        hasher.update(digest);
    }
    hex::encode(hasher.finalize())
}

/// True when the daemon assembles rootfs trees with root semantics (setuid
/// bits preserved — see the D6 ownership-coupled strip rule).
pub(crate) fn daemon_is_root() -> bool {
    // SAFETY: geteuid has no failure modes and touches no memory.
    unsafe { libc::geteuid() == 0 }
}

pub(crate) fn template_dir(template_root: &Path, key: &str) -> PathBuf {
    template_root.join("sha256").join(key)
}

pub fn is_complete(template_dir: &Path) -> bool {
    template_dir.join(ROOTFS_DIR).is_dir() && template_dir.join(COMPLETE_MARKER).is_file()
}

pub(crate) fn read_image_config(template_dir: &Path) -> Result<ImageConfig, OciError> {
    let path = template_dir.join(IMAGE_CONFIG_FILE);
    let bytes = std::fs::read(&path).map_err(|e| OciError::CacheIo {
        path: path.display().to_string(),
        source: e,
    })?;
    serde_json::from_slice(&bytes).map_err(|e| OciError::CacheIo {
        path: path.display().to_string(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
    })
}

pub(crate) fn write_image_config(
    template_dir: &Path,
    config: &ImageConfig,
) -> Result<(), OciError> {
    let path = template_dir.join(IMAGE_CONFIG_FILE);
    let bytes = serde_json::to_vec(config).map_err(|e| OciError::CacheIo {
        path: path.display().to_string(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
    })?;
    std::fs::write(&path, bytes).map_err(|e| OciError::CacheIo {
        path: path.display().to_string(),
        source: e,
    })
}

/// Cross-process template build lock (D29). `acquire` returns:
///
/// - `Ok(Some(lock))` — we own the lock and must build (the caller re-checks
///   `.complete` first: a sibling may have finished between our last poll
///   and the mkdir win).
/// - `Ok(None)` — a sibling finished the template while we waited; it is
///   complete and ready to clone.
/// - `Err(CacheLockTimeout)` — the wait budget elapsed.
///
/// While held, a background task refreshes `<lock>/heartbeat` every
/// [`HEARTBEAT_INTERVAL`] so waiters can distinguish a live multi-minute
/// build from a crashed holder.
pub(crate) struct TemplateBuildLock {
    _directory: super::temporary::TemporaryDirectory,
    heartbeat: tokio::task::JoinHandle<()>,
}

impl TemplateBuildLock {
    pub(crate) async fn acquire(template_dir: &Path, key: &str) -> Result<Option<Self>, OciError> {
        let parent = template_dir
            .parent()
            .expect("template dir always has a sha256/ parent")
            .to_path_buf();
        tokio::fs::create_dir_all(&parent)
            .await
            .map_err(|e| OciError::CacheIo {
                path: parent.display().to_string(),
                source: e,
            })?;
        let lock_dir = parent.join(format!(".{}.lock", key));
        let start = std::time::Instant::now();
        let directory = loop {
            match tokio::task::spawn_blocking({
                let lock_dir = lock_dir.clone();
                move || super::temporary::TemporaryDirectory::create(lock_dir)
            })
            .await
            .map_err(|join_err| OciError::CacheIo {
                path: lock_dir.display().to_string(),
                source: std::io::Error::other(join_err.to_string()),
            })? {
                Ok(directory) => break directory,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if is_complete(template_dir) {
                        return Ok(None);
                    }
                    if start.elapsed() >= LOCK_MAX_WAIT {
                        return Err(OciError::CacheLockTimeout {
                            path: lock_dir.display().to_string(),
                            waited_secs: LOCK_MAX_WAIT.as_secs(),
                        });
                    }
                    if lock_heartbeat_age(&lock_dir)
                        .await
                        .is_some_and(|age| age >= LOCK_STALE_AGE)
                        && !is_complete(template_dir)
                    {
                        tracing::warn!(
                            path = %lock_dir.display(),
                            "Stale rootfs-template build lock (no heartbeat); the \
                             previous holder appears to have crashed mid-build. \
                             Removing + retrying."
                        );
                        let _ = tokio::fs::remove_dir_all(&lock_dir).await;
                        continue;
                    }
                    tokio::time::sleep(LOCK_POLL_INTERVAL).await;
                }
                Err(source) => {
                    return Err(OciError::CacheIo {
                        path: lock_dir.display().to_string(),
                        source,
                    });
                }
            }
        };

        touch_heartbeat(&lock_dir).await;
        let heartbeat = tokio::spawn({
            let lock_dir = lock_dir.clone();
            async move {
                loop {
                    tokio::time::sleep(HEARTBEAT_INTERVAL).await;
                    touch_heartbeat(&lock_dir).await;
                }
            }
        });
        Ok(Some(TemplateBuildLock {
            _directory: directory,
            heartbeat,
        }))
    }

    pub(crate) async fn release(self) {
        self.heartbeat.abort();
    }
}

/// Cancellation/panic safety net (CodeRabbit review of PR #758): `release()`
/// only runs on the code paths that reach it. If the build future is
/// cancelled or panics between `acquire` and `release`, the detached
/// heartbeat task would otherwise keep refreshing the lock forever — the
/// lock never goes stale and every future build of that image stalls until
/// the daemon restarts. Dropping the guard aborts the heartbeat and
/// best-effort removes the lock dir (both idempotent, so the explicit
/// `release()` path is unaffected).
impl Drop for TemplateBuildLock {
    fn drop(&mut self) {
        self.heartbeat.abort();
    }
}

/// Age of the lock's liveness signal: the `heartbeat` file's mtime when it
/// exists, else the lock dir's own mtime (covers the window between mkdir
/// and the first heartbeat write). `None` means the lock vanished or
/// metadata is unreadable — treated as not-stale so the poll loop re-checks.
async fn lock_heartbeat_age(lock_dir: &Path) -> Option<Duration> {
    let probe = lock_dir.join(HEARTBEAT_FILE);
    let meta = match tokio::fs::metadata(&probe).await {
        Ok(m) => m,
        Err(_) => tokio::fs::metadata(lock_dir).await.ok()?,
    };
    meta.modified().ok()?.elapsed().ok()
}

async fn touch_heartbeat(lock_dir: &Path) {
    let path = lock_dir.join(HEARTBEAT_FILE);
    if let Err(e) = tokio::fs::write(&path, b"alive").await {
        // Best-effort: a failed heartbeat only risks a waiter treating a
        // live build as stale; the build itself proceeds.
        tracing::debug!(path = %path.display(), error = %e, "heartbeat write failed");
    }
}

/// COW-clone `src` (a template's `rootfs/`) to `dst` (D29). `dst` must not
/// exist; `cp` creates it as a copy of `src`.
///
/// - Linux: `cp --reflink=always -a` (XFS/btrfs FICLONE); on failure —
///   ext4/tmpfs dev boxes and CI — falls back to a plain `cp -a` full copy
///   with a warning. Still strictly cheaper than re-assembly (no gunzip, no
///   whiteout walk, no network).
/// - macOS: `cp -c -a` (APFS `clonefile(2)`; BSD cp silently degrades to a
///   full copy off-APFS, which is acceptable here — correctness is
///   identical, only speed differs), with the same plain-copy fallback.
///
/// `-a` preserves modes (including setuid bits when the daemon is root)
/// exactly as assembled. A failed attempt removes the partial `dst` before
/// falling back or surfacing the error.
pub(crate) async fn clone_rootfs(src: &Path, dst: &Path) -> Result<(), OciError> {
    #[cfg(target_os = "macos")]
    let clone_flag = "-c";
    #[cfg(not(target_os = "macos"))]
    let clone_flag = "--reflink=always";

    match run_cp(&[clone_flag, "-a"], src, dst).await {
        Ok(()) => Ok(()),
        Err(clone_err) => {
            remove_partial(dst).await;
            tracing::warn!(
                src = %src.display(),
                dst = %dst.display(),
                error = %clone_err,
                "COW clone of rootfs template failed; falling back to a plain copy \
                 (no reflink support on this filesystem?)"
            );
            match run_cp(&["-a"], src, dst).await {
                Ok(()) => Ok(()),
                Err(copy_err) => {
                    remove_partial(dst).await;
                    Err(copy_err)
                }
            }
        }
    }
}

async fn run_cp(flags: &[&str], src: &Path, dst: &Path) -> Result<(), OciError> {
    let output = tokio::time::timeout(
        CP_TIMEOUT,
        tokio::process::Command::new("cp")
            .args(flags)
            .arg(src)
            .arg(dst)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_elapsed| OciError::CacheIo {
        path: dst.display().to_string(),
        source: std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!(
                "cp {} {} {} did not finish within {}s; killed (degraded or \
                 network-backed storage?)",
                flags.join(" "),
                src.display(),
                dst.display(),
                CP_TIMEOUT.as_secs(),
            ),
        ),
    })?
    .map_err(|e| OciError::CacheIo {
        path: dst.display().to_string(),
        source: e,
    })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(OciError::CacheIo {
        path: dst.display().to_string(),
        source: std::io::Error::other(format!(
            "cp {} {} {} failed (status {}): {}",
            flags.join(" "),
            src.display(),
            dst.display(),
            output.status,
            stderr.trim(),
        )),
    })
}

async fn remove_partial(dst: &Path) {
    let _ = tokio::fs::remove_dir_all(dst).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digests(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn template_key_is_stable_for_same_inputs() {
        let a = template_key(false, "sha256:cfg", &digests(&["sha256:l1", "sha256:l2"]));
        let b = template_key(false, "sha256:cfg", &digests(&["sha256:l1", "sha256:l2"]));
        assert_eq!(a, b);
    }

    #[test]
    fn template_key_changes_with_every_identity_dimension() {
        let base = template_key(false, "sha256:cfg", &digests(&["sha256:l1", "sha256:l2"]));
        // Different config digest.
        assert_ne!(
            base,
            template_key(false, "sha256:other", &digests(&["sha256:l1", "sha256:l2"]))
        );
        // Different layer digest.
        assert_ne!(
            base,
            template_key(false, "sha256:cfg", &digests(&["sha256:l1", "sha256:lX"]))
        );
        // Different layer order.
        assert_ne!(
            base,
            template_key(false, "sha256:cfg", &digests(&["sha256:l2", "sha256:l1"]))
        );
        // Different uid class (root vs non-root assembly semantics, D6).
        assert_ne!(
            base,
            template_key(true, "sha256:cfg", &digests(&["sha256:l1", "sha256:l2"]))
        );
        // Added layer.
        assert_ne!(
            base,
            template_key(
                false,
                "sha256:cfg",
                &digests(&["sha256:l1", "sha256:l2", "sha256:l3"])
            )
        );
    }

    #[test]
    fn template_key_is_a_safe_single_path_component() {
        let key = template_key(false, "sha256:cfg", &digests(&["sha256:l1"]));
        assert_eq!(key.len(), 64);
        assert!(key
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    /// Boundary-shift resistance: the key must not concatenate digests in a
    /// way where moving bytes between adjacent fields collides (hence the
    /// NUL separators).
    #[test]
    fn template_key_separator_prevents_field_bleed() {
        let a = template_key(false, "sha256:ab", &digests(&["sha256:c"]));
        let b = template_key(false, "sha256:a", &digests(&["bsha256:c"]));
        assert_ne!(a, b);
    }

    #[test]
    fn is_complete_requires_rootfs_and_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let tpl = tmp.path().join("sha256").join("k");
        std::fs::create_dir_all(tpl.join(ROOTFS_DIR)).unwrap();
        // rootfs/ exists but no marker: NOT complete.
        assert!(!is_complete(&tpl));
        std::fs::write(tpl.join(COMPLETE_MARKER), b"ok").unwrap();
        assert!(is_complete(&tpl));
        // Marker without rootfs/: NOT complete.
        std::fs::remove_dir_all(tpl.join(ROOTFS_DIR)).unwrap();
        assert!(!is_complete(&tpl));
    }

    #[test]
    fn image_config_round_trips_through_template_file() {
        let tmp = tempfile::tempdir().unwrap();
        let config = ImageConfig {
            env: vec!["PATH=/usr/bin".into(), "FOO=bar".into()],
        };
        write_image_config(tmp.path(), &config).unwrap();
        let read = read_image_config(tmp.path()).unwrap();
        assert_eq!(read, config);
    }

    #[tokio::test]
    async fn clone_rootfs_copies_tree_and_preserves_modes() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("rootfs");
        std::fs::create_dir_all(src.join("etc")).unwrap();
        std::fs::write(src.join("etc/os-release"), b"ID=test").unwrap();
        std::fs::write(src.join("bin-tool"), b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(src.join("bin-tool"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("etc/os-release", src.join("release-link")).unwrap();

        let dst = tmp.path().join("vm-1");
        clone_rootfs(&src, &dst).await.unwrap();

        assert_eq!(
            std::fs::read(dst.join("etc/os-release")).unwrap(),
            b"ID=test"
        );
        let mode = std::fs::metadata(dst.join("bin-tool"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755);
        #[cfg(unix)]
        {
            let link = std::fs::read_link(dst.join("release-link")).unwrap();
            assert_eq!(link, PathBuf::from("etc/os-release"));
        }
        // Source is untouched (clone, not move).
        assert!(src.join("etc/os-release").is_file());
    }

    #[tokio::test]
    async fn clone_rootfs_fails_on_missing_source_and_leaves_no_partial_dst() {
        let tmp = tempfile::tempdir().unwrap();
        let dst = tmp.path().join("vm-1");
        let err = clone_rootfs(&tmp.path().join("nonexistent"), &dst)
            .await
            .unwrap_err();
        assert!(matches!(err, OciError::CacheIo { .. }));
        assert!(!dst.exists(), "partial destination must be cleaned up");
    }

    #[tokio::test]
    async fn build_lock_second_acquirer_sees_completion() {
        let tmp = tempfile::tempdir().unwrap();
        let tpl = tmp.path().join("sha256").join("k1");
        let lock = TemplateBuildLock::acquire(&tpl, "k1").await.unwrap();
        let lock = lock.expect("first acquirer owns the lock");
        // Simulate the winner finishing the build while a sibling waits.
        std::fs::create_dir_all(tpl.join(ROOTFS_DIR)).unwrap();
        std::fs::write(tpl.join(COMPLETE_MARKER), b"ok").unwrap();
        let waiter = TemplateBuildLock::acquire(&tpl, "k1").await.unwrap();
        assert!(
            waiter.is_none(),
            "waiter must observe the completed template, not re-acquire"
        );
        lock.release().await;
    }

    #[tokio::test]
    async fn build_lock_reacquirable_after_release() {
        let tmp = tempfile::tempdir().unwrap();
        let tpl = tmp.path().join("sha256").join("k2");
        let first = TemplateBuildLock::acquire(&tpl, "k2")
            .await
            .unwrap()
            .expect("first acquirer owns the lock");
        first.release().await;
        // No `.complete` was written; the next acquirer builds again.
        let second = TemplateBuildLock::acquire(&tpl, "k2").await.unwrap();
        assert!(second.is_some(), "released lock must be re-acquirable");
        second.unwrap().release().await;
    }
}
