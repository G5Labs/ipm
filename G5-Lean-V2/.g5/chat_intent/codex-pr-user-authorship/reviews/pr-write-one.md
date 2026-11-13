# Shared PR write review — one

Independent, read-only discovery review of the frozen shared checkout. Started with
`ontology-plan.md` → `Ontology changes and governance`, then `request.md`, the full
design, actual ontology diff, governing blocks and their metadata. No peer report or
author review artifact was read. No production, test or ontology files were edited.

The assignment supplied the scope and `/tmp/g5-pr-write-review-snapshot.tgz`, without
known-defect hints. This report is the initial finding record, before consolidation
or repairs.

## Findings

### F1 — P2: errors in the outer provider phase disappear from the response

**Defect; source-inspected, not fault-injection reproduced.**

`backend/g5-api-core/src/services/workspace_pull_request_create.rs:232–278` logs and
discards failures to acquire provider-phase authority, failures of the provider
operation, summary reload failures, and provider transaction commit failures. It
then returns `Ok` with the earlier summary at lines 281–285. Ordinary per-repository
GitHub errors do get recorded by `create_or_update_provider_pull_requests`; these
outer errors do not. They are distinct from the covered ordinary provider-error case.

There is a new reachable authority case for the VM adapter: after the local PR
transaction commits, the VM can stop, expire, be replaced or transfer before the
fresh `caller.source_db` call. The current-authority check correctly prevents further
provider effects, but its refusal is invisible to the initiating caller. A new PR
can be returned with `providerStatus=local_only` and no provider error; a reused PR
can retain its prior provider status even though this synchronization was skipped.
The CLI at `backend/g5d-cli/src/pr.rs:116–144` prints that summary and exits successfully.
Neither UI nor CLI gets an actionable explanation that this phase failed or was
not attempted. A lost provider transaction commit can also leave externally created
PRs with an unreported uncertain recording outcome.

This violates the outcome/recovery part of `g5.governance.pr.write` and the shared
design's promise to expose partial provider failure. The catch-and-log pattern was
retained from the UI implementation; extracting it does not meet the newly explicit
write contract for these paths.

**Proposed repair:** keep the single orchestration, preserve the already committed
G5 PR identity, and return an explicit provider-phase failure/unknown/not-attempted
outcome when no trustworthy refreshed summary can be returned. Make that outcome
visible in both UI and CLI, including selected JSON when requested, without claiming
that already completed publication was rolled back. Do not weaken the VM gate to
avoid the refusal.

**Discriminating check:** hold the operation after local PR commit and before
provider admission, revoke/replace its incarnation, then assert no provider call
and an explicit partial result containing the existing G5 PR ID and refusal reason.
Also force an outer provider transaction failure or summary/commit failure and
assert the client cannot report the stale summary as the current synchronization
result. The existing CLI fixture only supplies a pre-recorded per-repository error;
it would pass with this defect unchanged.

## Evidence gaps and follow-up notes

- **E1: the guest feedback path remains unexecuted in this review.** The real HTTP
  API tests use `TestWorkspaceRuntimePeer`. In
  `api_workspace_pull_requests.rs:116–124`, the test itself commits the durable
  mirror while the peer simulates `workspace.changes`; this proves ordering and
  fresh evidence capture, not that a running agent's `g5d pr create` can complete
  the real dispatcher → native Git → API callback loop. Source inspection supports
  feasibility: the shared service releases its initial transaction before RPC;
  the dispatcher admits multiple mutations, uses a per-module slot, and launches
  execution after returning its admission disposition. A real active-agent/dirty
  guest integration case is still needed before claiming that end-to-end outcome.
- **E2: live provider and deployment behavior remain unverified.** Mocked GitHub
  verifies server-side actor credentials through the internal route and reuse
  through the public route; this is not live GitHub authorship/OAuth or execution
  in the published worker image. No deployment was performed.
- **Documentation clarification:** the existing provider implementation preserves
  an existing GitHub PR's body/title; `--body` updates the stored G5 override and
  supplies the body for newly created provider PRs. The UI explicitly explains its
  creation-only provider body behavior in `PrepareToShipModal.tsx:282–289`.
  `docs/g5d-pr.md` and the new worker skill could state this limitation directly,
  so `pr update --body ...` does not suggest GitHub body editing. This is a clarity
  note, not a request to add a second provider update algorithm.
- No unresolved product-intent question was found. The saved readiness amendment
  explicitly distinguishes retained save/push behavior from later P6 readiness.
  The write/architecture footers still say “Planned”; update their scoped
  implementation/evidence assessments during main-agent reconciliation, retaining
  the limits above and any surviving findings.

## Coverage and commitment ledger

