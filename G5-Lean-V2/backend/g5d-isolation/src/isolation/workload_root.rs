//! Preparation of a disposable, fully populated workload filesystem.
//! Only trusted, pre-guest seeds are read by host tools. No host mount is used.
// Portable unit tests exercise validation without compiling the Linux caller.
#![cfg_attr(
    not(all(feature = "cloud-hypervisor", target_os = "linux")),
    allow(dead_code)
)]

use g5_contracts::error::G5Error;
use std::path::{Path, PathBuf};

pub(crate) const OWNED_ROOT_ENV: &str = "G5D_INTERNAL_OWNED_WORKLOAD_ROOT";
pub(crate) const BACKING_REL: &str = ".g5/workload.ext4";

pub(super) fn failure(message: impl Into<String>) -> G5Error {
    G5Error::Config(format!("block workload: {}", message.into()))
}

pub(super) fn io_failure(error: std::io::Error, phase: &str) -> G5Error {
    g5d_foundation::disk::classify_io(&error, phase)
        .map(G5Error::HostDiskPressure)
        .unwrap_or_else(|| failure(format!("{phase}: {error}")))
}

pub(super) fn write_failure(path: &Path, phase: &str, stderr: &[u8]) -> G5Error {
    if let Some(pressure) = g5d_foundation::disk::write_pressure(path, phase) {
        return G5Error::HostDiskPressure(pressure);
    }
    failure(format!(
        "{phase}: {}",
        String::from_utf8_lossy(stderr).trim()
    ))
}

/// Helpers inherit the vm-exec process group for normal stop. Parent-death
/// killing also covers a host crash or a startup path that kills only vm-exec.
pub(super) fn helper(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    // Only filesystem tools belong here. Native Git must use its typed SCM
    // owner; a dynamic executable cannot bypass that boundary through this API.
    let mut command = match program.as_ref().to_str() {
        Some("cp") => std::process::Command::new("cp"),
        Some("tar") => std::process::Command::new("tar"),
        Some("mkfs.ext4") => std::process::Command::new("mkfs.ext4"),
        Some("/usr/sbin/mkfs.ext4") => std::process::Command::new("/usr/sbin/mkfs.ext4"),
        Some("/sbin/mkfs.ext4") => std::process::Command::new("/sbin/mkfs.ext4"),
        Some("e2fsck") => std::process::Command::new("e2fsck"),
        Some("find") => std::process::Command::new("find"),
        Some("rsync") => std::process::Command::new("rsync"),
        Some("bash") => std::process::Command::new("bash"),
        _ => fixture_helper(program.as_ref()),
    };
    command.stdin(std::process::Stdio::null());
    kill_with_parent(&mut command);
    command
}

/// Attach lifetime supervision without selecting or launching an executable.
pub(super) fn kill_with_parent(command: &mut std::process::Command) {
    #[cfg(not(target_os = "linux"))]
    let _ = command;
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        let parent = unsafe { libc::getpid() };
        // SAFETY: only async-signal-safe syscalls run between fork and exec.
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    libc::raise(libc::SIGKILL);
                }
                Ok(())
            });
        }
    }
}

#[cfg(test)]
fn fixture_helper(program: &std::ffi::OsStr) -> std::process::Command {
    std::process::Command::new(program)
}

#[cfg(not(test))]
fn fixture_helper(_: &std::ffi::OsStr) -> std::process::Command {
    panic!("unsupported workload filesystem helper")
}

pub(crate) fn validate_owned_root(
    root: &Path,
    binding: Option<&std::ffi::OsStr>,
) -> Result<(), G5Error> {
    let bound = binding
        .ok_or_else(|| failure("requires a backend-owned OCI clone; bare rootfs is unsupported"))?;
    let bound =
        std::fs::canonicalize(Path::new(bound)).map_err(|e| io_failure(e, "image_preparation"))?;
    if bound != root {
        return Err(failure("root ownership handoff does not match this clone"));
    }
    Ok(())
}

/// Ensure each component is a real directory, including existing ancestors.
/// Image symlinks at mount destinations must never redirect host writes.
pub(crate) fn ensure_directory(root: &Path, relative: &str) -> Result<PathBuf, G5Error> {
    let mut path = root.to_path_buf();
    for component in Path::new(relative).components() {
        let std::path::Component::Normal(component) = component else {
            return Err(failure(
                "directory must be relative and contain no parent components",
            ));
        };
        path.push(component);
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => (),
            Ok(_) => {
                return Err(failure(format!(
                    "required directory is a symlink or non-directory: {}",
                    path.display()
                )))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&path).map_err(|e| io_failure(e, "image_directory"))?;
            }
            Err(e) => return Err(failure(format!("inspect {}: {e}", path.display()))),
        }
    }
    Ok(path)
}

