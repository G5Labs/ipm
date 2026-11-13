//! Host-owned native runtime environment shared by VM boot and child processes.

use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};

pub const HOST_DOOR_ALIAS: &str = "g5-host";

/// Only literal loopback (or localhost) needs the passt host door. Remote API
/// URLs use ordinary egress; arbitrary DNS names do not open a host port.
pub fn local_api_port(url: &str) -> Result<Option<u16>, g5_contracts::G5Error> {
    let config_error =
        |message: &str| g5_contracts::G5Error::Config(format!("guest egress: {message}"));
    let uri: http::Uri = url
        .parse()
        .map_err(|_| config_error("invalid host API URL"))?;
    let host = uri
        .host()
        .ok_or_else(|| config_error("host API URL has no host"))?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    if !loopback {
        return Ok(None);
    }
    let default_port = match uri.scheme_str() {
        Some("http") => 80,
        Some("https") => 443,
        _ => return Err(config_error("host API URL must use http or https")),
    };
    // `http::Uri` accepts an authority with a malformed port, but reports
    // no numeric port. Only an absent port may use the scheme default.
    let authority = uri.authority().unwrap().as_str();
    let host_and_port = authority.rsplit('@').next().unwrap();
    if uri.port_u16().is_none() && host_and_port != host {
        return Err(config_error("invalid host API port"));
    }
    let port = uri.port_u16().unwrap_or(default_port);
    if port == 0 {
        return Err(config_error("host API port must be nonzero"));
    }
    Ok(Some(port))
}

/// Map a guest transport endpoint without changing its admitted route or port.
pub fn rewrite_loopback_url_for_door(url: &str) -> Option<String> {
    let uri: http::Uri = url.parse().ok()?;
    local_api_port(url).ok().flatten()?;
    if uri.authority()?.as_str().contains('@') {
        return None;
    }
    let scheme = uri.scheme_str().unwrap_or("http");
    let port = uri.port_u16().map(|p| format!(":{p}")).unwrap_or_default();
    let path_and_query = uri
        .path_and_query()
        .map(http::uri::PathAndQuery::as_str)
        .filter(|p| *p != "/")
        .unwrap_or("");
    Some(format!(
        "{scheme}://{HOST_DOOR_ALIAS}{port}{path_and_query}"
    ))
}

pub const NATIVE_LOADER_PATH_KEYS: [&str; 2] = ["LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH"];

/// The read-only runtime bundle's single directory, mount-injected into every
/// microVM. It carries g5d, the provider CLIs and the shell tools the bundle
/// supplies for agent and owner use (`stage-local-worker-runtime.sh`).
pub const GUEST_RUNTIME_BIN_DIR: &str = "/opt/g5/bin";

/// The same directory contains g5d's and the provider launchers' shared
/// libraries. Guest images do not have to install that closure.
pub const GUEST_RUNTIME_LIBRARY_PATH: &str = GUEST_RUNTIME_BIN_DIR;

/// How the dispatcher tells its module children where the runtime bundle's
/// executables are when they are not at [`GUEST_RUNTIME_BIN_DIR`]: a
/// process-mode host (ontology `g5.runtime.boot.backend.process-local`)
/// validates a host-native runtime directory and the dispatcher names its
/// `bin`. Absent means the mounted guest bundle.
pub const RUNTIME_BIN_DIR_ENV: &str = "G5D_RUNTIME_BIN_DIR";

/// Where a process-mode dispatcher binds its workspace Git socket
/// (`g5.runtime.boot.backend.process-local`): the host chooses a short,
/// owner-only directory because a Unix socket path is bounded (104 bytes on
/// macOS) while a checkout path is not. A guest keeps the socket in its state
/// directory.
pub const PROCESS_SOCKET_DIR_ENV: &str = "G5D_PROCESS_SOCKET_DIR";

/// Marks every process of a process-mode incarnation with its VM id. The host
/// sets it on the dispatcher, the dispatcher re-stamps it into every module,
/// terminal and preview environment, and children inherit it across process
/// group and session changes, so a stop or a dispatcher exit can find and tear
/// down work that left the dispatcher's session (terminals, detached apps).
pub const PROCESS_INCARNATION_ENV: &str = "G5D_PROCESS_INCARNATION";

