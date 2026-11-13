# G5-Lean research: task execution, worker isolation, host/VM lifecycle

Historical implementation inventory. The ordinary `g5-tools` MCP server and agent
review-comment interfaces have been removed; see the [current provider contract](../lean-provider-cli-contract.md).

Repo: `/Users/zliu/Workspace/G5-Lean` (read-only). Paths below are relative to that root.

Legend:
- **OBSERVED** = read in code/config at the cited `file:line`.
- **INFERRED** = my reading of how observed pieces fit; not verified end-to-end.

## 0. Sizing (wc -l; includes in-file tests)

| Area | Files | Lines |
|---|---|---|
| `backend/g5d-host/src` (supervisor, assignment, golden_*, tunnel, agent_host, lifecycle, host_observer, ports) | 13 | 39,029 (supervisor.rs 29,684; production code ends near line 13,440, remainder is `#[cfg(test)]`) |
| `backend/g5d-isolation/src/isolation` + build.rs | 15 | 13,155 (libkrun.rs 3,482; microvm.rs 2,328; mod.rs 2,028; attestor.rs 1,462) |
| `backend/g5d-worker/src` host/VM subset (lifecycle, runtime_keeper, poll, inbox, task_agent, guest_runtime, host_lifecycle, host_support, task_env, vsock_transport, sidecar_transport, protocol, stable_workspace_metadata, llm_proxy, g5_api_session_store_proxy, mcp_preflight, port_scan, build_telemetry) | 18 | 21,762 |
| `backend/g5d-worker/src/{claude_code,codex}.rs` (provider adapters) | 2 | 37,898 |
| `backend/g5d-cli/src/{host_up,preview_serve,golden_build,worker_client,task_workspace,process}.rs` | 6 | 6,330 |
| `backend/g5d-foundation/src/workspace` (materializer + bare-repo cache) | 5 | 9,039 |
| `backend/g5d-foundation/src/golden` | 4 | 4,428 |
| `backend/g5d-foundation/src/oci` (rootfs extraction / template store) | 17 | 10,079 |
| `backend/g5d-foundation/src/terminal` | 2 | 1,761 |
| `backend/g5d/src/{host/*, cli/worker.rs, agent/agent_runner.rs}` | 10 | 6,615 (plus `worker/mod.rs` 19,397; `worker/bootstrap.rs` 4,859; `worker/runtime_keeper.rs` 3,274) |
| `backend/g5-api-core/src/services/{sticky_runtime,worker_host_service,chat_worker_loss}.rs` + `cloud/*` + `services/execution_lifecycle/*` | 22 | 24,176 (sticky_runtime.rs 8,073; `task_service.rs` is a further 33,023 and owns poll/claim/ack) |
| `backend/g5-api-routes/src/routes/{internal,workers,worker_hosts,preview,golden_builder}.rs` | 5 | 23,463 (internal.rs 15,051) |
| `backend/g5-internal-api/src/{stream_mux,client,dtos}.rs` | 3 | 9,515 |
| `backend/g5d/sidecars/{claude,codex}/sidecar.mjs` (bundled JS) | 2 | 47,916 |
| Migrations touching worker_hosts / sticky runtimes / attempts / previews / golden | ~45 of 292 | n/a |

## A. Topology

### A.1 Binaries and entrypoints (OBSERVED)

- Two Rust binaries: `g5d` (`backend/g5d/src/main.rs:3-5`, delegates to `g5d_cli::run_process_entrypoint(g5d::run)`) and `g5-api` (`backend/g5-api/src/main.rs`).
- `g5d` subcommands relevant here (`backend/g5d-cli/src/lib.rs:193-306`):
  - `Worker` — "start a worker bound to a task" (daemon polling mode without `--task`, direct-task mode with it; `backend/g5d/src/cli/worker.rs:1-11`).
  - `HostUp` — "start the host supervisor: register, poll, and boot a microVM per task".
  - `VmExec` (hidden, cfg `libkrun`) — boots a VM via libkrun FFI.
  - `PreviewServe` (hidden) — in-VM entrypoint for `preview` jobs.
  - `GoldenBuild` (hidden) — in-VM entrypoint for `golden_builder` tasks.
  - `PreviewAttachListener` (hidden) — in-VM idle listener beside preview-serve.
- Provider sidecars are Node bundles: `backend/g5d/sidecars/claude/sidecar.mjs` (46,379 lines; bundles `@anthropic-ai/claude-agent-sdk`, `sidecar.mjs:12`) and `backend/g5d/sidecars/codex/sidecar.mjs` (1,537).
- The SDK spawns the Claude Code CLI from `pathToClaudeCodeExecutable` (`sidecar.mjs:12108`). The worker resolves it from `G5_CLAUDE_PATH` (`backend/g5d-worker/src/claude_code.rs:1078-1080`), which the host pins to `/opt/g5/bin/claude` in the guest (`backend/g5d-host/src/assignment.rs:133`, `:1127`).
- The worker spawns `node <sidecar_path>` (`claude_code.rs:5552-5556`) and drives it with line-delimited JSON on stdio (`backend/g5d-worker/src/sidecar_transport.rs:3-6`).
- `g5-api` in-process loops: `MaintenanceRunner::spawn_all` (`backend/g5-api/src/main.rs:402,433`), worker inbox WS hub (`:340`), worker-host reconciler (`:550`), idle-rotation loop (`:559`), stale-host detector (`:573`).

### A.2 Where each process runs (OBSERVED unless noted)

- **Worker host**: a bare GCE VM, or a Docker container with `/dev/kvm` when self-hosted. Runs `g5d host-up` under systemd `g5d-host-up.service` (`infra/packer/g5-worker/files/g5d-host-up.service`); the ExecStart wrapper execs `$G5D_BIN host-up` (`files/g5d-host-up:36-38`).
  - Unit needs `CAP_NET_ADMIN CAP_SYS_ADMIN CAP_CHOWN CAP_DAC_OVERRIDE CAP_FOWNER CAP_FSETID CAP_SETUID CAP_SETGID CAP_MKNOD CAP_SETFCAP` (`:63`), `DeviceAllow=/dev/kvm rw` (`:74`), `LimitNOFILE=1048576` (`:30`), `TimeoutStopSec=2100s` to outlast the 1800 s drain (`:43`).
- **Per-task microVM**: a child process `g5d vm-exec ...` spawned from `current_exe()` by `LibkrunBackend` (`backend/g5d-isolation/src/isolation/microvm.rs:298-422`). `vm_exec_main` calls `krun_start_enter()`, which replaces the process with the VMM event loop (`backend/g5d-isolation/src/isolation/libkrun.rs:13-19`, `:975`). The virtio-fs backend runs inside that process (service unit comment `:52-53`).
- **Guest**: boots `/opt/g5/bin/g5d worker --task <id>` (`assignment.rs:984-991`).
  - Mounts: `/work` = per-task workspace (virtio-fs rw); `/opt/g5/bin` = host runtime bin dir (ro) (`assignment.rs:361-364`); `/opt/g5/assets`, `/opt/g5/codex-assets` = provider profile assets (`assignment.rs:373-384`, consts `:100`, `:105`).
  - Runtime dir contents staged at image build: `g5d, node, sidecar.mjs, codex-sidecar.mjs, claude, codex, gh, gcloud, kubectl, cargo, cargo-nextest, just, npm/pnpm` (`scripts/stage-local-worker-runtime.sh:6-21`).
  - In-guest attestor verifies a libkrun guest (CPUID hypervisor bit + `AF_VSOCK` + vendor) and fails closed when `G5D_REQUIRED_OUTER_ISOLATION=libkrun` (`backend/g5d-isolation/src/isolation/attestor.rs:1-27`); the host stamps that var for MicroVm boots (`assignment.rs:1098-1105`).
