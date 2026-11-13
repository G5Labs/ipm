#!/usr/bin/env bash
# Disposable Linux/KVM acceptance guest. Uses only generated workspace content.
# Caller provides the prepared pristine OCI template and complete runtime bundle.
set -euo pipefail
: "${G5_TEST_VM_ROOT:?owned QA/test VM directory required}"
: "${G5_TEST_TEMPLATE:?completed OCI template directory required}"
: "${G5_TEST_RUNTIME:?complete platform runtime directory required}"
: "${G5D_BIN:?candidate g5d executable required}"
size=${G5_TEST_DISK_MIB:-32768}
owned=$(mktemp -d "$G5_TEST_VM_ROOT/guest-check.XXXXXX")
work=$(mktemp -d "$G5_TEST_VM_ROOT/work-check.XXXXXX")
# vm-exec synchronously reaps its VMM/sidecar before returning. Keep failed
# inputs for diagnosis; never mount or inspect a guest-written disk on the host.
trap 'rm -rf "$work"' EXIT
mkdir -p "$owned/rootfs/.g5"
cp "$(dirname "$0")/test-guest-docker-exec.sh" "$work/docker-exec-check.sh"
cp "$(dirname "$0")/test-guest-nested-kvm.py" "$work/nested-kvm.py"
# With the fixture OCI layout and minimal inner runtime (G5_TEST_OCI_LAYOUT and
# G5_TEST_INNER_RUNTIME, from prepare-cloud-hypervisor-test.py) the acceptance
# also boots a third-level guest through the in-guest vm-exec.
if [ -n "${G5_TEST_OCI_LAYOUT:-}" ] || [ -n "${G5_TEST_INNER_RUNTIME:-}" ]; then
    # Both or neither: one without the other would silently downgrade an
    # intended third-level run to the prerequisite checks.
    [ -n "${G5_TEST_OCI_LAYOUT:-}" ] && [ -n "${G5_TEST_INNER_RUNTIME:-}" ] \
        || { echo "set both G5_TEST_OCI_LAYOUT and G5_TEST_INNER_RUNTIME for the third-level boot, or neither" >&2; exit 1; }
    mkdir -p "$work/nested/runtime"
    cp -a "$G5_TEST_OCI_LAYOUT" "$work/nested/fixture-oci"
    install -m 0644 "$G5_TEST_INNER_RUNTIME/runtime.ext4" "$work/nested/runtime/runtime.ext4"
fi
# Nested virtualization (g5.architecture.microvm_backend.nested) is part of the
# acceptance by default; G5_TEST_NESTED_VIRTUALIZATION=0 qualifies a host that
# keeps it off.
nested=${G5_TEST_NESTED_VIRTUALIZATION:-1}
nested_args=()
if [ "$nested" = 1 ]; then nested_args=(--nested-virtualization); fi
# Host cache profile (ontology g5.runtime.cache.profile): when the caller's
# runtime directory publishes one, the guest reads it byte for byte at the
# runtime path; otherwise it reads the shipped disk's empty placeholder.
if [ -f "$G5_TEST_RUNTIME/cache-profile.env" ]; then
    cp "$G5_TEST_RUNTIME/cache-profile.env" "$work/expected-cache-profile"
else
    : > "$work/expected-cache-profile"
fi
cat > "$work/check.sh" <<'GUEST'
#!/bin/bash
set -euo pipefail
[ "$(findmnt -n -o FSTYPE -T /work)" = ext4 ]
[ "$(findmnt -n -o FSTYPE -T /tmp)" = ext4 ]
[ "$(findmnt -n -o FSTYPE -T /var/lib/docker)" = ext4 ]
# The pre-existing Docker hook checks the exact mountpoint, not findmnt -T.
[ "$(findmnt -n -o FSTYPE /var/lib/docker)" = ext4 ]
if touch /opt/g5/should-not-write 2>/dev/null; then echo 'runtime is writable' >&2; exit 1; fi
[ "$(findmnt -n -o FSTYPE -T /opt/g5)" = ext4 ]
cmp /opt/g5/cache-profile.env /work/expected-cache-profile
if echo forbidden >> /opt/g5/cache-profile.env 2>/dev/null; then echo 'cache profile is writable' >&2; exit 1; fi
# The workspace name reaches the guest as its hostname (issue #685).
[ "$(hostname)" = guest-check ]
[ "$(uname -n)" = guest-check ]
[ "$(cat /etc/hostname)" = guest-check ]
grep -q '^127.0.1.1 guest-check$' /etc/hosts
mkdir /work/first /work/second
for repo in first second; do
    git -C "/work/$repo" init -q
    git -C "/work/$repo" config user.name 'QA fixture'
    git -C "/work/$repo" config user.email 'qa@example.invalid'
    printf 'base\n' > "/work/$repo/file"
    git -C "/work/$repo" add file
    git -C "/work/$repo" commit -qm base
    git -C "/work/$repo" checkout -qb work
    printf 'edit\n' >> "/work/$repo/file"
    test -n "$(git -C "/work/$repo" status --porcelain)"