/// A host that is itself a process of an outer incarnation (the `full-stack`
/// preview profile starts the previewed instance's own host inside the
/// preview) stamps the outer marker on its dispatchers under this key, and
/// they pass it on like the marker itself, so the outer incarnation's census
/// still finds the nested instance's work.
pub const PROCESS_INCARNATION_OUTER_ENV: &str = "G5D_PROCESS_INCARNATION_OUTER";

/// The host's Docker connection selection, carried into a process-mode
/// dispatcher and from there into modules, terminals and previews, so a
/// preview that runs Docker reaches the daemon the developer selected rather
/// than the default one. An allowlist: no other host environment crosses.
pub const HOST_DOCKER_ENV: [&str; 5] = [
    "DOCKER_HOST",
    "DOCKER_CONTEXT",
    "DOCKER_CONFIG",
    "DOCKER_CERT_PATH",
    "DOCKER_TLS_VERIFY",
];

/// The runtime bundle's executable directory for this process: the one the
/// dispatcher named, else the mounted guest bundle.
pub fn runtime_bin_dir() -> PathBuf {
    std::env::var_os(RUNTIME_BIN_DIR_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(GUEST_RUNTIME_BIN_DIR))
}

/// Names, colon-separated, of the customer image's declared environment keys
/// that survived the VM-spec merge at boot (ontology `g5.runtime.image-env`).
/// The host writes it into the guest env beside the merged values; the
/// dispatcher forwards exactly those keys, under the reserved-key policy, to
/// agent turns, terminals and previews. Reserved by prefix, so neither an
/// image nor a user setting can inject it.
pub const IMAGE_ENV_KEYS_ENV: &str = "G5D_IMAGE_ENV_KEYS";
pub const IMAGE_ENV_KEYS_SEPARATOR: char = ':';
/// Upper bound on the joined key list. It travels as one `-e` argument to
/// `g5d vm-exec`, and Linux caps a single argv string at 128 KiB; an image
/// whose declared key names alone approach that is pathological, so names
/// past the bound are dropped with a warning rather than failing the boot.
pub const IMAGE_ENV_KEYS_MAX_LEN: usize = 32 * 1024;

/// Expose only g5d, without adding the bundle's other executables or libraries
/// to repository commands.
pub fn install_g5d_launcher(
    bin: &Path,
    executable: &Path,
    loader: &[(String, String)],
) -> std::io::Result<PathBuf> {
    use std::io::Write;
    fn quoted(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
    std::fs::create_dir_all(bin)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // The dispatcher owns this directory, but terminal children run as
        // the module UID. Repair retained permissions as well as startup umask.
        // Provider-home ancestors keep their existing private permissions.
        std::fs::set_permissions(bin, std::fs::Permissions::from_mode(0o755))?;
    }
    let mut assignments = String::new();
    for (key, value) in loader {
        if !NATIVE_LOADER_PATH_KEYS.contains(&key.as_str()) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "unsupported launcher environment",
            ));
        }
        assignments.push_str(&format!("{key}={} ", quoted(value)));
    }
    let script = format!("#!/bin/sh\n# G5 workspace CLI. No credentials are stored here.\n{assignments}exec {} \"$@\"\n", quoted(&executable.to_string_lossy()));
    let mut staged = tempfile::NamedTempFile::new_in(bin)?;
    staged.write_all(script.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        staged
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o755))?;
    }
    staged
        .persist(bin.join("g5d"))
        .map_err(|error| error.error)?;
    Ok(bin.to_path_buf())
}

pub fn prepend_path(dir: &Path, path: &str) -> String {
    let dir = dir.to_string_lossy();
    let rest = path
        .split(':')
        .filter(|entry| *entry != dir)
        .collect::<Vec<_>>()
        .join(":");
    if rest.is_empty() {
        dir.into_owned()
    } else {
        format!("{dir}:{rest}")
    }
}