- **Preview VM**: separate `vm-exec` with entrypoint `g5d preview-serve` (`assignment.rs:730-735`) plus `g5d preview-attach-listener` when attach is supported (`lib.rs:296-306`).
- **Golden builder VM**: `vm-exec` running `g5d golden-build` as root with dockerd on a retained scratch disk (`lib.rs:284-294`).
- **Process mode** (`IsolationMode::Process`): same `g5d worker --task` as a host child in a git worktree, env allowlist + setpgid, "no security boundary" (`backend/g5d-isolation/src/isolation/process.rs:1-14`; `mod.rs:878-881`).

### A.3 Process tree for one running chat/regular task (MicroVm mode)

Pieces OBSERVED; the assembled tree is INFERRED.

```
worker host  (GCE, default instance_type c3-standard-4 [migration 130]  |  self-hosted docker + /dev/kvm)
└─ systemd g5d-host-up.service -> /usr/local/libexec/g5d-host-up -> g5d host-up        (supervisor, tokio)
   ├─ HTTPS -> g5-api: /api/internal/host/{register,heartbeat}, /workers/:id/{poll,tasks/:t/ack|nack}, /tasks/:id/{transition,timeline}
   ├─ WS   -> g5-api: GET /api/internal/hosts/:hostId/tunnel      (PreviewTunnelCarrier; outbound only)
   ├─ [per task] g5d vm-exec --rootfs /var/lib/g5d/rootfs/<vm-uuid> --cpus N --mem-mib M \
   │        -V <workspace>:/work -V <runtime>:/opt/g5/bin -e K=V ... [-p host:guest] -- /opt/g5/bin/g5d worker --task <id>
   │     (this process IS the libkrun VMM after krun_start_enter; virtio-fs served in-process)
   │     └─ guest kernel (libkrunfw fat kernel) -> /opt/g5/bin/g5d worker --task <id>        (guest root)
   │           ├─ reads /work/.g5/assignment.json      (pre-claimed handoff: host worker id + session token)
   │           ├─ HTTPS -> g5-api (TSI egress); WS /api/internal/workers/:id/inbox-ws; WS /api/v1/yjs/:ws/:branch (CRDT)
   │           ├─ node /opt/g5/bin/sidecar.mjs         (uid 1000; g5.claude.* JSON frames over stdio)
   │           │    └─ /opt/g5/bin/claude               (Claude Code CLI via agent SDK)
   │           │         ├─ g5d mcp intent-grounding    (MCP server from /work/.mcp.json)
   │           │         └─ tool subprocesses (bash, git, cargo, npm, docker on the scratch disk) in /work
   │           └─ PTY shells (TerminalManager) for the run_in_terminal tool
   ├─ [per VM, dev-flip only]  loopback-forward listener on 127.128.x.x:<api_port>
   ├─ [per VM, NIC mode only]  passt sidecar process
   └─ [per preview] g5d vm-exec ... -- /opt/g5/bin/g5d preview-serve   (+ preview-attach-listener inside)
```

Evidence for the tree:
- vm-exec argv construction: `microvm.rs:307-422`.
- Handoff path constant and write: `backend/g5-contracts/src/types/task_context.rs:3889`; `backend/g5d-host/src/supervisor.rs:5904,5941`.
- WS inbox: `backend/g5d/src/worker/mod.rs:5322`; route `backend/g5-api-routes/src/routes/internal.rs:3423`.
- CRDT WS route: `backend/g5-api-routes/src/routes/mod.rs:456-457,539`.
- Sidecar spawn: `claude_code.rs:5552-5556`; non-root sidecar flag `assignment.rs:1052`; uid 1000 `backend/g5d-worker/src/guest_runtime.rs:4-5`.
- MCP preflight spawns `g5d mcp intent-grounding`: `files/g5d-host-up:24-30`.
- TerminalManager: `backend/g5d/src/worker/mod.rs:5899`.
- Loopback forward: `microvm.rs:648-680`; passt: `backend/g5d-isolation/src/isolation/passt.rs:1-16`; tunnel: `backend/g5d-host/src/tunnel.rs:1-16`.

## B. Unit of isolation

OBSERVED:
- `IsolationMode` = `MicroVm` (default, libkrun), `Process` (local dev, worktree), `MicrovmAgent` (parsed for historical config only; execution rejected with "microvm-agent isolation mode is retired") (`backend/g5d-isolation/src/isolation/mod.rs:871-886`; `assignment.rs:311-316`).
- Backends `BackendType::{Libkrun, Wsl2 (parked, never available), Process}` (`mod.rs:840-848`). Detection picks libkrun if the feature is compiled and `/dev/kvm` (Linux) or HVF (macOS aarch64) is present, else Process if `git` is on PATH (`detect.rs:66-129`, `:156-207`). `IsolationManager::new` fails closed if the configured mode cannot be honored (`handle.rs:33-36`; `mod.rs:1128`).
- Selection order:
  - host-up: `--isolation-mode` / `G5D_ISOLATION_MODE` (`lib.rs:790-792`).
  - worker: `--isolation-mode` > `G5D_ISOLATION_MODE` > workspace.yaml `isolation.mode` > deployment default (cloud => microvm, local => process) (`lib.rs:731-738`).
  - cloud startup script writes `G5D_ISOLATION_MODE={isolation_mode}` (`backend/g5-api-core/src/cloud/startup_script.rs:265`); self-hosted host-up hardcodes `microvm` (`infra/self-hosted/scripts/g5-workerhost.sh:448`).
  - `workspace.yaml` top-level `base_image:` selects the guest image per task (`backend/g5d-cli/src/config.rs:287-299`).
