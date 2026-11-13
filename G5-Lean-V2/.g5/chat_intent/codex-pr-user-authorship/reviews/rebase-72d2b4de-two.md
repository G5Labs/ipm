# Incremental rebase review — reviewer two

Reviewed snapshot: `a9f8b17d3a2d8538043166da3c27645b2b353aed` on main
`72d2b4de`, compared with `codex/pr-user-authorship-pre-rebase-b1d3342e` and
its main base `967bf34c`. The supplied range-diff covers all seven replayed
commits. Also inspected the uncommitted plan/request and governing migration
metadata changes. Production and ontology files were not edited.

## Review mode and inputs

This is a read-only incremental discovery pass, independently investigated but
**not a fresh reviewer context**: this reviewer previously reviewed this branch's
earlier rebase and CI repair increment, without authoring their implementation.
No peer findings or the author's current review conclusions were read. The
current request, pinned plan starting at “Ontology changes and governance”,
AGENTS.md, ontology-review skill, governing contracts and relevant footer entries
were inputs. Supplied scope hints identified selected-file Save/acceptance,
runtime identity, divergence completion, inline tracking and migration ownership;
they did not supply a suspected defect. Main sent test counts near the end of
discovery; those are not treated below as independently executed evidence.

Scope is the complete incremental rebase integration, including semantic
nonconflicts. Previously validated branch behavior remains the baseline; this
pass does not reopen unrelated main-only debt.

## Initial findings

**No new actionable defect found in the reviewed increment.** No governing
meaning change or additional implementation mechanism is needed for this rebase.
The migration evidence edits match the actual renumbering. This conclusion is
source inspection plus the static checks below, not an assertion that deployed
end-to-end behavior was exercised by this reviewer.

## Full-scope obligation ledger

