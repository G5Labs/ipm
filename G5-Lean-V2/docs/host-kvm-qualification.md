# Host and real-KVM qualification record (K1)

Status date: 2026-09-22

Historical qualification of the prior runtime, not setup guidance or evidence for
the current backend. See the [current design and qualification](../.g5/pr_ontology_diff_design/virtiofs-393/ontology-plan.md).

This record closes the host portion of gap G5 in
[mvp-workspace-checkpoint-status-and-handoff.md](mvp-workspace-checkpoint-status-and-handoff.md)
(workstream K1). It names, for every host acceptance case, the test that
proves it, the exact candidate it was proven on, and what is still only
observable after deployment. The status words follow the handoff document:
*locally validated*, *Cloud/microVM validated*, *deferred*, *still unverified*.

## 1. Candidate

| Field | Value |
|---|---|
| Branch | `lean/k1-host-kvm-qualification` |
| Pull request | pending |
| Exact SHA for the Cloud evidence below | pending |
| Base | `main` at `47a5cc1694c32becf94e11a79e83faa4e5ea536b` (after the #117 checkpoint merge) |
| Protocols | API-to-host `ctl`, host-to-dispatcher mux, `VmSpec`, lifecycle states, migrations and Cloud Build configuration are unchanged |

## 2. What changed

1. **Deterministic host qualification tests** (`backend/g5d-host/src/qualification/`).
   The workspace runtime is driven through its real admission, journal,
   root-deletion and control-stream code by a scripted launcher
   (`g5d_host::test_support::FakeLauncher`, plus a reap-publisher seam in
   `g5d-isolation` behind `test-helpers`). No process fallback is involved:
   the launcher stands in for libkrun only where the case under test is a
   host race or refusal, and the real-KVM lane covers the boot itself.
2. **Host boot evidence.** At validated `runtime.ready` the host logs and
   journals `boot_to_ready_ms` and `restore_fetch_kib` on the
   `ready` event's bounded `log_tail` (`g5d_host::workspace_vm::BootSummary`).
   The API already stores that tail in `worker_vm_events`, so the
   measurement is durable without a protocol change.
3. **Real-KVM boot recipe.** A retained Rust E2E scenario,
   `host_boot_timings`, boots cold, restarted and new-child workspaces
   three times each through the public lifecycle API against the bound
   `g5d host` in the Cloud E2E lane and prints the observations for this
   record. It fails closed when the host cannot boot. (Updated when hosts
   began deleting a workspace's root at release: restarts no longer reuse a
   kept root, so the former warm assertions were removed.)
4. **Host-rooted defect fixed.** On macOS the clone-capability check asked
   `diskutil info` about an ordinary directory; `diskutil` only answers for
   mount points, so every workspace root on a `macos_dev` host was refused
   as non-APFS and the materializer unit tests returned early without
   running. The check now reads the filesystem type through `statfs(2)`.
5. **Host-relay defect found by the real-KVM run and fixed.** Host tunnels
   are process-local in the API (`TunnelRegistry`), while the workspace
   lifecycle reconciler runs on every API replica against the shared
   database every 2 s. In Cloud E2E build `820b0a62` the first cold boot
   reached `running` on the isolated replica's host, but the stop never
   completed: for three minutes the row carried
   `stop_transport_retry` / `HOST_TUNNEL_UNAVAILABLE` although the host's
   tunnel to the isolated replica stayed connected, because the other
   replica kept claiming the stop and its 5 s backoff starved the owner.
   `WorkspaceVmService::stop` now leaves a stop to the replica that holds
   the host tunnel (`stop_awaiting_tunnel_owner`, eligible again on the next
   tick) while the deadline allows; past the deadline the existing loss and
   release rules apply. Transport failures are now logged with host, VM and
   live-tunnel counts. Regression:
   `api_workspace_vms::stop_on_a_replica_without_the_host_tunnel_yields_to_the_tunnel_owner`.
   QA runs a single API replica, so this was not user-visible there; any
   multi-replica deployment and the CI isolated lanes were exposed.
6. **Host-rooted defect found by the next real-KVM run and fixed.** With
   the stop reaching the host, the dispatcher's shutdown flush failed with
   `FLUSH_API_UNAVAILABLE` (Cloud E2E build
   `5d6628b4-aa80-4b11-983b-9d88357a994e`, thirteen retries over three
   minutes). Under TSI networking the guest resolves names through public
   DNS, so the CI host maps its cluster-local API name (`g5-api`,
   `g5-api-re`) to guest loopback and expects the supervisor to stamp a
   per-VM control-plane forward for that name. The retired per-task path
   did (`loopback_api_forward` in the old `assignment.rs`); the workspace
   VM path passed an empty forward list, so no guest in CI could reach the
   API for flush, lease renewal or receipts. `WorkspaceVmRuntime` now
   derives the forward from `api_url` and `G5D_GUEST_HOST_ALIAS_RESOLVE`
   exactly as before (loopback URLs fixed, allowlisted aliases resolved per
   connection, none in NIC mode or for public control planes). Regressions:
   `workspace_vm::tests::control_plane_forward_covers_loopback_and_allowlisted_aliases_only`
   and `qualification::lifecycle::boot_stamps_the_control_plane_forward_for_a_trusted_alias_only`.
   QA's control plane is public (`https://qa.g5labs.ai`), so its guests were
   unaffected, which is why the two QA VM boots and stops on 2026-09-22
   succeeded while every CI guest could not flush.
7. **Third host-rooted defect, same symptom, next run.** With the forward
   stamped on the boot spec (build `bcfaad19-2349-4a19-b412-42234cb83875`),
   the owning replica drove the stop every fourteen seconds and the flush
   still failed with `FLUSH_API_UNAVAILABLE`: the libkrun backend bound the
   per-VM forwards only in the one-shot `run()` path and dropped them at
   return, while the long-lived `start()` path the workspace VM uses never
   bound them. `RunHandle` now carries the `LoopbackForwardGuard` for the
   VM's lifetime, bound on the derived VMM address exactly as `run()` does,
   and released with the handle. The cold boot in that run measured
   `boot_to_ready_ms=2297`, `api_observed_ms=2331`.

## 3. Acceptance cases

| G5 case | Evidence | Local | Cloud/microVM | Deployed |
|---|---|---|---|---|
| Host loss and reconnect | `qualification::control_session::reconnect_replays_the_journal_kills_superseded_vms_and_drains_to_completion`: tunnel lifetime 1 drops with a VM running, the VM survives, lifetime 2 opens with the full inventory in `host.hello`. API side: `api_worker_hosts::host_registry_reconciliation_marks_stale_hosts_with_tenant_scope` and `host_hello_inventory_*` (existing). | passed | rust-test lane (pending) | K2 / G7 |
| Event replay and ack | Same session test: events journaled while disconnected are the first messages after `host.accept`, contiguous per VM, and acknowledgement drains the journal. `ctl::tests` cover restart persistence and cumulative ack (existing). | passed | rust-test lane (pending) | K2 / G7 |
| Drain | Same session test: `host.drain` is accepted, `vm.start` is refused with `draining`, running VMs stay, `host.drained` carries the drain id after the last VM stops. | passed | rust-test lane (pending) | K2 / G7 |
| Unknown or superseded VM cleanup | Session test: a VM absent from `current_incarnations` is killed on reconnect and journaled `exited` then `stale_killed`. `qualification::lifecycle::reconcile_kills_superseded_incarnations_and_ignores_older_generations`: an older generation cannot revoke a newer set; `vm.stop` is fenced by ownership, not currency. | passed | rust-test lane (pending) | K2 / G7 |
| Root deletion at release | `qualification::lifecycle::start_then_stop_deletes_the_root_and_journals_the_boot_summary` (a restart sees nothing the previous guest wrote), `a_release_deletes_only_roots_no_vm_holds`, `natural_exit_settles_through_status_and_through_the_exit_monitor`, `workspace_vm::tests::host_start_deletes_every_root_and_leaves_other_entries`; receipt restore unit tests (existing). | passed | rust-test lane (pending) | K2 / G7 |
| Slot, boot-capacity and workspace-busy refusals | `capacity_refusals_are_definite_and_release_with_the_slot`, `concurrent_boots_beyond_the_boot_budget_are_refused_not_queued`, `start_then_stop_…` (`workspace_busy`, `already_running`). | passed | rust-test lane (pending) | n/a |
| Stop during boot | `stop_during_boot_cancels_the_reservation_and_frees_the_slot`: `accepted`, late VMM reaped, `boot_failed(boot_cancelled)`, slot released. | passed | rust-test lane (pending) | n/a |
| Spec refusals | `spec_refusals_name_the_first_failed_check`: org, size, credential-in-env, tag-only image, stale catalog, empty repo vector. | passed | rust-test lane (pending) | n/a |
| Idempotent commands | Session test: a retried `vm.start` returns the retained `started` without a second launch; a changed payload under the same key is `idempotency_conflict`. | passed | rust-test lane (pending) | n/a |
| No process fallback | `g5d host` refuses any isolation level below `Vm` (`backend/g5d/src/cli/host.rs`); the E2E host chains fail with `FATAL` without `/dev/kvm`; `smoke-libkrun-quick` is strict; `host_boot_timings` fails rather than skips. | n/a | E2E and Rust lanes (pending) | K2 / G7 |
| Cold, restart and new-child measurements | `host_boot_timings` (section 4). | not measurable on macOS (no KVM) | E2E lane (pending) | K2 / G7 |
| Relay | `tunnel::tests::relay_releases_http_streams_after_request_eof` (existing); end-to-end preview and terminal relay remain with R2 and Z2 under frozen protocols. | passed | existing value-loop selectors | deferred |

## 4. Boot measurements

Method: `boot_to_ready_ms` is the host's own clock from `vm.start`
admission (root materialization, receipt restore, VMM launch, dispatcher
validation) to the validated `runtime.ready`; `restore_fetch_kib` is the
object-store growth reported by `git count-objects` across the restore
fetches; `api_observed_ms` is the E2E client's wall time from the resume
request to the API reporting `running`, sampled every 250 ms. Cold runs fork
a fresh workspace from the seeded org's base each time (no retained root;
the host's URL-keyed bare cache for the org repository is cold on run 1 and
warm afterwards, because a host serves exactly one org); restart runs start
the first cold workspace again from its last save, on whichever host placement
picks; new-child runs fork that workspace and boot the fork for the first
time. Fixture: the seeded
rust-e2e org's docs repository; image and slot size: the E2E lane's pinned
`G5D_HOST_BASE_IMAGE` and `CPUS_PER_VM=2`, `MEM_MIB_PER_VM=4096`,
`G5D_SCRATCH_DISK_MIB=10240`. These are comparable observations, not a
latency target.

