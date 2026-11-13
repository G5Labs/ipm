#!/usr/bin/env python3
"""Nested-virtualization probes, run inside a disposable guest booted by the
candidate kernel/initramfs/g5d (``scripts/test-cloud-hypervisor-ci.sh`` and the
full-environment acceptance). Governing contract:
``g5.architecture.microvm_backend.nested``.

``--expect-present``  hard gate for a guest booted with the switch on: the CPU
                      shows ``vmx``/``svm``, ``/dev/kvm`` is a 0666 character
                      device, an unprivileged user opens it read-write and
                      ``KVM_GET_API_VERSION`` answers 12.
``--expect-absent``   hard negative control for the switch off: no VMX/SVM flag
                      and no ``/dev/kvm``.
``--inner-host-prereqs``
                      hard checks for an inner host inside this guest: a
                      Landlock ruleset can be created (Cloud Hypervisor's
                      ``--landlock`` is a hard requirement), the platform's
                      ``passt`` under ``/opt/g5/platform`` runs, the exact passt
                      egress policy installs through ``nft``, and a loop-mounted
                      XFS image gives the reflink root a microvm host requires.
``--vm-exec DIR``     the prerequisites above, then a third-level boot through
                      the real path: ``/opt/g5/bin/g5d vm-exec`` with the
                      platform from the runtime disk (``--landlock``, the
                      ``g5-vmm`` and ``g5-passt`` identities, the egress
                      policy), a workload template prepared from DIR's
                      ``fixture-oci`` layout on the reflink root, and DIR's
                      ``runtime/runtime.ext4``, the minimal fixture runtime
                      (never this guest's own runtime disk, which a Packer
                      gate's baked runtime makes several GB). Staging failures
                      are reported, not crashes. Report-only unless
                      ``G5_NESTED_BOOT_STRICT=1`` (the worker-image README
                      records three-deep boots as unreliable on C3 build
                      workers), except that a failure naming Landlock or the
                      passt launch is a platform defect and fails the gate
                      regardless.
"""
import argparse
import ctypes
import fcntl
import grp
import os
import pwd
import re
import shutil
import stat
import subprocess
import sys
import time

KVM_GET_API_VERSION = 0xAE00
KVM_API_VERSION = 12
UNPRIVILEGED = 65534
BOOT_TIMEOUT_SECONDS = 240
PLATFORM = "/opt/g5/platform"
GUEST_G5D = "/opt/g5/bin/g5d"
# Where the probe builds the inner host's root: the guest's ext4 workload
# disk (/tmp lives on it), never tmpfs, so the inner template and runtime
# copy do not come out of guest memory.
INNER_ROOT = "/tmp/nested-l3"
INNER_IMAGE_GIB = 3
INNER_WORKLOAD_MIB = 512
SYS_LANDLOCK_CREATE_RULESET = 444
LANDLOCK_CREATE_RULESET_VERSION = 1
PROBE_PATH = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
# A failure that names these is the platform (Landlock ruleset, the passt
# launch, the egress policy), not boot flakiness. A guest that merely times
# out after passt started is left to the strict switch.
# The passt alternatives are the launcher's own messages (passt.rs:
# "passt exited before readiness", "passt readiness timed out",
# "passt startup: …", "launch passt") and the egress installer's.
PLATFORM_DEFECT = re.compile(
    r"landlock|cannot (?:install passt policy|start nftables)|missing non-root g5-passt"
    r"|installer-owned g5-vmm|launch passt|passt (?:exited before readiness|readiness timed out|startup|must use a non-root identity)"
    r"|passt.*(?:No such file|not found|Permission denied|Operation not permitted)"
    r"|VM network backend exited: exit status: [1-9]",
    re.IGNORECASE)


def cpu_flag():
    with open("/proc/cpuinfo", encoding="ascii", errors="replace") as cpuinfo:
        for line in cpuinfo:
            if line.startswith("flags"):
                flags = set(line.split(":", 1)[1].split())
                return "vmx" if "vmx" in flags else "svm" if "svm" in flags else None
    return None


def drop_privileges():
    os.setgroups([])
    os.setgid(UNPRIVILEGED)
    os.setuid(UNPRIVILEGED)


