# V9 MVP scope amendment: keep the original substrate

> Ontology rebase note: links to current owners were retargeted. Immutable main-commit links preserve the historical ontology/source context of this record; they are not additional current contracts.


Status: **accepted by the program owner on 2026-09-19 and applied in the same change** to [lean-backend-design-v9.md](lean-backend-design-v9.md), its three companion notes, [lean-implementation-plan-v9.md](lean-implementation-plan-v9.md) and the tracker. §8 lists what was edited. Merged PRs and earlier versions stay unchanged. The GitHub issues are updated separately, on the owner's word.

## 1. Decision

The MVP is the nine proposals of `high-level-req.md`, plus the decisions made explicitly since (single owner and transfer, Git-only reads for non-owners, exact-name fork, flush only at graceful teardown with a nudge, no Commit or Flush-now, workspace search removed, preview as a visible task, native provider questions, forward migrations).

For the microVM, the host and for secret and identity management, the MVP keeps what the original G5 built. The binding statement is the team's: **"Environment variables are the initial credential mechanism (today's model); files, secrets managers and HTTPS-proxy capture come later."**

Decisions taken with this amendment:

- The sixteen mechanisms in §2 leave the MVP. Each one replaced a working original mechanism and none of the nine proposals needs it.
- Start and stop of a workspace VM ride the **new `ctl` stream** of design §6.2 (not the original heartbeat and poll path). P4.4a keeps the `ctl` protocol.
- **P4.1 and P4.2b are in progress and ship as planned.** Their prompts are not changed. What they land outside the amended scope is handled by their follow-up PRs (§4), not by new PRs.
- The four points of §7, all decided as recommended: no host-side lease kill timer in the MVP; the daily rotation of cloud hosts is accepted; the provider key rides the dispatch; the dropped scope gets the ids of §6.

How the design came to say otherwise is recorded in §9.

## 2. What the MVP keeps

Pointers are to today's code. † marks a pointer taken from the section audits that I did not recheck myself.

