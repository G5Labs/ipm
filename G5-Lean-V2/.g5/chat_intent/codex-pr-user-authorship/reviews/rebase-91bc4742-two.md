# Independent rebase review two — main 91bc4742

## Review boundary and inputs

Read-only incremental full-scope discovery for PR #268, on HEAD
`fc2516da31ec3177310127ff3fae01bac229da7b`, rebased onto `91bc4742`. Comparison
tip: `codex/pr-user-authorship-pre-rebase-1ce454d7`, based on `72d2b4de`.
The working tree also contained the assigned plan/review/governance-reference updates.
No production, test, design or ontology files were edited by this reviewer. This report
is the only authored artifact. No further delegation or database/build work was done.

Started with `ontology-plan.md` → `Ontology changes and governance`, the original
`request.md`, the actual ontology diff, and the cited current commitments and applicable
metadata. Read `AGENTS.md`, `ontology-review/SKILL.md`, and its design-review guide.
Inspected the complete rebase range-diff, the branch's production changes and material
unchanged consumers: PR creation/publication/refresh/merge, credentials, runtime context,
CLI presentation and transport, account connection, workspace UI, and main's comments,
identity and after-commit integration. No peer report or parent review conclusions were
read before writing this report.

Known-context limitation: the supplied design already describes prior repairs, previous
test evidence, migration collisions and current known verification gaps. Those were
inputs, not independently discovered defects. Historical implementation/review receipts
were not treated as proof that this exact checkout executes correctly. This is a fresh
reviewer context with no implementation history supplied beyond that pinned design.

## Raw findings

No actionable defect or incompatible governing commitment found in the reviewed rebase.
No code or ontology repair is proposed from this independent pass.

`git range-diff 72d2b4de..codex/pr-user-authorship-pre-rebase-1ce454d7
91bc4742..HEAD -- backend frontend` shows the prior implementation preserved, with the
branch migrations/test renumbered to 055/056 and the PR destination retry block adapted
to main's workspace page nesting. This reduces the rebase-specific change surface but
does not substitute for reviewing auto-merges or shared consumers.

The latest reconciliation is consistent with the actual ontology: current governance
metadata points to 055/056; main's review-thread, principal, page-refresh and notification
contracts remain present. Historical 052/053 statements are clearly dated by the plan's
earlier reconciliation and superseded by its latest section. No historical receipt needs
rewriting to imply it ran against the new numbers.

## Obligation ledger

Results below distinguish source inspection (I), executed lightweight checks (T), and
unexecuted runtime boundaries (U). Grouped rows retain separately named obligations.

