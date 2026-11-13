---
name: g5-api-test-binaries-fill-disk
description: g5-api integration tests are one `integration` target since #585 (commit 65af712c); select files by module prefix, not `--test <file>`. Older branches still have ~140 binaries that fill the VM disk.
metadata:
  node_type: memory
  type: reference
  originSessionId: 7a49315d-fa87-44cb-82e6-63d3c9bf43a2
  modified: 2026-10-16T14:51:40.180Z
---

Since #585 (commit `65af712c`, in main by 2026-10-16) `backend/g5-api` sets
`autotests = false` with one `[[test]] name = "integration"` rooted at
`tests/main.rs`, which declares every test file as a `mod`. A new test file needs
a `mod` line there.

**How to apply:** select a file's tests by module prefix:
`cargo test -p g5-api --test integration -- remote_tracking_background_loop::`
(or nextest `-E 'test(/^remote_tracking_background_loop::/)'`). `--test <file>`
now fails with "available test targets: integration".

Before #585, each of the ~144 files was its own ~550 MB binary;
`cargo build -p g5-api --tests` needed 55+ GB and failed with
`ld ... signal 7 [Bus error]` on the 63 GB VM. On a branch older than `65af712c`,
build only named `--test` targets, and if the disk fills delete the extensionless
executables in `target/debug/deps`. Related: [[rust-and-integration-test-setup]].
