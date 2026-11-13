---
name: guest-g5d-lags-api-visibility
description: "The VM's /opt/g5/bin/g5d dispatcher can predate the deployed API; Files-tree visibility changes need both, or live listings silently come back empty."
metadata:
  node_type: memory
  type: project
  originSessionId: 53ba4932-e056-4e27-9b2b-f40807be3457
  modified: 2026-10-20T15:05:58.594Z
---

On 2026-10-20 (QA), the Core Ontology sidebar showed "No ontology files" in a running workspace. The code repo's live listing returned `entries: []` with a real `repoOid`. The API already had #606, which made `.g5/ontology` visible in the Files tree. The guest `g5d` dispatcher (built 2026-10-19 18:58 UTC) predated #606, which merged at 2026-10-20 01:17 UTC. The dispatcher's `fs.tree` applies `g5_contracts::g5_paths::is_files_tree_visible_path` itself, so the stale binary skipped every ontology entry.

**Why:** visibility is enforced in two binaries that ship separately: the API, and the guest runtime bundle under `/opt/g5`.

**How to apply:**
- A live listing with `source: "live"`, a non-null `repoOid` and empty `entries` points at the dispatcher's walk.
- `repoOid: null` with empty `entries` means the root was missing (`FILE_MISSING`), for example a repo without `.g5/ontology`.
- Read-only diagnosis works with `sudo -n`: the dispatcher is `/opt/g5/bin/g5d dispatch` (check `/proc/<pid>/fd` and `mountinfo`), and the repo roots are in `/var/lib/g5d/journal` (`root_path`).
- Strings in `g5d` can't date the build; the compiler inlines short prefix literals. Compare the mtime of `/opt/g5/bin/g5d` with the merge time instead.
- Reproduce `fs.tree` with an ignored test that calls `service(path).call("fs.tree", …)` in `g5d-dispatch/src/files.rs`, in a scratch worktree. See [[rust-and-integration-test-setup]].
