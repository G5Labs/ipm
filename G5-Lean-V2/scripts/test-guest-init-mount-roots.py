#!/usr/bin/env python3
"""Mount-topology oracle for the trusted guest init (`infra/guest/init.c`).

The guest init cannot run outside a VM (it must be PID 1 and owns /dev/vda), so
this script replays its mount sequence with tmpfs stand-ins for the disks inside a
private mount namespace, then checks the one property the sequence must deliver:
the workload tree is the mount namespace root. `setns(2)` into a mount namespace
lands on that root, so this is what `docker exec`, Compose health checks and
`nsenter -m` see after runc's `pivot_root`. A workload entered by `chroot` alone
leaves the namespace root at the empty initramfs, and every exec into a container
then fails with `stat /bin/sh: no such file or directory` (observed 2026-10-16:
the full-stack preview profile died in `wait-for-healthy.sh`).

Requires x86_64 Linux and root (CAP_SYS_ADMIN and CAP_SYS_CHROOT); it never writes outside
tmpfs it mounts itself and all mounts die with its private namespace.

    sudo python3 scripts/test-guest-init-mount-roots.py            # current init.c order
    sudo python3 scripts/test-guest-init-mount-roots.py --legacy   # chroot-only order must fail
"""
import argparse
import ctypes
import errno
import os
import platform
import shutil
import signal
import sys

libc = ctypes.CDLL(None, use_errno=True)
MS_RDONLY, MS_NOSUID, MS_NODEV, MS_NOEXEC = 1, 2, 4, 8
MS_REMOUNT = 0x20
MS_REC, MS_BIND, MS_MOVE, MS_PRIVATE = 0x4000, 0x1000, 0x2000, 1 << 18
# The host's cache profile as vm-exec stages it on the bootstrap disk (ontology
# g5.runtime.cache.profile); the runtime disk ships an empty placeholder at the
# same name for the init to bind it over.
PROFILE = "G5_CACHE_DOOR=http://g5-host:7111\nG5_BASE_IMAGE_MIRROR_TIERS=base base-rust\n"
PROFILE_NAME = "cache-profile.env"
MNT_DETACH = 2
SYS_PIVOT_ROOT = 155  # x86_64; the qualified guest architecture
# A static shell copied into the tmpfs stand-ins; dynamic binaries cannot run there.
SHELL_CANDIDATES = ["/opt/g5/bootstrap/busybox", "/bin/busybox", "/usr/bin/busybox"]


def check(result, what):
    if result != 0:
        error = ctypes.get_errno()
        raise OSError(error, f"{what}: {os.strerror(error)}")


def mount(source, target, kind=None, flags=0, data=None):
    check(
        libc.mount(
            source.encode(),
            target.encode(),
            kind.encode() if kind else None,
            ctypes.c_ulong(flags),
            data.encode() if data else None,
        ),
        f"mount {source} -> {target}",
    )


def mount_fs(source, path, kind, flags=0, data=None):
    os.makedirs(path, mode=0o755, exist_ok=True)
    mount(source, path, kind, flags, data)


def pivot_root(new_root, put_old):
    check(libc.syscall(SYS_PIVOT_ROOT, new_root.encode(), put_old.encode()), "pivot_root")


def escape_to_namespace_root():
    """Make the real mount-namespace root this process's root (root-only chroot escape).

    The script may start inside a guest whose userland is itself chrooted; the
    sequence under test must be replayed from the true namespace root exactly as
    PID 1 runs it.
    """
    os.chroot("/proc")  # any directory that is not the current root
    for _ in range(64):
        os.chdir("..")
    os.chroot(".")
    os.chdir("/")


def read_static_shell():
    for candidate in SHELL_CANDIDATES:
        if os.path.exists(candidate):
            with open(candidate, "rb") as f:
                return f.read()
    raise SystemExit(f"no static shell among {SHELL_CANDIDATES}")


