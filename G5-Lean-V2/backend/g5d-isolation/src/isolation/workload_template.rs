//! One rebuildable pristine block artifact owned by the OCI template cache.
//! Host prewarm replaces it; a cold Start prepares and pins under the same lock.
//! Optional access-scoped repository artifacts share this bounded owner and lock.
//! No guest-written seeds or host filesystem mounts are used.
// Portable unit tests exercise validation without compiling the Linux caller.
#![cfg_attr(
    not(all(feature = "cloud-hypervisor", target_os = "linux")),
    allow(dead_code)
)]

use super::workload_root::{self, ensure_directory, failure, helper, BackingGuard};
use g5_contracts::error::G5Error;
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};

pub(super) const TEMPLATE_ENV: &str = "G5D_INTERNAL_WORKLOAD_TEMPLATE";
const FORMAT_VERSION: u32 = 2;
const MIB: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct Identity {
    version: u32,
    pub(super) oci_template: String,
    pub(super) size_mib: u32,
    pub(super) architecture: String,
    pub(super) bootstrap_version: u32,
}

pub(super) fn location(template: &Path, size_mib: u32) -> Result<(PathBuf, Identity), G5Error> {
    if size_mib == 0 {
        return Err(failure("template capacity must be positive"));
    }
    let template = template
        .canonicalize()
        .map_err(|e| workload_root::io_failure(e, "cold_image"))?;
    let key = template.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let sha = template
        .parent()
        .ok_or_else(|| failure("invalid OCI template directory"))?;
    if key.len() != 64
        || !key.bytes().all(|b| b.is_ascii_hexdigit())
        || sha.file_name().is_none_or(|s| s != "sha256")
        || !g5d_foundation::oci::template_is_complete(&template)
    {
        return Err(failure(
            "requires a completed content-addressed OCI template",
        ));
    }
    let parent = sha
        .parent()
        .ok_or_else(|| failure("invalid OCI template cache"))?;
    Ok((
        parent.join("workload-block"),
        Identity {
            version: FORMAT_VERSION,
            oci_template: key.to_string(),
            size_mib,
            architecture: std::env::consts::ARCH.into(),
            bootstrap_version: 2,
        },
    ))
}

/// Daemon-side readers and publishers keep locks close-on-exec so unrelated VM
/// children cannot retain them. Dedicated builder processes explicitly enable
/// inheritance until parent-death-killed writers have physically exited.
pub(super) fn lock(cache: &Path, exclusive: bool) -> Result<File, G5Error> {
    lock_named(cache, "lock", exclusive, false)?.ok_or_else(|| failure("template lock unavailable"))
}

pub(super) fn lock_named(
    cache: &Path,
    name: &str,
    exclusive: bool,
    nonblocking: bool,
) -> Result<Option<File>, G5Error> {
    match std::fs::create_dir(cache) {
        Ok(()) => std::fs::set_permissions(cache, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| workload_root::io_failure(e, "cold_image"))?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(workload_root::io_failure(e, "cold_image")),
    }
    let meta =
        std::fs::symlink_metadata(cache).map_err(|e| workload_root::io_failure(e, "cold_image"))?;
    if !meta.is_dir() || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o022 != 0 {
        return Err(failure(
            "template cache must be an owned, non-writable-by-others real directory",
        ));
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(cache.join(name))
        .map_err(|e| workload_root::io_failure(e, "cold_image"))?;
    loop {
        // SAFETY: a live owned fd; flock has no memory arguments.
        if unsafe {
            libc::flock(
                file.as_raw_fd(),
                (if exclusive {
                    libc::LOCK_EX
                } else {
                    libc::LOCK_SH
                }) | if nonblocking { libc::LOCK_NB } else { 0 },
            )
        } == 0
        {
            break;
        }
        let error = std::io::Error::last_os_error();
        if nonblocking && error.kind() == std::io::ErrorKind::WouldBlock {
            return Ok(None);
        }
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(failure(format!("lock template: {error}")));
        }
    }
    Ok(Some(file))
}