pub(crate) fn stage_workspace(root: &Path, source: &Path) -> Result<(), G5Error> {
    let started = std::time::Instant::now();
    let meta = std::fs::symlink_metadata(source).map_err(|e| io_failure(e, "image_preparation"))?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(failure("workspace seed must be a real directory"));
    }
    let source = source
        .canonicalize()
        .map_err(|e| io_failure(e, "image_preparation"))?;
    if source.starts_with(root) || root.starts_with(&source) {
        return Err(failure("workspace seed and private root must not overlap"));
    }
    for relative in [
        "dev",
        "proc",
        "sys",
        "opt/g5",
        "var/lib/docker",
        ".g5/workload-root",
    ] {
        ensure_directory(root, relative)?;
    }
    let target = root.join("work");
    match std::fs::symlink_metadata(&target) {
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
            return Err(failure("image /work must be a real directory or absent"));
        }
        Ok(_) => {
            std::fs::remove_dir_all(&target).map_err(|e| io_failure(e, "image_preparation"))?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(failure(format!("inspect image /work: {e}"))),
    }
    // cp is a host tool, never an executable from the image. Preserve ownership,
    // modes, symlinks and Git metadata; do not hardlink a live guest to its seed.
    let output = helper("cp")
        .args(["-a", "--reflink=auto", "--"])
        .arg(&source)
        .arg(&target)
        .output()
        .map_err(|e| io_failure(e, "image_preparation"))?;
    if !output.status.success() {
        return Err(write_failure(root, "workspace_staging", &output.stderr));
    }
    tracing::info!(
        preparation_seconds = started.elapsed().as_secs_f64(),
        "staged fresh workspace for block workload"
    );
    Ok(())
}

/// A guard is installed before formatting, since a failed mkfs can leave GiBs.
pub(crate) struct BackingGuard(pub(crate) PathBuf);
impl Drop for BackingGuard {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.0) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(path = %self.0.display(), %error, "cannot remove workload backing");
            }
        }
    }
}

/// Optional source preparation's free-space gate; VM starts use the single V admission.
pub(super) fn check_capacity(root: &Path, bytes: u64) -> Result<(), G5Error> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(root.as_os_str().as_bytes())
        .map_err(|_| failure("invalid root path"))?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: path is NUL-terminated and statvfs writes exactly its output type.
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err(failure(format!(
            "read backing filesystem capacity: {}",
            std::io::Error::last_os_error()
        )));
    }
    // SAFETY: the successful statvfs call initialized the value.
    let stats = unsafe { stats.assume_init() };
    // statvfs integer widths differ between supported Unix targets.
    #[allow(clippy::unnecessary_cast)]
    let available = (stats.f_bavail as u64).saturating_mul(stats.f_frsize as u64);
    if available < bytes {
        return Err(failure(format!("backing filesystem has {available} available bytes, below requested total capacity {bytes}; sparse sizing does not reserve space")));
    }
    Ok(())
}

pub(super) fn find_mkfs() -> Result<&'static Path, G5Error> {
    ["mkfs.ext4", "/usr/sbin/mkfs.ext4", "/sbin/mkfs.ext4"]
        .into_iter()
        .find(|candidate| {
            helper(candidate)
                .arg("-V")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        })
        .map(Path::new)
        .ok_or_else(|| failure("requires host mkfs.ext4 (e2fsprogs)"))
}