def replay_init(legacy, shell, profile):
    """Replay init.c's mount sequence. `legacy` reproduces the chroot-only order.

    `profile` is whether the host staged a cache profile on the bootstrap disk.
    """
    mount_fs("tmpfs", "/root", "tmpfs")  # /dev/vda stand-in
    os.makedirs("/root/bin", exist_ok=True)
    with open("/root/bin/sh", "wb") as f:
        f.write(shell)
    os.chmod("/root/bin/sh", 0o755)
    open("/root/.g5-oracle-workload", "w").close()
    os.makedirs("/root/etc", exist_ok=True)
    os.makedirs("/root/work", exist_ok=True)
    os.makedirs("/root/var/lib/docker", exist_ok=True)
    os.makedirs("/root/opt", exist_ok=True)
    # /dev/vdb stand-in: populated like build-runtime-disk.sh (the empty
    # placeholder), then sealed read-only as the init mounts the real disk.
    mount_fs("tmpfs", "/root/opt/g5", "tmpfs")
    open(f"/root/opt/g5/{PROFILE_NAME}", "w").close()
    os.chmod(f"/root/opt/g5/{PROFILE_NAME}", 0o644)
    mount("none", "/root/opt/g5", None, MS_REMOUNT | MS_RDONLY)
    mount_fs("tmpfs", "/root/run", "tmpfs", MS_NOSUID | MS_NODEV, "mode=755")
    # /dev/vdc stand-in: populated like vm-exec's bootstrap input, then sealed.
    bootstrap_flags = MS_RDONLY | MS_NOSUID | MS_NODEV | MS_NOEXEC
    mount_fs("tmpfs", "/root/run/g5-bootstrap", "tmpfs", MS_NOSUID | MS_NODEV | MS_NOEXEC)
    if profile:
        with open(f"/root/run/g5-bootstrap/{PROFILE_NAME}", "w") as f:
            f.write(PROFILE)
        os.chmod(f"/root/run/g5-bootstrap/{PROFILE_NAME}", 0o644)
    mount("none", "/root/run/g5-bootstrap", None, MS_REMOUNT | bootstrap_flags)
    if os.path.isfile(f"/root/run/g5-bootstrap/{PROFILE_NAME}"):
        mount(f"/root/run/g5-bootstrap/{PROFILE_NAME}", f"/root/opt/g5/{PROFILE_NAME}", None, MS_BIND)
    mount_fs("tmpfs", "/root/dev", "tmpfs")  # devtmpfs stand-in
    mount_fs("proc", "/root/proc", "proc")
    mount_fs("tmpfs", "/root/sys", "tmpfs")  # sysfs stand-in
    os.chdir("/root")
    if not legacy:
        mount(".", "/", None, MS_MOVE)
    os.chroot(".")
    os.chdir("/")
    mount("/var/lib/docker", "/var/lib/docker", None, MS_BIND)
    mount_fs("tmpfs", "/run/g5-trusted", "tmpfs", MS_NOSUID | MS_NODEV, "mode=755,size=64M")
    base = "/run/g5-trusted"
    for name in ["etc", "tmp", "root", "opt", "opt/g5", "run", "run/g5-bootstrap", "work", "image", "dev", "proc", "sys"]:
        os.makedirs(f"{base}/{name}", mode=0o755, exist_ok=True)
    mount("/opt/g5", f"{base}/opt/g5", None, MS_BIND)
    mount("/run/g5-bootstrap", f"{base}/run/g5-bootstrap", None, MS_BIND)
    mount("/work", f"{base}/work", None, MS_BIND)
    mount("/dev", f"{base}/dev", None, MS_BIND | MS_REC)
    mount("/proc", f"{base}/proc", None, MS_BIND)
    mount("/sys", f"{base}/sys", None, MS_BIND | MS_REC)
    mount("/", f"{base}/image", None, MS_BIND)
    for path in ["/opt/g5", "/run", "/dev", "/proc", "/sys", "/var/lib/docker"]:
        flags = MS_BIND | (MS_REC if path != "/run" else 0)
        mount(path, f"{base}/image{path}", None, flags)
    if not legacy:
        mount(f"{base}/image", "/", None, MS_BIND | MS_REC)


def expect(condition, message, failures):
    print(("  ok   " if condition else "  FAIL ") + message)
    if not condition:
        failures.append(message)


