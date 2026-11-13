//! Trusted bootstrap, invoked only by the platform's initramfs before customer code.
use g5_contracts::G5Error;
use g5d_isolation::isolation::cloud_hypervisor::GuestBootConfig;
use std::{path::Path, process::Stdio};

async fn busybox(args: &[&str]) -> anyhow::Result<()> {
    let status = tokio::process::Command::new("/opt/g5/bootstrap/busybox")
        .args(args)
        .env_clear()
        .env("PATH", "/opt/g5/bootstrap:/opt/g5/bin")
        .stdin(Stdio::null())
        .status()
        .await?;
    anyhow::ensure!(status.success(), "trusted bootstrap utility failed");
    Ok(())
}

pub async fn execute() -> Result<(), G5Error> {
    run()
        .await
        .map_err(|error| G5Error::Config(format!("guest bootstrap: {error:#}")))
}

/// A reboot is not a new admitted incarnation. Claim before any source mutation,
/// and persist the claim before a panic/reboot can replay the original inputs.
fn claim_bootstrap(directory: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    anyhow::ensure!(
        std::fs::symlink_metadata(directory)?.is_dir(),
        "invalid bootstrap state directory"
    );
    let marker = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(directory.join("bootstrapped"))?;
    marker.sync_all()?;
    std::fs::File::open(directory)?.sync_all()?;
    Ok(())
}

async fn run() -> anyhow::Result<()> {
    use anyhow::{ensure, Context};
    ensure!(
        cfg!(target_os = "linux") && unsafe { libc::geteuid() } == 0,
        "guest bootstrap requires guest root"
    );
    for directory in ["/etc", "/work", "/run/g5-bootstrap", "/opt/g5"] {
        ensure!(
            std::fs::symlink_metadata(directory)?.is_dir(),
            "invalid bootstrap directory: {directory}"
        );
    }
    claim_bootstrap(Path::new("/image/.g5"))?;
    #[cfg(target_os = "linux")]
    {
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        ensure!(
            unsafe { libc::statvfs(c"/run/g5-bootstrap".as_ptr(), &mut stat) } == 0
                && stat.f_flag & libc::ST_RDONLY != 0,
            "bootstrap input must be read-only"
        );
    }
    // This path is the separate read-only input disk mounted by trusted PID 1.
    let path = Path::new("/run/g5-bootstrap/config.json");
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= 4 * 1024 * 1024,
        "invalid bootstrap configuration"
    );
    let config: GuestBootConfig = serde_json::from_slice(&std::fs::read(path)?)?;
    ensure!(
        config.version == 2 && !config.command.is_empty(),
        "unsupported bootstrap configuration"
    );
    ensure!(
        Path::new(&config.workdir).is_absolute(),
        "guest cwd must be absolute"
    );
    busybox(&["ip", "link", "set", "lo", "up"]).await?;
    let nic = std::fs::read_dir("/sys/class/net")?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|n| n != "lo")
        .context("virtio NIC missing")?;
    busybox(&["ip", "addr", "add", "10.85.0.2/24", "dev", &nic]).await?;
    busybox(&["ip", "link", "set", &nic, "up"]).await?;
    busybox(&["ip", "route", "add", "default", "via", "10.85.0.1"]).await?;
    let hostname = guest_hostname(&config.env);
    set_hostname(&hostname)?;
    let mut hosts = "127.0.0.1 localhost\n169.254.1.2 g5-host\n".to_string();
    hosts.push_str(&format!("127.0.1.1 {hostname}\n"));
    for (host, ip) in &config.host_aliases {
        hosts.push_str(&format!("{ip} {host}\n"));
    }
    for (name, contents) in [
        (
            "nsswitch.conf",
            "passwd: files\ngroup: files\nhosts: files dns\n".into(),
        ),
        ("hostname", format!("{hostname}\n")),
        ("hosts", hosts),
        (
            "resolv.conf",
            "nameserver 169.254.1.1\noptions timeout:2 attempts:3\n".into(),
        ),
    ] {
        let path = Path::new("/etc").join(name);
        match std::fs::remove_file(&path) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        };
        std::fs::write(path, contents)?;
    }
    if config.import_workspace {
        busybox(&["tar", "-xpf", "/run/g5-bootstrap/work.tar", "-C", "/work"]).await?;
    }
    if let Some(restore) = config.restore.as_ref() {
        crate::cli::guest_restore::restore(restore).await?;
    }
    let limits = libc::rlimit {
        rlim_cur: 1048576,
        rlim_max: 1048576,
    };
    ensure!(
        unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limits) } == 0,
        "set guest descriptor limits"
    );
    ensure!(
        std::fs::symlink_metadata("/image/etc")?.is_dir(),
        "image /etc must be a directory"
    );
    for name in ["nsswitch.conf", "hostname", "hosts", "resolv.conf"] {
        let destination = Path::new("/image/etc").join(name);
        match std::fs::remove_file(&destination) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        std::fs::copy(Path::new("/etc").join(name), destination)?;
    }
    // Image hooks run only after exact source recovery and never inherit platform tokens.
    let hook = Path::new("/image/usr/local/bin/g5-guest-boot");
    if hook.is_file() {
        use std::os::unix::fs::PermissionsExt;
        if hook.metadata()?.permissions().mode() & 0o111 != 0 {
            std::fs::create_dir_all("/image/var/log")?;
            let log = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open("/image/var/log/g5-guest-boot.log")?;
            let mut hook_command = tokio::process::Command::new("/usr/local/bin/g5-guest-boot");
            enter_workload(&mut hook_command, "/")?;
            hook_command
                .env_clear()
                .env(
                    "PATH",
                    "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                )
                .stdin(Stdio::null())
                .stdout(log.try_clone()?)
                .stderr(log)
                .spawn()?;
        }
    }
    // The runtime disk pins g5d's ELF interpreter and library closure. Direct
    // exec is essential: dispatcher/module self-reexec must identify g5d,
    // rather than an explicitly invoked dynamic loader.
    let mut command = tokio::process::Command::new(&config.command[0]);
    command
        .args(&config.command[1..])
        .env_clear()
        .env(
            "PATH",
            "/opt/g5/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        )
        .env("HOME", "/root")
        .envs(&config.env);
    enter_workload(&mut command, &config.workdir)?;
    let status = command.status().await?;
    use std::os::unix::process::ExitStatusExt;
    std::process::exit(status.code().unwrap_or(128 + status.signal().unwrap_or(1)));
}

/// The guest's hostname: the workspace name the control plane carried as a
/// hostname label (`G5D_WORKSPACE_HOSTNAME`), re-normalized here so the
/// kernel nodename is always a valid label even if the spec env was not, and
/// the fallback label when an older control plane sent none.
fn guest_hostname(env: &std::collections::BTreeMap<String, String>) -> String {
    g5_internal_api::host_ctl::workspace_hostname(
        env.get(g5_internal_api::host_ctl::WORKSPACE_HOSTNAME_ENV)
            .map(String::as_str)
            .unwrap_or_default(),
    )
}

/// Set the kernel nodename for the whole guest (trusted init shares its UTS
/// namespace with the workload), so bash's `\h`, `hostname` and `uname -n`
/// name the workspace rather than the kernel default `(none)`.
fn set_hostname(hostname: &str) -> anyhow::Result<()> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = hostname;
        anyhow::bail!("guest hostname requires Linux");
    }
    #[cfg(target_os = "linux")]
    {
        anyhow::ensure!(
            !hostname.is_empty() && hostname.len() <= g5_internal_api::host_ctl::MAX_HOSTNAME_LEN,
            "invalid guest hostname"
        );
        if unsafe { libc::sethostname(hostname.as_ptr().cast(), hostname.len()) } != 0 {
            return Err(
                anyhow::Error::new(std::io::Error::last_os_error()).context("set guest hostname")
            );
        }
        Ok(())
    }
}