| # | The MVP keeps (original mechanism) | Leaves the MVP | Accepted limit |
|---|---|---|---|
| 1 | The provider key reaches the VM as environment and an in-guest proxy attaches it: `backend/g5d-host/src/assignment.rs` (provider env forwarded into the guest boot env), `backend/g5d-worker/src/llm_proxy.rs` (unix socket `G5D_LLM_PROXY_UNIX_SOCKET`), per-task provider secrets through `task_secrets_for_resolved_agent` and `apply_task_secrets_to_claude_code_config`. The only change: the account is the current workspace owner's, resolved at dispatch (`ProviderAccountService::resolve_frozen_for_dispatch`, already required by P4.6 item 1). | Host-held keys, the per-VM `g5-llm` listener behind the passt door, the proxy secret, the guest-local model broker | The key is inside the VM, as today |
| 2 | The VM's platform token is an environment variable and git and API calls go straight to the control plane: today `G5D_TOKEN` (`backend/g5d-worker/src/task_env.rs`); in v9 terms the incarnation token in `G5D_WORKSPACE_TOKEN` and the dispatcher's git credential helper (both landed in P4.2a). Existing `g5d` and MCP platform tools keep authenticating with it; this amendment adds and removes no tool. | Host git broker, host platform broker, "no platform credential in the guest" | Processes in the VM can use the token, as today |
| 3 | Bootstrap creates its child tasks through `POST /api/internal/jobs/:job_id/tasks` with the VM's token and the API's existing check that the caller owns the running bootstrap task (`submit_task_graph` and `lock_and_authorize_bootstrap_task_mutation` in `backend/g5-api-routes/src/routes/internal.rs`), re-keyed from worker to incarnation. | The invoke-bound 256-bit capability and its broker route | |
| 4 | Secrets are environment variables of the process that needs them. Repository and task secrets travel as the provider key does: resolved by the API at dispatch and carried with it, the way `TaskAssignment.secrets` do today; the dispatcher puts them in the environment of the process it launches. A changed secret or key applies at the next launch. | `credential_grants` references, `grants.resolve`, live `vm.grants.update`, and with them `VmSpec.secrets_env`, whose references the host resolves through `grants.resolve` | No mid-turn rotation |
| 5 | Host identity as today: `POST /orgs/:org/worker-hosts` mints a 24 h host JWT (`backend/g5-api-core/src/auth/jwt.rs` `mint_host_jwt`), delivered by the startup script on GCE and by `scripts/seed-ci-org.sh` in CI; external hosts receive a fresh token on the heartbeat response (`HeartbeatResponse.host_token`); cloud hosts are rotated before expiry (`cloud/rotation.rs`). The `ctl` tunnel authenticates with that JWT, as today's tunnel does. | One-use registration exchange, renewable credential, generations, runtime identity hash. **P4.4a no longer retargets the four CI seed/start/wait chains.** | See §7 point 2 |
| 6 | Usage is recorded as today: adapter result usage plus the proxy's provider usage into `usage_records` (`spend_service.rs`†, `llm_proxy.rs`), typed quota headers (`provider_quota_service.rs`†). | Request-id billing key, durable metering replay, display-only adapter usage | |
| 7 | **Amended 2026-10-09, SEC-002:** Linux passt uses a dedicated UID and an atomic host output policy before guest boot; TSI rejects metadata at its host socket boundary. The [guest-egress commitment](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/concepts/security.g5.md#protect-worker-metadata-from-guests) owns the exact limits and required DNS/cache/API exceptions. | General private-network policy, conntrack marks, the strict `workspace-vm-egress` lane, the org `workspace_egress_policy` and its admin route | Process mode and macOS host-loopback access remain trusted development facilities; deployed GCE denial still requires observation. |
| 8 | Network mode stays selectable by `G5D_VM_NET`: TSI by default (CI, self-hosted), passt on GCE. | "passt is the only mode", pinned DNS flags | |
| 9 | Internal git serves upload-pack and receive-pack on the authoritative repository with the repository-level read check and the landed P1.2 policy (`backend/g5-api-routes/src/routes/git.rs`); the host keeps its URL-keyed bare cache and materializer (`backend/g5d-foundation/src/workspace/repo_cache.rs`). P4.3a still adds the owner and ref-scope rules and the flush-ref namespace. | Authorized object views, scoped receive-pack through temporary repositories, view cache budgets, the host-only git-view artifact | Workspaces that share a repository can fetch each other's objects by OID, as today. **Decided 2026-10-10 ([G5Labs/G5-Lean#423](https://github.com/G5Labs/G5-Lean/issues/423)):** the repository is the unit of Git read access by design, not as an MVP concession; P8.12 is retired. |
| 10 | Preview origin as today (`backend/g5-api-routes/src/routes/preview.rs`): a per-preview subdomain under the platform domain when `G5_PREVIEW_HOST_SUFFIX` is set, with the single-use handoff ticket, the `__Host-g5_preview` cookie and CSP; the path relay otherwise. Re-keyed from job to preview task; owner-only checks stay. **Amended 2026-10-13, #488:** the path relay is removed and the session endpoint answers `PREVIEW_ORIGIN_UNCONFIGURED` without a suffix, per the [issue-488 design](../.g5/pr_ontology_diff_design/issue-488-remove-legacy-preview/ontology-plan.md); the [owning commitment](../.g5/ontology/software_architecture/workspace_vm.g5.md#g5.workspace.preview-origin-security) governs. | Separate registrable domain, `PREVIEW_ORIGIN_UNCONFIGURED`, removal of the fallback, two-domain CI TLS fixtures, QA DNS and certificate work | Preview shares the platform's registrable domain, as today |
| 11 | No deploy guard (none exists today). | `deploy-with-workspace-lease-guard.py`, the advisory lock, `POST /api/internal/ops/workspace-leases/observe`, the API-restart lease tests | See §7 point 1 |
| 12 | Base image from `.g5/workspace.yaml` with today's fail-closed resolution (`assignment.rs`†) and the org track/pin policy (`routes/worker_hosts.rs`†). | `allowed_images` allowlist, image consent and audit | |
| 13 | `runtime-labels/refresh` and registry-authoritative labels, and the fallback-region basket for stockouts (`cloud/bringup.rs`†). | Self-reported runtime identity, "no silent cross-region fallback" | |
| 14 | Guest users and images as shipped: the agent runs as uid 1000 (`G5D_GUEST_SIDECAR_NONROOT`), images carry rootful Docker (`infra/docker/Dockerfile.g5d-g5dev`†). **Amended 2026-09-23:** uid 1000 stays (the Claude CLI refuses `bypassPermissions` as root) and the g5dev image grants it passwordless sudo (`/etc/sudoers.d/g5-agent`); the host hands the materialized `/work` tree and the dispatcher the provider home to that identity (dispatch runtime reference). | Separate trusted-runtime UID rules, rootless-Docker requirement, revoking the blanket sudo, running flush git and the bootstrap membership read under a trusted identity (PR #133 review) | With blanket sudo the uid boundary is a convenience, not an isolation boundary: a module can become root in the guest |
| 15 | Configuration as P4.1 lands it (item 10 ships as planned, see §4). Follow-up PRs build no further consolidation on it: no `configs`-based egress policy, allowed-images or region keys, and no cutover gate on remaining legacy keys, which keep resolving through the live resolver (`config_service.rs`). | Further single-authority consolidation and the per-key migration that blocks cutover | |
| 16 | The `task_events` rename ships with P4.1 as planned and follow-ups use that name; the 30-day cleaner (`timeline_cleanup.rs`), pointed at the renamed table; the 128 MiB per-file session cap; the 60 s membership cache (`g5-authz`†; **Amended 2026-10-09:** removed with Redis, G5Labs/G5-Lean#405: membership is read on every check); the existing rate limiter (`middleware/rate_limit.rs`†; the shared platform-report quota now lives in Postgres); `repo_gc.rs`†. Flush-receipt retention (newest two plus pins) stays because the flush is new. | 90-day retention rewrite, 1 GiB session cap and 30-day bundle retention, `orgs.access_generation`, per-operation admission bounds, packed-object GC and the backup/maintenance barrier | Access removal reaches the next request immediately since the 2026-10-09 cache removal (G5Labs/G5-Lean#405); open durable reads, issued tokens and open streams keep their own TTLs until P8.14 |

The preview-origin gate of the v8 security review remains an accepted MVP exposure. Repository-wide Git read access is the intended boundary under row 9. Guest egress is governed by the bounded SEC-002 amendment in row 7; it does not reinstate the broader deferred network-policy system.

## 3. Replacement text for the normative statements

**Design §2, Credentials constraint.** "Environment variables are the credential mechanism in the MVP, as in the original: the provider key, repository and task secrets and the VM's platform token reach processes as environment. Files, secrets managers and HTTPS-proxy capture come later. GitHub and GitLab tokens still never enter the VM; publication is server-side."

**Design §4.3, invariant 9.** "**Credentials are environment variables.** The provider key is the current owner's, resolved at dispatch; the platform token is the incarnation's. Transfer releases the old VM before the new owner's first dispatch, so an old owner's key or token is never used after the owner commit."

**Design §6.5.** The section becomes "Credentials and secrets": the three rows 1, 2 and 4 above, the `secrets_env` paragraph as it stands, and one paragraph "Guest network" stating rows 7 and 8 as amended. The host-proxy tables, grant rules and broader configurable egress policy remain in the post-MVP item of §6.

**Design §4.1, worker host.** "…boots and stops workspace VMs on command and relays streams." (no proxies and, by §7 point 1, no host-side lease timer)

## 4. Consequences per PR

Item numbers are those of the plan §6 prompts before this amendment. A removed item keeps its number in the prompt as a one-line note naming the post-MVP PR that now owns it, so numbering stays stable.

**P4.1 (rahul) and P4.2b (kaihui) are in progress and ship as planned; their prompts are unchanged.** What they land outside the amended scope is handled by follow-up PRs:

- *Schema from P4.1 that the MVP no longer uses stays dormant:* `orgs.access_generation`, the `workspace_egress_policy` key, the host registration and credential columns (`registration_*`, `credential_generation`, `credential_expires_at`, identity hash, `runtime_identity`) and `workspace_vms.source_manifest_digest`. No MVP PR reads or writes them. They belong to P8.10, P8.11, P8.12 and P8.14; P7.2 drops whatever is still unused when it runs. Dropping them earlier would cost a forward migration and a rehearsal for no MVP value.
- *The `task_events` rename and the configuration-authority work of P4.1 stand.* Follow-ups use them as landed and extend neither.
- *The bootstrap capability that P4.2b item 9 adds to `modules.invoke` is removed by P4.8*, the first follow-up that touches bootstrap, because the MVP authorizes graph submission API-side (row 3).

| PR | Removed | Replaced by |
|---|---|---|
| P4.3a | item 9 (object views, scoped receive-pack, budgets); the Git-scope half of item 11; the S1 half of item 13; item 14 (host-only git-views route); the barrier in item 15; packed-object GC, backup barrier and IOPS budget in item 3 | receive-pack and upload-pack as today plus the owner, ref-scope and flush-ref rules |
| P4.4a | item 1; the register route in items 2 and 8; the renewable credential in items 3, 4 and 5; items 6 and 7; the CI retargeting in item 9; the credential type/hash/generation half of item 10; `lease.acquire`, `grants.resolve` and the host-monotonic `vm.status(refresh_lease)` fields in items 2 and 10 | the `ctl` tunnel authenticates with today's host JWT; CI host bring-up stays as it is; `lease.renew` stays, relayed to the control plane |
| P4.4b | item 2's `credential_grants`; item 6's manifest materializer; items 9, 10, 11, 12, 13, 15; the host kill timer of item 4; the lease, egress and process-isolation parts of item 14; the grant wording of item 1 | boot env built as today; today's repo cache and materializer; network mode from `G5D_VM_NET`; a VM that is no longer current is stopped by the reconnect inventory rule or by `vm.stop`, never by a host timer |
| P4.5 | items 19, 20, 27 (the `access_generation` column from P4.1 stays unwired), 31; the broker half of item 21; the allowlist, provenance and `lease.acquire` parts of item 26; the per-operation bounds in item 28; `source_manifest_digest` in item 30; `credential_grants` and the host-originated lease in item 2 | the API grants the lease at placement and extends it on `lease.renew`; lease expiry marks the row failed and the fence rejects the old token |
| P4.6 | the capability in item 15; the billing rule in item 17 | item 15: today's check re-keyed to the incarnation; usage recorded as today |
| P4.7, P4.8 | the P4.12 dependency; P4.8 item 9's capability wording | P4.8 also removes from the dispatcher the bootstrap capability mint and delivery that P4.2b landed |
| P4.11 | item 2's broker and "no token to descendants" rule; item 12; the matching half of item 10; "no inline credentials" in item 1 | `agent.turn` receives the owner's provider key with the dispatch, the way `TaskAssignment.secrets` travel today, starts today's in-guest `llm_proxy` with it and builds the provider environment as the P3.2 runner does |
| **P4.12** | the whole PR leaves the MVP (§6) | |
| P4.13 | the one-use exchange in items 1 and 7; item 4's retirement of `runtime-labels/refresh`; the egress policy and deploy-guard lock in item 6; allowed-images and the region rule in item 7 | the existing provisioning route and token service |
| P5.1 | items 5, 9, 10, 13 and the S2 half of item 12 | today's origin handling, keyed by the preview task |
| P5.2 | the P4.12 dependency; "proxied egress" in item 2 | |
| P4.9a | **must not delete the in-guest LLM proxy** (item 2 lists it); "strict egress and scoped-cache regressions remain required" in the acceptance | |
| P4.9b | item 9's one-use registration note; the same acceptance sentence. Whether `worker_tokens` is dropped waits for P9.1, which decides self-hosted host registration | |
| Plan §2.2, §7.1 | the three security gates as MVP gates; the rows "Git object confidentiality", "Preview origin and guest egress" and "MVP control-plane deployment"; the proxy, typed-credential and scoped-manifest clauses in "Host protocol, lease and proxies" and "Attachment and cache"; P4.12 in the owner columns | |

## 5. PR map after the amendment

Dependency edges that existed only because of removed items go away: P4.3a no longer needs P4.4a (the host-only git-view route was the reason), P4.4b no longer needs P4.3a (scoped packs were the reason), and P4.5, P4.7, P4.8, P5.1 and P5.2 no longer need P4.12.

| | today | amended |
|---|---|---|
| MVP PRs left | 21 | 20 |
| Longest dependency chain | 11 | 9: P4.1 → P4.4a → P4.4b → P4.5 → P4.11 → P4.6 → P2.1 → P2.2 → P4.10 |
| Dependency-safe rounds for three people, with the current executors | 12 | 11 |

| Round | zcl | kaihui | rahul |
|---|---|---|---|
| 1 | — | P4.2b (in progress) | P4.1 (in progress) |
| 2 | P4.4a | — | — |
| 3 | P4.3a | P4.13 | — |
| 4 | P4.4b | — | — |
| 5 | P4.5 | — | — |
| 6 | P4.11 | P4.3b | P5.2 |
| 7 | P4.6 | — | — |
| 8 | P2.1 | P4.7 | P4.8 |
| 9 | P5.1 | P2.2 | — |
| 10 | P4.9a | P2.4 | P4.10 |
| 11 | P4.9b | — | — |

No executor changes. P4.3a no longer waits for P4.4a and could run beside it, but it stays with zcl by the owner's decision, so zcl carries ten of the twenty PRs and the table has eleven rounds while the dependency depth is nine.

The three merges from the earlier consolidation evaluation that still apply (P4.9a+P4.9b, P2.1+P2.2, P4.11+P4.6) would bring this to 17 PRs and a chain of 7; the fourth (P4.4b+P4.12) is moot. That remains a separate decision.

## 6. Where the dropped scope goes

Nothing is deleted from the record. The specification text of the dropped mechanisms stays in the design and the companion notes, marked post-MVP with the id that owns it. The work moves after the MVP, behind the audits of P8.8.

| PR | Title | Owner | Size | Depends on | Holds |
|---|---|---|---|---|---|
| P4.12 (id and prompt kept, state post-MVP) | Host proxies: owner provider account, git and platform credential proxy, grant updates | Worker & Host | L | P4.4b | rows 1, 2, 3, 4 and 6 |
| P8.10 | Host registration exchange and renewable host credential | Worker & Host | M | P4.4a, P4.13 | row 5 |
| P8.11 | Guest egress enforcement, network mode and runtime process isolation | Worker & Host | L | P4.4b | rows 7, 8 and 14 |
| P8.12 | Git object views and scoped receive-pack. **Retired 2026-10-10 ([G5Labs/G5-Lean#423](https://github.com/G5Labs/G5-Lean/issues/423)):** the repository is the unit of Git read access by decision; no object-view work is planned. | Workspace (+Git) | L | P4.3a, P4.4b | row 9 |
| P8.13 | Preview origin isolation on a separate registrable domain | Workspace | M | P5.1 | row 10 |
| P8.14 | Operational policies: host lease timer and deploy guard, image and fleet policy, configuration authority, retention, admission bounds, GC and backup barrier | Platform | L | P4.5, P4.13 | rows 11, 12, 13, 15 and 16, and the host-side lease kill timer |

## 7. The four points, as decided

1. **No host-side lease kill timer in the MVP.** The deploy guard (row 11) existed to protect running VMs from the timer of design §6.4 during an API outage or a Recreate deploy. Both leave together (P8.14). The MVP keeps the incarnation fence at every API boundary and the server-side lease: the dispatcher renews through `lease.renew`, a lease that expires marks the row failed, and a VM that is no longer current is stopped by the reconnect inventory rule or by `vm.stop`. Today's hosts tolerate an API outage and so do MVP hosts.
2. **Daily rotation of cloud hosts is accepted.** With the 24 h JWT, `cloud/rotation.rs` replaces each cloud host before expiry, so every workspace on it gets one graceful teardown a day, with the nudge and the flush. P8.10 removes this later.
3. **The provider key rides the dispatch**, the way `TaskAssignment.secrets` do today. This reverses "no inline credentials" in `AgentTurnInputV1`; the Agents owner confirms the field when P4.11 starts.
4. **The ids of §6 stand**, and P4.12 keeps its id as a post-MVP PR.

Two choices made while applying the amendment, to keep it coherent, both confirmed by the owner on 2026-09-19:

5. **All secrets ride the dispatch.** `VmSpec.secrets_env` resolved its references through `grants.resolve`, which left with the grants (row 4). Rather than keep that protocol for one field, repository and task secrets travel with the dispatch like the provider key, which is how `TaskAssignment.secrets` work today. The alternative is to keep `grants.resolve` in P4.4a and P4.4b for `secrets_env` only.
6. **The incarnation token's expiry is not the lease TTL.** With the token in the environment of long-running processes (row 2), a token that expires with a 300 s lease would die inside an agent turn. The MVP mints it at placement with a 24 h default, keeps `lease.renew`, and relies on the fence for currency, as decision 1 already does. Today's worker token is long-lived in the same way.

## 8. What was edited

- Design v9: §2 (credentials), §4.1, §4.2 diagram, §4.3 invariant 9, §5.1 (`access_generation`, `source_manifest_digest`), §5.3 (views), §5.4 (GC, barrier), §5.8 (rate bounds), §6.2 (credential, registration, runtime labels), §6.3 (grants, images, git cache, network), §6.4 (deploy guard; the timer per §7 point 1), §6.5 (rewritten), §6.8 (origin), §6.10 (region), §8.7 and §11.1 (metering, platform credential), §12 (configs, rename, retention, caps, egress owner), the matching rows of §14, §15, §17.2 and the §18 register.
- `worker-host-api-v9.md` §2 to §7 and §12 to §14; `agent-api-surface-v9.md` §4 (usage), §6.5, §6.6, §9; `workspace-vm-api-v9.md` where it cites the git-view artifact.
- Plan v9: the status paragraph, §2.2, a new §3.9, §4, §5.1 to §5.4, the prompts of §4 above, new prompts P8.10 to P8.14 that quote every moved item verbatim with its origin (P4.12 received its moved items the same way), the later prompts that assumed a moved mechanism had landed (P4.10, P7.2, P8.4, P8.7, P8.8, P9.1, P9.3; P8.7 and P9.3 now depend on P8.14), §7, §7.1 and §7.2; the tracker's Depends on cells and new rows; a decision entry in `v9-meeting-decisions.md`.
- GitHub issues (not part of this change; done on the owner's word): regenerate the bodies of #81, #86, #87, #88, #89, #90, #91, #93, #94, #96, #97, #98, #100, #102 and #103, the fifteen issues whose prompt or dependencies changed; take the `MVP` label off #89 (P4.12), which stays open because P4.12 remains planned after the MVP. #84 (P4.1) and #85 (P4.2b) are in progress and stay as they are.

## 9. Why v9 said otherwise (record)

1. Draft 1's open question Q42 asked "Keep the host-side LLM proxy (provider keys stay on the host…) rather than placing keys in the VM" and was answered "Keep". The premise was inverted: the research notes it drew on say the deployed `microvm` mode passes the key into the guest and runs the proxy there. The answer confirmed today's mechanism and was recorded as "provider keys never enter the VM".
2. The worker-host note built host proxies because of that line ("Under v6 no provider credential enters the VM, so the proxy has to live on the host").
3. Draft 7 narrowed the team's environment-variable decision to "repository and task secrets" and did not flag the narrowing for confirmation.
4. v8 and v9 went single-owner, which removed the multi-user reason for the proxies; the security reviews then hardened what was there (door exposure, egress gate, object views, preview origin).