def workload_checks(init_pid, profile):
    """Run in a child of the bootstrap: enter the workload as vm-init does."""
    failures = []
    ns = os.open(f"/proc/{init_pid}/ns/mnt", os.O_RDONLY)
    os.setns(ns, os.CLONE_NEWNS)
    os.close(ns)
    os.chdir("/")
    expect(os.path.exists("/.g5-oracle-workload"), "workload root is the workload tree", failures)
    expect(os.readlink("/proc/self/root") == "/", "/proc/self/root of the workload is /", failures)
    # /run is a non-recursive bind: the bootstrap's tmpfs is not mounted under it.
    expect(not os.path.exists("/run/g5-trusted/etc"), "bootstrap root is hidden from the workload", failures)
    expect(not os.path.exists(f"/run/g5-bootstrap/{PROFILE_NAME}"), "bootstrap disk is hidden from the workload", failures)
    expect(os.path.isdir("/var/lib/docker"), "docker data root present", failures)
    if not os.path.exists("/.g5-oracle-workload"):
        # Wrong root (the legacy order): the remaining probes would touch the
        # real machine's /opt/g5 rather than the replayed runtime.
        failures.append("cache profile checks skipped outside the workload tree")
        return failures
    # The host's cache profile reaches the workload at the runtime path, through
    # the recursive /opt/g5 binds, and the runtime stays read-only around it.
    with open(f"/opt/g5/{PROFILE_NAME}") as f:
        seen = f.read()
    if profile:
        expect(seen == PROFILE, "host cache profile is read at /opt/g5/cache-profile.env", failures)
    else:
        expect(seen == "", "without a host profile the placeholder reads as no profile", failures)
    try:
        with open(f"/opt/g5/{PROFILE_NAME}", "a"):
            writable = True
    except OSError as error:
        writable = error.errno != errno.EROFS
    expect(not writable, "cache profile stays read-only in the workload", failures)
    try:
        open("/opt/g5/oracle-probe", "w").close()
        writable = True
    except OSError as error:
        writable = error.errno != errno.EROFS
    expect(not writable, "runtime stays read-only in the workload", failures)
    # A runc-style container: new mount namespace, pivot_root onto its own rootfs.
    ready_read, ready_write = os.pipe()
    container = os.fork()
    if container == 0:
        os.close(ready_read)
        try:
            os.unshare(os.CLONE_NEWNS)
            mount("none", "/", None, MS_REC | MS_PRIVATE)
            rootfs = "/var/lib/docker/oracle-container"
            mount_fs("tmpfs", rootfs, "tmpfs")
            os.makedirs(f"{rootfs}/bin")
            shutil.copy2("/bin/sh", f"{rootfs}/bin/sh")
            open(f"{rootfs}/.g5-oracle-container", "w").close()
            os.chdir(rootfs)
            pivot_root(".", ".")
            check(libc.umount2(b".", MNT_DETACH), "detach old root")
            os.chdir("/")
        except OSError as error:
            print(f"  container setup failed: {error}")
            os.write(ready_write, b"0")
            os._exit(1)
        os.write(ready_write, b"1")
        os.close(ready_write)
        signal.pause()
    os.close(ready_write)
    if os.read(ready_read, 1) != b"1":
        os.waitpid(container, 0)
        failures.append("container could not be created from the workload")
        return failures
    # `docker exec`: join the container's mount namespace from outside.
    probe = os.fork()
    if probe == 0:
        ns = os.open(f"/proc/{container}/ns/mnt", os.O_RDONLY)
        os.setns(ns, os.CLONE_NEWNS)
        found = os.path.exists("/.g5-oracle-container")
        not_workload = not os.path.exists("/.g5-oracle-workload")
        try:
            os.execv("/bin/sh", ["sh", "-c", f"exit {0 if found and not_workload else 1}"])
        except OSError:
            os._exit(2)
    _, status = os.waitpid(probe, 0)
    expect(os.WIFEXITED(status) and os.WEXITSTATUS(status) == 0,
           "joining a container's mount namespace lands in the container (docker exec)", failures)
    os.kill(container, 9)
    os.waitpid(container, 0)
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--legacy", action="store_true", help="replay the chroot-only order; the checks must fail")
    parser.add_argument("--without-profile", action="store_true",
                        help="replay a boot on a host without a cache profile; the placeholder must read as none")
    args = parser.parse_args()
    if sys.platform != "linux" or platform.machine() != "x86_64" or os.geteuid() != 0:
        print("requires x86_64 Linux and root", file=sys.stderr)
        return 2
    shell = read_static_shell()
    profile = not args.without_profile
    os.unshare(os.CLONE_NEWNS)
    escape_to_namespace_root()
    mount("none", "/", None, MS_REC | MS_PRIVATE)
    replay_init(args.legacy, shell, profile)
    init_pid = os.getpid()
    # The trusted bootstrap: chrooted into the platform-owned root, like vm-init.
    result_read, result_write = os.pipe()
    bootstrap = os.fork()
    if bootstrap == 0:
        os.close(result_read)
        os.chdir("/run/g5-trusted")
        os.chroot(".")
        os.chdir("/")
        failures = []
        expect(os.path.exists("/image/.g5-oracle-workload"), "bootstrap still reaches the workload tree at /image", failures)
        expect(os.path.isdir("/run/g5-bootstrap") and os.path.isdir("/opt/g5"), "bootstrap inputs mounted", failures)
        workload = os.fork()
        if workload == 0:
            os.close(result_write)
            os._exit(0 if not workload_checks(init_pid, profile) else 1)
        _, status = os.waitpid(workload, 0)
        if not (os.WIFEXITED(status) and os.WEXITSTATUS(status) == 0):
            failures.append("workload checks failed")
        os.write(result_write, b"0" if not failures else b"1")
        os._exit(0)
    os.close(result_write)
    verdict = os.read(result_read, 1)
    os.waitpid(bootstrap, 0)
    passed = verdict == b"0"
    if args.legacy:
        print("legacy chroot-only order " + ("wrongly passed" if passed else "fails as expected"))
        return 0 if not passed else 1
    print("guest init mount topology " + ("ok" if passed else "FAILED"))
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