| Commitment / obligation | Actual path and discriminating counterexample | Result / evidence limit |
| --- | --- | --- |
| Preserve both branches' compatible intent; keep one authoritative rule | Read pinned reconciliation and request, current governing text/metadata, branch/main file intersections and seven-commit range-diff. The meaningful replay adaptations are migration filenames/upgrade boundary, main tracking design tokens with the branch's readable `merge_active` text, and relocated ontology experiment links. | No dropped branch change or contradictory policy found. Commits four through seven are patch-identical; earlier patch differences were inspected. |
| Existing valid database state and tenant RLS; main's immutable historical migrations | Compared every main migration SQL file with `72d2b4de`; compared branch SQL against old 046/047; inspected `g5-api-data/src/db.rs` historical-owner dispatch and `migration_053_pr_push_refresh.rs`. Counterexample: version-only owner exception would now apply to the branch's old 047; current dispatch instead requires main 047/048's exact checksum and transactional migration. | All main 001–051 SQL files unchanged. Branch 052/053 SQL and SHA384 unchanged from backup 046/047. All 53 manifest entries verify and all 53 numeric versions are unique. Upgrade fixture copies versions below 53, populates existing PR state, and applies/replays production migration 053 as a non-BYPASSRLS owner. Runtime execution belongs to main's test batch. |
| `g5.git.durability.no-implicit`, `g5.git.checkpoint.selected`, `.isolation`, `.retry`; `g5.governance.accept.checkpoint`, `.accept.applied-pending` | Inspected `routes/workspaces.rs::write_workspace_file_and_maybe_push` (3560), `commit_and_push_files` (3660), review acceptance consumers, `workspace_runtime.rs::workspace_changes_commit_paths`/`workspace_changes_push_head` (825/852), and guest `modules/builtin.rs`. Checked selected paths plus expected blobs, private index/index lock, exact returned-head push, no-op/error handling and same-content retry. | Main's selected-file behavior remains intact. Explicit shared PR preparation still supplies no paths and therefore retains its intentional save-all behavior; automatic refresh does not call preparation. No accidental broadening of automatic commit triggers found. Concurrent native-index/runtime execution was not run here. |
| Current runtime identity, admission and one UI/CLI publication flow: `g5.governance.pr.write`, `g5.architecture.pr_flow`, runtime operation/lease and security admission rules | Compared both sides of `workspace_runtime.rs`; inspected shared create caller of `prepare_pull_request_source`, one pinned owner handle for its commit/push, VM expected-incarnation check, stopped-workspace refusal for VM calls and retained pending work; checked adapters and route-auth audit. Main's selected Save VM token and invocation deadline/retry/replay additions remain present. Counterexample: rebased extras could accidentally select partial PR preparation or omit VM validation. | No conflicting defaults or missing call argument found. UI and CLI remain adapters to the typed shared service; expected VM ID remains a concurrency token in file-route audit, while typed PR caller authority is verified separately. Actual guest cancellation/replay not executed by this reviewer. |
| Successful committed pushes reach PR recovery: `g5.governance.pr.follow-push`, version-bound approval and merge pinning | Traced selected guest exact-head push into internal Git publication, native Git route `WorkspaceBranchUpdated` at 760 and committed multi-member `branch_commit.rs` event at 404; followed maintenance event subscription plus durable open-PR scan into shared refresh and provider lifecycle. Counterexample: main selected-file pushes land without a refresh trigger or a dropped hint loses work. | Existing committed-ref consumers receive these pushes; periodic inventory recovers missed hints. Branch refresh continues to use published refs, refuses terminal PRs, and shares existing evidence/serialization owners. Main changes do not alter merge pins or approval-version rules. No selected-editor-to-live-provider end-to-end run performed here. |
| Divergence completion and remote auto/manual publication: `g5.workspace.remote.divergence-auto-complete`, `.divergence-auto-publish` | Inspected `remote_integrations/worker.rs::auto_complete_proven_divergence` (506), `merge.rs` automatic versus owner requester preparation/application, proof/race checks, retry counters and owner-only push authority. Followed branch `workspace_pull_request_provider_lifecycle.rs::synchronize_after_push` (478), current-authority rechecks, `resolve_publication_auth` and exact remote-head observation at 1060–1078. Counterexample: automatic PR refresh force-pushes an internally checkpointed commit when provider push mode is manual. | Automatic continuation observes provider refs using read authority and waits on a missing source head; only explicit publication invokes source push. Automatic divergence completion does not fabricate owner push authority; normal worker policy governs provider publication, including manual WorkspaceAhead handling. No bypass found. Successful live/App-backed implicit publication remains unexecuted here. |
| Recovery progress, durable failure and leader cancellation: `g5.architecture.recovery.progress`, `g5.governance.pr.follow-push` | Confirmed unchanged branch shared refresh deadlines, separately bounded failure recording and pre-scope lock limit; maintenance loop keeps a dedicated advisory-lock connection, rechecks leadership on ticks, and per-PR current authority remains authoritative. Main's new main-only remote integration retry fields live separately from PR recovery state. | No rebase regression or new cross-owner coupling found. Earlier CI repair is retained, not reopened as a new defect. Process restart, mid-scan connection loss and stalled filesystem behavior not injected here. |
| PR target/title/read/JSON/authorship and Account connection contracts | Used full range-diff and affected consumer searches to ensure migration rename does not alter pending title/refresh queries, all UI/CLI consumers retain typed shared service/default target behavior, owner-bound credentials and provider outcomes. Earlier Home issue-search Account connection integration remains replayed unchanged. | No new main consumer of deleted connection API or second publication flow found. These unchanged behaviors retain prior review evidence; live OAuth/GitHub and CLI binaries were not executed in this pass. |
| Main inline tracking presentation and honest retry, branch PR UI | Inspected `BranchTrackingInline.tsx` known-state gates, error retry, in-flow detail and action delegation; branch delta in `BranchTrackingPopover.tsx` only specializes merge-active wording and preserves main design tokens. Inspected `WorkspacePage.tsx` destination error/retry integration, main selected Save consumers and current PR state helper. Counterexamples: duplicate head mutation, stale unavailable status exposing controls, stopped owner losing target retry. | Inline actions remain single-owned by detail, unavailable data gates expansion, and owner target retry is independent of live-edit eligibility. Shared frontend types preserve main runtime identity and branch pending-refresh fields. No new rendering/build defect found by inspection; frontend suite/build results are main-owned evidence. |
| Minimal design and accurate ontology evidence | Current ontology diff only changes migration 047→053 and 046→052 in paired metadata; plan records preserved main behaviors without creating new policy. No new abstraction is introduced by conflict resolution. | Conforms within scope; no proposed ontology meaning edits. The historical receipts intentionally retain their historical migration numbering. |

## Checks performed here

- Read the supplied full range-diff and compared main/branch changes at intersecting
  owners and their actual downstream consumers.
- Independently byte-compared all 51 main SQL files with main and branch 052/053
  with backup 046/047: no SQL differences.
- Independently recomputed all 53 SHA384 manifest entries and checked unique
  numeric migration versions: all pass.
- `git diff --check 72d2b4de HEAD -- backend frontend`: passes.
- No heavy tests, database mutation, provider calls, builds or browser execution
  were performed. Main's concurrent runtime evidence must be recorded separately
  during consolidation, with its exact commands/results and remaining limits.

The inspected paths support this integration. A deployed selected-file
Save → internal push → remote policy → automatic PR refresh execution, live
provider/OAuth behavior and fault-injected runtime restart/connection loss are
not established by this review.