| Commitment / responsibility | Actual path and discriminating case | Assessment |
|---|---|---|
| `g5.architecture.pr_flow`, `g5.governance.pr.write`: exactly one flow | UI hook and both frontend create handlers POST to the public adapter; public and internal adapters call only `workspace_pull_request_create::create_or_update`; CLI `update` is the `Create` alias. VM create followed by UI update reuses one durable PR. | Preserved by inspection and API/CLI tests. No second save/push/provider algorithm was added. |
| Source/target policy; `g5.security.owner`, `g5.security.read`, tenant/write rules | Shared owner validates readable same-tenant target before source save, source owner/mutability/base/same-target restrictions; review source guard is inside the shared owner. Denied target and read-only source produce no RPC; review route test proves no source save. | Covered on inspected paths and executed tests. Access changes during the entire multi-phase request were not exhaustively raced. |
| `g5.security.admission`, `g5.runtime.lease`, service identities | Exact internal POST middleware route accepts incarnation JWTs; human/host credentials cannot use it, VM cannot use public adapter or another source. Repeated `source_db` checks owner/org/VM/epoch/live lease/stop/transfer and locks current authority. `prepare_pull_request_source` pins the original incarnation. | Current/replaced/expired/transferred/stopped cases and post-push recheck pass. F1 affects reporting after later refusal, not the refusal itself. |
| `g5.governance.publish.readiness`, `g5.governance.publish` | Shared operation commits/pushes source before evidence; failure returns before local PR phases; stopped human source checks dirty flush receipts; VM cannot start compute. Commit/push retry retains work. | HTTP ordering/refusal/retry tests pass; actual guest save/push loop has E1. |
| `g5.governance.publish.actor`, `.github-author`, `g5.security.credentials.publication` | Provider owner uses `WritePullRequests` for discovery and `CreatePullRequest` only for a new PR; credential owner checks actor membership, current repo URL/binding/permissions/user access and keeps tokens server-side. VM actor is claims owner; existing PR reuse needs no new user grant. | Preserved by inspection and the executed App route test. Live provider/permission matrix remains E2. |
| `g5.governance.pr.write`: honest partial outcomes/recovery | Per-repo errors are recorded and rendered with status/lastError; outer phase failures are swallowed. Internal POST uses `post_json_once`, avoiding blind mutation replay. | Ordinary error and no-retry CLI tests pass; F1 remains. |
| `g5.governance.pr.json` | `Selection::parse` and available-field early return precede target/body/context/network work. Create/update use object projection; argument conflict is parser-owned. Existing boolean flags, comment demand, null/multi-provider semantics preserved. | 10 compiled CLI transport/validation cases and 8 PR-specific units pass. |
| `g5.governance.pr.read`, durable read authority | Read routes/read service remain distinct from mutation; create path dispatch does not change selector/read transport, credential operation or stored/live provenance. | Inspected interactions and all existing PR command read cases pass; unrelated prior read implementation was not re-reviewed exhaustively. |
| `g5.architecture.governance_owner`, `g5.design.minimal-mechanism`, `.change-locality` | Extraction moves orchestration to one domain service; typed caller enum confines VM delegation; existing credential/runtime/publication owners retained; no new schema, token store, merge or approval authority. | Minimal and appropriately scoped. Shared request/outcome types remove route-owned duplication. |
| Worker guidance and packaging | Both profile instructions point to `g5-pull-requests`; byte-identical skills, both manifests, Claude ignore allowlist/surface manifest, directory-copy staging, Codex manifest validation and skill installation inspected. | New skill/manifest checks and 3 staging tests pass. Broader profile parity has an unrelated existing mismatch (below). |
| `g5.quality.review.cases`, `.evidence.claims` | Real HTTP routes, restricted-role DB harness, mock provider, compiled CLI transport, valid/rejected cases, retry and partial failure examined. | Report distinguishes executed boundaries from fixture simulation and source inspection; F1/E1 are not covered by passing suites. |

## Executed checks

- `cargo test -p g5d --no-default-features --test pr_commands -- --test-threads=2`:
  **10 passed**. Initial sandbox run compiled but could not bind mock-server ports;
  rerun with local network permission passed.
- `cargo test -p g5-api --test api_workspace_pull_requests -- --test-threads=2`:
  **27 passed** using the test harness's unique disposable databases.
- `cargo test -p g5-api --test api_workspace_reviews owner_rejects_while_review_workspaces_are_restricted -- --exact`:
  **1 passed**.
- `cargo test -p g5d-cli --lib pr::`: **3 passed**;
  `cargo test -p g5d-cli --lib pr_json::`: **5 passed**.
- `python3 scripts/check_surface_sync.py --only g5-pull-requests`: **passed**;
  direct byte comparison and both manifest presence assertions: **passed**.
- `python3 -m unittest scripts.test_stage_local_worker_runtime -q`: **3 passed**.
- `git diff --check`: **passed**.
- Broader `python3 scripts/check_codex_profile_parity.py`: **failed** because the
  existing `g5-node-explainer/SKILL.md` copies differ; neither file is changed by this
  write amendment. This is not a new PR-write defect.
- An overbroad `cargo test -p g5d-cli pr --lib` substring run included unrelated
  tests: **74 passed, 3 failed** (two host-image preparation tests and
  `cli_registry_and_agent_registry_produce_identical_grounding_results`). It was
  run in the restricted sandbox; no whole-CLI pass is claimed. The properly
  scoped PR suites above passed. Unrelated failures were not repaired.

Discovery is complete across the assigned scope. Implementation is not fully
verified: F1 remains a source-confirmed reporting defect, with E1/E2 explicit
runtime limits. Main-agent consolidation owns repair and ontology metadata updates.
