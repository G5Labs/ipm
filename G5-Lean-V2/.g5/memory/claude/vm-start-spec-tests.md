---
name: vm-start-spec-tests
description: "Asserting on a workspace VM's Start spec in g5-api tests: TestWorkspaceRuntimePeer::attach fakes a running VM (no vm.start); an empty member root_path drops the personal mount."
metadata:
  node_type: memory
  type: feedback
  originSessionId: 084cda88-014d-41b9-979a-eeef245cc471
  modified: 2026-10-20T05:41:03.957Z
---

Two traps made a "the review VM has no personal mount" test pass vacuously (PR #645, 2026-10-20):

1. `TestWorkspaceRuntimePeer::attach(&app, &ws, owner)` inserts a **running** VM row directly, so no Start runs and no `vm.start` is ever sent. To capture a real boot spec, either stop+resume an ordinary workspace through the API (`api_personal_workspace_runtime.rs::restart`), or register a host with `attach_with_host_resources` on a separate workspace after `WorkerHostService::reconcile_unconfigured_org_defaults(..., "us-central1", ...)`, then read `host.received_host_command_params("vm.start")` (formal reviews need two `workflow::run_once` passes).
2. Fixture workspaces have a member at `root_path = ""`, which overlaps every root, so `load_personal_mount` drops the mount whatever the code does. Set member roots to `code` first.

**Why:** a negative assertion ("no reference_repos") passes for the wrong reason.

**How to apply:** for any absence assertion on a Start spec, temporarily revert the fix and confirm the test fails before trusting it.
