#!/usr/bin/env bash
# Run as root on a disposable Linux worker after installing the pinned passt.
# Uses test-owned paths and processes; never reloads or disables a host policy.
set -euo pipefail
[[ $(id -u) == 0 && $(uname -s) == Linux ]]
uid=$(id -u g5-passt)
gid=$(id -g g5-passt)
[[ "$uid" != 0 && "$gid" != 0 ]]
test_parent=${G5_TEST_VM_ROOT:-/var/lib/g5d/rootfs}
test_root=$(mktemp -d "$test_parent/sandbox-test-XXXXXXXX")
child=
cleanup() {
    if [[ -n "$child" ]]; then
        kill "$child" 2>/dev/null || true
        wait "$child" 2>/dev/null || true
    fi
    rm -rf -- "$test_root"
}
trap cleanup EXIT
chmod 0711 "$test_root"
install -d -o "$uid" -g "$gid" -m 0700 "$test_root/net"
args=(--foreground --quiet --one-off --vhost-user -4 -t none -u none
    --socket "$test_root/net/net.sock" --pid "$test_root/net/pid")
setpriv --reuid="$uid" --regid="$gid" --clear-groups \
    env -i PATH=/usr/bin:/bin /usr/bin/passt "${args[@]}" \
    >"$test_root/log" 2>&1 &
child=$!
for attempt in $(seq 1 200); do
    kill -0 "$child"
    [[ -s "$test_root/net/pid" ]] && break
    sleep 0.025
done
[[ $(cat "$test_root/net/pid") == "$child" ]]
python3 - "$child" "$uid" <<'PY'
import pathlib, sys, time

pid, uid = sys.argv[1:]
proc = pathlib.Path('/proc') / pid
deadline = time.monotonic() + 5
while True:
    status = dict(line.split(':', 1) for line in (proc / 'status').read_text().splitlines())
    if status['NoNewPrivs'].strip() == '1' and status['Seccomp'].strip() == '2':
        break
    assert time.monotonic() < deadline, 'passt did not finish sandbox setup'
    time.sleep(0.01)
assert status['Uid'].split() == [uid] * 4, 'passt is not fully unprivileged'
assert int(status['CapEff'].strip(), 16) == 0, 'passt retained effective capabilities'
assert int(status['CapPrm'].strip(), 16) == 0, 'passt retained permitted capabilities'
assert status['NoNewPrivs'].strip() == '1', 'no_new_privs missing'
assert status['Seccomp'].strip() == '2', 'seccomp filter missing'
assert (proc / 'attr/current').read_text().strip() == 'passt (enforce)', 'AppArmor not enforcing'
assert (proc / 'ns/mnt').readlink() != pathlib.Path('/proc/self/ns/mnt').readlink(), 'host mount namespace retained'
assert list((proc / 'root').iterdir()) == [], 'passt retained host filesystem visibility'
print('PASST_SANDBOX_OK: non-root, no capabilities, seccomp, AppArmor, empty root')
PY
kill "$child"
wait "$child" 2>/dev/null || true
child=
rm -f "$test_root/net/net.sock" "$test_root/net/net.sock.repair" "$test_root/net/pid"

# Give DAC permission deliberately: the unchanged sentinel demonstrates that
# AppArmor refuses files outside the exact communication-file allowance.
printf 'unchanged\n' > "$test_root/net/forbidden"
chown "$uid:$gid" "$test_root/net/forbidden"
chmod 0600 "$test_root/net/forbidden"
set +e
timeout 5 setpriv --reuid="$uid" --regid="$gid" --clear-groups \
    env -i PATH=/usr/bin:/bin /usr/bin/passt "${args[@]}" \
    --log-file "$test_root/net/forbidden" >"$test_root/negative-log" 2>&1
result=$?
set -e
[[ "$result" != 0 && "$result" != 124 ]]
[[ $(cat "$test_root/net/forbidden") == unchanged ]]
grep -q 'Permission denied' "$test_root/negative-log"
echo 'PASST_PATH_REFUSAL_OK: unrelated owner-writable file remains unchanged'
