//! Isolated virtio-net networking through a supervised passt vhost-user process.
#[cfg(target_os = "linux")]
use g5_contracts::error::G5Error;
use std::{net::Ipv4Addr, path::PathBuf, process::Child};
#[cfg(target_os = "linux")]
use std::{path::Path, process::Stdio};

pub const GUEST_ADDR: Ipv4Addr = Ipv4Addr::new(10, 85, 0, 2);
/// Gateway passt answers on; NOT a door to the host (`--no-map-gw`).
pub const GUEST_GATEWAY: Ipv4Addr = Ipv4Addr::new(10, 85, 0, 1);
/// Guest netmask prefix length.
pub const GUEST_NETMASK: u8 = 24;
/// In-guest DNS address; passt forwards queries to the host's real resolver
/// over UDP and TCP (D51 — replaces the TSI-era hardcoded public TCP DNS).
pub const GUEST_DNS_ADDR: Ipv4Addr = Ipv4Addr::new(169, 254, 1, 1);
/// The one guest→host door (D50, human decision 1A): guest traffic to this
/// address reaches host loopback. Seeded into the guest as `g5-host`.
pub const HOST_DOOR_ADDR: Ipv4Addr = Ipv4Addr::new(169, 254, 1, 2);
/// `/etc/hosts` name the door is seeded under (D50a).
pub use g5d_foundation::runtime_environment::HOST_DOOR_ALIAS;

/// Env override for the passt binary path; default resolves `passt` on PATH.
pub const PASST_BIN_ENV: &str = "G5D_PASST_BIN";

/// Optional dedicated numeric uid for passt's host egress policy.
/// Linux defaults to the installer-owned `g5-passt` account; root is forbidden.
pub const PASST_UID_ENV: &str = "G5D_PASST_UID";
/// Optional numeric gid paired with [`PASST_UID_ENV`]. Linux defaults to g5-passt.
pub const PASST_GID_ENV: &str = "G5D_PASST_GID";
pub use g5d_foundation::runtime_environment::rewrite_loopback_url_for_door;

#[cfg(target_os = "linux")]
fn parse_id_env(name: &str) -> Result<Option<u32>, G5Error> {
    match std::env::var(name) {
        Err(std::env::VarError::NotPresent) => Ok(None),
        // A set-but-non-Unicode value is a malformed configuration, not an
        // absent one — treating it as unset would silently run passt with an
        // inherited identity (CodeRabbit review).
        Err(std::env::VarError::NotUnicode(raw)) => Err(G5Error::Internal {
            message: format!("vm-exec: {name} is set but not valid Unicode ({raw:?})"),
        }),
        Ok(v) if v.is_empty() => Ok(None),
        Ok(v) => v.parse::<u32>().map(Some).map_err(|_| G5Error::Internal {
            message: format!("vm-exec: {name}={v:?} is not a numeric id"),
        }),
    }
}

pub fn guest_mac_for_pid(pid: u32) -> [u8; 6] {
    [
        0x02,
        0xb5,
        (pid >> 24) as u8,
        (pid >> 16) as u8,
        (pid >> 8) as u8,
        pid as u8,
    ]
}

pub fn build_passt_args(vmm_addr: Ipv4Addr, forwards: &[(u16, u16)]) -> Vec<String> {
    let mut args = [
        "--foreground",
        "--quiet",
        "--vhost-user",
        "--one-off",
        "-4",
        "--address",
        "10.85.0.2",
        "--netmask",
        "24",
        "--gateway",
        "10.85.0.1",
        "--dns-forward",
        "169.254.1.1",
        "--map-host-loopback",
        "169.254.1.2",
        "--no-map-gw",
        "-u",
        "none",
    ]
    .map(str::to_owned)
    .to_vec();
    for (host, guest) in forwards {
        args.extend(["-t".into(), format!("{vmm_addr}/{host}:{guest}")]);
    }
    args
}