- **One microVM per task attempt.** Supervisor state is `records: BTreeMap<task_id, VmRecord>` (`supervisor.rs:3484`); workspace is `<workspaces_root>/<task_id>/` (`assignment.rs:847-849`); rootfs is a fresh COW clone `G5D_ROOTFS_STATE_DIR/<vm-uuid>` removed after exit (`microvm.rs:145-157`, `:242-260`).
- **Chat = one VM per chat session for as long as the sticky runtime is held.** In `--task` mode the guest worker "park[s] on retained runtime ... instead of exiting" while `RuntimeKeeper` holds a runtime (`backend/g5d/src/worker/mod.rs:7409-7418`; exit claim `:1461-1475`), so the host `VmRecord` stays non-terminal and keeps its slot, and the host reports `held_runtime_ids` (`dtos.rs:1187`). One live `chat_sticky_runtimes` row per `chat_session_id` (`backend/g5-api/src/db/migrations/136_chat_sticky_runtimes.sql:19-20`).
- Other execution kinds: `TaskExecutionKind::{Microvm, HostBootstrap (no VM; analysis pipeline in the supervisor process), PreviewAttach (rides an existing preview VM, consumes no slot)}` (`backend/g5d-host/src/lifecycle.rs:103-122`; classification `supervisor.rs:6550-6567`; bootstrap semantics `backend/g5d/src/worker/bootstrap.rs:1-10`).
- Capacity: `max_concurrent_microvms` (default 4; `G5D_HOST_MAX_MICROVMS`, `lib.rs:781`) is the shared budget for one-shot VMs plus previews; `max_concurrent_previews` is a sub-cap (`supervisor.rs:483-490`; poll gate `:4223-4227`). Per-VM size is derived from hardware / max unless pinned (`supervisor.rs:513-519`); CLI defaults 2 vCPU / 8192 MiB (`lib.rs:857-863`); self-hosted 2 / 4096 (`g5-workerhost.sh:461-462`).
- "Warm attach" is not VM reuse: it reflink-clones a sealed golden scratch-disk image (`backend/g5d-isolation/src/isolation/warm_attach.rs:1-10`).

INFERRED: no path lets two tasks of one workspace share a VM; the only two-tenant VM is preview + attached chat.

## C. Environment materialization per task

### C.1 Rootfs (OBSERVED)
- `MicrovmRunSpec.image` is an existing directory (used as-is) or an OCI reference / OCI layout dir extracted in-process by `OciExtractor::materialize_rootfs_cow` into `G5D_ROOTFS_STATE_DIR/templates/` (default `/var/lib/g5d/rootfs`) and COW-cloned into a per-VM dir (`microvm.rs:145-235`). Layer cache `G5D_OCI_CACHE_DIR` (`files/g5d-host-up.env:44-46`).
- Image choice per task: post-materialization read of the checkout's `workspace.yaml` `base_image` (`resolve_and_stamp_task_image`, `supervisor.rs:143`; `resolve_task_image`, `assignment.rs:907`), else the host's `G5D_HOST_BASE_IMAGE` (required; `startup_script.rs:240`). Org defaults: migrations 133, 135; per-host pin/track: 113, 240.

### C.2 Repo checkout (OBSERVED)
- Warm bare-repo cache `<cache_root>/<sha256(repo_url)>/` (`/var/g5d-data/cache/repos`): `git clone --bare` on first touch, then `git fetch --prune`; in-process + cross-process lock with a 600 s budget (`backend/g5d-foundation/src/workspace/repo_cache.rs:1-24`, `:45`; roots `workspace/mod.rs:170-184`).
- Per-task workspace: reflink the bare repo into `<workspaces_root>/<task_id>/<repo>/.git`, set `core.bare=false` + `core.worktree`, `git checkout -f <branch|HEAD>`; repeated per `TaskAssignment.repo_paths` (`materialize.rs:1-28`). Reflink-or-fail-loud with an opt-in copy fallback (`materialize.rs:285`). Hence XFS `reflink=1` on the host (packer `g5-worker.pkr.hcl:8-9`; self-hosted 64 GB loop image `mkfs.xfs -m reflink=1`, `g5-workerhost.sh:15-19,386-410`).
- The host chowns `/work` to uid 1000 before boot (`supervisor.rs:6008`); macOS dev relies on the in-guest chown instead (`guest_runtime.rs:15-33`).

### C.3 Boot sequence in `accept_assignment_with_authority` (OBSERVED, `supervisor.rs:5318-6395`)
1. Preview branch (`:5368`) and golden_builder branch (`:5383`); otherwise `build_run_spec` (`:5436`).
2. Interactive golden lookup and warm-source stamping onto the scratch disk (`:5525-5589`).
3. Chat with `chat_context.attach_vm` -> `spawn_preview_attach_drive` instead of booting (`:5656-5672`).
4. Spawned per-task future:
   - `materializer.materialize(assignment, repo_url, branch)` (`:5761`);
   - `resolve_and_stamp_task_image` (`:5813`);
   - write `/work/.g5/assignment.json` 0600 (`:5904-5941`; `write_handoff_no_follow` `:2306`);
   - `append_docs_repo_hardening_mounts` (`:5966`);
   - host-side chown (`:6008`);
   - `launcher.run` (boot-and-await);
   - `report_started` -> `host_microvm_started` timeline entry + `TaskObservation{isolation_backend, resources, image, port_map}` (`:249-259`);
   - VM exit -> `report_exited` -> `materializer.cleanup(task_id)`.
- The handoff lets the guest skip re-claiming and act under the host's worker identity (`task_context.rs:3889-3915`; `HostWorkerIdentity` `supervisor.rs:3973-3977`).

### C.4 In-guest lifecycle (OBSERVED)
- `TaskLifecycleManager` phases `Prepare -> ClonePatch -> Execute -> Collect -> Cleanup` (`backend/g5d-worker/src/lifecycle.rs:1-6`, `:334-340`; `execute_task` `:1837`).
- `phase_prepare` no longer clones: comments only plus best-effort cache hydration (`:2178-2220`).
- `phase_clone_patch` opens the host-materialized repo and fails when `HEAD != base_commit` ("materialized task repository does not match its exact assigned cut") (`:2263-2292`). The turn then connects a CRDT peer and runs the provider behind a filesystem -> CRDT bridge (`run_agent_with_provider_fs_bridge`, `:2304-2320`).
- Cleanup is skipped when a Waiting turn transfers its live runtime into the `RuntimeKeeper` (`:1946-1961`).

