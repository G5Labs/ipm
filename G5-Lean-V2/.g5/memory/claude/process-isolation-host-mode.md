---
name: process-isolation-host-mode
description: Non-obvious facts about the local process-isolation host mode (PR #487, 2026-10-12): stop semantics, exit-proof census, RPC event delivery, and how to run its tests.
metadata:
  type: project
---

Process isolation mode for the local `g5d host` (plan:
`.g5/pr_ontology_diff_design/process-isolation-local/`, block
`g5.runtime.boot.backend.process-local`).

**Why:** facts that cost time to rediscover and are not obvious from the code:
- Only the dispatcher's first RPC stream (the host's control stream) receives
  `runtime.ready` and pushed `module.event` frames; an owner stream opened through
  `DispatcherEndpoint` must poll `events.replay {exec_id, from_seq}`.
- A workspace stop must not signal the dispatcher tree with SIGTERM: `preview-serve`
  treats SIGTERM as owner Close (runs cleanup) and keeps the app in its own process
  group, and `portable-pty` terminals `setsid`. The launcher starts the dispatcher
  with `setsid`; a stop SIGKILLs the whole session plus every process carrying the
  inherited `G5D_PROCESS_INCARNATION` marker.
- Exit proof is a census: session groups, env-tagged pids and "unknown" same-user
  processes whose environment is unreadable and that started after the leader.
  Unknowns block proof rather than being guessed; a killed orphan is a zombie until
  init reaps it, so liveness probes treat state `Z` as gone only when no task in
  `/proc/<pid>/task/*` runs. `kill(pid, 0)` is true for zombies. A KERN_PROCARGS2
  reply without an environment is uncertainty; args and env are searched together
  because an empty argument is indistinguishable from padding.
- Hidden-environment fixtures (`prctl(PR_SET_DUMPABLE, 0)`) do not hide anything
  from a root runner, so those tests skip loudly when `geteuid() == 0` (CI runs the
  Rust lanes as root).
- The launcher writes `incarnation.json` before spawning; `g5d host` calls
  `settle_process_incarnations` at startup in every mode and fails closed on any
  registry read error, non-directory entry or malformed proof. `Path::exists()` is
  never proof: the settled marker must be an empty regular file. Never bound a
  census by a registry directory's mtime (removing a file bumps it).
- The API's `VmSpec.env` always carries `G5D_OWNER_GIT_NAME/EMAIL` and
  `G5D_MODULE_UID/GID`; test specs from `HostFixture` carry none and the dispatcher
  refuses to start without the Git identity.
- Dev stack identity is `G5_COMPOSE_PROJECT`/`G5_COMPOSE_VOLUMES`/`G5_IMAGE_TAG`/
  `G5_PORT_*`, resolved once per recipe by `scripts/lib/stack-identity.sh` (exports,
  overlays, `.env`, defaults) because `set -a; source .env` overrides caller exports.
  The `full-stack` preview profile uses `g5-preview-<marker>` and ports
  13000/17500/15433/19876, and its Close runs `just dev-down` (its own worker host, then the containers);
  `full-stack-no-worker` closes with `just dev-stack-down` alone. Apple's `sleep` refuses `infinity`.
- anyhow's `Display` shows only the outer context: assert on `format!("{e:#}")` to
  see pids in a cause chain.

**How to apply:** run the end-to-end test with `TMPDIR=/mnt/xfs` (see
[[xfs-reflink-loop-mount-for-host-tests]]):
`cargo test -p g5d --no-default-features --features test-helpers --test process_host`.
Local flow: `G5D_ISOLATION_MODE=process G5D_ALLOW_PROCESS_ISOLATION=1 just dev-up`;
the API admits such hosts only with `G5_ALLOW_PROCESS_ISOLATION_HOSTS=true`
(set by the dev compose file only).