def open_kvm_unprivileged():
    """Open /dev/kvm read-write as an unprivileged user in a child and query the API version."""
    read_end, write_end = os.pipe()
    pid = os.fork()
    if pid == 0:
        try:
            os.close(read_end)
            drop_privileges()
            fd = os.open("/dev/kvm", os.O_RDWR | os.O_CLOEXEC)
            version = fcntl.ioctl(fd, KVM_GET_API_VERSION)
            os.write(write_end, str(version).encode())
            os._exit(0)
        except OSError as error:
            os.write(write_end, f"error:{error}".encode())
            os._exit(1)
    os.close(write_end)
    with os.fdopen(read_end) as reader:
        answer = reader.read()
    _, status = os.waitpid(pid, 0)
    return status, answer


def expect_present():
    flag = cpu_flag()
    assert flag, "guest CPU shows neither vmx nor svm; was the VMM launched with nested=on?"
    info = os.stat("/dev/kvm")
    assert stat.S_ISCHR(info.st_mode), "/dev/kvm is not a character device"
    mode = stat.S_IMODE(info.st_mode)
    assert mode == 0o666, f"/dev/kvm mode is {mode:o}, expected 666 (trusted init)"
    assert info.st_uid == 0 and info.st_gid == 0, "/dev/kvm must stay root:root"
    status, answer = open_kvm_unprivileged()
    assert status == 0 and answer == str(KVM_API_VERSION), (
        f"unprivileged /dev/kvm open or KVM_GET_API_VERSION failed: status={status} answer={answer!r}")
    print(f"NESTED_KVM_PRESENT_OK flag={flag} mode={mode:o} api_version={answer}")


def expect_absent():
    assert cpu_flag() is None, "guest CPU shows a virtualization flag although nested is off"
    assert not os.path.exists("/dev/kvm"), "/dev/kvm exists although nested is off"
    print("NESTED_KVM_ABSENT_OK")


def captured_text(buffer):
    if buffer is None:
        return ""
    return buffer.decode("utf-8", errors="replace") if isinstance(buffer, bytes) else buffer


def run(command, **kwargs):
    env = dict(kwargs.pop("env", os.environ), PATH=PROBE_PATH)
    return subprocess.run(command, env=env, capture_output=True, text=True, **kwargs)


def check(command, what, **kwargs):
    completed = run(command, **kwargs)
    assert completed.returncode == 0, (
        f"{what} failed (exit {completed.returncode}): {command[0]}\n"
        f"stdout: {completed.stdout.strip()[-1024:]}\nstderr: {completed.stderr.strip()[-1024:]}")
    return completed


def landlock_abi():
    libc = ctypes.CDLL(None, use_errno=True)
    libc.syscall.restype = ctypes.c_long
    abi = libc.syscall(ctypes.c_long(SYS_LANDLOCK_CREATE_RULESET), None, ctypes.c_size_t(0),
                       ctypes.c_uint32(LANDLOCK_CREATE_RULESET_VERSION))
    if abi < 0:
        error = ctypes.get_errno()
        raise AssertionError(
            f"landlock_create_ruleset failed: {os.strerror(error)} (errno {error}); "
            "the guest kernel lacks CONFIG_SECURITY_LANDLOCK or an LSM list with landlock, "
            "so Cloud Hypervisor's --landlock cannot start a guest here")
    return abi


def nft_policy(uid, host_ports):
    """The exact text of backend/g5d-isolation/src/isolation/guest_egress.rs::nft_policy."""
    assert uid != 0 and 0 not in host_ports
    table = f"g5_guest_{uid}"
    rules = (f"add table inet {table}\ndelete table inet {table}\n"
             f"table inet {table} {{\n"
             "chain output { type filter hook output priority 0; policy accept;\n"
             f"meta skuid {uid} jump guest\n}}\n"
             "chain guest {\n"
             "ct direction reply accept\n"
             "meta l4proto { tcp, udp } ct original proto-dst 53 accept\n")
    if host_ports:
        ports = ", ".join(str(port) for port in sorted(host_ports))
        rules += f"ip daddr 127.0.0.1 tcp dport {{ {ports} }} accept\n"
    rules += ("ip daddr { 0.0.0.0/8, 127.0.0.0/8, 169.254.0.0/16 } reject\n"
              "ip6 daddr { ::/128, ::1/128, fe80::/10, fd00:ec2::254/128, fd20:ce::254/128 } reject\n"
              "fib daddr type local reject\n}\n}\n")
    return rules


