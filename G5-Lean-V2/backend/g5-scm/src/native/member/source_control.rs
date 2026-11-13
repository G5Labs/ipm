//! Git-reported paths are the only files inspected at a Save checkpoint.
use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::Read;

impl MemberGit {
    pub(super) async fn source_control_inspect(
        &self,
        root: &Path,
        mode: &str,
        base: &str,
    ) -> Result<serde_json::Value, NativeGitError> {
        let mut result = self.source_control_inspect_once(root, mode, base).await?;
        if mode == "source_diff" {
            let after = self
                .source_control_inspect_once(root, "source_state", base)
                .await?;
            if result["fingerprint"] != after["fingerprint"] {
                return Err(invalid(
                    "inspect Source Control",
                    "Files changed while reading this comparison; click Show again",
                ));
            }
            result["stateVerified"] = serde_json::json!(true);
        }
        Ok(result)
    }

    async fn source_control_inspect_once(
        &self,
        root: &Path,
        mode: &str,
        base: &str,
    ) -> Result<serde_json::Value, NativeGitError> {
        const OP: &str = "inspect Source Control";
        let head = self
            .read(root, OP, &["rev-parse", "--verify", "HEAD"])
            .await?;
        let statuses = self.status(root).await?;
        let changed = statuses
            .iter()
            .map(|s| s.path.clone())
            .collect::<BTreeSet<_>>();
        if changed.len() > 1000 {
            return Err(invalid(
                OP,
                "More than 1,000 dirty files; Source Control is unavailable",
            ));
        }
        let index = self
            .read(
                root,
                OP,
                &[
                    "diff",
                    "--cached",
                    "--raw",
                    "--no-abbrev",
                    "--no-ext-diff",
                    "--no-textconv",
                    "-z",
                    "HEAD",
                    "--",
                ],
            )
            .await?;
        let local = self
            .read(root, OP, &["rev-list", &format!("{base}..HEAD")])
            .await?;
        let local_descends = self
            .read(root, OP, &["merge-base", base, head.trim()])
            .await?
            == base;
        let mut digest = Sha256::new();
        for value in [&head, &index, &local] {
            digest.update(value.as_bytes());
            digest.update([0]);
        }
        let mut bytes_left = 256_u64 * 1024 * 1024;
        for status in &statuses {
            digest.update(status.status.as_bytes());
            digest.update(status.path.as_bytes());
            digest.update([0]);
            digest.update(status.from.as_deref().unwrap_or("").as_bytes());
            digest.update([0]);
        }
        for path in &changed {
            digest_changed_path(root, path, &mut digest, &mut bytes_left)?;
            digest.update([0]);
        }
        let fingerprint = format!("{:x}", digest.finalize());
        let paths = self
            .read(
                root,
                OP,
                &[
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--name-only",
                    "-M",
                    "-z",
                    base,
                    "--",
                ],
            )
            .await?;
        let mut paths = paths
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        paths.extend(
            statuses
                .iter()
                .filter(|s| s.status == "??")
                .map(|s| s.path.clone()),
        );
        if paths.len() > 1000 {
            return Err(invalid(OP, "Unpushed changes exceed 1,000 files"));
        }
        let mut patches = Vec::new();
        if mode == "source_diff" {
            // Preserve exact trailing bytes; MemberGit::read trims text output.
            let output = self
                .output(
                    root,
                    OP,
                    &[
                        "diff",
                        "--no-ext-diff",
                        "--no-textconv",
                        "--no-color",
                        "--src-prefix=a/",
                        "--dst-prefix=b/",
                        "-M",
                        base,
                        "--",
                    ],
                    true,
                    None,
                    None,
                )
                .await?;
            if !output.completed_successfully() || output.truncated {
                return Err(invalid(OP, "Live diff exceeded its output limit or failed"));
            }
            patches.push(
                String::from_utf8(output.into_protocol_bytes())
                    .map_err(|_| invalid(OP, "Live patch is not UTF-8"))?,
            );
            for status in statuses.iter().filter(|s| s.status == "??") {
                let output = self
                    .output(
                        root,
                        OP,
                        &[
                            "diff",
                            "--no-index",
                            "--no-ext-diff",
                            "--no-textconv",
                            "--no-color",
                            "--src-prefix=a/",
                            "--dst-prefix=b/",
                            "--",
                            "/dev/null",
                            &status.path,
                        ],
                        true,
                        None,
                        None,
                    )
                    .await?;
                // --no-index exits 1 when differences exist. All other failures
                // remain explicit; never turn a failed member into zero files.
                if output.truncated || output.status.as_ref().and_then(|s| s.code()) != Some(1) {
                    return Err(invalid(
                        OP,
                        "New-file diff exceeded its output limit or failed",
                    ));
                }
                let patch = String::from_utf8(output.into_protocol_bytes())
                    .map_err(|_| invalid(OP, "New-file patch unavailable"))?;
                if !patch.starts_with("diff --git ") {
                    return Err(invalid(OP, "New-file diff failed"));
                }
                if patches.iter().map(String::len).sum::<usize>() + patch.len() > 16 * 1024 * 1024 {
                    return Err(invalid(OP, "Live comparison exceeds 16 MiB"));
                }
                patches.push(patch);
            }
        }
        Ok(
            serde_json::json!({"headCommit":head,"selectable":local_descends,"unavailableReason":if local_descends {None} else {Some("Local HEAD does not descend from the pushed source; push or reconcile local history first.")},"dirty":!statuses.is_empty(),"fingerprint":fingerprint,"localCommits":local.lines().collect::<Vec<_>>(),"paths":paths,"patches":patches}),
        )
    }
}