/// Make a directory's tools reachable by name without displacing the ones
/// already on the path. Unlike [`prepend_path`], an existing entry of the same
/// command name keeps winning lookup, so the runtime bundle can be exposed to
/// a guest shell whose image ships its own `node`, `cargo` or `just`. Repeated
/// application is a no-op: a child whose parent already appended the directory
/// must not accumulate duplicates.
pub fn append_path(dir: &str, path: &str) -> String {
    if path.is_empty() {
        dir.to_owned()
    } else if path.split(':').any(|entry| entry == dir) {
        path.to_owned()
    } else {
        format!("{path}:{dir}")
    }
}

/// The same workspace-scoped CLI context for provided agents and owner shells.
/// Provider publication credentials and dispatcher control secrets stay out.
pub fn workspace_cli_environment(
    api_url: &str,
    workspace_id: &str,
    token_file: &str,
) -> [(String, String); 3] {
    [
        ("G5D_API_URL".into(), api_url.into()),
        ("G5D_WORKSPACE_ID".into(), workspace_id.into()),
        ("G5D_WORKSPACE_TOKEN_FILE".into(), token_file.into()),
    ]
}

/// Remove the platform loader paths before starting repository-owned code.
pub fn remove_native_loader_paths(command: &mut tokio::process::Command) {
    for key in NATIVE_LOADER_PATH_KEYS {
        command.env_remove(key);
    }
}

/// Keep the inherited paths for a trusted g5d child before clearing its env.
pub fn inherited_native_loader_paths() -> Vec<(OsString, OsString)> {
    NATIVE_LOADER_PATH_KEYS
        .into_iter()
        .filter_map(|key| std::env::var_os(key).map(|value| (key.into(), value)))
        .collect()
}

/// Remove loader paths from the environment handed to a provider CLI, while
/// retaining their values for the trusted g5d shim that provider can invoke.
pub fn take_native_loader_paths(
    environment: &mut BTreeMap<String, String>,
) -> Vec<(String, String)> {
    NATIVE_LOADER_PATH_KEYS
        .into_iter()
        .filter_map(|key| {
            environment
                .remove(key)
                .map(|value| (key.to_string(), value))
        })
        .collect()
}