/// Only dedicated vm-exec preparation processes may enable inheritance.
/// Daemon-side cache readers and publication never call this function.
pub(super) fn inherit_builder_lock(file: &File) -> Result<(), G5Error> {
    // SAFETY: clear only CLOEXEC on this dedicated preparation lock fd.
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFD, 0) } == -1 {
        return Err(failure(format!(
            "inherit template lock: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

pub(super) fn complete(cache: &Path, expected: &Identity) -> bool {
    let current = cache.join("current");
    if !std::fs::symlink_metadata(&current).is_ok_and(|m| m.is_dir()) {
        return false;
    }
    let Ok(meta) = std::fs::symlink_metadata(current.join("identity.json")) else {
        return false;
    };
    if !meta.is_file() || meta.len() > 4096 {
        return false;
    }
    let Ok(bytes) = std::fs::read(current.join("identity.json")) else {
        return false;
    };
    if serde_json::from_slice::<Identity>(&bytes).ok().as_ref() != Some(expected) {
        return false;
    }
    std::fs::symlink_metadata(current.join("image.ext4")).is_ok_and(|m| {
        m.is_file()
            && m.len() == u64::from(expected.size_mib) * MIB
            && m.uid() == unsafe { libc::geteuid() }
            && m.mode() & 0o222 == 0
    })
}

pub(super) fn remove_owned(path: &Path) -> Result<(), G5Error> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(failure(format!("inspect owned template staging: {e}"))),
    }
    .map_err(|e| workload_root::io_failure(e, "cold_image"))
}
pub(super) struct Staging(pub(super) PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        if let Err(error) = remove_owned(&self.0) {
            tracing::warn!(%error, "template staging cleanup failed");
        }
    }
}

/// Explicit kernel reflink: never silently turn a cache hit into a full copy.
pub(super) fn reflink(source: &Path, destination: &Path) -> Result<bool, G5Error> {
    #[cfg(target_os = "linux")]
    {
        let source = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(source)
            .map_err(|e| workload_root::io_failure(e, "cold_image"))?;
        let target = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(destination)
            .map_err(|e| workload_root::io_failure(e, "cold_image"))?;
        // SAFETY: FICLONE takes the source fd as its scalar third argument.
        if unsafe { libc::ioctl(target.as_raw_fd(), libc::FICLONE, source.as_raw_fd()) } == 0 {
            return Ok(true);
        }
        let error = std::io::Error::last_os_error();
        if matches!(
            error.raw_os_error(),
            Some(libc::EOPNOTSUPP | libc::EXDEV | libc::ENOTTY | libc::EINVAL)
        ) {
            return Ok(false);
        }
        Err(workload_root::io_failure(error, "private_disk_clone"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (source, destination);
        Ok(false)
    }
}

/// A successful clone is private before the read lock is released.
pub(super) fn try_clone(
    template: &Path,
    root: &Path,
    size_mib: u32,
) -> Result<Option<BackingGuard>, G5Error> {
    let (cache, identity) = location(template, size_mib)?;
    if !cache.exists() {
        return Ok(None);
    }
    let _lock = lock(&cache, false)?;
    clone_locked(&cache, &identity, root, size_mib)
}

fn clone_locked(
    cache: &Path,
    identity: &Identity,
    root: &Path,
    size_mib: u32,
) -> Result<Option<BackingGuard>, G5Error> {
    if !complete(cache, identity) {
        return Ok(None);
    }
    let start = std::time::Instant::now();
    let path = root.join(workload_root::BACKING_REL);
    if std::fs::symlink_metadata(&path).is_ok() {
        return Err(failure("private block clone already exists"));
    }
    let guard = BackingGuard(path);
    // A sparse reflink: physical use grows only with the guest's own writes.
    if !reflink(&cache.join("current/image.ext4"), &guard.0)? {
        tracing::warn!("block reflink unsupported; rejecting unqualified host");
        return Ok(None);
    }
    tracing::info!(
        preparation_seconds = start.elapsed().as_secs_f64(),
        size_mib,
        "cloned pristine block workload template"
    );
    Ok(Some(guard))
}

/// Do not make a workspace's image depend on which image activated the host.
/// Pin the private clone before a competing cold Start can replace the cache.
pub(super) fn clone_or_prepare(
    template: &Path,
    root: &Path,
    size_mib: u32,
    reserve_mib: u64,
) -> Result<BackingGuard, G5Error> {
    if let Some(guard) = try_clone(template, root, size_mib)? {
        return Ok(guard);
    }
    let (cache, identity) = location(template, size_mib)?;
    let _lock = lock(&cache, true)?;
    let _seed_builder = lock_named(&cache, "seed-build.lock", true, false)?;
    inherit_builder_lock(&_lock)?;
    if let Some(builder) = _seed_builder.as_ref() {
        inherit_builder_lock(builder)?;
    }
    prewarm_locked(
        template,
        size_mib,
        reserve_mib,
        workload_root::find_mkfs()?,
        &cache,
        &identity,
    )?;
    clone_locked(&cache, &identity, root, size_mib)?
        .ok_or_else(|| failure("prepared block template cannot be privately cloned"))
}

const PRIVATE_PIN: &str = ".g5/prepared-workload.ext4";

pub(super) fn publish_private_pin(guard: BackingGuard, root: &Path) -> Result<(), G5Error> {
    let pin = root.join(PRIVATE_PIN);
    if pin.exists() {
        return Err(failure("private workload already prepared"));
    }
    std::fs::rename(&guard.0, pin).map_err(|e| workload_root::io_failure(e, "private_disk_pin"))
}

pub(super) fn take_private_pin(
    root: &Path,
    size_mib: u32,
) -> Result<Option<BackingGuard>, G5Error> {
    let pin = root.join(PRIVATE_PIN);
    let metadata = match std::fs::symlink_metadata(&pin) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(failure(format!("inspect private workload pin: {error}"))),
    };
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
        || metadata.len() != u64::from(size_mib) * MIB
    {
        return Err(failure("invalid private workload pin"));
    }
    let destination = root.join(workload_root::BACKING_REL);
    if destination.exists() {
        return Err(failure("private workload already consumed"));
    }
    std::fs::rename(pin, &destination)
        .map_err(|e| workload_root::io_failure(e, "private_disk_pin"))?;
    Ok(Some(BackingGuard(destination)))
}

pub(super) fn prewarm(
    template: &Path,
    size_mib: u32,
    reserve_mib: u64,
    mkfs: &Path,
) -> Result<(), G5Error> {
    if !cfg!(target_os = "linux") {
        return Err(failure("block templates require Linux"));
    }
    let (cache, identity) = location(template, size_mib)?;
    let _lock = lock(&cache, true)?;
    let _seed_builder = lock_named(&cache, "seed-build.lock", true, false)?;
    inherit_builder_lock(&_lock)?;
    if let Some(builder) = _seed_builder.as_ref() {
        inherit_builder_lock(builder)?;
    }
    prewarm_locked(template, size_mib, reserve_mib, mkfs, &cache, &identity)
}

fn prewarm_locked(
    template: &Path,
    size_mib: u32,
    reserve_mib: u64,
    mkfs: &Path,
    cache: &Path,
    identity: &Identity,
) -> Result<(), G5Error> {
    let started = std::time::Instant::now();
    // Preparation reserves no slot capacity: only Start admission requires V
    // free. Retain the helper flag for rolling upgrades.
    let _ = reserve_mib;
    remove_owned(&cache.join("building"))?;
    remove_owned(&cache.join("repo-building"))?;
    if complete(cache, identity) {
        tracing::info!(size_mib, "pristine block workload template ready");
        return Ok(());
    }
    // Reclaim obsolete artifacts before admission too: a previous, larger
    // capacity must not strand this host below its new free-space budget.
    // Existing VM reflinks stay independent; there is no replacement overlap.
    remove_owned(&cache.join("current"))?;
    remove_owned(&cache.join("repo-current"))?;
    remove_owned(&cache.join("repo-prepared"))?;
    let stage = Staging(cache.join("building"));
    std::fs::create_dir(&stage.0).map_err(|e| workload_root::io_failure(e, "cold_image"))?;
    let probe = stage.0.join("probe");
    std::fs::write(&probe, b"reflink probe")
        .map_err(|e| workload_root::io_failure(e, "cold_image"))?;
    if !reflink(&probe, &stage.0.join("probe-copy"))? {
        return Err(failure(
            "host filesystem must support reflinks before admission",
        ));
    }
    let root = stage.0.join("rootfs");
    let output = helper("cp")
        .args(["-a", "--reflink=always", "--"])
        .arg(template.join("rootfs"))
        .arg(&root)
        .output()
        .map_err(|e| workload_root::io_failure(e, "cold_image"))?;
    if !output.status.success() {
        return Err(workload_root::write_failure(
            &stage.0,
            "cold_image",
            &output.stderr,
        ));
    }
    let empty = stage.0.join("empty-work");
    std::fs::create_dir(&empty).map_err(|e| workload_root::io_failure(e, "cold_image"))?;
    workload_root::stage_workspace(&root, &empty)?;
    ensure_directory(&root, "etc")?;
    // Reserve /.g5 for bootstrap; do not inherit an image's stale handoff.
    remove_owned(&root.join(".g5"))?;
    ensure_directory(&root, ".g5/workload-root")?;
    for name in ["hosts", "resolv.conf"] {
        remove_owned(&root.join("etc").join(name))?;
    }
    let guard = workload_root::prepare_disk(&root, size_mib, mkfs)?;
    let image = stage.0.join("image.ext4");
    std::fs::rename(&guard.0, &image).map_err(|e| workload_root::io_failure(e, "cold_image"))?;
    drop(guard);
    let output = helper("e2fsck")
        .args(["-f", "-n"])
        .arg(&image)
        .output()
        .map_err(|e| workload_root::io_failure(e, "cold_image"))?;
    if !output.status.success() {
        return Err(failure(format!(
            "pristine ext4 validation failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    std::fs::set_permissions(&image, std::fs::Permissions::from_mode(0o400))
        .map_err(|e| workload_root::io_failure(e, "cold_image"))?;
    File::open(&image)
        .and_then(|f| f.sync_all())
        .map_err(|e| workload_root::io_failure(e, "cold_image"))?;
    let identity_path = stage.0.join("identity.json");
    std::fs::write(
        &identity_path,
        serde_json::to_vec(&identity).map_err(|e| failure(e.to_string()))?,
    )
    .map_err(|e| workload_root::io_failure(e, "cold_image"))?;
    File::open(&identity_path)
        .and_then(|f| f.sync_all())
        .map_err(|e| workload_root::io_failure(e, "cold_image"))?;
    for entry in ["rootfs", "empty-work", "probe", "probe-copy"] {
        remove_owned(&stage.0.join(entry))?;
    }
    File::open(&stage.0)
        .and_then(|f| f.sync_all())
        .map_err(|e| workload_root::io_failure(e, "cold_image"))?;
    std::fs::rename(&stage.0, cache.join("current"))
        .map_err(|e| workload_root::io_failure(e, "cold_image"))?;
    File::open(cache)
        .and_then(|f| f.sync_all())
        .map_err(|e| workload_root::io_failure(e, "cold_image"))?;
    tracing::info!(
        size_mib,
        preparation_seconds = started.elapsed().as_secs_f64(),
        allocated_bytes = std::fs::metadata(cache.join("current/image.ext4"))
            .map(|m| m.blocks() * 512)
            .unwrap_or(0),
        "prepared pristine block workload template"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(parent: &Path, key: char) -> PathBuf {
        let template = parent.join("sha256").join(key.to_string().repeat(64));
        std::fs::create_dir_all(template.join("rootfs/work")).unwrap();
        std::fs::create_dir_all(template.join("rootfs/etc")).unwrap();
        std::fs::create_dir_all(template.join("rootfs/.g5")).unwrap();
        std::fs::write(template.join("rootfs/fixture"), b"pristine image").unwrap();
        std::fs::write(template.join("rootfs/work/stale-work"), b"must not persist").unwrap();
        std::fs::write(template.join("rootfs/.g5/vm-env.sh"), b"must not persist").unwrap();
        std::os::unix::fs::symlink("/run/resolv.conf", template.join("rootfs/etc/resolv.conf"))
            .unwrap();
        std::fs::write(template.join(".complete"), b"").unwrap();
        template
    }

    #[test]
    fn only_resolved_complete_templates_supply_cache_identity() {
        let dir = tempfile::tempdir().unwrap();
        assert!(location(dir.path(), 32).is_err());
        let a = fixture(dir.path(), 'a');
        let b = fixture(dir.path(), 'b');
        assert_ne!(location(&a, 32).unwrap().1, location(&b, 32).unwrap().1);
        assert_ne!(location(&a, 32).unwrap().1, location(&a, 64).unwrap().1);
        assert!(location(&a, 0).is_err());
        std::fs::remove_file(a.join(".complete")).unwrap();
        assert!(location(&a, 32).is_err());
    }

    #[test]
    fn partial_wrong_version_writable_and_truncated_artifacts_are_misses() {
        let dir = tempfile::tempdir().unwrap();
        let template = fixture(dir.path(), 'a');
        let (cache, identity) = location(&template, 32).unwrap();
        let _lock = lock(&cache, true).unwrap();
        let current = cache.join("current");
        std::fs::create_dir(&current).unwrap();
        let identity_path = current.join("identity.json");
        let image = current.join("image.ext4");
        assert!(!complete(&cache, &identity));
        std::fs::write(&identity_path, b"partial {").unwrap();
        assert!(!complete(&cache, &identity));
        std::fs::write(&identity_path, serde_json::to_vec(&identity).unwrap()).unwrap();
        let file = File::create(&image).unwrap();
        file.set_len(32 * MIB).unwrap();
        assert!(
            !complete(&cache, &identity),
            "writable template is never eligible"
        );
        std::fs::set_permissions(&image, std::fs::Permissions::from_mode(0o400)).unwrap();
        assert!(complete(&cache, &identity));
        let different = Identity {
            version: FORMAT_VERSION + 1,
            ..identity.clone()
        };
        assert!(!complete(&cache, &different));
        file.set_len(MIB).unwrap();
        assert!(!complete(&cache, &identity));
    }

    #[test]
    fn cache_directory_symlink_cannot_redirect_preparation() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("cache")).unwrap();
        assert!(lock(&dir.path().join("cache"), true).is_err());
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    }

    #[test]
    fn daemon_lock_is_not_retained_by_an_unrelated_exec() {
        use std::io::Read;
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache");
        let held = lock(&cache, true).unwrap();
        let mut child = std::process::Command::new("/bin/sh")
            .args(["-c", "printf ready; exec sleep 10"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut ready = [0; 5];
        child.stdout.take().unwrap().read_exact(&mut ready).unwrap();
        assert_eq!(&ready, b"ready");
        drop(held);
        let acquired = lock_named(&cache, "lock", true, true).unwrap().is_some();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(
            acquired,
            "unrelated exec must not keep the daemon's cache lock"
        );
    }

    #[test]
    fn cold_preparation_replaces_obsolete_images_without_a_second_v_check() {
        let dir = tempfile::tempdir().unwrap();
        let a = fixture(dir.path(), 'a');
        let b = fixture(dir.path(), 'b');
        let (cache, identity) = location(&a, 32).unwrap();
        let lock = lock(&cache, true).unwrap();
        let current = cache.join("current");
        std::fs::create_dir(&current).unwrap();
        std::fs::write(
            current.join("identity.json"),
            serde_json::to_vec(&identity).unwrap(),
        )
        .unwrap();
        File::create(current.join("image.ext4"))
            .unwrap()
            .set_len(32 * MIB)
            .unwrap();
        std::fs::set_permissions(
            current.join("image.ext4"),
            std::fs::Permissions::from_mode(0o400),
        )
        .unwrap();
        for name in ["repo-current", "repo-prepared"] {
            std::fs::create_dir(cache.join(name)).unwrap();
        }
        drop(lock);
        let root = dir.path().join("vm");
        std::fs::create_dir_all(root.join(".g5")).unwrap();
        assert!(prewarm(&b, 32, u64::from(u32::MAX), Path::new("/bin/false")).is_err());
        assert!(!complete(&cache, &identity));
        assert!(!cache.join("repo-current").exists());
        assert!(!cache.join("repo-prepared").exists());
    }

    #[test]
    fn private_pin_is_validated_and_consumed_once() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".g5")).unwrap();
        let path = dir.path().join(workload_root::BACKING_REL);
        let disk = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        disk.set_len(32 * MIB).unwrap();
        publish_private_pin(BackingGuard(path.clone()), dir.path()).unwrap();
        assert!(!path.exists());
        assert!(take_private_pin(dir.path(), 64).is_err());
        let guard = take_private_pin(dir.path(), 32).unwrap().unwrap();
        assert!(take_private_pin(dir.path(), 32).unwrap().is_none());
        assert!(guard.0.exists());
        drop(guard);
        assert!(!path.exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires Linux e2fsprogs and a reflink filesystem; run the block acceptance lane"]
    fn real_template_publication_invalidation_and_private_clones() {
        use std::io::{Read, Seek, SeekFrom, Write};
        let parent = std::env::var_os("G5_TEST_REFLINK_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = tempfile::tempdir_in(parent).unwrap();
        let template = fixture(dir.path(), 'a');
        let mkfs = workload_root::find_mkfs().unwrap();
        // Two independent builders must publish one complete artifact.
        std::thread::scope(|scope| {
            for _ in 0..2 {
                scope.spawn(|| prewarm(&template, 32, 64, mkfs).unwrap());
            }
        });
        let (cache, identity) = location(&template, 32).unwrap();
        assert!(complete(&cache, &identity));
        let image = cache.join("current/image.ext4");
        let inode = std::fs::metadata(&image).unwrap().ino();
        prewarm(&template, 32, 64, Path::new("/bin/false")).unwrap();
        assert_eq!(
            inode,
            std::fs::metadata(&image).unwrap().ino(),
            "warm prepare does not format again"
        );
        let out = std::process::Command::new("debugfs")
            .args(["-R", "ls -p /work"])
            .arg(&image)
            .output()
            .unwrap();
        assert!(!String::from_utf8_lossy(&out.stdout).contains("stale-work"));
        let out = std::process::Command::new("debugfs")
            .args(["-R", "stat /.g5/vm-env.sh"])
            .arg(&image)
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&out.stderr).contains("File not found"));
        let roots: Vec<_> = ["vm-a", "vm-b"]
            .into_iter()
            .map(|name| {
                let p = dir.path().join(name);
                std::fs::create_dir_all(p.join(".g5")).unwrap();
                p
            })
            .collect();
        let a = try_clone(&template, &roots[0], 32).unwrap().unwrap();
        let b = try_clone(&template, &roots[1], 32).unwrap().unwrap();
        fn read_at(path: &Path) -> [u8; 16] {
            let mut f = File::open(path).unwrap();
            f.seek(SeekFrom::Start(128 * 1024)).unwrap();
            let mut bytes = [0; 16];
            f.read_exact(&mut bytes).unwrap();
            bytes
        }
        let pristine = read_at(&image);
        let mut writer = OpenOptions::new().write(true).open(&a.0).unwrap();
        writer.seek(SeekFrom::Start(128 * 1024)).unwrap();
        writer.write_all(b"private VM write").unwrap();
        assert_eq!(read_at(&image), pristine);
        assert_eq!(read_at(&b.0), pristine);
        assert_ne!(read_at(&a.0), pristine);
        assert!(try_clone(&template, &roots[0], 64).unwrap().is_none());
        // Replacement cannot invalidate already handed-off private clones.
        let changed = fixture(dir.path(), 'b');
        prewarm(&changed, 64, 64, mkfs).unwrap();
        assert_eq!(read_at(&b.0), pristine);
        assert!(try_clone(&template, &roots[0], 32).unwrap().is_none());
        assert_eq!(std::fs::read_dir(cache.join("current")).unwrap().count(), 2);
        assert!(!cache.join("building").exists());
        std::fs::create_dir(cache.join("building")).unwrap();
        std::fs::write(cache.join("building/interrupted"), b"incomplete").unwrap();
        prewarm(&changed, 64, 64, mkfs).unwrap();
        assert!(!cache.join("building").exists());
        // Reclaim abandoned staging even on a warm hit. A rolling helper
        // may pass the obsolete reserve flag; it cannot impose extra V.
        std::fs::create_dir(cache.join("building")).unwrap();
        std::fs::write(
            cache.join("building/interrupted"),
            b"reclaim before admission",
        )
        .unwrap();
        prewarm(&changed, 64, u64::from(u32::MAX), mkfs).unwrap();
        assert!(!cache.join("building").exists());
        assert!(complete(&cache, &location(&changed, 64).unwrap().1));
        // A now-ineligible artifact is rebuildable cache, not recovery state.
        // A failed formatter must remove obsolete and partial output.
        assert!(prewarm(&changed, 128, u64::from(u32::MAX), Path::new("/bin/false")).is_err());
        assert!(!cache.join("current").exists());
        prewarm(&changed, 128, u64::from(u32::MAX), mkfs).unwrap();
        assert!(complete(&cache, &location(&changed, 128).unwrap().1));
        assert_eq!(read_at(&b.0), pristine);
        // Image source was never mutated to create the generic template.
        assert!(template.join("rootfs/work/stale-work").exists());
        assert!(template.join("rootfs/etc/resolv.conf").is_symlink());
        drop((a, b));
        assert!(!roots[0].join(workload_root::BACKING_REL).exists());
        assert!(!roots[1].join(workload_root::BACKING_REL).exists());
    }

    /// Production clones are sparse: their virtual capacity is V, but only the
    /// template's blocks are allocated (shared by reflink) until the guest writes.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires Linux e2fsprogs and a reflink filesystem; run the block acceptance lane"]
    fn private_disks_stay_sparse_until_guest_writes() {
        use std::os::unix::fs::{FileExt, MetadataExt};
        let parent = std::env::var_os("G5_TEST_REFLINK_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = tempfile::tempdir_in(parent).unwrap();
        let v_mib = 256;
        let v = u64::from(v_mib) * MIB;
        let allocated = |path: &Path| std::fs::metadata(path).unwrap().blocks() * 512;
        let sparse = |path: &Path| {
            assert_eq!(std::fs::metadata(path).unwrap().len(), v);
            assert!(
                allocated(path) < v / 4,
                "{} allocates {} of {v}",
                path.display(),
                allocated(path)
            );
        };
        let root = |name: &str| {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.join(".g5")).unwrap();
            path
        };
        let template = fixture(dir.path(), 'a');
        // Cold Start: the helper prepares the image, then clones it.
        let cold = clone_or_prepare(&template, &root("cold"), v_mib, v).unwrap();
        sparse(&cold.0);
        // Warm Start: the daemon's pre-boot clone, pin handoff and consumption.
        let warm = root("warm");
        let pinned = try_clone(&template, &warm, v_mib).unwrap().unwrap();
        sparse(&pinned.0);
        publish_private_pin(pinned, &warm).unwrap();
        let disk = take_private_pin(&warm, v_mib).unwrap().unwrap();
        sparse(&disk.0);
        // The VMM writes guest blocks into this file; only they add allocation.
        let before = allocated(&disk.0);
        let file = OpenOptions::new().write(true).open(&disk.0).unwrap();
        // Past block group 1's metadata, which the template already allocates.
        file.write_all_at(&vec![7_u8; 64 << 20], 160 << 20).unwrap();
        file.sync_all().unwrap();
        let after = allocated(&disk.0);
        assert!(after >= before + (60 << 20), "{before} -> {after}");
        assert!(
            after < v / 2,
            "{after} of {v}: no full-capacity reservation"
        );
    }
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires Linux e2fsprogs and a reflink filesystem; run the block acceptance lane"]
    fn cold_starts_pin_their_requested_image_before_cache_replacement() {
        let parent = std::env::var_os("G5_TEST_REFLINK_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = tempfile::tempdir_in(parent).unwrap();
        let a = fixture(dir.path(), 'a');
        let b = fixture(dir.path(), 'b');
        std::fs::write(a.join("rootfs/image-id"), b"image-a").unwrap();
        std::fs::write(b.join("rootfs/image-id"), b"image-b").unwrap();
        let roots: Vec<_> = (0..3)
            .map(|i| {
                let root = dir.path().join(format!("vm-{i}"));
                std::fs::create_dir_all(root.join(".g5")).unwrap();
                root
            })
            .collect();
        let guards = std::thread::scope(|scope| {
            let first = scope.spawn(|| clone_or_prepare(&a, &roots[0], 32, 96).unwrap());
            let second = scope.spawn(|| clone_or_prepare(&b, &roots[1], 32, 96).unwrap());
            vec![first.join().unwrap(), second.join().unwrap()]
        });
        for (guard, expected) in guards.iter().zip(["image-a", "image-b"]) {
            let output = std::process::Command::new("debugfs")
                .args(["-R", "cat /image-id"])
                .arg(&guard.0)
                .output()
                .unwrap();
            assert!(output.status.success());
            assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
        }
        let third = clone_or_prepare(&a, &roots[2], 32, 96).unwrap();
        let output = std::process::Command::new("debugfs")
            .args(["-R", "cat /image-id"])
            .arg(&third.0)
            .output()
            .unwrap();
        assert_eq!(output.stdout, b"image-a");
        let (cache, _) = location(&a, 32).unwrap();
        assert_eq!(std::fs::read_dir(cache.join("current")).unwrap().count(), 2);
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "subprocess fixture for template_builder_crash_is_recoverable"]
    fn template_builder_child() {
        let Some(path) = std::env::var_os("G5_TEST_TEMPLATE_DIR") else {
            return;
        };
        let formatter = std::env::var_os("G5_TEST_TEMPLATE_FORMATTER").unwrap();
        prewarm(Path::new(&path), 32, 0, Path::new(&formatter)).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires Linux root tools and a reflink filesystem; run the block acceptance lane"]
    fn template_builder_crash_is_recoverable() {
        struct Parent(std::process::Child);
        impl Drop for Parent {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let parent = std::env::var_os("G5_TEST_REFLINK_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = tempfile::tempdir_in(parent).unwrap();
        let template = fixture(dir.path(), 'c');
        let pid_file = dir.path().join("formatter-pid");
        let formatter = dir.path().join("formatter");
        std::fs::write(
            &formatter,
            "#!/bin/sh\nset -eu\nprintf '%s' $$ > \"$G5_TEST_TEMPLATE_PID\"\nexec /bin/sleep 30\n",
        )
        .unwrap();
        std::fs::set_permissions(&formatter, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut child = Parent(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "isolation::workload_template::tests::template_builder_child",
                    "--ignored",
                ])
                .env("G5_TEST_TEMPLATE_DIR", &template)
                .env("G5_TEST_TEMPLATE_FORMATTER", &formatter)
                .env("G5_TEST_TEMPLATE_PID", &pid_file)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !pid_file.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let writer: u32 = std::fs::read_to_string(&pid_file)
            .expect("formatter started")
            .parse()
            .unwrap();
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let running = std::fs::read_to_string(format!("/proc/{writer}/stat"))
                .ok()
                .is_some_and(|s| {
                    s.rsplit_once(") ")
                        .is_some_and(|(_, tail)| !tail.starts_with('Z'))
                });
            if !running {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "formatter survived parent death"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let (cache, identity) = location(&template, 32).unwrap();
        assert!(!complete(&cache, &identity));
        assert!(
            cache.join("building").exists(),
            "SIGKILL leaves owned staging for recovery"
        );
        // A fresh helper reclaims abandoned staging, and failed format or fsck
        // never publishes a completion marker.
        assert!(prewarm(&template, 32, 0, Path::new("/bin/false")).is_err());
        assert!(!complete(&cache, &identity));
        assert!(!cache.join("building").exists());
        assert!(prewarm(&template, 32, 0, Path::new("/bin/true")).is_err());
        assert!(!complete(&cache, &identity));
        prewarm(&template, 32, 0, workload_root::find_mkfs().unwrap()).unwrap();
        assert!(complete(&cache, &identity));
    }
}