pub(crate) fn prepare_disk(
    root: &Path,
    size_mib: u32,
    mkfs: &Path,
) -> Result<BackingGuard, G5Error> {
    if size_mib == 0 {
        return Err(failure("total filesystem capacity must be positive"));
    }
    let started = std::time::Instant::now();
    let bytes = u64::from(size_mib) * 1024 * 1024;
    let parent = root
        .parent()
        .ok_or_else(|| failure("private root must have an owned parent"))?;
    let temporary = parent.join(format!(".g5-workload-{}.ext4", uuid::Uuid::new_v4()));
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|e| io_failure(e, "image_preparation"))?;
    let mut guard = BackingGuard(temporary);
    file.set_len(bytes)
        .map_err(|e| io_failure(e, "image_preparation"))?;
    drop(file);
    let destination = root.join(BACKING_REL);
    if std::fs::symlink_metadata(&destination).is_ok() {
        return Err(failure(
            "private seed already contains a workload backing; refusing recursive population",
        ));
    }
    // The output must remain OUTSIDE the mkfs -d input until population ends.
    let output = helper(mkfs)
        .args(["-F", "-q", "-d"])
        .arg(root)
        .arg(&guard.0)
        .output()
        .map_err(|e| io_failure(e, "image_preparation"))?;
    if !output.status.success() {
        return Err(write_failure(parent, "image_population", &output.stderr));
    }
    let allocated_bytes = {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(&guard.0)
            .map_err(|e| io_failure(e, "image_preparation"))?
            .blocks()
            * 512
    };
    std::fs::rename(&guard.0, &destination).map_err(|e| io_failure(e, "image_preparation"))?;
    guard.0 = destination;
    tracing::info!(
        size_mib,
        allocated_bytes,
        preparation_seconds = started.elapsed().as_secs_f64(),
        "prepared complete block workload root"
    );
    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unowned_and_mismatched_roots() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let root = a.path().canonicalize().unwrap();
        assert!(validate_owned_root(&root, None).is_err());
        assert!(validate_owned_root(&root, Some(b.path().as_os_str())).is_err());
        validate_owned_root(&root, Some(a.path().as_os_str())).unwrap();
    }
    #[test]
    fn mount_destinations_never_follow_image_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("opt")).unwrap();
        assert!(ensure_directory(root.path(), "opt/g5").is_err());
        assert!(!outside.path().join("g5").exists());
        assert!(ensure_directory(root.path(), "../escape").is_err());
    }
    #[test]
    fn workspace_alias_rejected_before_seed_mutation() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        assert!(stage_workspace(&root, &root).is_err());
        assert!(!root.join("work").exists());
    }
    #[test]
    fn failed_population_removes_sibling_backing() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("seed");
        std::fs::create_dir_all(root.join(".g5")).unwrap();
        assert!(prepare_disk(&root, 1, Path::new("/usr/bin/false")).is_err());
        assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 1);
        assert!(!root.join(BACKING_REL).exists());
    }

    #[test]
    fn invalid_or_unavailable_capacity_fails_before_image_creation() {
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path().join("rootfs");
        std::fs::create_dir_all(root.join(".g5")).unwrap();
        assert!(prepare_disk(&root, 0, Path::new("/usr/bin/true")).is_err());
        assert!(check_capacity(&root, u64::MAX).is_err());
        assert_eq!(std::fs::read_dir(owner.path()).unwrap().count(), 1);
        assert!(!root.join(BACKING_REL).exists());
    }

    #[test]
    fn backing_never_exists_inside_mkfs_input() {
        use std::os::unix::fs::PermissionsExt;
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path().join("rootfs");
        std::fs::create_dir_all(root.join(".g5")).unwrap();
        let formatter = owner.path().join("formatter");
        // Validate the formatter boundary without invoking any image binaries.
        // -F -q -d ROOT OUTPUT: output is a sibling of ROOT, never a descendant.
        std::fs::write(&formatter, "#!/bin/sh\nset -eu\n[ \"$1\" = -F ]\n[ \"$3\" = -d ]\n[ ! -e \"$4/.g5/workload.ext4\" ]\ncase \"$5\" in \"$4\"/*) exit 9;; esac\nprintf populated > \"$5\"\n").unwrap();
        std::fs::set_permissions(&formatter, std::fs::Permissions::from_mode(0o700)).unwrap();
        let guard = prepare_disk(&root, 1, &formatter).unwrap();
        assert_eq!(std::fs::read(&guard.0).unwrap(), b"populated");
        assert_eq!(guard.0, root.join(BACKING_REL));
        assert_eq!(std::fs::read_dir(owner.path()).unwrap().count(), 2);
        drop(guard);
        assert!(!root.join(BACKING_REL).exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn workspace_copy_preserves_links_without_sharing_writable_inodes() {
        use std::os::unix::fs::MetadataExt;
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path().join("rootfs");
        std::fs::create_dir(&root).unwrap();
        let seed = tempfile::tempdir().unwrap();
        std::fs::write(seed.path().join("tracked"), b"original").unwrap();
        std::os::unix::fs::symlink("tracked", seed.path().join("link")).unwrap();
        stage_workspace(&root, seed.path()).unwrap();
        assert_eq!(
            std::fs::read_link(root.join("work/link")).unwrap(),
            Path::new("tracked")
        );
        assert_ne!(
            std::fs::metadata(seed.path().join("tracked"))
                .unwrap()
                .ino(),
            std::fs::metadata(root.join("work/tracked")).unwrap().ino()
        );
        std::fs::write(root.join("work/tracked"), b"guest changed").unwrap();
        assert_eq!(
            std::fs::read(seed.path().join("tracked")).unwrap(),
            b"original"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "subprocess fixture for helper_parent_death_kills_writer"]
    fn helper_parent_death_child() {
        let Some(path) = std::env::var_os("G5_TEST_HELPER_PID_FILE") else {
            return;
        };
        let mut child = helper("/bin/sleep").arg("30").spawn().unwrap();
        // Written whole: the parent polls for the file and parses it as
        // soon as it exists, so it must never see it created but empty.
        let path = std::path::PathBuf::from(path);
        let staged = path.with_extension("tmp");
        std::fs::write(&staged, child.id().to_string()).unwrap();
        std::fs::rename(&staged, &path).unwrap();
        child.wait().unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn helper_parent_death_kills_writer() {
        struct Parent(std::process::Child);
        impl Drop for Parent {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("child-pid");
        let mut parent = Parent(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "isolation::workload_root::tests::helper_parent_death_child",
                    "--ignored",
                ])
                .env("G5_TEST_HELPER_PID_FILE", &pid_file)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !pid_file.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let child: u32 = std::fs::read_to_string(&pid_file)
            .expect("helper started")
            .parse()
            .unwrap();
        parent.0.kill().unwrap();
        parent.0.wait().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            // A zombie is already dead and cannot write; its reaper is PID 1.
            let running = std::fs::read_to_string(format!("/proc/{child}/stat"))
                .ok()
                .and_then(|stat| {
                    stat.rsplit_once(") ")
                        .map(|(_, tail)| tail.starts_with('Z'))
                })
                .is_some_and(|zombie| !zombie);
            if !running {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "helper survived vm-exec parent death"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires Linux root, git and e2fsprogs; run in the block acceptance lane"]
    fn real_ext4_population_contains_workspace_and_no_backing() {
        use std::os::unix::{ffi::OsStrExt, fs::PermissionsExt};
        assert_eq!(
            unsafe { libc::geteuid() },
            0,
            "UID preservation fixture requires Linux root"
        );
        fn git(work: &Path, args: &[&str]) -> String {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(work)
                .args(args)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap()
        }
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path().join("rootfs");
        std::fs::create_dir_all(root.join(".g5")).unwrap();
        let source = tempfile::tempdir().unwrap();
        let probe = source.path().join("probe");
        std::fs::write(&probe, b"block-root-probe").unwrap();
        std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let cprobe = std::ffi::CString::new(probe.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::chown(cprobe.as_ptr(), 1000, 1000) }, 0);
        std::fs::hard_link(&probe, source.path().join("hardlink")).unwrap();
        std::os::unix::fs::symlink("probe", source.path().join("symlink")).unwrap();
        git(source.path(), &["init", "--bare", ".git"]);
        git(
            source.path(),
            &["--git-dir=.git", "config", "core.bare", "false"],
        );
        git(
            source.path(),
            &["--git-dir=.git", "config", "core.worktree", ".."],
        );
        git(source.path(), &["add", "."]);
        git(
            source.path(),
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-m",
                "fixture",
            ],
        );
        let head = git(source.path(), &["rev-parse", "HEAD"]);
        stage_workspace(&root, source.path()).unwrap();
        let guard = prepare_disk(&root, 32, Path::new("mkfs.ext4")).unwrap();
        let debugfs = |command: &str| {
            let output = std::process::Command::new("debugfs")
                .args(["-R", command])
                .arg(&guard.0)
                .output()
                .unwrap();
            assert!(output.status.success());
            output
        };
        assert_eq!(debugfs("cat /work/probe").stdout, b"block-root-probe");
        let stat = String::from_utf8(debugfs("stat /work/probe").stdout).unwrap();
        let hard = String::from_utf8(debugfs("stat /work/hardlink").stdout).unwrap();
        assert_eq!(
            stat.split_whitespace().nth(1),
            hard.split_whitespace().nth(1),
            "hardlinks retain one inode"
        );
        assert!(stat.contains("0755"), "executable mode: {stat}");
        let fields = stat.split_whitespace().collect::<Vec<_>>();
        for name in ["User:", "Group:"] {
            assert_eq!(
                fields[fields.iter().position(|field| *field == name).unwrap() + 1],
                "1000"
            );
        }
        let link = String::from_utf8(debugfs("stat /work/symlink").stdout).unwrap();
        assert!(
            link.contains("Type: symlink") && link.contains("\"probe\""),
            "{link}"
        );
        assert!(
            String::from_utf8_lossy(&debugfs("stat /.g5/workload.ext4").stderr)
                .contains("File not found")
        );
        // This image has never been guest-writable. Extract only our trusted
        // fixture with debugfs to verify Git relocation, without a host mount.
        let extraction = tempfile::tempdir().unwrap();
        debugfs(&format!("rdump /work {}", extraction.path().display()));
        let restored = extraction.path().join("work");
        assert_eq!(git(&restored, &["rev-parse", "HEAD"]), head);
        git(&restored, &["fsck", "--full"]);
        assert_eq!(git(&restored, &["status", "--porcelain"]), "");
    }
}
