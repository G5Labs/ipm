//! The existing VM custody includes every host process, even after the supervisor
//! is killed. A cgroup's populated flag is the exit proof for VMM and passt.
use std::path::PathBuf;

#[derive(Debug)]
pub(super) struct VmScope {
    pub(super) path: PathBuf,
}
impl VmScope {
    #[cfg(target_os = "linux")]
    pub(super) fn create(command: &mut tokio::process::Command) -> anyhow::Result<Self> {
        use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
        let membership = std::fs::read_to_string("/proc/self/cgroup")?;
        let relative = membership
            .lines()
            .find_map(|line| line.strip_prefix("0::"))
            .ok_or_else(|| anyhow::anyhow!("microVM supervision requires cgroup v2"))?;
        anyhow::ensure!(
            relative.starts_with('/') && !relative.split('/').any(|part| part == ".."),
            "invalid cgroup membership"
        );
        let mut parent = PathBuf::from("/sys/fs/cgroup").join(relative.trim_start_matches('/'));
        if parent.file_name().is_some_and(|name| name == "supervisor") {
            parent.pop();
            std::fs::write(
                parent.join("cgroup.subtree_control"),
                "+cpu +memory +io +pids",
            )?;
        }
        let path = parent.join(format!("g5-vm-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).map_err(|e| {
            anyhow::anyhow!("create VM custody scope (systemd Delegate=yes required): {e}")
        })?;
        let scope = Self { path };
        let file = std::fs::OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(scope.path.join("cgroup.procs"))?;
        // Moving the child before exec closes the spawn-to-registration race.
        // All descendants inherit this scope regardless of uid/session changes.
        unsafe {
            command.pre_exec(move || {
                if libc::write(file.as_raw_fd(), b"0".as_ptr().cast(), 1) != 1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Ok(scope)
    }
    #[cfg(not(target_os = "linux"))]
    pub(super) fn create(_: &mut tokio::process::Command) -> anyhow::Result<Self> {
        anyhow::bail!("microVM process custody requires Linux/cgroup v2")
    }
    #[cfg(target_os = "linux")]
    pub(super) fn background_limits(&self, data: &std::path::Path) -> anyhow::Result<()> {
        use std::os::unix::fs::MetadataExt;
        for (file, value) in [
            ("cpu.max", "100000 100000"),
            ("memory.max", "4294967296"),
            ("memory.swap.max", "0"),
            ("pids.max", "512"),
            ("io.weight", "default 10"),
        ] {
            std::fs::write(self.path.join(file), value)?;
        }
        let device = std::fs::metadata(data)?.dev();
        let device = io_device(
            std::path::Path::new("/sys/dev/block"),
            libc::major(device),
            libc::minor(device),
        )?;
        std::fs::write(
            self.path.join("io.max"),
            format!("{device} rbps=52428800 wbps=52428800 riops=1000 wiops=1000"),
        )?;
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    pub(super) fn background_limits(&self, _: &std::path::Path) -> anyhow::Result<()> {
        anyhow::bail!("source preparation requires Linux resource controls")
    }
    fn empty(&self) -> std::io::Result<bool> {
        Ok(std::fs::read_to_string(self.path.join("cgroup.events"))?
            .lines()
            .any(|line| line == "populated 0"))
    }
    pub(super) fn kill(&self) -> std::io::Result<()> {
        std::fs::write(self.path.join("cgroup.kill"), "1")
    }
    /// A VM's owner must outlive individual reap deadlines. Optional background
    /// preparation uses bounded finish(), retaining its staging on failure.
    pub(super) async fn wait_for_exit(&self) {
        loop {
            match self.finish().await {
                Ok(()) => return,
                Err(error) => {
                    tracing::warn!(%error, "VM exit unconfirmed; retaining custody and retrying");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
    }
    pub(super) async fn finish(&self) -> anyhow::Result<()> {
        self.kill()?;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if self.empty()? {
                return Ok(());
            }
            anyhow::ensure!(
                tokio::time::Instant::now() < deadline,
                "VM descendants did not exit; retain disks and custody"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}

/// blkcg rejects partition device numbers; limits belong to their whole disk.
#[cfg(any(target_os = "linux", test))]
fn io_device(sysfs: &std::path::Path, major: u32, minor: u32) -> anyhow::Result<String> {
    let device = sysfs.join(format!("{major}:{minor}")).canonicalize()?;
    let disk = if device.join("partition").is_file() {
        device
            .parent()
            .ok_or_else(|| anyhow::anyhow!("partition has no parent device"))?
    } else {
        device.as_path()
    };
    let number = std::fs::read_to_string(disk.join("dev"))?;
    let number = number.trim();
    let (major, minor) = number
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("invalid block device number"))?;
    Ok(format!(
        "{}:{}",
        major.parse::<u32>()?,
        minor.parse::<u32>()?
    ))
}
impl Drop for VmScope {
    fn drop(&mut self) {
        let _ = self.kill();
        if self.empty().unwrap_or(false) {
            let _ = std::fs::remove_dir(&self.path);
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn exit_owner_retries_after_a_bounded_attempt_expires() {
        // Controlled cgroup-file fixture: no physical-exit claim. The real
        // delegated-cgroup test below owns the kernel/process boundary.
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("scope");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("cgroup.events"), "populated 1\n").unwrap();
        let scope = VmScope { path: path.clone() };
        let proof = scope.wait_for_exit();
        tokio::pin!(proof);
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(6), proof.as_mut())
                .await
                .is_err(),
            "one expired attempt must not release the owner"
        );
        assert!(path.exists());
        std::fs::write(path.join("cgroup.events"), "populated 0\n").unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(3), proof)
            .await
            .expect("later exit proof must settle the same owner");
    }

    #[tokio::test]
    #[ignore = "requires a delegated Linux cgroup-v2 systemd test service"]
    async fn detached_children_remain_in_custody_after_leader_exit() {
        let temp = tempfile::tempdir().unwrap();
        let pidfile = temp.path().join("child.pid");
        let mut command = tokio::process::Command::new("/bin/sh");
        command
            .args([
                "-c",
                "setsid /bin/sh -c 'echo $$ > \"$1\"; exec sleep 120' child \"$1\" &",
                "leader",
            ])
            .arg(&pidfile)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let scope = VmScope::create(&mut command).unwrap();
        let mut leader = command.spawn().unwrap();
        assert!(leader.wait().await.unwrap().success());
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !pidfile.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(
            !scope.empty().unwrap(),
            "leader reap must not release detached children"
        );
        let pid: u32 = std::fs::read_to_string(&pidfile)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let membership = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap();
        assert!(membership.contains(scope.path.file_name().unwrap().to_str().unwrap()));
        scope.finish().await.unwrap();
        assert!(scope.empty().unwrap());
        let path = scope.path.clone();
        drop(scope);
        assert!(
            !path.exists(),
            "the empty scope is removed only after exit proof"
        );
    }
}

#[cfg(test)]
mod device_tests {
    use super::*;

    #[test]
    fn partition_io_limit_uses_whole_device_and_missing_mapping_fails() {
        let temp = tempfile::tempdir().unwrap();
        let devices = temp.path().join("devices/nvme0n1");
        let sysfs = temp.path().join("sysfs");
        std::fs::create_dir_all(devices.join("nvme0n1p1")).unwrap();
        std::fs::create_dir(&sysfs).unwrap();
        std::fs::write(devices.join("dev"), "259:0\n").unwrap();
        std::fs::write(devices.join("nvme0n1p1/dev"), "259:1\n").unwrap();
        std::fs::write(devices.join("nvme0n1p1/partition"), "1\n").unwrap();
        std::os::unix::fs::symlink(&devices, sysfs.join("259:0")).unwrap();
        std::os::unix::fs::symlink(devices.join("nvme0n1p1"), sysfs.join("259:1")).unwrap();
        assert_eq!(io_device(&sysfs, 259, 0).unwrap(), "259:0");
        assert_eq!(io_device(&sysfs, 259, 1).unwrap(), "259:0");
        assert!(io_device(&sysfs, 259, 2).is_err());
        std::fs::write(devices.join("dev"), "invalid").unwrap();
        assert!(io_device(&sysfs, 259, 1).is_err());
    }
}
