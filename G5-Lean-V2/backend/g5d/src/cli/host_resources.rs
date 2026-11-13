//! Equal workspace VM slots derived from the host's effective hardware budget.

#[derive(Clone, Copy, Debug)]
pub(super) struct HostHardware {
    pub cpus: u32,
    pub ram_mib: u64,
}

pub(super) fn probe() -> Result<HostHardware, String> {
    let cpus = std::thread::available_parallelism()
        .map_err(|error| format!("cannot probe host CPUs: {error}"))?
        .get() as u32;
    let ram_mib = ram_mib()?;
    Ok(HostHardware { cpus, ram_mib })
}

#[cfg(target_os = "linux")]
fn ram_mib() -> Result<u64, String> {
    linux_ram_mib(
        &std::fs::read_to_string("/proc/meminfo").map_err(|e| e.to_string())?,
        &std::fs::read_to_string("/proc/self/cgroup").map_err(|e| e.to_string())?,
        &std::fs::read_to_string("/proc/self/mountinfo").map_err(|e| e.to_string())?,
    )
}

#[cfg(any(target_os = "linux", test))]
fn linux_ram_mib(meminfo: &str, membership: &str, mounts: &str) -> Result<u64, String> {
    let physical = meminfo
        .lines()
        .find_map(|line| {
            line.strip_prefix("MemTotal:")?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
        })
        .ok_or_else(|| "cannot probe host MemTotal".to_string())?
        / 1024;
    let (current, unified) = membership
        .lines()
        .find_map(|line| {
            let mut fields = line.splitn(3, ':');
            fields.next()?;
            let controllers = fields.next()?;
            let path = fields.next()?;
            controllers
                .split(',')
                .any(|c| c == "memory")
                .then_some((path, false))
        })
        .or_else(|| {
            // On hybrid hosts, memory's explicit v1 membership is authoritative;
            // a simultaneous unified membership may own only other controllers.
            membership
                .lines()
                .find_map(|line| line.strip_prefix("0::").map(|path| (path, true)))
        })
        .ok_or_else(|| "cannot determine host memory controller membership".to_string())?;
    if std::path::Path::new(current)
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(
            "cannot locate host memory membership outside its cgroup namespace".to_string(),
        );
    }
    let mut selected = None;
    let mut controller_mounted = false;
    let mut root_error = None;
    for line in mounts.lines() {
        let Some((before, after)) = line.split_once(" - ") else {
            continue;
        };
        let fields: Vec<_> = before.split_whitespace().collect();
        let controller: Vec<_> = after.split_whitespace().collect();
        if fields.len() < 5 || controller.len() < 3 {
            continue;
        }
        let matches = if unified {
            controller[0] == "cgroup2"
        } else {
            controller[0] == "cgroup" && controller[2].split(',').any(|c| c == "memory")
        };
        if !matches {
            continue;
        }
        controller_mounted = true;
        let mounted_root = unescape_mount_path(fields[3]);
        let root = std::path::PathBuf::from(unescape_mount_path(fields[4]));
        let Some(relative) = std::path::Path::new(current)
            .strip_prefix(&mounted_root)
            .ok()
        else {
            continue;
        };
        // A delegated alias must not hide a complete mount of this controller.
        // Even a finite namespace-root limit can hide a stricter ancestor.
        if mounted_root != "/" {
            continue;
        }
        let complete_hierarchy = if unified {
            unrestricted_v2_root(&root)
        } else {
            root.join("cgroup.sane_behavior")
                .try_exists()
                .map_err(|error| format!("cannot probe memory controller root: {error}"))
        };
        match complete_hierarchy {
            Ok(true) => {
                selected = Some((root.clone(), root.join(relative)));
                break;
            }
            Ok(false) => {}
            Err(error) => {
                root_error.get_or_insert(error);
            }
        }
    }
    let (root, mut path) = selected.ok_or_else(|| {
        root_error.unwrap_or_else(|| {
            if controller_mounted {
                "cannot measure host RAM with hidden cgroup ancestors; run the host container with --cgroupns=host and expose the full memory controller mount".to_string()
            } else {
                "cannot locate host memory controller mount".to_string()
            }
        })
    })?;
    let filename = if unified {
        "memory.max"
    } else {
        "memory.limit_in_bytes"
    };
    let mut ram = physical;
    loop {
        let limit_path = path.join(filename);
        let value = match std::fs::read_to_string(&limit_path) {
            Ok(value) => value,
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && unified
                    && ((path == root && unrestricted_v2_root(&root)?)
                        || (path != root && inactive_v2_memory_controller(&path)?)) =>
            {
                // A genuine root or a visibly inactive child controller has
                // no limit file. Continue inspecting every visible ancestor.
                // An unknown namespace-root budget still fails closed.
                "max".to_string()
            }
            Err(error) => {
                return Err(format!("cannot probe {}: {error}", limit_path.display()));
            }
        };
        let value = value.trim();
        if !(unified && value == "max") {
            let bytes = value
                .parse::<u64>()
                .map_err(|e| format!("invalid memory limit in {}: {e}", limit_path.display()))?;
            ram = ram.min(bytes / (1024 * 1024));
        }
        if path == root {
            break;
        }
        if !path.pop() || !path.starts_with(&root) {
            return Err("memory controller escaped its mount".to_string());
        }
    }
    Ok(ram)
}