| Commitment | Actual path | Counterexample/check | Result or gap |
|---|---|---|---|
| `g5.governance.pr.write`, `g5.architecture.pr_flow`, `g5.design.adapters` | Public/internal routes → `workspace_pull_request_create::create_or_update`; CLI `pr::execute` | UI and VM must share preparation, target admission, evidence/reuse and provider stages; no adapter-created PR algorithm | I: both routes delegate with typed callers; CLI `update` aliases the same create command. Shared service owns effects. |
| `g5.security.owner`, `.admission`, `.tenant`, `.tenant.write`, `.login.service-identities`; `g5.runtime.lease` | `PullRequestCaller`, scoped database helpers, `workspace_runtime` preparation | Wrong source, replaced incarnation, revoked target, transfer, deleted workspace, unauthorized read/write | I: source binding, workspace ordering, owner/runtime checks, target admission after waits and phases remain. Existing integration tests address those cases; U: not rerun here. |
| `g5.governance.publish`, `.publish.readiness` | Explicit preparation → prepared local PR → source publication; separate automatic mode | Failed save or dirty stopped snapshot must not publish older work; automatic refresh must not save/start compute | I: explicit source preparation precedes capture; VM incarnation is passed to preparation; automatic path calls published-snapshot capture and skips runtime preparation. |
| `g5.governance.pr.target` | Shared `targets`; target resolver; `useWorkspacePullRequestTargets`; `WorkspacePage` | Inaccessible parent/Base, no target, access loss, destination query failure in running/stopped layout | I: ordered readable live same-org choices; pinned target rechecked. T: mounted Base-pull/destination recovery suite passed, including current error banner placement. |
| `g5.governance.pr.title` | `store_title_override`; `pending_provider_title`; provider row lock; `sync_recorded_title` | Omission erases titles, latest title lost after partial failure, unchanged members excluded | I: durable intent and serialization preserved; existing provider records receive titles without new empty PRs. U: provider/database concurrency cases not executed in this pass. |
| `g5.governance.publish.github-author`, `.publish.actor`; `g5.security.credentials.publication` | `RepositoryCredentials::CreatePullRequest`; `github_user_grants`; `GithubAppClient`; provider lifecycle | Missing/revoked actor grant, stale identity, uncovered binding, bot fallback, already-created PR | I: operation-specific user grant, repository ID probe and binding capability retained; other App operations keep installation policy; existing PR lookup precedes new-author credential request. Tokens remain server-side. U: live GitHub. |
| `g5.governance.pr.follow-push` | Refresh discovery → published preflight → provider observation/sync → completion; branch event and worker | Dirty retained snapshot, old hint, no/open/terminal PR, manual external push policy, provider failure | I: durable open-PR inventory, pending/error fields, current heads, existing identity, remote-ref observation and terminal-provider check preserved. Native receive-pack and other Git publishers converge through branch hints/periodic scan; no duplicate push flow. U: API/DB/native Git replay here. |
| `g5.governance.approval.version`, `.merge`, `.merge.effects` | PR row lock; provider merge and automatic refresh; job merge cut revalidation/landing | Refresh silently renews approvals or repins accepted provider merge; changed source/target accepted | I: refresh does not update approval records; provider completion refuses repinning; merge validates recorded/live cuts and shared lock serializes acceptance. Existing landing retains complete merge checks despite narrower PR helper locks. U: race tests not run. |
| `g5.architecture.recovery`, `.recovery.progress` | `maintenance/workspace_pull_requests`; bounded shared refresh/failure reconciliation | Lost event, timeout, held authority/PR lock, follower scan duplication | I: dedicated advisory scan connection, leadership ping, durable inventory, per-item 120s attempt and independent 5s failure recording, transaction-local lock timeout before scope admission. U: new connection-loss/process-restart/stalled-filesystem experiments. |
| `g5.design.commit-notify`; main comment/review-thread notification contracts | `ScopedDb::on_commit`; comment/notification owners; PR `publish_pr_updated` | Notification observed for rolled-back state, PR event overwrites main events | I: main's queue/fork/savepoint behavior is unchanged; PR notifications occur after successful commit. T: comment and PR-authority WebSocket suites both pass with distinct event handling. |
| `g5.governance.pr.read`, `.pr.json`; `g5.security.read`, `.read.durable` | Public/VM info routes, shared read adapter, CLI selector and JSON projection | Arbitrary URL/repo read, mutation on field help, unknown facts manufactured, wrong JSON shape | I: attached-repository selection, bounded no-redirect provider reads, sanitized errors, read-only G5 listing; fields validate before context/network/write; view/create object and list/status arrays; multi-provider single values remain null. U: compiled CLI/API tests not run here. |
| `g5.settings.github-connection`, `g5.security.login` | Shared hook/card/page; login; PR, issue composer and Home recovery links | Login blocked by grant failure, nested return lost, separate grant store, connection bypass of repo admission | I: existing grant endpoints reused, optional continuation retained, local destination validation and direct connection-page resume remain. T: connection, external issue composer and App suites pass. |
| `g5.security.login.principal`; main comment/review-thread attribution/access | `RequireAuth`, `api.ts`, composer owners, workspace page, internal routes | Rebase removes principal-bound comments or treats service requests as human-authored | I: relevant main files/auto-merges preserve principal/attribution code. Non-comment principal binding is explicitly deferred by governing text, so it is not added as a PR requirement. T: comment invalidation and existing connection consumers pass. U: full principal browser scenarios not rerun. |
| `g5.workspace.page.refresh` | `toRefreshingState`, `useWorkspace`, name/VM readers; workspace page wrapper | PR retry banner replaces retained page or drops main refresh status/typed drafts | I: main's retained-content/refresh status and composer identity remain; PR retry lies within that shell and targets only its own query. T: page and hook suites pass. U: real browser failure scenarios. |
| `g5.design.authority`; current PR UI consumers | `useCreateWorkspacePullRequest`, `currentWorkspacePullRequest`, both source dialogs, target list | Old in-flight list overwrites successful mutation; open dialog hides pending/error state | I: matching-row cache seeding follows read cancellation; dialogs consume same-ID current query state; pending provider refresh is separately rendered. T: hooks, both dialogs and list pass (66 tests across these four files). |
| `g5.runtime.workspace-cli`, `.workspace-cli.context`; `.credentials.platform-token`, `.reserved-env`; `g5.harness.provided`; `g5.agents.admission.permission-policy` | Shared launcher/environment builder; dispatcher terminal and agent runner; root `AGENTS.md` | Terminal lacks `g5d`, loader paths pollute image tools, settings replace identity, network policy bypass | I: one launcher, quoting and G5-only loader assignments, shared three-binding context, reserved-env admission and root harness guidance preserved; no host/provider credential addition. U: deployed Linux standard-path/login-shell behavior and actual agent choice. |
| Forward-migration/RLS/evidence commitments | Main SQL 001–054; branch 055/056; checksum catalog; populated upgrade test; unchanged `g5-api-data/src/db.rs` | Collision, edited immutable migration, bad checksum, historical owner exception lost | T: all 56 SHA384 entries pass; main 54 SQL files byte-identical to base; branch SQL byte-identical to old 052/053; version range unique/continuous. I: test applies 056 after populated 001–055 and replays under non-BYPASSRLS owner. U: execution delegated to parent. |
| `g5.design.minimal-mechanism`, `.reuse`, `.change-locality`; ontology format | Changed contracts and owner boundaries | New parallel authority, duplicated target policy, metadata-less rule | I: existing grant/storage/runtime infrastructure reused; single shared PR owner and narrow automatic mode have concrete obligations. T: each body commitment in all 7 changed ontology files has exactly one metadata entry. No new meaning needed for the rebase. |