#[cfg(unix)]
fn digest_changed_path(
    root: &Path,
    path: &str,
    digest: &mut Sha256,
    remaining: &mut u64,
) -> Result<(), NativeGitError> {
    use std::{
        ffi::CString,
        os::fd::{AsRawFd, FromRawFd},
    };
    const OP: &str = "fingerprint changed Git path";
    let mut directory =
        std::fs::File::open(root).map_err(|_| invalid(OP, "Checkout unavailable"))?;
    let parts = path.split('/').collect::<Vec<_>>();
    for part in &parts[..parts.len().saturating_sub(1)] {
        if part.is_empty() || *part == "." || *part == ".." {
            return Err(invalid(OP, "Invalid Git path"));
        }
        let name = CString::new(*part).map_err(|_| invalid(OP, "Invalid Git path"))?;
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::NotFound {
                digest.update(b"deleted");
                return Ok(());
            }
            return Err(invalid(
                OP,
                "Changed path is unavailable or traverses a symlink",
            ));
        }
        directory = unsafe { std::fs::File::from_raw_fd(fd) };
    }
    let last = parts
        .last()
        .filter(|p| !p.is_empty() && **p != "." && **p != "..")
        .ok_or_else(|| invalid(OP, "Invalid Git path"))?;
    let name = CString::new(*last).map_err(|_| invalid(OP, "Invalid Git path"))?;
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe {
        libc::fstatat(
            directory.as_raw_fd(),
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } < 0
    {
        if std::io::Error::last_os_error().kind() == std::io::ErrorKind::NotFound {
            digest.update(b"deleted");
            return Ok(());
        }
        return Err(invalid(OP, "Changed file unavailable"));
    }
    let stat = unsafe { stat.assume_init() };
    digest.update(stat.st_mode.to_le_bytes());
    if stat.st_mode & libc::S_IFMT == libc::S_IFLNK {
        let mut bytes = [0_u8; 8192];
        let len = unsafe {
            libc::readlinkat(
                directory.as_raw_fd(),
                name.as_ptr(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
            )
        };
        if len < 0 || len as usize == bytes.len() {
            return Err(invalid(OP, "Changed link unavailable or too long"));
        }
        digest.update(&bytes[..len as usize]);
        return Ok(());
    }
    if stat.st_mode & libc::S_IFMT != libc::S_IFREG {
        return Err(invalid(OP, "Unsupported changed file type"));
    }
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(invalid(OP, "Changed file unavailable"));
    }
    let file = unsafe { std::fs::File::from_raw_fd(fd) };
    let meta = file
        .metadata()
        .map_err(|_| invalid(OP, "Changed file unavailable"))?;
    if !meta.is_file() || meta.len() > *remaining {
        return Err(invalid(
            OP,
            "Changed files exceed the 256 MiB checkpoint limit or are unavailable",
        ));
    }
    *remaining -= meta.len();
    let mut file = file.take(meta.len().saturating_add(1));
    let mut count = 0;
    let mut buf = [0_u8; 65536];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|_| invalid(OP, "Changed file unreadable"))?;
        if n == 0 {
            break;
        }
        count += n as u64;
        digest.update(&buf[..n]);
    }
    if count != meta.len() {
        return Err(invalid(OP, "Changed file moved during checkpoint"));
    }
    Ok(())
}
#[cfg(not(unix))]
fn digest_changed_path(
    _: &Path,
    _: &str,
    _: &mut Sha256,
    _: &mut u64,
) -> Result<(), NativeGitError> {
    Err(invalid(
        "fingerprint changed Git path",
        "Source Control live reads require a Unix workspace",
    ))
}