def inner_host_prereqs():
    """Hard: Landlock, the platform's passt, the egress policy, a reflink root."""
    abi = landlock_abi()
    print(f"NESTED_LANDLOCK_OK abi={abi}")
    for name in ("cloud-hypervisor", "vmlinux", "initramfs.cpio", "kernel.config", "passt"):
        assert os.path.isfile(os.path.join(PLATFORM, name)), f"runtime disk ships no {PLATFORM}/{name}"
    with open(os.path.join(PLATFORM, "kernel.config"), encoding="ascii", errors="replace") as config:
        assert "CONFIG_SECURITY_LANDLOCK=y\n" in config.read(), "the platform kernel config lacks Landlock"
    passt = check([os.path.join(PLATFORM, "passt"), "--version"], "platform passt")
    passt_user = pwd.getpwnam("g5-passt")
    vmm_user = pwd.getpwnam("g5-vmm")
    assert passt_user.pw_uid != 0 and passt_user.pw_gid != 0, "g5-passt must be unprivileged"
    assert vmm_user.pw_uid != 0 and vmm_user.pw_gid != 0, "g5-vmm must be unprivileged"
    kvm_group = grp.getgrnam("kvm")
    assert "g5-vmm" in kvm_group.gr_mem or vmm_user.pw_gid == kvm_group.gr_gid, "g5-vmm must be in the kvm group"
    check(["/usr/sbin/nft", "-f", "-"], "passt egress policy", input=nft_policy(passt_user.pw_uid, {7500}))
    check(["/usr/sbin/nft", "delete", "table", "inet", f"g5_guest_{passt_user.pw_uid}"], "egress policy removal")
    print(f"NESTED_PASST_POLICY_OK passt={passt.stdout.strip().splitlines()[0] if passt.stdout.strip() else 'ok'}")
    root = reflink_root()
    print(f"NESTED_REFLINK_ROOT_OK root={root}")
    return root


def reflink_root():
    """Loop-mount a sparse XFS image on the guest disk; prove the reflink clone the host makes."""
    image = INNER_ROOT + ".xfs"
    os.makedirs(INNER_ROOT, exist_ok=True)
    if not os.path.ismount(INNER_ROOT):
        if not os.path.exists(image):
            with open(image, "wb") as handle:
                handle.truncate(INNER_IMAGE_GIB * 1024 ** 3)
            check(["mkfs.xfs", "-q", "-m", "reflink=1", image], "mkfs.xfs")
        # Older libmount holds a writable loop FD through mount(2), which
        # conflicts with the guest kernel's blocked raw writes to mounted
        # devices. Let losetup exit and close that FD before mounting XFS.
        loop = check(["losetup", "--find", "--show", image], "allocate XFS loop device").stdout.strip()
        try:
            check(["mount", "-t", "xfs", "-o", "noatime", loop, INNER_ROOT], "loop mount of the XFS image")
        except BaseException:
            try:
                check(["losetup", "--detach", loop], "release XFS loop device")
            except (OSError, AssertionError) as error:
                print(f"Loop cleanup also failed: {error}", file=sys.stderr)
            raise
        # A mounted loop is detached lazily: AUTOCLEAR releases it on unmount.
        check(["losetup", "--detach", loop], "mark XFS loop device for cleanup")
    # The clone is proven on every run, an existing mount included: the
    # reflink prerequisite is hard, whatever mounted the root.
    probe = os.path.join(INNER_ROOT, "probe")
    with open(probe, "w", encoding="ascii") as handle:
        handle.write("probe")
    check(["cp", "--reflink=always", probe, probe + ".clone"], "reflink clone on the XFS root")
    os.unlink(probe)
    os.unlink(probe + ".clone")
    return INNER_ROOT


