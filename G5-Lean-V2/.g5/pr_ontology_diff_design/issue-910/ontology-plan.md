# Retire `codex exec` from G5-managed execution

Work: [#910](https://github.com/G5Labs/G5-Lean/issues/910), [owner request](../../chat_intent/issue-910/request.md).
Authorized branch: `issue-910-retire-codex-exec-from-all-g5-managed-agent`.
Baseline: `origin/main` / `ce6e4bb50` (rebased 2026-10-28 from the design baseline
`9e72c262b`; both contain the issue's inspected `80623150c`). Owner approved the design
on 2026-10-28 and asked for the real-binary tests first.

Governing blocks: [supervisor](../../ontology/features/chat.g5.md#g5.chat.supervisor),
[exec retirement](../../ontology/features/chat.g5.md#g5.chat.codex-exec-retirement),
[old-runtime refusal](../../ontology/features/chat.g5.md#g5.chat.codex-runtime-refusal),
[frozen attempt](../../ontology/features/chat.g5.md#g5.chat.frozen-attempt),
[Codex formal launch](../../ontology/features/agent_approvals.g5.md#g5.agent-approval.codex-launch),
[confined execution](../../ontology/features/agent_approvals.g5.md#g5.agent-approval.execution).
This continues the [#809 design](../issue-809/ontology-plan.md); its transport, acceptance,
custody and settlement contracts are unchanged.

## Owners

No new owner, lifecycle, table or migration. The API dispatcher still owns admission and
frozen-contract selection; the guest dispatcher owns module/input validation; `g5d-agent`
owns launch and the transport adapter; the approval service owns assignments and evidence.
The change removes a transport variant and moves formal review onto the existing
`Transport::CodexAppServer` adapter with a review-specific launch and thread policy.

## Retirement boundary

Managed production callers at baseline:

| Caller | Selected by | Launcher |
|---|---|---|
| Ordinary Codex `agent.turn/1.0.5` (`exec`, `exec resume`) | API when the VM catalog predates `2026.10.24-codex-app-server` or is unknown; absent snapshot fields on reconstruction | `runtime.rs` exec branch |
| Codex formal review `1.0.6`/`1.0.7`/`1.0.9` (assignment schemas 2/3/4) | API from the assignment schema, for every provider | `formal_review::launch` Codex branch |

Non-production consumers to migrate or remove: fake-provider exec mode and its tests,
`dispatch_modules` exec cases, `test-native-provider-transport.py --codex-transport exec`,
`check-codex-cli-flags.sh`, the exec leg of `codex-memory-native-test.py`, and the
`codex exec --help` gates in `Dockerfile.g5d` and `Dockerfile.g5d-prebuilt`.
Recorded exec frames remain historical evidence. `CodexFrameMapper` item translation stays
because App Server uses it; only exec top-level stream handling is removed.

Deployment facts that bound the problem:

- A VM's catalog is chosen by the API that starts it and must match at Ready
  (`host_control_state.rs`). A host that cannot provide the catalog fails the boot through
  the existing path, so "incompatible workers" reduce to **VM incarnations still running
  from a pre-retirement catalog**.
- Unconfirmed reservations are re-invoked only on their exact `vm_id`/`epoch`
  (`load_unconfirmed_agent_dispatch`). A replaced VM gets a new attempt with a newly
  frozen contract. An exec-frozen reservation therefore can only reach the older guest
  binary that admitted it, and that binary still has exec.
- Formal reviews run in temporary workspaces on freshly started VMs.

Hence one release can remove exec from the guest. The new API never creates an exec
reservation. The new guest never sees one. Existing exec work drains on old incarnations
without conversion. The API's reconstruction mapping
for old Codex contracts (`frozen_ordinary_module` absent fields; formal `1.0.6`/`1.0.7`/`1.0.9`)
is the only remaining compatibility, and it launches nothing itself.

**Removal condition** for that mapping (stage 2): a read-only inventory on each deployment
shows zero `starting`/`running` VM incarnations from catalogs before the retirement catalog
with an attempt where `ended_at IS NULL`, provider Codex, and no frozen `codex_transport`.
I have no access to deployed databases. The owner or an operator must run this before
stage 2, and before closing the issue.

## Ordinary admission

`reserve_queued_agent_dispatch` keeps one rule: Codex requires
`catalog_supports(vm_catalog, CODEX_APP_SERVER_CATALOG)`. If not, refuse admission through the existing kept-input
refusal (`permission_policy` precedent, `requeue_after_admission_failure = false`) with
"This workspace's runtime predates the current Codex integration. Stop and start the
workspace, then send again." No reservation, no exec. Claude keeps `1.0.5`. Old-catalog VMs that already
have App Server (`2026.10.24`+) keep receiving `1.0.8`.

## Formal review on App Server

New guest module `agent.turn/1.0.10` (Codex formal, App Server), in a new catalog version
appended to `SHIPPED_MODULE_CATALOGS`. API: a Codex formal attempt freezes
`agent_module_version=1.0.10` and `codex_transport` in the policy snapshot; reconstruction
derives formal versions from the snapshot first (snapshot present → `1.0.10`; absent →
historical `1.0.6`/`1.0.7`/`1.0.9` by schema). Claude formal is unchanged. A Codex formal attempt on a
VM without `1.0.10` gets the same refusal (rare: VM started before deploy). The
review request stays pending, per [attempts](../../ontology/features/agent_approvals.g5.md#g5.agent-approval.attempts).

Guest validation: `codex_transport` ⇔ module ∈ {1.0.8, 1.0.10}; formal ⇔ module ∈
{1.0.6, 1.0.7, 1.0.9, 1.0.10}; `1.0.10` ⇒ Codex and accepts assignment schema 2, 3 or 4
(schema 4 is the only shape new captures write; 2/3 remain reachable through frozen retries);
`1.0.5`/`1.0.6`/`1.0.7`/`1.0.9` with Codex is refused (`CODEX_TRANSPORT_CONTRACT_MISMATCH`).

Launch (`formal_review::launch`, Codex): `codex app-server --listen stdio:// --strict-config`
plus the existing formal `-c`/`--disable` set (approval never, `project_doc_max_bytes=0`,
the `g5_formal_review` root-deny profile, skills/tools/web/instructions off, the disabled
feature list, `model_instructions_file`, the `formal_review` MCP server with pre-approved
tools and supervised env), pin feature opt-outs, memory disabled. Exec-only flags map as follows:

| exec flag | App Server equivalent |
|---|---|
| `--ephemeral` | `thread/start.ephemeral=true`; response must show `ephemeral=true` and no rollout path |
| `--ignore-user-config` | fresh private `CODEX_HOME` with only `auth.json` and the skill; sanitized env |
| `--ignore-rules` | fresh home has no rules; shell tool disabled; native test plants hostile rules |
| `--strict-config` | same flag on `app-server` |
| `--skip-git-repo-check`, `--json` | not applicable |

`thread/start` sends model, fresh `cwd`, `approvalPolicy=never`, `ephemeral=true` and **no
`sandbox`** field, so the permission profile is not overridden. The adapter's thread check
gains, for formal only: `thread.ephemeral==true` (stable, required), no non-null
`thread.path` (unstable, optional), `instructionSources==[]`, sandbox `readOnly`. `turn/start` carries only the frozen prompt and
effort. `thread/resume` is unreachable (formal input already rejects attempt>1, resume,
continuation, attachments). Native requests remain unsupported and fail the turn. The
execution deadline still starts at confirmed acceptance (turn/start response). Harvest and
evidence are unchanged.

Planning probe (0.164.0, no upstream turn) supports the table above; see the block metadata.
`activePermissionProfile` is returned but not in the stable schema, so confinement is
proven by native tests, not by a runtime field check.

## Real-binary formal validation (2026-10-28)

Run first, at the owner's request, before any other implementation. Code: the formal
App Server launch and adapter mode, guest `1.0.10` catalog entry and validation. Exec
still exists for the fake run until the fake provider migrates. Pinned binaries:
`/opt/g5/bin` codex-cli 0.164.0 and Claude 2.1.308, synthetic Responses/Messages upstream.

- `formal_review_real_native_clients_enforce_tools_and_report_transport`: **passed**.
  Both providers × (approved/schema 4, rejected/3, rejected/2), Codex over App Server
  `1.0.10`. Hostile workspace `AGENTS.md`/`.claude`/`.codex/config.toml` and a hostile
  owner home (config with shell, multi-agent and an inherited MCP server, `AGENTS.md`,
  rules, Claude hooks and `CLAUDE.md`). Refused attempts: shell (`unsupported call:
  exec_command`), working-tree read via `apply_patch` (denied by the profile), the
  inherited MCP server (`unknown MCP server 'inherited_helper'`), delegation
  (`unsupported call: spawn_agent`), an unassigned report path. No hostile
  instruction reached the model. No rollout under the review home. Exact report bytes
  and digest, exactly-once replay, checkout unchanged. MCP tools ran without any native
  approval request.
- The Codex tool surface is identical under the former exec launcher and App Server;
  the test now asserts it exactly.
- `formal_review_real_codex_cancellation_and_missing_report_settle_no_verdict`: **passed**.
  Owner Stop after the input reached the model settles `cancelled`. A completed turn
  without a report fails `approval_output_invalid`; neither has verdict evidence or session files.
- Mutations proved discrimination: sending `sandbox: "read-only"` made the working tree
  readable (test failed); a non-ephemeral thread with the adapter check removed left a
  rollout the test detected.
- `g5d-agent`, `g5d-dispatch`, `g5-internal-api` lib suites and the fake formal roundtrip passed.

Both new native tests run in the `formal-review-native-clients` CI step.

Exec-written history (scratch probe reusing `scripts/test-codex-app-server.py`'s driver
and fixture, 2026-10-28): a manual `codex exec` turn in home A, with only its rollout
copied to home B, resumed under App Server with `excludeTurns`. **Passed**: same thread,
earlier fact present once in the next Responses request, prior input not replayed.
Implementation keeps this as a checked-in exec rollout fixture resumed by the repo
probe, so the test needs no exec at run time.

## Historical chats

Sessions whose earlier turns ran on exec continue in a new `1.0.8` attempt via
`thread/resume` of the saved rollout, or the existing resume-refusal → history
reconstruction path. Both already apply to sessions that crossed the #809 deploy. Add a
native probe that resumes a checked-in exec-written rollout under App Server. If 0.164.0
refuses it, the reconstruction path must produce a clear outcome without replaying input.

## Implementation (stage 1)

- API (`agent_dispatch.rs`): admission refuses Codex when the reserved VM's catalog
  predates `2026.10.24-codex-app-server` (ordinary) or `2026.10.28-codex-formal-app-server`
  (formal), with the kept-input notice; every new Codex attempt freezes `codex_transport`
  with `1.0.8`/`1.0.10`; `frozen_module` rebuilds reservations snapshot-first, mapping
  absent/`1.0.5` snapshots to the historical per-schema module.
- Guest: catalog `2026.10.28-codex-formal-app-server` adds `agent.turn/1.0.10`; the
  dispatcher refuses Codex without the App Server transport on any module; the runtime
  has no `Transport::CodexExec`, exec launch, exec resume or last-message file; formal
  review launches App Server (`formal_review::launch`, `codex_app_server_adapter`).
  `CodexFrameMapper` keeps only the shared `map_frame` item translation.
- Fake `fake-codex` serves App Server only (accepting the formal `--strict-config` and
  `--disable` flags, echoing `ephemeral`, persisting no rollout for ephemeral threads)
  and refuses `exec`. Dispatcher shell fixtures use the shared
  `tests/fixtures/codex-app-server/items_peer.py` peer.
- Probes/images: `check-codex-cli-flags.sh` validates the App Server launch surface and
  formal configuration through `app-server --print-startup-config`; both Dockerfiles gate on
  `app-server --help`; native transport, catalog and memory scripts drop their exec legs;
  `scripts/test-codex-app-server.py` resumes the checked-in exec-written rollout.
- The test runtime peer records the bundled catalog on its VM, matching production Ready.

## Stage-2 inventory (read-only, per deployment)

```sql
-- Running incarnations by catalog.
SELECT module_catalog_version, count(*) FROM workspace_vms
WHERE status IN ('starting','running') GROUP BY 1 ORDER BY 1;
-- Unsettled Codex attempts frozen on exec (must be zero for stage 2).
SELECT vm.module_catalog_version, count(*)
FROM task_attempts a
JOIN workspace_vms vm ON vm.org_id = a.org_id AND vm.vm_id = a.vm_id AND vm.epoch = a.epoch
WHERE a.ended_at IS NULL AND vm.status IN ('starting','running')
  AND COALESCE(a.policy_snapshot_ref #>> '{agent_launch,resolved,provider,value}',
               a.policy_snapshot_ref #>> '{agent_launch,provider,value}') = 'codex'
  AND NOT (a.policy_snapshot_ref ? 'codex_transport')
GROUP BY 1;
```

## Staging

1. **Stage 1 (this PR):** the ontology above; API admission/selection/reconstruction; guest
   catalog + validation; formal App Server launch; removal of `Transport::CodexExec`,
   exec launch branches, fake exec mode, exec probes/flag gates/image checks (replaced
   by App Server equivalents); tests. After deploy, no G5 code path launches exec
   except old guest binaries finishing already-reserved work.
2. **Stage 2 (small follow-up PR, after the removal condition holds):** the API refuses
   Codex+exec reconstruction explicitly and deletes the remaining compatibility tests.
   The issue closes only after stage 2 and the evidence below.

## Risk evaluation

| Risk | Likelihood / impact | Mitigation |
|---|---|---|
| App Server formal launch is less confined than exec (permission profile overridden, project instructions/config loaded, MCP approvals prompting) | Medium / **high**: review integrity | No `sandbox` in thread/start; runtime thread-response checks; native tests with hostile `config.toml`, `AGENTS.md`, rules and hooks, and attempted shell/file/network/delegation; strict two-tool assertion. Blocker rule: if equivalence fails, formal migration stays outstanding (no exec exception). |
| MCP tool approval arrives as `mcpServer/elicitation/request` under App Server | Medium / high: every review fails | Native real-binary formal test with synthetic upstream for both verdicts before any other formal changes land. |
| In-flight exec work broken by deploy | Low / medium | Incarnation binding keeps it on old guests. Running turns settle normally. Tests cover lost ack, stale events, retry and replacement. |
| Old-catalog VMs refuse Codex (UX regression for long-running VMs) | Medium / low | Clear kept-input notice. Inventory sizes it. Claude unaffected. |
| Exec-written rollouts not resumable by App Server | Low–medium / medium | Existing reconstruction path, plus a new native probe. |
| Frozen-contract misclassification (silent conversion) | Low / high | Snapshot-first selection. Guest refusal of Codex on exec modules. Table tests over snapshot/schema/provider/catalog combinations. |
| Claude regression via shared modules 1.0.5/1.0.6/1.0.7 | Low / high | Claude paths untouched. Existing Claude dispatcher and native formal tests stay required. |
| Large test/fake churn hides behavior change | Medium / medium | Port exec cases to App Server one-for-one, preserving their asserted outcomes. |
| Image/build gates drop a needed check | Low / medium | Replace with `app-server --help` flag gate plus the existing schema/feature probe. |

Overall: **medium**. Ordinary chat changes little (#809 already made App Server the
default). The high-impact uncertainty is formal-review confinement, so it is validated first.

## Verification plan

- Unit/contract: admission and reconstruction over compatible, App-Server-only, older,
  missing and unknown catalogs; formal snapshot/schema/provider matrix; guest validation
  matrix; adapter formal thread checks.
- Dispatcher (`dispatch_modules`): Codex ordinary and formal through the fake App Server,
  covering retries, lost ack, stale events, Stop/Cancel, failed harvest, queued successor,
  replacement worker. Claude suites unchanged.
- Native (real 0.164.0, synthetic upstream): formal confinement and both verdicts, bounded
  output, cancellation, no session files; transport/catalog matrix without exec; exec-rollout resume;
  memory test App Server leg.
- API/DB integration and VM/browser CI on the exact head.
- Not executable by me: authenticated provider journeys, deployed inventories, deployed
  observation. These are reported as outstanding until the owner provides them.

## Executed verification (2026-10-28, local, before CI)

Source: commit `978c5b496` plus the follow-up commit with Clippy and test-helper fixes. Pins: codex-cli 0.164.0 and Claude
2.1.308 (`/opt/g5/bin`), synthetic upstreams, private Postgres 16.

- `cargo test -p g5d-agent -p g5d-dispatch -p g5-internal-api --lib`: 183 + 66 + 151 passed.
- `cargo test -p g5-api-core --lib agent_dispatch`: 24 passed (frozen-module matrix, catalogs).
- `cargo test -p g5d --test dispatch_modules` (fake providers, full): **100 passed**, 4
  ignored native tests run separately below.
- Native, `--ignored`: `pinned_native_catalog_and_production_launches_match` (412 Claude
  and 352 Codex combinations, all eight launch/auth routes with Codex over App Server,
  stable schema, hostile features, same-/replacement-home and exec-written history
  resume, Claude lifecycle) and both formal native tests: **passed**.
- `scripts/tests/codex-memory-native-test.py`: save, recall and read-only deny **passed**.
- `cargo test -p fake-provider-cli`: 39 unit + 51 CLI + 3 steering passed.
- `scripts/check-codex-cli-flags.sh` on the pin: passed; a mutation adding a valid
  value to its invalid list made it fail.
- Strict Clippy on `g5d-agent`, `g5d-dispatch`, `g5-internal-api`, `fake-provider-cli`,
  `g5-api-core` (all targets): clean. `cargo fmt --check`: clean.
- API integration, agent/chat/formal/VM/worker-host modules (554 tests): the first run
  exposed 11 Codex device-auth/steering fixtures whose test VM recorded no catalog, which
  admission now correctly refuses. The test VM helpers record the bundled catalog, as
  production Ready does; `api_task_lifecycle` then passed 91/91, and the other 463 selected
  tests passed in the first run.
- Strict Clippy on `g5d` and `g5-api` (all targets, `test-helpers`): clean.

Not executed here, so still outstanding: Docker image builds, VM/browser E2E CI, the
full API integration suite, authenticated provider journeys (fresh text/tool turns,
attachments, queued successor, Stop/Cancel, reconnect, worker replacement, resume refusal
and failed harvest on real accounts), the stage-2 deployed inventory and deployed
observation.

## Self-review

Every managed caller is covered. No frozen contract is rewritten. Exec work drains through
the existing incarnation binding. Formal review keeps its isolation, enforced by checks
the App Server surface exposes and proven with the pinned binary. Claude and the shared
modules are preserved. The removal condition is explicit. The two uncertainties named in
the plan, native formal confinement/approvals and exec-rollout resume, were validated
first and passed. What remains is deployment-side evidence the owner must supply.
