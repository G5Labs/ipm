---
name: uutils-mkdir-not-exclusive
description: Ubuntu 26.04's uutils mkdir reports success to every caller racing to create one directory, so mkdir-based locks are not exclusive there.
metadata:
  type: reference
---

Ubuntu 26.04 ships uutils coreutils (`mkdir --version` → `mkdir (uutils
coreutils) 0.8.0`, `/usr/lib/cargo/bin/coreutils/mkdir`), in both the G5
MicroVM and the CI builder image. When several processes run `mkdir d` at the
same moment, **all of them can succeed** (143/200 two-way races on
2026-10-10). It still fails correctly against a directory that already
exists, so sequential tests do not reveal it.

**Why it matters:** `scripts/ensure-base-images.sh` used `mkdir` as its lock.
Concurrent e2e fixture seeds then failed with "could not initialize
base-image lock ownership", which skipped provider-agent Playwright and the
value-loop receipt (PR #382, builds fd345c52/2d7f8a6b). Fixed in `c30b0eac`:
exclusive `set -C` (O_EXCL) creation of the owner file decides the holder.

**How to apply:** do not rely on `mkdir`, and verify `ln`/`mv` before relying
on them, for mutual exclusion in scripts that run on these images. Use bash
noclobber (`( set -C; > file )`), or `/usr/bin/gnumkdir` where it exists.
This VM's `/tmp` is FUSE (`fuseblk`); reproduce races on `/dev/shm` (tmpfs)
to rule out the filesystem.