def stage_inner_host(directory, root):
    """Template, runtime and owned directory for the inner vm-exec on the reflink root."""
    layout = os.path.join(directory, "fixture-oci")
    assert os.path.isfile(os.path.join(layout, "index.json")), f"{layout} is not an OCI layout"
    source_runtime = os.path.join(directory, "runtime/runtime.ext4")
    assert os.path.isfile(source_runtime), f"{source_runtime} is missing (the minimal inner runtime)"
    state = os.path.join(root, "state")
    env = dict(os.environ, G5D_ROOTFS_STATE_DIR=state, G5D_OCI_CACHE_DIR=os.path.join(root, "layers"),
               G5D_CLOUD_HYPERVISOR_BIN=os.path.join(PLATFORM, "cloud-hypervisor"),
               G5D_GUEST_KERNEL=os.path.join(PLATFORM, "vmlinux"),
               G5D_GUEST_INITRAMFS=os.path.join(PLATFORM, "initramfs.cpio"),
               G5D_PASST_BIN=os.path.join(PLATFORM, "passt"))
    # The inner template, prepared by the in-guest g5d through the real OCI ->
    # ext4 pipeline on the reflink root.
    check([GUEST_G5D, "oci", "prepare-workload", layout, "--size-mib", str(INNER_WORKLOAD_MIB)],
          "inner workload template preparation", env=env)
    templates = os.listdir(os.path.join(state, "templates/sha256"))
    assert len(templates) == 1, f"expected one inner template, found {templates}"
    template = os.path.join(state, "templates/sha256", templates[0])
    # The inner runtime is the minimal fixture runtime staged into DIR, copied
    # onto the reflink root where the unprivileged VMM can read it (DIR is the
    # imported /work, private to root).
    runtime = os.path.join(root, "runtime")
    os.makedirs(runtime, exist_ok=True)
    os.chmod(runtime, 0o755)
    shutil.copyfile(source_runtime, os.path.join(runtime, "runtime.ext4"))
    os.chmod(os.path.join(runtime, "runtime.ext4"), 0o644)
    owned = os.path.join(root, "vm")
    work = os.path.join(root, "l3-work")
    shutil.rmtree(owned, ignore_errors=True)
    os.makedirs(os.path.join(owned, "rootfs/.g5"))
    os.makedirs(work, exist_ok=True)
    env.update(G5D_INTERNAL_OWNED_WORKLOAD_ROOT=os.path.join(owned, "rootfs"),
               G5D_INTERNAL_WORKLOAD_TEMPLATE=template)
    command = [GUEST_G5D, "vm-exec", "--rootfs", os.path.join(owned, "rootfs"),
               "--cpus", "1", "--mem-mib", "512", "--scratch-disk-mib", str(INNER_WORKLOAD_MIB),
               "--block-workload", "-V", f"{work}:/work", "-V", f"{runtime}:/opt/g5:ro",
               "--", "/bin/sh", "-c", "echo L3_OK"]
    return command, env


def vm_exec(directory):
    strict = os.environ.get("G5_NESTED_BOOT_STRICT") == "1"
    started = time.monotonic()
    root = inner_host_prereqs()
    outcome = "fail"
    output = ""
    try:
        # Staging (template preparation, the runtime copy, the owned directory)
        # is a probe result, never a crash of the guest check.
        command, env = stage_inner_host(directory, root)
        completed = run(command, env=env, timeout=BOOT_TIMEOUT_SECONDS)
        output = completed.stdout + completed.stderr
        detail = f"vm_exec_exit={completed.returncode}"
        if completed.returncode == 0 and "L3_OK" in completed.stdout:
            outcome = "pass"
        if completed.stderr.strip():
            detail += f" stderr={completed.stderr.strip()[-512:]!r}"
    except subprocess.TimeoutExpired as expired:
        # TimeoutExpired carries bytes even under text=True; the tail and the
        # platform-defect match need what the boot printed before the timeout.
        output = captured_text(expired.stdout) + captured_text(expired.stderr)
        detail = f"timeout after {BOOT_TIMEOUT_SECONDS}s"
    except (OSError, AssertionError) as error:
        output = str(error)
        detail = f"staging error: {error}"
    elapsed = time.monotonic() - started
    hard = outcome != "pass" and bool(PLATFORM_DEFECT.search(output))
    print(f"NESTED_VM_EXEC_PROBE={outcome} elapsed_s={elapsed:.1f} strict={int(strict)} platform_defect={int(hard)} {detail}")
    if outcome != "pass":
        tail = output.splitlines()[-40:]
        print("--- inner vm-exec output tail ---")
        print("\n".join(tail) if tail else "(no output)")
        if strict or hard:
            sys.exit(1)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--expect-present", action="store_true")
    group.add_argument("--expect-absent", action="store_true")
    group.add_argument("--inner-host-prereqs", action="store_true")
    group.add_argument("--vm-exec", metavar="DIR")
    args = parser.parse_args()
    if args.expect_present:
        expect_present()
    elif args.expect_absent:
        expect_absent()
    elif args.inner_host_prereqs:
        inner_host_prereqs()
    else:
        vm_exec(args.vm_exec)


if __name__ == "__main__":
    main()