## Executed checks

All commands were local and used the existing dependencies; no install/build/database
startup was performed.

- `git diff --check`: passed on the reviewed working tree.
- `shasum -a 384 -c checksums.sha384` in the migration directory: all 56 passed.
- Read-only Python comparison: 54 main SQL migrations byte-identical; renamed branch
  SQL byte-identical; unique continuous versions 001–056; stable commitment/metadata
  pairing in all 7 changed ontology files passed.
- `npm test -- src/pages/WorkspacePageBasePull.test.tsx src/lib/ws.comments.test.ts
  src/lib/ws.workspaceAuthority.test.ts src/components/settings/GithubConnectionCard.test.tsx
  src/components/external-issues/ActivityCommentComposer.test.tsx`: 5 files, 30 tests passed.
- `npm test -- src/hooks/useWorkspace.test.ts src/components/workspace/CreatePullRequestDialog.test.tsx
  src/components/workspace/PrepareToShipModal.test.tsx src/components/workspace/PullRequestList.test.tsx
  src/App.test.tsx`: 5 files, 99 tests passed.

Total executed frontend evidence: 10 files / 129 tests. The test runs emitted React
`act` warnings, jsdom canvas/unhandled fixture-request diagnostics in App tests, Node
experimental/deprecation notices, and stale Browserslist data; commands still exited 0.
These are recorded as limits of the fixture run, not browser or backend verification.

## Verification limits and handoff

Implementation inspection found no repair obligation. Verification is deliberately
partial: backend compile/static-auth/API/database/migration execution belongs to the
parent's checks; it is not established by this report. No live GitHub/OAuth, deployed
worker, browser automation, Linux guest startup, process restart, mid-scan connection
loss or stalled-filesystem injection was executed. Existing source tests and metadata
name these boundaries but do not replace execution at this snapshot.

No PR publication or provider status was queried or changed. Preserve this raw report
before consolidation with the other independent reviews.
