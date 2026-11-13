---
name: runtime-bundle-not-on-path
description: "/opt/g5/bin is the read-only runtime bundle; new guests append it after image tools, while older guests may require absolute paths."
metadata:
  node_type: memory
  type: reference
  originSessionId: f58d3807-7811-4b92-b086-ad7fc3f9cbee
  modified: 2026-10-07T00:04:45.454Z
---

`/opt/g5/bin` is the platform runtime bundle, virtiofs-mounted read-only into every
workspace VM (`g5v1 /opt/g5 virtiofs ro`). It holds `gcloud`, `gh`, `kubectl`,
`gke-gcloud-auth-plugin`, `cargo-nextest`, `node`, `claude`, `codex`, `just`, `npm`,
`pnpm`, `python3`, `cargo`, `rustc` and the real `g5d` binary.

Guests running the runtime-tools PATH change append this directory after the image's
directories in terminal and agent environments. The `g5d` launcher directories
(`/var/lib/g5d/bin` and `~/.g5/bin`) stay ahead of the bundle, and only platform
processes receive its native loader path. See the owning
[runtime contract](../../ontology/software_architecture/runtime.g5.md#the-bundles-tools-are-reachable-without-displacing-the-image).

**Older guests can still lack the bundle on PATH**, as observed on 2026-10-07 before
this change was deployed. There, invoke tools by absolute path or use
`PATH="$PATH:/opt/g5/bin" <cmd>` (append, never prepend). A repository update alone
does not replace an already-running guest runtime.

Two gotchas:

- The bundle's wrapper scripts (`gcloud`, `npm`, `npx`, `pnpm`, `python3`, `cargo`,
  `rustc`, `codex`) resolve siblings via `dirname "$0"` without `readlink -f`, so
  symlinking them into another directory breaks them. Use a forwarder shim
  (`exec /opt/g5/bin/gcloud "$@"`) or the absolute path.
- In that older g5dev guest, `gcloud`, `gh`, `kubectl`, `gke-gcloud-auth-plugin` and
  `cargo-nextest` were missing from PATH; the image already supplied `node`, `npm`, `npx`,
  `cargo`, `rustc`, `just`, `python3` and `corepack` in `/usr/local/bin`.

The shell tools were observed to start with `LD_LIBRARY_PATH` unset in that x86_64
guest. Continue invoking `g5d` through its generated launcher, which supplies its
own loader environment. Related: [[cloudbuild-log-reader-sa]].