### C.5 Scratch disk, caches, golden images (OBSERVED)
- Scratch disk: `G5D_SCRATCH_DISK_MIB` -> sparse ext4 backing at `<rootfs>/.g5/scratch.ext4`, attached via `krun_add_disk3`, mounted at `/var/lib/docker` (`assignment.rs:417-427`, `:727`; `libkrun.rs:1876-1884`; canonical 64 GiB per `supervisor.rs:539-546`).
- Preview npm cache: per-workspace host dir mounted at `/cache/npm` (`assignment.rs:62`; `supervisor.rs:526-533`).
- Golden store: write-once, content-keyed, sealed images at `<root>/<workspace_uuid>/sha256/<key>/golden.img`, flock singleflight (`backend/g5d-foundation/src/golden/mod.rs:17-27`). Docker recipe classes `DevUp`, `CiDocker` (`golden/recipe.rs:92-97`); key inputs = recipe source, base image digests, dockerfile/compose digests, engine fence (`recipe.rs:702-713`; this repo's own recipe is `.g5/workspace.yaml` `golden.docker`).
- Builder lane: host derives the key, dispatches `builder_task_specs` (`288_golden_builder_task_class.sql`), boots the builder VM with `retain_backing_to`, finalizes and seals at reap (`backend/g5d-host/src/golden_builder.rs:1-16`).
- Consumer lane: preview tasks derive the key from cache blobs at the pinned commit (no materialization) and warm-attach, park behind a builder, or boot cold (`golden_consumer.rs:1-14`); interactive tasks attach the most recently sealed golden (`supervisor.rs:5525-5589`). Watermark + maintenance tick (`golden_maintenance.rs:1-14`; tick `supervisor.rs:4171-4190`).
- Knobs: `G5D_GOLDEN_STORE_ENABLE`, `G5D_DOCKER_GOLDEN_ENABLE`, `G5D_CARGO_GOLDEN_ENABLE`, `G5D_NODE_GOLDEN_ENABLE`, `G5D_GOLDEN_FREE_STOP_CACHING_PCT`, `G5D_GOLDEN_FREE_STOP_WARM_PCT`, `G5D_GOLDEN_WORKSPACE_BUDGET_BYTES`, `G5D_GOLDEN_MAINTENANCE_INTERVAL_SECS` (`golden/knobs.rs:17-83`); cloud bring-up forwards `G5_WORKER_GOLDEN_STORE_ENABLE` / `G5_WORKER_DOCKER_GOLDEN_ENABLE` (`startup_script.rs:100-106,208-213`).
- Telemetry feeding the triggers: guest shims append `/work/.g5/telemetry/*.jsonl`; the worker reads it at teardown with contained `openat2` (`backend/g5d-worker/src/build_telemetry.rs:1-18`) -> `POST /api/internal/tasks/:id/build-telemetry` (`internal.rs:3189`) -> `session_build_telemetry`, `workspace_prebuild_state` (`287_build_telemetry.sql`) -> `maintenance/golden_builder_triggers.rs` (979 lines).
- Code cost (OBSERVED): host lanes 2,109 + 1,430 + 740 lines; foundation store 4,428; in-VM `golden_build.rs` 1,730; API triggers 979 + `routes/golden_builder.rs`; migrations 287-291. Runtime cost (INFERRED): one builder VM per (workspace, recipe key) plus a sealed disk image per key on every host that builds it.

### C.6 Timing and readiness (OBSERVED)
- The only whole-VM timing is `MicroVM run completed (libkrun) duration_ms` = VM lifetime, logged at exit (`microvm.rs:641`, `:804-810`). Cache hydration logs `duration_ms` (`lifecycle.rs:2204`).
- No boot-to-ready metric exists for agent tasks. `host_microvm_ready` exists only for previews: a host HTTP probe every 2 s, 3 s IO timeout, 7200 s backstop (`supervisor.rs:12576-12591`; entry `:2086-2108`).
- Host-emitted timeline entry types: `host_microvm_started`, `host_microvm_ready`, `preview_attach_started`, `error`, `output` (grep of `entry_type:` in supervisor production code).
- Deadlines: carrier accept 30 s (`supervisor.rs:650`); deferred re-dispatch 120 s (`:3853`); parked teardown VM op 8 s / CRDT 1 s (`:3862-3863`); host-finalized completion 300 s (`:350`); preview attach ready 300 s (`:12386`); drain 1800 s (`lib.rs:795`).
- INFERRED: cold time-to-first-token = bare-repo fetch + reflink checkout + OCI template clone + VM boot + attestor + handoff read/registration + CRDT peer connect + sidecar/Claude start; none of these steps is individually instrumented in the code read.

## D. Host lifecycle

### D.1 Schema (OBSERVED, `backend/g5-api/src/db/migrations`)
- `worker_hosts` (078): `host_id, org_id, vpc_id, cloud_instance_id, instance_type (default 'n2-standard-8', changed to 'c3-standard-4' in 130), region, libkrun_version, libkrunfw_version, kernel_version, cache_state JSONB, attestation_state JSONB, status CHECK IN ('provisioning','ready','draining','terminated'), created_at, last_heartbeat`; RLS by org.
- Later `worker_hosts` columns:
  - 083: `active_microvm_count, cache_disk_used_bytes, cache_disk_total_bytes`.
  - 102/105/232: `provision_error_code IN ('zone_capacity_exhausted','abandoned_bringup','never_registered')`, `orphan_check_misses`, `provision_retry_count`, `provision_next_retry_at`; table `cloud_orphan_sightings(instance_name, zone, host_id, first_seen, last_seen)`.
  - 113: `base_image_ref, base_image_digest (sha256 CHECK), runtime_schema, runtime_labels, runtime_labels_refreshed_at, worker_profile IN ('fat-default','slim')`.
  - 132: `rotation_replaced_by, last_active_at`. 146: `concurrent_jobs`. 168: `boot_image_id`. 171: `bringup_heartbeat_at, cloud_gone_since`. 240: `base_image_source_kind IN ('track','pin'), base_image_source_value`. 264: glibc floor error.
- `workers` (001:631-662): `id TEXT, role, status IN ('provisioning','idle','assigned','executing','terminated'), owner_user_id, org_id, memory_mb, cpu_cores, task_id, workspace_id, branch, base_commit, last_heartbeat, ...`; plus `host_id` FK (079), `external_host_id` (222), `worker_session_generation` (241), `host_discard_fence_challenge/receipt` JSONB (294).
- Triggers `trg_workers_guard_revival_dead_host` / `trg_workers_guard_insert_dead_host` block reviving or inserting a worker on a terminated host (276:119-194; 275 adds the guard grants).
- Rust mirrors: `WorkerHostStatus::{Provisioning, Ready, Draining, Terminated}` (`backend/g5-contracts/src/types/worker_host.rs:51-63`); `WorkerHost` DTO (`:129`); `HostObservation{host_id, active_microvm_count, cache_disk_used_bytes, cache_disk_total_bytes, image_digests_cached, last_attestation_result, libkrun_version, libkrunfw_version, golden_store}` (`:649-658`).

### D.2 Cloud path, GCE only (OBSERVED)
- Operator API: `POST/GET /api/v1/orgs/:orgId/worker-hosts`, `/image-policy`, `DELETE /:hostId`, `POST /:hostId/runtime-labels/refresh` (`backend/g5-api-routes/src/routes/worker_hosts.rs:159-175`), mounted only when not self-hosted (`routes/mod.rs:273-276,380`).
- `CloudProvider` trait; single production impl `GcpCloudProvider` (raw REST via reqwest) (`backend/g5-api-core/src/cloud/mod.rs:1-17`; `gcp.rs:1-9`). "A future AWS/Azure backend is contained to a new impl" (`mod.rs:12`).
- Bring-up is detached (`bringup.rs:1-9`). The startup script writes `/etc/g5/host-up.env` with `G5_HOST_ID, G5_HOST_JWT, G5_CONTROL_PLANE_URL, G5_API_URL, G5D_API_URL, G5_HOST_REGION, G5D_HOST_BASE_IMAGE, G5D_HOST_MAX_MICROVMS, G5D_ISOLATION_MODE, G5D_LLM_PROXY=1, [G5D_VM_NET, G5D_SCRATCH_DISK_MIB, golden knobs], G5_RUNTIME_NODE/BIN_DIR/LIB_DIR, G5D_OCI_CACHE_DIR`, then `systemctl enable/restart g5d-host-up.service` (`startup_script.rs:255-294`).
- Image: Packer `g5-worker` (Ubuntu 24.04, XFS reflink root, libkrun + fat libkrunfw kernel, runtime binaries in `/var/g5d-data/runtime/bin`) (`infra/packer/g5-worker/g5-worker.pkr.hcl:1-17`). Dev Terraform = regional MIG of size 1 (`infra/terraform/modules/worker-host/main.tf:1-15`).
- Reconciler every `G5_RECONCILER_INTERVAL_SECS` (30 s): `provisioning` + cloud Running + heartbeat -> `ready`; Terminated/404 -> confirmed-gone window -> `terminated`; orphan pass by instance name `g5-host-{host_id}` -> `abandoned_bringup` (`cloud/reconciler.rs:1-30`).
- Rotation: host JWT minted at provision with 24 h TTL and never refreshed; rotate when idle + image-stale, or forced at TTL-2h; provision replacement first, then drain (`cloud/rotation.rs:1-30`).
- Stale-host detector: never-registered reap, silent-ready reap, cloud-orphan sweep via `cloud_orphan_sightings`, zone-stockout retry (`cloud/stale_host.rs:1-27`).
- Lease reaper (API maintenance): expired claims/leases, max-duration fail, dead workers (`last_heartbeat` > 3x lease TTL -> `terminated`) (`maintenance/lease_reaper.rs:1-16`).

### D.3 Self-hosted, Azure, macOS (OBSERVED)
- Self-hosted: `infra/self-hosted/scripts/g5-workerhost.sh` runs the worker image with `docker run --device=/dev/kvm ... host-up` on a loop-mounted XFS image (`:386-410`) with env `G5D_ISOLATION_MODE=microvm, G5D_HOST_BASE_IMAGE, G5D_ROOTFS_STATE_DIR=/var/g5d-data/rootfs, G5D_HOST_STATE_DIR, G5D_HOST_CACHE_ROOT, G5D_HOST_WORKSPACES_ROOT, G5D_HOST_MAX_MICROVMS (default 1), G5D_HOST_MAX_PREVIEWS (1), G5D_HOST_CPUS_PER_VM (2), G5D_HOST_MEM_MIB_PER_VM (4096), G5D_LLM_PROXY (1)` (`:429-466`). Registration uses a `g5w_*` registration token (`G5D_REGISTRATION_TOKEN`, `:444`), not a host JWT. `compose.kvm.yml` adds `/dev/kvm` to `g5d-worker`.
- Azure: a CI canary only, provisioning a nested-virt VM (`Standard_D4s_v3`) and running the self-hosted installer (`.github/workflows/self-hosted-azure-install.yml:24-38`). No Azure `CloudProvider`.
- macOS dev: `just worker-host-setup` runs native `g5d host-up` under HVF (`Justfile:323-344`, `:430`, `:510`); runtime dir staged by `scripts/stage-local-worker-runtime.sh`.

### D.4 Registration and heartbeat (OBSERVED)
- host-up wiring (`backend/g5d-cli/src/host_up.rs:1010-1424`): layout check for `/var/g5d-data/runtime/bin/g5d` -> `IsolationManager::new` -> `InternalApiClient` -> `register_worker_session` (host_jwt path -> `/api/internal/host/register`; token path -> `/workers/register` with `G5_API_KEY`) -> `GoldenMaintenance` -> `RepoCache` + `Materializer` -> `Supervisor::new` -> signal handler -> `PreviewTunnelCarrier` (host_jwt path only) -> `run` -> `/api/internal/host/terminate`.
- `HeartbeatRequest{active_task_ids, system_load, available_memory_mb, host_observability, held_runtime_ids, wake_runtime_ids, wake_completions, busy_runtime_ids, busy_runtime_pending, teardown_acks, workspace_reconciliation_receipts, workspace_reconciliation_obligations, parked_runtime_identities}` (`backend/g5-internal-api/src/dtos.rs:1180-1204`).
- `HeartbeatResponse{session_token, host_token, negotiated_capabilities, host_discard_fence_receipt, host_discard_fence_activation_v1, host_discard_fence_rollout_drain_v1, host_discard_fence_baseline_transition_v1, cancelled_task_ids, teardown_runtime_ids, teardown_instructions, workspace_reconciliation_instructions, accepted_workspace_reconciliation_receipts, superseded_workspace_reconciliation_obligations}` (`:1466-1505`).
- Host discard fence: a challenge/proof/receipt handshake binding host build, guest image digest, isolation backend and providers to the worker session generation (`dtos.rs:787-830`; migration 294; runbook `infra/runbooks/hosted-sticky-discard-fence-v2-rollout.md`).

### D.5 `concurrent_jobs` (OBSERVED)
- `worker_hosts.concurrent_jobs` is the operator-pinned per-host VM budget (146). When NULL the API uses `default_concurrent_jobs(instance_vcpus)`, targeting ~16 GB RAM per job (`cloud/bringup.rs:32`; tests `:741-746`: c3-standard-4 -> 1, n2-standard-8 -> 2, c3-standard-22 -> 5).
- It becomes `G5D_HOST_MAX_MICROVMS` in the startup script (`routes/worker_hosts.rs:359-360,608-629`; `startup_script.rs:264`) and is what the host advertises as `max_concurrent_tasks` at registration (comment `supervisor.rs:4221-4227`).

## E. RPC / transport (OBSERVED)

### E.1 api <-> host/worker control plane (HTTPS JSON, polling)
- Routes (`internal.rs:3103-3148, 3153-3260, 3490-3500`): `POST /api/internal/workers/register`; `/workers/:id/poll`; `/workers/:id/tasks/:task_id/{claim,ack,nack,observation}`; `/workers/:id/tasks/:task_id/sticky-runtime/:runtime_id/decline`; `/workers/:id/heartbeat`; `/workers/:id/terminate`; `/tasks/:id/{transition,transition/preflight,status,timeline,checkpoints,build-telemetry,self-context,...}`; `/host/{register,heartbeat,terminate}`.
- `PollTaskRequest{mode: TaskPollMode::{Any, ZeroSlotOnly}}` (`dtos.rs:769-783`).
- `RegisterWorkerRequest{hostname, capabilities, max_concurrent_tasks, org_id, workspace_id, display_name, existing_worker_id, registration_token}` (`:833-847`); `HostRegisterWorkerRequest{+host_discard_fence_proof}` (`:851`); `RegisterWorkerResponse{worker_id, heartbeat_interval_secs, credential_encryption_key, session_token, host_id}` (`:882-891`).
- `AssignmentAckResponse{execution_attempt_id, granted_sticky_runtime_id, granted_sticky_runtime_teardown_generation, grant_worker_session_generation, grant_host_discard_fence_receipt}` (`:963-975`).
- `TaskAssignment{task_id, execution_attempt_id, job_id, task_type, job_type, agent_execution, branch_name, base_commit, workspace_git_url, repo_paths, task_crdt_head_commits, session_id, workspace_id, workspace_branch_id, guardrails, scm_credentials, chat_context, preview_boot_generation, golden_builder_context, secrets, ...}` (`task_context.rs:3659-3885`).
- Capabilities advertised by a host: `DEFAULT_TASK_CAPABILITIES = [chat, uplift, downlift, sync, merge]` plus `agent_provider:claude_code`, `isolation:libkrun-microvm` (`backend/g5d-worker/src/host_support.rs:9`; `lib.rs:880-889`).

### E.2 api -> worker push (WebSocket)
- `GET /api/internal/workers/:id/inbox-ws?session_id=` (`internal.rs:3423`); `WorkerHub` pushes `InboxItem`s so the worker has no Redis dependency (`backend/g5-api-core/src/ws/worker_hub.rs:1-12`). Guest uses `WsInboxReader` (`backend/g5d/src/worker/mod.rs:5322`); a `RedisInboxReader` still exists (`inbox.rs:362`).
- Types: `InboxItem`, `ControlAction`, `MessageSource::{System, AgentLoop, WebClient, Api, Slack}` (`backend/g5d-worker/src/inbox.rs:17-42,260`).

### E.3 worker <-> api document sync (WebSocket, yjs)
- `WS /api/v1/yjs/:workspaceId/:branchId?token=&gitSha=[&taskId=]` (`routes/mod.rs:456-457,539`); `CrdtPeer` with bounded retry (`host_support.rs:24`).

### E.4 api -> preview VM (WebSocket reverse tunnel + stream mux)
- Host dials `GET /api/internal/hosts/:hostId/tunnel` with the host JWT; g5-api is the `Mux` initiator; `Channel::Preview` streams carry a 2-byte host_port header; the host dials `<vmm_loopback_addr>:<host_port>` (`backend/g5d-host/src/tunnel.rs:1-16`). API side `TunnelRegistry` / `HostTunnel::open_preview_stream` (`backend/g5-api-core/src/ws/preview_tunnel.rs:65-101`); browser route `/api/v1/preview/:jobId[/*path]` (`routes/preview.rs:281-287`).

### E.5 host <-> guest (virtio-fs, env, handoff file, TSI port maps)
- `-V host:guest` volumes, `-e K=V`, `-p host:guest`, `--scratch-disk-*`, `--net`, `--workdir` (`microvm.rs:318-422`).
- TSI loopback isolation maps each VMM PID to a `127.128.0.0/10` address (`vmm_loopback.rs:1-13`); host ports 49152-65535 via `PortAllocator`/`PortLease` (`port_map.rs:1-20,102-118`).
- Optional passt NIC: guest 10.85.0.2, host door 169.254.1.2 alias `g5-host` (`passt.rs:26-33,61-73`). Dev-flip inbound loopback forward for `G5D_API_URL=localhost` (`loopback_forward.rs:1-18`).
- Guest env (MicroVm): `G5D_API_URL, G5_API_KEY, G5D_ORG_ID, G5D_ISOLATION_MODE, PATH, G5D_CLAUDE_CODE_ENABLED, G5D_CODEX_ENABLED, G5D_REQUIRED_OUTER_ISOLATION, G5D_GUEST_SIDECAR_NONROOT, G5_NODE_PATH, G5_CLAUDE_PATH, G5_CLAUDE_CODE_SIDECAR, G5_CODEX_SIDECAR, G5_CODEX_PATH, LD_LIBRARY_PATH, G5_CLAUDE_CODE_ASSETS_DIR, G5_CODEX_ASSETS_DIR, G5D_LLM_PROXY, G5D_LLM_PROXY_UNIX_SOCKET` (`assignment.rs:993-1190,1216-1265`).

### E.6 host <-> guest for preview attach only (TCP over TSI + stream mux)
- Host connects to the guest listener port P2, sends a 64-hex capability token frame, then multiplexes `Channel::{Stdio, Llm, SessionStore}` (`agent_host.rs:20-49,55,116,188`; `preview_attach_wire.rs:24-49`; `vsock_transport.rs:1-28`). The mux lives in `g5_internal_api::stream_mux` (`vsock_mux.rs:1-13`). Real `AF_VSOCK` code exists but is "real-VM-only" (`vsock_transport.rs:260-282`).

### E.7 worker <-> agent (child stdio, line-delimited JSON)
- Request frames `g5.claude.send / .input / .control / .resolve_approval`; responses `g5.claude.*` including `done`, `error`, `wake`, `busy` (`sidecar_transport.rs:3-6`; `host_lifecycle.rs:23`; `runtime_keeper.rs:96-135`).
- Traits `SidecarTransport / SidecarWriter / SidecarReader`; impls `LocalPipeTransport`, `VsockSidecarTransport` (`sidecar_transport.rs:154-224`; `vsock_transport.rs:60`).
- Result: `AgentLoopResult{outcome: AgentOutcome::{Completed, AqlBlockingWait, DeferredLifecycle, Blocked, Failed, ProviderInterrupted, BudgetExhausted, Aborted, EndTurn}, turns, usage: UsageStats, ...}` (`protocol.rs:10-66`).

### E.8 agent -> LLM and agent -> G5 tools
- `G5D_LLM_PROXY=1` and `G5D_LLM_PROXY_UNIX_SOCKET=/run/g5/llm.sock` are stamped into the guest env (`assignment.rs:1216-1217,1263-1265`); the provider credential is passed to the guest (test `assignment.rs:1781`). INFERRED: in MicroVm mode the `llm_proxy` runs inside the guest and egresses over TSI; the host-side relay described in `llm_proxy.rs:1-25` belongs to the retired no-identity path.
- Claude Code launches `g5d mcp intent-grounding` from `/work/.mcp.json`, authenticated with the worker session token and stamped provenance env (`files/g5d-host-up:24-30`; `task_env.rs:20-22`).

## F. Sticky runtime

OBSERVED:
- Purpose (module header): keep a chat's provider runtime live across turns. Callers: chat ACK grant, transition waiting arm, LiveLease resume, worker/host heartbeat, lease-reaper sweep, rotation pre-step, poll/claim pressure eviction, worker termination (`backend/g5-api-core/src/services/sticky_runtime.rs:1-20`). Opt-out `G5D_STICKY_RUNTIME=0|false` on host and worker (`supervisor.rs:776-786`).
- Table `chat_sticky_runtimes` (136): `id (fencing token), org_id, workspace_id, chat_session_id, task_id, worker_id, host_id, microvm_id, provider IN ('claude_code','codex'), status IN ('active','reaping','expired','lost'), idle_since, expires_at, last_liveness_at`; `workspace_settings.chat_sticky_idle_timeout_min` default 60, range 0..240.
- Later columns:
  - 165/166/167: pending-question window and `task_question_windows`. 169: `lost_reason`. 175: busy metadata.
  - 211: `teardown_generation, teardown_operation_id, teardown_requested_at, teardown_deadline_at (= requested_at + 30 s), teardown_repository_scopes, teardown_disposition IN ('durablyFlushed','preservedForRecovery'), teardown_outcome IN ('tornDown','alreadyAbsent','forceEvicted'), teardown_evidence, teardown_acknowledged_at`.
  - 227: `warm_resume_claimed_at`. 241: `worker_session_generation`. 244: `teardown_workspace_released_at` (release proof). 247: `worker_workspace_root_lease_v1, teardown_reconciliation_kind ('registration_retirement'), teardown_retired_worker_session_generation, teardown_reconciliation_receipt`. 248: delivery receipt. 269/271: task-workspace recovery. 273: serialized claim transitions. 294: `host_discard_fence_receipt`.
- Status semantics: `active` (held live), `reaping` (teardown instructed), `expired` (teardown confirmed), `lost` (worker dead/stale; task stays waiting, next send takes cold recovery) (`backend/g5-api-data/src/models/chat_sticky_runtime.rs:24-35`).
- Related: `task_execution_attempts{id, org_id, task_id, attempt_number, worker_id, provider, provider_session_id, continuation_summary, disposition IN (completed, interrupted, blocked, failed, cancelled), reason IN (15 values), retry_policy IN (automatic, manual, none), started_at, ended_at, close_evidence}` and `tasks.active_execution_attempt_id` (239, 298).
- API-side state machine: `execution_lifecycle` kernel, `LifecycleCommand::{Claim, RejectClaim, Ack, Complete, Retry, Park, Fail, ...}`, families Admission / AttemptOutcome / HumanControl / RuntimeControl / Recovery / JobRollup / Liveness (`services/execution_lifecycle/mod.rs:1-5`; `command.rs:12-19,178-185`).
- Wire: the grant rides the ACK (`AssignmentAckResponse`); teardown rides the heartbeat (`StickyRuntimeTeardownInstructionV1{runtime_id, task_id, worker_id, host_id, generation, operation_id, repository_scopes[{workspace_branch_repo_id, crdt_session_id}], requested_at, drain_deadline_at}`, `StickyRuntimeTeardownAckV1{runtime_id, generation, operation_id, disposition, outcome, evidence}`, `dtos.rs:983-1080`); wakes for background subagent completions (`wake_runtime_ids`, `MAX_WAKE_DISPATCHES_PER_HEARTBEAT = 8`, `:977`).
- Worker side: `RuntimeKeeper` with `KeeperSlot{task_id, runtime_generation, state: Parked|Resuming|TearingDown, teardown_tx, wake_pending, wake_generation, wake_results, busy_until_epoch_ms, busy_pending_count}` and `TeardownSignal::{Idle, ServerLegacy, LocalPreserve, Typed(instruction)}` (`backend/g5d-worker/src/runtime_keeper.rs:255-342`). Resumed turns run only Execute + Collect (`lifecycle.rs:1985`, comment `:1979-1984`).
- Host side: `parked_runtimes` registry of `ParkedRuntime{runtime_id, task_id, execution_attempt_id, teardown_generation, pending_exit_reason, teardown_retry, teardown: MicrovmAgentTeardown{launcher, handle, router, session_store_handle, llm_handle, crdt_peer}}` (`supervisor.rs:2949-3025,9783-9795`), documented as the microvm-agent path (`:3509-3514`). `VmRecord.granted_sticky_runtime_id/_teardown_generation` fence force-eviction (`lifecycle.rs:141-215`). Saturated-probe polling when the host is full of parked holds (`supervisor.rs:4277-4330`).

INFERRED mapping to hosts/VMs on the live MicroVm path: runtime id granted at ACK -> handed to the guest via `.g5/assignment.json` -> registered in the guest `RuntimeKeeper` -> guest worker parks instead of exiting -> host `VmRecord` stays non-terminal and occupies a slot -> host echoes `held_runtime_ids` -> server teardown instruction arrives by heartbeat -> keeper signals the sidecar -> worker exits -> VM exits -> slot frees. `chat_sticky_runtimes.host_id/microvm_id` persist the mapping.

Problem solved (OBSERVED from comments): avoid re-materializing the workspace and cold-starting the provider session per chat turn; keep background subagent work alive between turns; bound idle cost by timeout and pressure eviction. Cost: ~8k lines of guarded CAS SQL, ~20 migrations, and most of the supervisor's teardown / fence / reconciliation machinery.

## G. Preview

OBSERVED:
- Job type `preview` (`288_golden_builder_task_class.sql`). `job_preview_sessions` (096): `id, org_id, job_id, task_id, mode IN ('unmerged','merged'), status TEXT, guest_port, host_port, endpoint_url, expires_at, started_at, stopped_at`; plus `host_id` (097), `profile` (107), `attach_supported` (233), `boot_generation` (236). Status strings written: `'pending'` at insert (`backend/g5-api-core/src/services/job_service.rs:5320`), `'serving'` (`task_service.rs:15662`; `chat_service.rs:2075`), `'stopped'` (chat_service). No Rust enum for it.
- Host path: preview assignment -> `build_preview_run_spec` (entrypoint `g5d preview-serve`; env only `G5D_PREVIEW_WORK_DIR, PATH, LD_LIBRARY_PATH, [G5D_PREVIEW_PROFILE]`; no G5 credential) (`assignment.rs:662-780`) -> materialize -> lease host port -> `port_maps = [(host, guest)]` -> optional attach port + 32-byte token (`with_preview_attach`, `:784-845`) -> boot -> readiness probe -> `host_microvm_ready`.
- Preview VMs are kept in `preview_vms` and count against the shared VM budget (`supervisor.rs:3494-3498`). The guest reads `.g5/workspace.yaml` `preview:` or `.g5/preview_profiles/<name>.yaml` (`backend/g5d-cli/src/preview_serve.rs:1-36`).
- Browser access: `/api/v1/preview/:jobId[/*path]` -> reverse tunnel -> VMM loopback host port (E.4). TTL: `preview_reaper` cancels the job on `expires_at`; the host tears down on `cancelled_task_ids` (`maintenance/preview_reaper.rs:1-15`).
- Chat attach: a chat whose `chat_context.attach_vm` is set becomes `TaskExecutionKind::PreviewAttach`; the host connects into the already-serving preview VM's listener, lazily spawns the sidecar there, and drives Claude Code with the host-held identity (`lifecycle.rs:110-113`; `supervisor.rs:6395`; `backend/g5d/src/host/agent_drive.rs:1-6`; `chat_sessions.attach_preview_task_id`, migration 233).
- Answer to "same VM as the task?": preview and its attached chat share one VM; a preview never shares a VM with a regular task.

## H. Terminal

OBSERVED:
- `g5d-foundation/src/terminal` is a PTY session manager (`TerminalManager`; `TerminalSession::spawn / execute_command / execute_background / resize / write_raw / subscribe_output`) consumed by the agent's `run_in_terminal` tool inside the worker process (`terminal/mod.rs:1-8`; `backend/g5d-tooling/src/tools/terminal.rs:1-4`; wired at `backend/g5d/src/worker/mod.rs:5899`). The `connection/streams.rs` consumer the doc mentions does not exist (`backend/g5d-foundation/src/connection/` is empty).
- Frontend `TerminalPanel` renders `output` timeline entries streamed over the app WebSocket; there is no xterm dependency in `frontend/package.json` (`frontend/src/components/execution/TerminalPanel.tsx:1-6`).
- A legacy per-command VM shim (`TaskIsolationHandle::exec_command_in_vm`) boots one VM per terminal command when a worker runs at `IsolationLevel::Vm` outside the host-supervisor path (`backend/g5d-isolation/src/isolation/handle.rs:22-41`).
- Conclusion: no user-facing interactive terminal into a task VM exists; the only interactive-into-VM surface is preview chat-attach.

## Coupling

- **Supervisor <-> g5-api protocol.** The supervisor depends on ~20 internal routes and the heartbeat DTO pair, including sticky teardown/reconciliation, discard-fence, zero-slot and saturated-probe negotiation, and execution-attempt ids. Changing the unit of execution changes `task_service.rs` (33k lines) poll/claim/ack semantics and the `execution_lifecycle` kernel.
- **Materializer <-> CRDT.** The guest verifies `HEAD == base_commit` and then bridges `/work` into the CRDT peer (`lifecycle.rs:2263-2320`); `crdt_peer_plan` and `TaskAssignment.task_crdt_head_commits` tie workspace state to CRDT sessions; teardown dispositions are defined by CRDT flush evidence (`repository_scopes.crdt_session_id`, `dtos.rs:983-985`).
- **Golden / telemetry <-> supervisor loop.** Maintenance tick, consumer decision and builder lane are inlined into `accept_assignment_with_authority` and `run`.
- **Preview <-> host identity.** Tunnel and attach require the host-JWT path (`host_up.rs:1376`), which only the cloud provisioning path produces; self-hosted registers with `g5w_*` tokens.
- **Frontend.** `host_microvm_started/ready` timeline entries, `TaskObservation` (ports, image digest), preview `endpoint_url`, worker/host admin pages (`worker_hosts` routes; `workers::org_workers_router`).
- **Infra.** Packer, Terraform, startup script, systemd unit and the self-hosted script all encode the `/var/g5d-data/{runtime/bin,cache/repos,lib/workspaces,state,rootfs}` layout and the env var names above.
- **Attestation and discard fence** couple the guest image digest and host build to task admission (migration 294; `dtos.rs:787-830`).

## Simplification candidates (against the stated goals)

1. **One VM per workspace with an in-VM dispatcher (goals 1, 8, 9).** Today's unit is per-task-attempt VM + per-preview VM + parked chat VMs. Moving to per-workspace VMs removes per-task materialization (`workspace/materialize.rs` 5,760 + `repo_cache.rs` 2,542), per-VM rootfs COW cloning (`microvm.rs:145-260`), the handoff-file protocol, and the slot / deferred / superseded bookkeeping in `supervisor.rs` (`deferred_assignments`, `pending_superseded_exit_reports`, capacity backoff, saturated probe, `max_concurrent_previews` sub-cap).
   - Risks: libkrun here exposes no exec-into-running-VM API; the only in-repo precedent for talking to a live guest is a guest listener + TSI port + mux (`agent_host.rs`, retired microvm-agent path). Resource contention between tasks inside one VM; a hung VM now takes the whole workspace down; `ScratchDiskSpec` assumes one docker data-root per VM.
2. **Drop sticky runtimes as a server-side concept (goal 8).** If the workspace VM is long-lived and a chat is a process the dispatcher starts on demand, `chat_sticky_runtimes` (~20 migrations), `sticky_runtime.rs` (8k), `RuntimeKeeper` (1.7k + 3.3k outer), `chat_worker_loss.rs`, the parked-runtime teardown / reconciliation / discard-fence code in `supervisor.rs`, and the heartbeat fields `held/wake/busy/teardown/reconciliation` can go.
   - Risks: background subagent wakes (`g5.claude.wake`) and pending-question windows ride this machinery today; provider session continuity (`provider_session_id`, `continuation_summary` on `task_execution_attempts`) must be re-homed on the VM filesystem.
3. **Remove the CRDT bridge from the execution path (goal 2).** `run_agent_with_provider_fs_bridge`, `crdt_peer_plan`, `task_crdt_head_commits`, teardown "durablyFlushed" evidence and workspace release proofs are all CRDT-defined. With git as persistence, teardown collapses to "commit/push succeeded or not".
   - Risk: sync/merge job types and document editing use the same peer.
4. **Golden images (goals 1 and 3 trade-off).** With a persistent workspace VM, docker/cargo/npm caches live on the VM disk; the golden store, builder task class, consumer lane, telemetry shims and triggers (~10k lines incl. API) become unnecessary.
   - Risk: cold caches after VM discard/restore; needs a VM-disk snapshot or restore-from-git plus rebuild story.
5. **Host provisioning surface.** If hosts map 1:1 to workspaces, `concurrent_jobs`, `default_concurrent_jobs`, rotation and stale-host loops, and `cloud_orphan_sightings` simplify to per-workspace VM lifecycle (create on demand, checkpoint to git, delete on idle).
   - Risks: idle-VM cost; the host-JWT TTL/rotation problem persists unless credentials become per-VM and short-lived.
6. **Keep.** `stream_mux` + reverse tunnel (preview reachability without ingress; 605 + 2,199 lines), `PortAllocator`, `vmm_loopback`, `attestor`, OCI extraction/template store, `passt` (if NIC mode stays), PTY `TerminalManager` (basis for a user terminal), execution-attempt tracking minus runtime-custody fields.
7. **User terminal (cheap add).** A terminal could be one more `Channel` on the existing mux to the workspace VM's dispatcher, reusing `TerminalSession`; nothing exists today.

## Open questions (not settled by the code)

1. May a workspace VM run tasks for several branches at once, or is "workspace" = one checkout on disk? Multi-repo assignments (`repo_paths`) and the Project Docs alias layout assume a synthetic task root per task.
2. What is the dispatcher: a `g5d` daemon inside the guest (successor of `g5d worker` daemon mode, which already polls and multiplexes tasks with `concurrency`), or a host-side exec-into-VM RPC? libkrun offers no exec API here; the guest-listener + mux pattern is the only precedent.
3. How does a long-lived guest hold its g5-api identity? Today the host claims and hands the identity over per task; a per-workspace credential with rotation would be new.
4. Preview in the workspace VM (goal 9) puts the customer app beside agents. Today the preview guest deliberately has no G5 identity and no credentials. Is that no-identity property being dropped?
5. VM checkpoint semantics (goal 3): what is checkpointed besides git (untracked files, docker data-root, node_modules, provider session files under `/work/.g5`)? The scratch disk at `/var/lib/docker` is ephemeral today.
6. Chat without a running process (goal 8): where do queued user messages live while the VM is down, and who boots the VM on the next message? Today: API poll -> host -> VM; the inbox WS hub assumes a connected worker.
7. Cloud target: keep the GCE-only `CloudProvider`, or move per-workspace VMs to a different substrate? Self-hosted currently nests microVMs inside a Docker container with `/dev/kvm`.
8. Capacity defaults: `default_concurrent_jobs` targets ~16 GB per VM; with one VM per workspace, what are per-VM CPU/RAM and the idle-shutdown timeout (`chat_sticky_idle_timeout_min` = 60 is the closest analogue)?
9. Do approval/HIL flows (`hil_escalation`, pending-question windows on sticky runtimes) survive VM shutdown, or must they be answered while the VM is up?
10. Is the attestor / discard-fence guarantee (guest image digest bound to task admission) still required once tasks are ordinary processes in a persistent VM?
