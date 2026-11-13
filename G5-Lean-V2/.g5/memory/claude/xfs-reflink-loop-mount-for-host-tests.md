---
name: xfs-reflink-loop-mount-for-host-tests
description: g5d-host and host-boot tests need reflink; the VM's virtiofs root lacks it, but sudo + xfsprogs + a loop image at /mnt/xfs with TMPDIR works.
metadata:
  type: reference
---

The workspace materializer is reflink-or-fail-loud, so every `g5d-host` test that
materializes a root (boot retry, retained root, the real process-mode boot in
`backend/g5d/tests/process_host.rs`) fails on this VM's virtiofs root with
"reflinks unsupported by host filesystem". It is environmental, not a regression.

**How to apply:** `sudo apt-get update -qq && sudo apt-get install -y xfsprogs`
(the first install attempt fails until the index is refreshed), then
`truncate -s 8G /tmp/g5-xfs.img && sudo mkfs.xfs -f -q -m reflink=1,rmapbt=0 -i nrext64=0 /tmp/g5-xfs.img && sudo mkdir -p /mnt/xfs && sudo mount -o loop /tmp/g5-xfs.img /mnt/xfs && sudo chown 1000:1000 /mnt/xfs`,
and run tests with `TMPDIR=/mnt/xfs G5_TEST_REFLINK_ROOT=/mnt/xfs`. The
`workspace_vm.rs` boot-retry tests use `G5_TEST_REFLINK_ROOT`, while
`tempfile::tempdir()` users use `TMPDIR`. Mirrors `scripts/ci-host-xfs-entrypoint.sh`. sudo is available in the MicroVM, but
loop devices are not guaranteed: on 2026-10-21 (issue #685 workspace) the guest kernel
had no `/dev/loop-control`, no `loop` entry in `/proc/devices` and no `modprobe`, so
`mount -o loop` failed and the reflink tests could not run at all. Check
`ls /dev/loop-control` first; if absent, confirm the failures on the parent commit in a
temporary worktree (`CARGO_TARGET_DIR=/work/code/target` reuses the build) and report
them as environmental. Related: [[rust-and-integration-test-setup]].