/// Restore the loader contract after clearing an inherited environment.
/// Apply this after dispatch configuration: tenant settings cannot replace the
/// Linux runtime mount with an arbitrary library search path.
pub fn native_loader_environment() -> Vec<(OsString, OsString)> {
    #[cfg(target_os = "linux")]
    {
        vec![("LD_LIBRARY_PATH".into(), GUEST_RUNTIME_LIBRARY_PATH.into())]
    }
    #[cfg(target_os = "macos")]
    {
        // Direct laptop process tests run Mach-O binaries, not Linux guests.
        // Preserve the developer process's linker setup; never take it from a
        // dispatch secret. An explicit empty value also blocks such overrides.
        vec![(
            "DYLD_LIBRARY_PATH".into(),
            std::env::var_os("DYLD_LIBRARY_PATH").unwrap_or_default(),
        )]
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn launcher_preserves_arguments_and_limits_library_paths_to_g5d() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let exe = root.path().join("g5d's real executable");
        std::fs::write(
            &exe,
            "#!/bin/sh\nprintf '%s\\n' \"$LD_LIBRARY_PATH\" \"$@\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let bin = root.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o700)).unwrap();
        let bin = super::install_g5d_launcher(
            &bin,
            &exe,
            &[("LD_LIBRARY_PATH".into(), "/opt/G5's libraries".into())],
        )
        .unwrap();
        assert_eq!(
            std::fs::metadata(&bin).unwrap().permissions().mode() & 0o777,
            0o755
        );
        let output = std::process::Command::new("/bin/sh").env_clear()
            .env("PATH", super::prepend_path(&bin, "/usr/bin:/bin"))
            .args(["-c", "g5d pr create --title \"a ' title\"; printf 'parent:%s\\n' \"${LD_LIBRARY_PATH-unset}\""])
            .output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            "/opt/G5's libraries\npr\ncreate\n--title\na ' title\nparent:unset\n"
        );
        assert_eq!(
            super::prepend_path(&bin, &format!("/usr/bin:{}:/bin", bin.display())),
            format!("{}:/usr/bin:/bin", bin.display())
        );
        super::install_g5d_launcher(&bin, &exe, &[]).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires root to exercise the guest terminal UID boundary"]
    fn launcher_is_executable_by_the_guest_terminal_uid() {
        use std::os::unix::{fs::PermissionsExt, process::CommandExt};
        assert_eq!(unsafe { libc::geteuid() }, 0, "run this test as root");
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let bin = root.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o700)).unwrap();
        super::install_g5d_launcher(&bin, std::path::Path::new("/bin/echo"), &[]).unwrap();
        let output = std::process::Command::new("/bin/sh")
            .uid(1000)
            .gid(1000)
            .env_clear()
            .env("PATH", super::prepend_path(&bin, "/usr/bin:/bin"))
            .args(["-c", "g5d terminal-access"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, b"terminal-access\n");
    }

    #[test]
    fn appending_the_bundle_never_displaces_an_existing_tool_or_duplicates() {
        let bin = super::GUEST_RUNTIME_BIN_DIR;
        // An image tool of the same name keeps winning: the bundle goes last.
        assert_eq!(
            super::append_path(bin, "/usr/local/bin:/usr/bin"),
            format!("/usr/local/bin:/usr/bin:{bin}")
        );
        // A module whose parent already appended it must not accumulate a
        // second entry, wherever that entry sits.
        assert_eq!(
            super::append_path(bin, &format!("/usr/bin:{bin}")),
            format!("/usr/bin:{bin}")
        );
        assert_eq!(
            super::append_path(bin, &format!("{bin}:/usr/bin")),
            format!("{bin}:/usr/bin")
        );
        assert_eq!(super::append_path(bin, ""), bin);
        // A near-miss entry is a different directory, not a duplicate.
        assert_eq!(
            super::append_path(bin, &format!("{bin}-extra")),
            format!("{bin}-extra:{bin}")
        );
        // The launcher directory must stay ahead of the bundle's own g5d.
        let launcher = std::path::Path::new("/var/lib/g5d/bin");
        let path = super::append_path(bin, &super::prepend_path(launcher, "/usr/bin"));
        assert_eq!(path, format!("/var/lib/g5d/bin:/usr/bin:{bin}"));
        assert!(
            path.split(':').position(|dir| dir == "/var/lib/g5d/bin")
                < path.split(':').position(|dir| dir == bin),
            "the g5d shim must win lookup over the bundle binary: {path}"
        );
    }

    #[test]
    fn provider_environment_removes_only_platform_loader_paths() {
        let mut environment = std::collections::BTreeMap::from([
            ("LD_LIBRARY_PATH".to_string(), "/linux".to_string()),
            ("DYLD_LIBRARY_PATH".to_string(), "/mac".to_string()),
            ("PATH".to_string(), "/bin".to_string()),
        ]);
        assert_eq!(
            super::take_native_loader_paths(&mut environment),
            vec![
                ("LD_LIBRARY_PATH".to_string(), "/linux".to_string()),
                ("DYLD_LIBRARY_PATH".to_string(), "/mac".to_string()),
            ]
        );
        assert_eq!(environment.len(), 1);
        assert_eq!(environment["PATH"], "/bin");
    }

    #[test]
    fn loader_environment_uses_the_platform_runtime_contract() {
        let env = super::native_loader_environment();
        #[cfg(target_os = "linux")]
        assert_eq!(
            env,
            vec![(
                "LD_LIBRARY_PATH".into(),
                super::GUEST_RUNTIME_LIBRARY_PATH.into()
            )]
        );
        #[cfg(target_os = "macos")]
        assert_eq!(
            env,
            vec![(
                "DYLD_LIBRARY_PATH".into(),
                std::env::var_os("DYLD_LIBRARY_PATH").unwrap_or_default()
            )]
        );
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        assert!(env.is_empty());
    }
}