| Scenario | Run 1 | Run 2 | Run 3 | Median `boot_to_ready_ms` | Median `api_observed_ms` | `restore_fetch_kib` |
|---|---|---|---|---|---|---|
| cold | pending | pending | pending | pending | pending | pending |
| restart | pending | pending | pending | pending | pending | pending |
| new_child | pending | pending | pending | pending | pending | pending |

Lane artifact: pending (`g5-ci-e2e` build, stage `rust-e2e`, lines
prefixed `HOST_BOOT_TIMINGS`). On head `15f6a96` (build
`820b0a62-b92c-40bf-beee-de80925557a0`) the first cold boot reached
`running` on the real-KVM host (`boot_to_ready_ms=2182`,
`api_observed_ms=2221`, `restore_fetch_kib=0`, and `warm_root=false`, a field
hosts reported before they began deleting roots at release; captured from the
failed test's output), but the following stop did not complete
within 180 s; see section 2, item 5, for the cause and fix. The table is
filled from the first run after that fix.

## 5. Local validation on the candidate

| Command | Result |
|---|---|
| `cargo test -p g5d-host --lib` | 27 passed (18 existing, 1 control-session, 8 lifecycle); no `SKIP` lines |
| `cargo test -p g5d-foundation --lib -- workspace::restore` | 5 passed, including the macOS clone-check regression |
| `cargo clippy -p g5d-host -p g5d-isolation -p g5d-foundation -p g5-e2e --all-targets --all-features -- -D warnings` | clean |
| `cargo fmt --all -- --check` | clean |
| `cargo test -p g5-api --test api_workspace_vms --test api_workspaces --test api_task_lifecycle --test api_worker_hosts` (local Postgres) | 33, 45, 22 and 28 passed, including the new tunnel-owner regression |
| `python3 scripts/test_rust_e2e_lanes.py` | 3 passed (retained list now includes `host_boot_timings`) |
| `just validation-preflight` | pending |

The macOS host has no `/dev/kvm`; the real boot, the measurements and the
no-fallback proof come only from the exact-head Cloud lanes.

## 6. Still unverified after this record

- Deployed host loss, reconnect, replay, drain and root deletion at release
  on the QA GCE host: K2 / G7, after this PR and the checkpoint deploy.
- Guest OOM reporting (`vm.event oom`) has no deterministic host test; the
  host reaps the VMM and journals `exited` on any exit, which the natural
  exit test covers, but the cgroup signal itself is not simulated.
- Lease-expiry kill and the host timer are post-MVP (P8.14) and are not
  claimed here.
- Preview and terminal relay under resize, reconnect and transfer fencing
  stay with R2 and Z2.