#[cfg(any(target_os = "linux", test))]
fn unrestricted_v2_root(root: &std::path::Path) -> Result<bool, String> {
    match std::fs::metadata(root.join("cgroup.type")) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let controllers = std::fs::read_to_string(root.join("cgroup.controllers"))
                .map_err(|error| format!("cannot probe root memory controller: {error}"))?;
            Ok(controllers.split_whitespace().any(|c| c == "memory"))
        }
        Err(error) => Err(format!("cannot probe root cgroup type: {error}")),
    }
}

#[cfg(any(target_os = "linux", test))]
fn inactive_v2_memory_controller(path: &std::path::Path) -> Result<bool, String> {
    let kind = std::fs::read_to_string(path.join("cgroup.type"))
        .map_err(|error| format!("cannot probe cgroup type: {error}"))?;
    if !matches!(kind.trim(), "domain" | "domain threaded" | "threaded") {
        return Err("invalid memory-controller cgroup type".to_string());
    }
    let controllers = std::fs::read_to_string(path.join("cgroup.controllers"))
        .map_err(|error| format!("cannot probe memory controller availability: {error}"))?;
    Ok(!controllers.split_whitespace().any(|c| c == "memory"))
}

#[cfg(any(target_os = "linux", test))]
fn unescape_mount_path(path: &str) -> String {
    path.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

#[cfg(target_os = "macos")]
fn ram_mib() -> Result<u64, String> {
    let mut bytes = 0_u64;
    let mut size = std::mem::size_of::<u64>();
    let result = unsafe {
        libc::sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&mut bytes as *mut u64).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 {
        return Err("cannot probe host hw.memsize".to_string());
    }
    Ok(bytes / (1024 * 1024))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn ram_mib() -> Result<u64, String> {
    Err("host resource probing is unsupported on this platform".to_string())
}

pub(super) fn resolve(
    hw: HostHardware,
    slots: u32,
    cpu_override: Option<u8>,
    mem_override: Option<u32>,
) -> Result<(u8, u32), String> {
    if slots == 0 || slots > hw.cpus || hw.ram_mib == 0 {
        return Err("VM count must fit the measured host CPU/RAM budget".to_string());
    }
    let reserve = 1536_u64.max(hw.ram_mib / 10).min(hw.ram_mib / 2);
    let usable = hw.ram_mib - reserve;
    let cpus = cpu_override.map(u32::from).unwrap_or(hw.cpus / slots);
    let mem = mem_override
        .map(u64::from)
        .unwrap_or(usable / u64::from(slots));
    if cpus == 0
        || cpus > u32::from(u8::MAX)
        || !(256..=1_048_576).contains(&mem)
        || u64::from(cpus) * u64::from(slots) > u64::from(hw.cpus)
        || mem * u64::from(slots) > usable
    {
        return Err(format!("VM slot size ({cpus} CPUs, {mem} MiB) exceeds supported sizes or the host budget for {slots} VMs"));
    }
    Ok((cpus as u8, mem as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mount_fixture(root: &std::path::Path, version: u8, mounted_root: &str) -> String {
        let controller = if version == 2 {
            "cgroup2 cgroup rw"
        } else {
            "cgroup cgroup rw,memory"
        };
        format!(
            "1 0 0:1 {mounted_root} {} rw - {controller}",
            root.display()
        )
    }

    #[test]
    fn memory_probe_obeys_nested_service_and_ancestor_limits() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("system.slice/worker.service")).unwrap();
        std::fs::write(root.path().join("cgroup.controllers"), "cpu memory pids").unwrap();
        std::fs::write(
            root.path().join("system.slice/memory.max"),
            (8_u64 * 1024 * 1024 * 1024).to_string(),
        )
        .unwrap();
        std::fs::write(
            root.path().join("system.slice/worker.service/memory.max"),
            (12_u64 * 1024 * 1024 * 1024).to_string(),
        )
        .unwrap();
        assert_eq!(
            linux_ram_mib(
                "MemTotal: 33554432 kB",
                "0::/system.slice/worker.service",
                &mount_fixture(root.path(), 2, "/")
            )
            .unwrap(),
            8192
        );
    }

    #[test]
    fn memory_probe_prefers_complete_hierarchy_over_delegated_mounts() {
        for version in [1, 2] {
            let root = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(root.path().join("parent/worker")).unwrap();
            let filename = if version == 1 {
                std::fs::write(root.path().join("cgroup.sane_behavior"), "0").unwrap();
                std::fs::write(root.path().join("memory.limit_in_bytes"), "8589934592").unwrap();
                "memory.limit_in_bytes"
            } else {
                std::fs::write(root.path().join("cgroup.controllers"), "cpu memory pids").unwrap();
                std::fs::write(root.path().join("parent/cgroup.type"), "domain").unwrap();
                "memory.max"
            };
            std::fs::write(root.path().join("parent").join(filename), "536870912").unwrap();
            std::fs::write(
                root.path().join("parent/worker").join(filename),
                "1073741824",
            )
            .unwrap();
            let complete = mount_fixture(root.path(), version, "/");
            let delegated = mount_fixture(&root.path().join("parent"), version, "/parent");
            let memberships = if version == 1 {
                [("5:memory:/parent/worker", 512), ("5:memory:/", 8192)]
            } else {
                [("0::/parent/worker", 512), ("0::/", 32768)]
            };
            for (membership, expected) in memberships {
                for mounts in [
                    format!("{complete}\n{delegated}"),
                    format!("{delegated}\n{complete}"),
                ] {
                    assert_eq!(
                        linux_ram_mib("MemTotal: 33554432 kB", membership, &mounts).unwrap(),
                        expected,
                        "v{version}: complete hierarchy must win in either mount order"
                    );
                }
                assert!(linux_ram_mib("MemTotal: 33554432 kB", membership, &delegated).is_err());
            }
        }
    }

    #[test]
    fn memory_probe_accepts_unbounded_global_v2_root() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("cgroup.controllers"), "cpu memory pids").unwrap();
        let mounts = mount_fixture(root.path(), 2, "/");
        // Unlike a container's namespaced root, the actual hierarchy root has
        // neither cgroup.type nor memory.max (the observed QA host layout).
        assert_eq!(
            linux_ram_mib("MemTotal: 33554432 kB", "0::/", &mounts).unwrap(),
            32768
        );
        std::fs::create_dir(root.path().join("worker.service")).unwrap();
        std::fs::write(root.path().join("worker.service/cgroup.type"), "domain").unwrap();
        std::fs::write(
            root.path().join("worker.service/memory.max"),
            (8_u64 * 1024 * 1024 * 1024).to_string(),
        )
        .unwrap();
        assert_eq!(
            linux_ram_mib("MemTotal: 33554432 kB", "0::/worker.service", &mounts).unwrap(),
            8192
        );
        std::fs::remove_file(root.path().join("worker.service/memory.max")).unwrap();
        assert!(linux_ram_mib("MemTotal: 33554432 kB", "0::/worker.service", &mounts).is_err());
    }

    #[test]
    fn memory_probe_requires_limit_metadata_at_nonroot_v2_mounts() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("cgroup.controllers"), "cpu memory pids").unwrap();
        std::fs::write(root.path().join("cgroup.type"), "domain").unwrap();
        assert!(linux_ram_mib(
            "MemTotal: 33554432 kB",
            "0::/",
            &mount_fixture(root.path(), 2, "/")
        )
        .is_err());
        std::fs::remove_file(root.path().join("cgroup.type")).unwrap();
        assert!(linux_ram_mib(
            "MemTotal: 33554432 kB",
            "0::/",
            &mount_fixture(root.path(), 2, "/parent")
        )
        .is_err());
        std::fs::write(root.path().join("cgroup.controllers"), "cpu pids").unwrap();
        assert!(linux_ram_mib(
            "MemTotal: 33554432 kB",
            "0::/",
            &mount_fixture(root.path(), 2, "/")
        )
        .is_err());
    }

    #[test]
    fn memory_probe_requires_complete_v1_and_v2_hierarchies() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("cgroup.sane_behavior"), "0").unwrap();
        std::fs::create_dir(root.path().join("worker")).unwrap();
        std::fs::write(
            root.path().join("memory.limit_in_bytes"),
            (8_u64 * 1024 * 1024 * 1024).to_string(),
        )
        .unwrap();
        std::fs::write(
            root.path().join("worker/memory.limit_in_bytes"),
            (4_u64 * 1024 * 1024 * 1024).to_string(),
        )
        .unwrap();
        assert_eq!(
            linux_ram_mib(
                "MemTotal: 33554432 kB",
                "5:memory:/worker",
                &mount_fixture(root.path(), 1, "/")
            )
            .unwrap(),
            4096
        );
        assert!(linux_ram_mib(
            "MemTotal: 33554432 kB",
            "5:memory:/parent/worker",
            &mount_fixture(root.path(), 1, "/parent")
        )
        .is_err());
        std::fs::remove_file(root.path().join("cgroup.sane_behavior")).unwrap();
        assert!(linux_ram_mib(
            "MemTotal: 33554432 kB",
            "5:memory:/worker",
            &mount_fixture(root.path(), 1, "/")
        )
        .is_err());
        std::fs::write(
            root.path().join("memory.max"),
            (2_u64 * 1024 * 1024 * 1024).to_string(),
        )
        .unwrap();
        for member in ["0::/", "0::/../../outside"] {
            assert!(linux_ram_mib(
                "MemTotal: 33554432 kB",
                member,
                &mount_fixture(root.path(), 2, "/parent")
            )
            .is_err());
        }
    }

    #[test]
    fn memory_probe_uses_v1_memory_controller_on_hybrid_hosts() {
        let memory = tempfile::tempdir().unwrap();
        let unified = tempfile::tempdir().unwrap();
        std::fs::write(memory.path().join("cgroup.sane_behavior"), "0").unwrap();
        std::fs::write(
            memory.path().join("memory.limit_in_bytes"),
            (4_u64 * 1024 * 1024 * 1024).to_string(),
        )
        .unwrap();
        std::fs::write(unified.path().join("cgroup.controllers"), "cpu pids").unwrap();
        let mounts = format!(
            "{}\n{}",
            mount_fixture(memory.path(), 1, "/"),
            mount_fixture(unified.path(), 2, "/")
        );
        assert_eq!(
            linux_ram_mib("MemTotal: 33554432 kB", "0::/\n5:memory:/", &mounts).unwrap(),
            4096
        );
    }

    #[test]
    fn memory_probe_obeys_ancestor_limit_when_child_memory_controller_is_inactive() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("cgroup.controllers"), "cpu memory pids").unwrap();
        std::fs::create_dir_all(root.path().join("parent/worker")).unwrap();
        std::fs::write(
            root.path().join("parent/memory.max"),
            (8_u64 * 1024 * 1024 * 1024).to_string(),
        )
        .unwrap();
        std::fs::write(root.path().join("parent/worker/cgroup.type"), "domain").unwrap();
        std::fs::write(
            root.path().join("parent/worker/cgroup.controllers"),
            "cpu pids",
        )
        .unwrap();
        let mounts = mount_fixture(root.path(), 2, "/");
        assert_eq!(
            linux_ram_mib("MemTotal: 33554432 kB", "0::/parent/worker", &mounts).unwrap(),
            8192
        );
        std::fs::remove_file(root.path().join("parent/worker/cgroup.controllers")).unwrap();
        assert!(linux_ram_mib("MemTotal: 33554432 kB", "0::/parent/worker", &mounts).is_err());
        std::fs::write(
            root.path().join("parent/worker/cgroup.controllers"),
            "memory pids",
        )
        .unwrap();
        assert!(linux_ram_mib("MemTotal: 33554432 kB", "0::/parent/worker", &mounts).is_err());
    }

    #[test]
    fn memory_probe_distinguishes_unlimited_from_missing_or_invalid_limits() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("cgroup.controllers"), "cpu memory pids").unwrap();
        std::fs::create_dir(root.path().join("worker")).unwrap();
        let mounts = mount_fixture(root.path(), 2, "/");
        for value in ["", "unlimited", "-1"] {
            std::fs::write(root.path().join("worker/memory.max"), value).unwrap();
            assert!(linux_ram_mib("MemTotal: 33554432 kB", "0::/worker", &mounts).is_err());
        }
        std::fs::remove_file(root.path().join("worker/memory.max")).unwrap();
        assert!(linux_ram_mib("MemTotal: 33554432 kB", "0::/worker", &mounts).is_err());
        std::fs::write(root.path().join("worker/memory.max"), "max").unwrap();
        assert_eq!(
            linux_ram_mib("MemTotal: 33554432 kB", "0::/worker", &mounts).unwrap(),
            32768
        );
        assert!(linux_ram_mib("MemTotal: 33554432 kB", "1:cpu:/", &mounts).is_err());
        assert!(linux_ram_mib("MemTotal: 33554432 kB", "0::/", "broken mount").is_err());
    }

    #[test]
    fn memory_probe_rejects_hidden_ancestors_even_with_a_finite_leaf_limit() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("cgroup.type"), "domain").unwrap();
        std::fs::write(root.path().join("cgroup.controllers"), "cpu memory pids").unwrap();
        for value in ["max", "1073741824"] {
            std::fs::write(root.path().join("memory.max"), value).unwrap();
            assert!(linux_ram_mib(
                "MemTotal: 33554432 kB",
                "0::/",
                &mount_fixture(root.path(), 2, "/")
            )
            .is_err());
        }
    }

    #[test]
    fn six_slots_share_the_observed_qa_host() {
        assert_eq!(
            resolve(
                HostHardware {
                    cpus: 22,
                    ram_mib: 88_490
                },
                6,
                None,
                None
            )
            .unwrap(),
            (3, 13_273)
        );
    }

    #[test]
    fn every_budget_preserves_host_ram_and_no_cpu_oversubscription() {
        let hw = HostHardware {
            cpus: 22,
            ram_mib: 90_112,
        };
        for slots in 1..=22 {
            let (cpu, mem) = resolve(hw, slots, None, None).unwrap();
            assert!(u32::from(cpu) * slots <= hw.cpus);
            assert!(u64::from(mem) * u64::from(slots) <= 81_101);
        }
        assert_eq!(resolve(hw, 1, None, None).unwrap().0, 22);
    }

    #[test]
    fn overrides_are_independent_and_must_fit_the_whole_host() {
        let hw = HostHardware {
            cpus: 22,
            ram_mib: 88_490,
        };
        assert_eq!(resolve(hw, 6, Some(2), None).unwrap(), (2, 13_273));
        assert_eq!(resolve(hw, 6, None, Some(4096)).unwrap(), (3, 4096));
        assert!(resolve(hw, 6, Some(4), None).is_err());
        assert!(resolve(hw, 6, None, Some(16_384)).is_err());
        assert!(resolve(hw, 0, None, None).is_err());
        assert!(resolve(hw, 23, None, None).is_err());
        assert!(resolve(hw, 6, Some(0), None).is_err());
        assert!(resolve(hw, 6, None, Some(255)).is_err());
        assert!(resolve(
            HostHardware {
                cpus: 4,
                ram_mib: 512
            },
            4,
            None,
            None
        )
        .is_err());
    }
}
