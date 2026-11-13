//! Host-owned passt policy. The guest cannot change its socket owner's UID or
//! this nftables output chain. Install the complete policy atomically before
//! handing a network fd to the VMM; VM teardown needs no firewall cleanup.

use std::collections::BTreeSet;
#[cfg(target_os = "linux")]
use std::io::Write;
#[cfg(target_os = "linux")]
use std::process::{Command, Stdio};

use g5_contracts::error::G5Error;

/// Set only by the host's isolation manager, never from the guest environment.
pub const HOST_API_URL_ENV: &str = "G5D_PASST_HOST_API_URL";

fn config_error(message: impl Into<String>) -> G5Error {
    G5Error::Config(format!("guest egress: {}", message.into()))
}

pub use g5d_foundation::runtime_environment::local_api_port;

#[cfg(target_os = "linux")]
fn env_optional(name: &str) -> Result<Option<String>, G5Error> {
    match std::env::var(name) {
        Ok(v) if v.is_empty() => Ok(None),
        Ok(v) => Ok(Some(v)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(_) => Err(config_error(format!("{name} is not Unicode"))),
    }
}

#[cfg(target_os = "linux")]
fn host_tcp_ports() -> Result<BTreeSet<u16>, G5Error> {
    let mut ports = BTreeSet::new();
    if let Some(raw) = env_optional("G5D_CACHE_DOOR_PORT")? {
        let port = raw
            .parse::<u16>()
            .ok()
            .filter(|p| *p != 0)
            .ok_or_else(|| config_error("invalid G5D_CACHE_DOOR_PORT"))?;
        ports.insert(port);
    }
    // The manager supplies the effective CLI/config URL. Direct vm-exec uses
    // the host environment, independently of its untrusted `-e` guest values.
    let api = env_optional(HOST_API_URL_ENV)?
        .or(env_optional("G5_CONTROL_PLANE_URL")?)
        .or(env_optional("G5D_API_URL")?)
        .or(env_optional("G5_API_URL")?);
    if let Some(api) = api {
        if let Some(port) = local_api_port(&api)? {
            ports.insert(port);
        }
    }
    Ok(ports)
}

/// Numeric inputs only. `add; delete; add` is one netlink transaction and works
/// for the first boot, retries, and concurrent boots without an unfiltered gap.
/// Only a positive socket-owner match enters the guest chain; kernel-generated
/// traffic without a socket owner must remain outside this policy.
pub fn nft_policy(uid: u32, host_ports: &BTreeSet<u16>) -> Result<String, G5Error> {
    if uid == 0 || host_ports.contains(&0) {
        return Err(config_error(
            "passt requires a non-root UID and nonzero host ports",
        ));
    }
    let table = format!("g5_guest_{uid}");
    // Docker's embedded resolver DNATs port 53 to a loopback high port before
    // this output hook. Authorize the original DNS destination, not an arbitrary
    // high port; direct connections to that port still reach the deny rules.
    let mut rules = format!(
        "add table inet {table}\ndelete table inet {table}\n\
         table inet {table} {{\n\
         chain output {{ type filter hook output priority 0; policy accept;\n\
         meta skuid {uid} jump guest\n}}\n\
         chain guest {{\n\
         ct direction reply accept\n\
         meta l4proto {{ tcp, udp }} ct original proto-dst 53 accept\n"
    );
    if !host_ports.is_empty() {
        let ports = host_ports
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        rules.push_str(&format!(
            "ip daddr 127.0.0.1 tcp dport {{ {ports} }} accept\n"
        ));
    }
    rules.push_str(
        "ip daddr { 0.0.0.0/8, 127.0.0.0/8, 169.254.0.0/16 } reject\n\
         ip6 daddr { ::/128, ::1/128, fe80::/10, fd00:ec2::254/128, fd20:ce::254/128 } reject\n\
         fib daddr type local reject\n}\n}\n",
    );
    Ok(rules)
}

#[cfg(target_os = "linux")]
pub fn default_identity() -> Result<(u32, u32), G5Error> {
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result = std::ptr::null_mut();
    let mut buffer = vec![0u8; 16384];
    let rc = unsafe {
        libc::getpwnam_r(
            c"g5-passt".as_ptr(),
            &mut entry,
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut result,
        )
    };
    if rc != 0 || result.is_null() || entry.pw_uid == 0 || entry.pw_gid == 0 {
        return Err(config_error(
            "missing non-root g5-passt account; run the worker passt installer",
        ));
    }
    Ok((entry.pw_uid, entry.pw_gid))
}

#[cfg(target_os = "linux")]
pub fn install(uid: u32) -> Result<(), G5Error> {
    let policy = nft_policy(uid, &host_tcp_ports()?)?;
    let mut child = Command::new("/usr/sbin/nft")
        .args(["-f", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| config_error(format!("cannot start nftables: {e}")))?;
    let written = child.stdin.take().unwrap().write_all(policy.as_bytes());
    let output = child
        .wait_with_output()
        .map_err(|e| config_error(format!("cannot wait for nftables: {e}")))?;
    if written.is_err() || !output.status.success() {
        return Err(config_error(format!(
            "cannot install passt policy: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_api_ports_are_derived_only_from_loopback_urls() {
        for (url, expected) in [
            ("http://localhost:8080/path", Some(8080)),
            ("https://127.0.0.2/path", Some(443)),
            ("http://[::1]:3000", Some(3000)),
            ("https://api.g5.example", None),
            ("http://169.254.169.254", None),
        ] {
            assert_eq!(local_api_port(url).unwrap(), expected, "{url}");
        }
        for url in [
            "bad url",
            "/relative",
            "http://localhost:0",
            "http://localhost:65536",
            "http://localhost:bad",
            "http://localhost:",
            "http://[::1]:65536",
            "ftp://localhost",
        ] {
            assert!(local_api_port(url).is_err(), "{url}");
        }
        assert!(nft_policy(0, &BTreeSet::new()).is_err());
        assert!(nft_policy(1000, &BTreeSet::from([0])).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires scripts/test-guest-egress.sh in a disposable network namespace"]
    #[serial_test::serial]
    fn passt_vhost_user_policy_gate() {
        assert_eq!(std::env::var("G5_EGRESS_ISOLATED_TEST").as_deref(), Ok("1"));
        std::env::remove_var("G5D_PASST_UID");
        std::env::remove_var("G5D_PASST_GID");
        std::env::set_var("G5D_CACHE_DOOR_PORT", "7111");
        std::env::set_var(HOST_API_URL_ENV, "http://localhost:8080");
        let parent = tempfile::tempdir().unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(parent.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let boot = || {
            super::super::passt::spawn_passt_at(
                "127.128.0.7".parse().unwrap(),
                &[(49152, 8081)],
                &parent.path().join(uuid::Uuid::new_v4().to_string()),
            )
        };
        boot()
            .expect("the provisioned default account must start passt")
            .abort();
        std::env::set_var("G5D_PASST_GID", "499");
        std::env::set_var("G5D_PASST_UID", "0");
        assert!(
            matches!(boot(), Err(G5Error::Config(_))),
            "root passt must never start"
        );
        std::env::set_var("G5D_PASST_UID", "499");
        std::env::set_var("G5D_CACHE_DOOR_PORT", "invalid");
        assert!(
            matches!(boot(), Err(G5Error::Config(_))),
            "invalid policy must never start passt"
        );
        std::env::set_var("G5D_CACHE_DOOR_PORT", "7111");
        std::env::set_var(HOST_API_URL_ENV, "http://localhost:65536");
        assert!(
            matches!(boot(), Err(G5Error::Config(_))),
            "invalid host API port must never start passt"
        );
        std::env::set_var(HOST_API_URL_ENV, "http://localhost:8080");
        // Only this explicitly requested disposable-container test moves nft.
        // Restore before asserting so failures leave a usable test environment.
        std::fs::rename("/usr/sbin/nft", "/usr/sbin/nft.sec002-test").unwrap();
        let unavailable = boot();
        std::fs::rename("/usr/sbin/nft.sec002-test", "/usr/sbin/nft").unwrap();
        assert!(
            matches!(unavailable, Err(G5Error::Config(_))),
            "missing nft must prevent boot"
        );
        let handle = boot().expect("policy must be installed before vhost-user readiness");
        use std::os::unix::fs::{FileTypeExt, MetadataExt};
        let socket = std::fs::symlink_metadata(&handle.socket).unwrap();
        assert!(socket.file_type().is_socket());
        assert_eq!(socket.uid(), 499);
        assert_eq!(socket.mode() & 0o777, 0o660);
        let pid = handle.child.id();
        handle.abort();
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
        boot()
            .expect("restart after prior VM teardown must preserve enforcement")
            .abort();
        // Both explicit numeric IDs work without the default account, as on
        // custom hosts. This test runs only in its disposable container.
        assert!(Command::new("userdel")
            .arg("g5-passt")
            .status()
            .unwrap()
            .success());
        boot()
            .expect("complete UID/GID overrides must not require the default account")
            .abort();
    }
}