/// Change roots only in a newly forked child. The asynchronous bootstrap stays
/// in its trusted root until it has observed workload exit.
///
/// Trusted PID 1 stacks the workload tree on the mount namespace root, so
/// joining its mount namespace lands there (the same resolution `docker exec`
/// and `nsenter -m` use). A `chroot("/image")` from the bootstrap root would
/// leave the namespace root at the empty initramfs, and every exec into a
/// container the workload starts would fail with a missing `/bin/sh`.
fn enter_workload(command: &mut tokio::process::Command, cwd: &str) -> anyhow::Result<()> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (command, cwd);
        anyhow::bail!("guest mount namespace entry requires Linux");
    }
    #[cfg(target_os = "linux")]
    {
        let cwd = std::ffi::CString::new(cwd)?;
        unsafe {
            command.pre_exec(move || {
                let namespace =
                    libc::open(c"/proc/1/ns/mnt".as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC);
                if namespace < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                let joined = libc::setns(namespace, libc::CLONE_NEWNS);
                libc::close(namespace);
                if joined != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                // The namespace root must be the workload tree PID 1 prepared, never
                // the initramfs: the bootstrap wrote this file into /image/etc above.
                // No allocation after fork: report the missing workload tree as ENOENT.
                if libc::access(c"/etc/hosts".as_ptr(), libc::F_OK) != 0 {
                    return Err(std::io::Error::from_raw_os_error(libc::ENOENT));
                }
                if libc::chdir(cwd.as_ptr()) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn workload_entry_rejects_unsupported_platform() {
        let mut command = tokio::process::Command::new("/bin/sh");
        let error = enter_workload(&mut command, "/").unwrap_err();
        assert!(error.to_string().contains("requires Linux"));
    }

    #[test]
    fn guest_hostname_normalizes_the_spec_value_and_defaults_when_absent() {
        let mut env = std::collections::BTreeMap::new();
        assert_eq!(guest_hostname(&env), "workspace");
        env.insert(
            "G5D_WORKSPACE_HOSTNAME".to_string(),
            "issue-685-prompt".to_string(),
        );
        assert_eq!(guest_hostname(&env), "issue-685-prompt");
        env.insert(
            "G5D_WORKSPACE_HOSTNAME".to_string(),
            "Not A Label/".to_string(),
        );
        assert_eq!(guest_hostname(&env), "not-a-label");
        env.insert("G5D_WORKSPACE_HOSTNAME".to_string(), "a".repeat(80));
        assert_eq!(guest_hostname(&env).len(), 63);
    }

    #[test]
    fn reboot_refuses_bootstrap_before_overwriting_guest_changes() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join(".g5");
        std::fs::create_dir(&state).unwrap();
        claim_bootstrap(&state).unwrap();
        let source = temp.path().join("source");
        std::fs::write(&source, b"unsaved guest change").unwrap();
        assert!(claim_bootstrap(&state).is_err());
        assert_eq!(std::fs::read(&source).unwrap(), b"unsaved guest change");
        let alias = temp.path().join("alias");
        std::os::unix::fs::symlink(&state, &alias).unwrap();
        assert!(claim_bootstrap(&alias).is_err());
    }
}