done
python3 - <<'PY'
import os, pty, socket, subprocess
master, slave = pty.openpty()
p = subprocess.Popen(['/bin/sh', '-c', 'printf PTY_OK'], stdin=slave, stdout=slave, stderr=slave)
os.close(slave)
assert b'PTY_OK' in os.read(master, 1024)
assert p.wait(timeout=5) == 0
os.close(master)
# The shipped executable must re-exec as g5d, not ld.so. Exercise the real
# module lifetime entrypoint and its exit receipt inside the guest boundary.
assert subprocess.check_output(['/opt/g5/bin/g5d', '--version'], text=True).startswith('g5d ')
parent, child = socket.socketpair()
parent.settimeout(10)
p = subprocess.Popen(['/opt/g5/bin/g5d', '__g5-execution-owner', str(child.fileno()),
    '-', '-', '/bin/sh', '-ec',
    'test "$(readlink /proc/$PPID/exe)" = /opt/g5/bin/g5d; echo G5_SELF_EXEC_OK'],
    pass_fds=[child.fileno()], stdout=subprocess.PIPE, text=True)
child.close()
assert parent.recv(1) == bytes([0xa7])
out, _ = p.communicate(timeout=10)
assert p.returncode == 0 and 'G5_SELF_EXEC_OK' in out
parent.close()
print('G5_SELF_EXEC_AND_REAP_OK')
PY
/opt/g5/bootstrap/git --version
curl --fail --max-time 20 -s -o /dev/null https://example.com
if [ -x /usr/local/bin/g5-dev-dockerd-start ]; then
    /usr/local/bin/g5-dev-dockerd-start
    [ "$(docker info --format '{{.Driver}}')" = overlay2 ]
    mkdir -p /work/container/bin
    cp /opt/g5/bootstrap/busybox /work/container/bin/busybox
    tar -C /work/container -cf - . | docker import - g5-guest-check:local >/dev/null
    docker run --rm g5-guest-check:local /bin/busybox sh -c 'echo DOCKER_OK; echo private > /tmp-value; test "$(/bin/busybox cat /tmp-value)" = private'
    if [ "$G5_TEST_NESTED_VIRTUALIZATION" = 1 ]; then
        timeout -k 5 30 python3 /work/nested-kvm.py --expect-present
        G5_TEST_EXPECT_KVM=1 timeout -k 5 180 bash /work/docker-exec-check.sh
        # The real g5dev image as an inner host: Landlock, the platform from
        # the runtime disk, the egress policy and the reflink root (hard), and
        # the inner vm-exec when the caller supplies the fixture layout.
        if [ -d /work/nested/fixture-oci ]; then
            timeout -k 5 600 python3 /work/nested-kvm.py --vm-exec /work/nested
        else
            timeout -k 5 120 python3 /work/nested-kvm.py --inner-host-prereqs
        fi
    else
        timeout -k 5 30 python3 /work/nested-kvm.py --expect-absent
        timeout -k 5 180 bash /work/docker-exec-check.sh
    fi
else
    echo 'Docker acceptance requires the G5 development environment hook' >&2
    exit 1
fi
echo G5_CH_GUEST_CHECK_OK
GUEST
export G5D_INTERNAL_OWNED_WORKLOAD_ROOT="$owned/rootfs"
export G5D_INTERNAL_WORKLOAD_TEMPLATE="$G5_TEST_TEMPLATE"
"$G5D_BIN" vm-exec --rootfs "$owned/rootfs" --cpus 2 --mem-mib 4096 \
    --scratch-disk-mib "$size" --block-workload "${nested_args[@]}" \
    -e G5D_WORKSPACE_HOSTNAME=guest-check -e "G5_TEST_NESTED_VIRTUALIZATION=$nested" \
    -e "G5_NESTED_BOOT_STRICT=${G5_NESTED_BOOT_STRICT:-0}" \
    -V "$work:/work" -V "$G5_TEST_RUNTIME:/opt/g5:ro" -- /bin/bash /work/check.sh
rm -rf "$owned"
# A nonzero guest command must preserve its status through PID 1 and the VMM
# supervisor, after both host children have been reaped.
owned=$(mktemp -d "$G5_TEST_VM_ROOT/exit-check.XXXXXX")
mkdir -p "$owned/rootfs/.g5"
export G5D_INTERNAL_OWNED_WORKLOAD_ROOT="$owned/rootfs"
status=0
"$G5D_BIN" vm-exec --rootfs "$owned/rootfs" --cpus 1 --mem-mib 512 \
    --scratch-disk-mib "$size" --block-workload \
    -V "$work:/work" -V "$G5_TEST_RUNTIME:/opt/g5:ro" -- /bin/sh -c 'exit 17' || status=$?
[ "$status" = 17 ] || { echo "guest exit status was $status, expected 17" >&2; exit 1; }
rm -rf "$owned"
echo 'Cloud Hypervisor guest acceptance: PASS'