pub struct PasstHandle {
    pub child: Child,
    pub socket: PathBuf,
}
impl PasstHandle {
    pub fn abort(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Drop for PasstHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(target_os = "linux")]
pub fn identity() -> Result<(u32, u32), G5Error> {
    let requested_uid = parse_id_env(PASST_UID_ENV)?;
    let requested_gid = parse_id_env(PASST_GID_ENV)?;
    let (uid, gid) = match (requested_uid, requested_gid) {
        (Some(uid), Some(gid)) => (uid, gid),
        (None, None) => super::guest_egress::default_identity()?,
        _ => {
            return Err(G5Error::Config(
                "set both passt uid and gid, or neither".into(),
            ))
        }
    };
    if uid == 0 || gid == 0 {
        return Err(G5Error::Config("passt must use a non-root identity".into()));
    }
    Ok((uid, gid))
}

#[cfg(target_os = "linux")]
pub fn spawn_passt_at(
    vmm_addr: Ipv4Addr,
    forwards: &[(u16, u16)],
    directory: &Path,
) -> Result<PasstHandle, G5Error> {
    use std::os::unix::{fs::PermissionsExt, process::CommandExt};
    let fail = |e: std::io::Error| G5Error::Config(format!("passt startup: {e}"));
    let (uid, gid) = identity()?;
    super::guest_egress::install(uid)?;
    std::fs::create_dir(directory).map_err(fail)?;
    std::os::unix::fs::chown(directory, Some(uid), Some(gid)).map_err(fail)?;
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o750)).map_err(fail)?;
    let socket = directory.join("net.sock");
    // UNIX sockets have a short address bound. Fail before launching either child.
    if socket.as_os_str().len() > 100 {
        return Err(G5Error::Config(
            "VM network socket path exceeds 100 bytes".into(),
        ));
    }
    let pid = directory.join("pid");
    let bin = std::env::var_os(PASST_BIN_ENV).unwrap_or_else(|| "/usr/bin/passt".into());
    let mut command = super::platform_command(bin);
    command
        .args(build_passt_args(vmm_addr, forwards))
        .arg("--socket")
        .arg(&socket)
        .arg("--pid")
        .arg(&pid)
        .stdin(Stdio::null());
    let parent = unsafe { libc::getpid() };
    unsafe {
        command.pre_exec(move || {
            if libc::setgroups(0, std::ptr::null()) != 0
                || libc::setgid(gid) != 0
                || libc::setuid(uid) != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            // setuid clears PDEATHSIG, so arm it after changing credentials.
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() != parent {
                libc::raise(libc::SIGKILL);
            }
            Ok(())
        });
    }
    let mut handle = PasstHandle {
        child: command.spawn().map_err(fail)?,
        socket,
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(status) = handle.child.try_wait().map_err(fail)? {
            return Err(G5Error::Config(format!(
                "passt exited before readiness: {status}"
            )));
        }
        if handle.socket.exists() && std::fs::metadata(&pid).is_ok_and(|m| m.len() > 0) {
            break;
        }
        if std::time::Instant::now() >= deadline {
            return Err(G5Error::Config("passt readiness timed out".into()));
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    std::fs::set_permissions(&handle.socket, std::fs::Permissions::from_mode(0o660))
        .map_err(fail)?;
    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn host_door_urls_keep_the_endpoint_and_reject_invalid_inputs() {
        for (input, expected) in [
            (
                "http://localhost:7500/git/r?x=1",
                Some("http://g5-host:7500/git/r?x=1"),
            ),
            ("http://127.0.0.1:7500", Some("http://g5-host:7500")),
            ("https://[::1]/git", Some("https://g5-host/git")),
            ("http://g5-api:7500", None),
            ("https://qa.g5labs.ai", None),
            ("http://localhost:65536", None),
            ("http://localhost:0", None),
            ("ftp://localhost:7500", None),
            ("not a URL", None),
        ] {
            assert_eq!(
                rewrite_loopback_url_for_door(input).as_deref(),
                expected,
                "{input}"
            );
        }
    }
    #[test]
    fn forwards_are_guest_isolated_and_udp_closed() {
        let args = build_passt_args("127.128.0.7".parse().unwrap(), &[(49152, 8080)]);
        assert!(args.contains(&"--vhost-user".into()));
        assert!(args.contains(&"127.128.0.7/49152:8080".into()));
        assert!(args.windows(2).any(|a| a == ["-u", "none"]));
        assert!(!args.contains(&"--fd".into()));
    }
}
