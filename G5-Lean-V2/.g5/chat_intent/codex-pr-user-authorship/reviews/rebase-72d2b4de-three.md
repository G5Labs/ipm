# Incremental rebase review three

Snapshot: `a9f8b17d`, rebased onto `72d2b4de`, compared with
`codex/pr-user-authorship-pre-rebase-b1d3342e`. Discovery includes the frozen
uncommitted plan/request and governing migration-reference edits. Read-only review;
no production, tests or ontology were modified.

This is an independent incremental assessment by a reused reviewer context. I did
not implement the rebase, but previously reviewed this PR's CLI and CI follow-ups.
It is therefore not a fresh-from-zero review. I read the assigned request, first
governance/reconciliation section and relevant plan material, AGENTS, ontology-review,
affected contracts and their metadata. I did not read peer reports or the author's
current review narrative. The assignment supplied the migration collision and
semantic integration areas; these are disclosed review inputs, not independently
discovered defects.

## Initial findings

No actionable rebase defect found in the inspected incremental scope. The seven
commits retain their patch behavior except the intended migration renumbering,
main's design-token selection and its relocated experiment reference. Four commits
are patch-identical in the supplied range-diff. This is source-level assurance,
not a claim that every combined runtime behavior was executed by this reviewer.

## Full-scope obligation ledger

| Commitment | Actual path and counterexample/check | Result or gap |
| --- | --- | --- |
| Preserve main's migration history and branch's new persisted PR state | Compared every main migration against the checkout; inspected production runner `backend/g5-api-data/src/db.rs`, new `052_pending_pr_title.sql` / `053_pr_push_refresh.sql`, checksum ledger and renamed upgrade test. Counterexamples: duplicate versions, altered historical SQL, stale SHA, accidentally treating branch 053 as historical owner SQL. | All 51 main SQL files are byte-identical; main checksum manifest is an exact prefix. All 53 listed SHA384 values match, versions are unique 001–053. New SQL is byte-identical to backup 046/047. Runner is unchanged from main; only exact historical 047/048 checksums qualify for owner context. |
| Preserve populated upgrade/replay and current ontology references | `migration_053_pr_push_refresh.rs` builds 001–052, inserts a PR, adds 053, then runs the production runner twice through a role explicitly checked as neither superuser nor BYPASSRLS. Searched maintained backend/docs/ontology for old branch filenames. Reviewed uncommitted `g5.governance.pr.follow-push` and `.title` footer diff. | Correct new test boundary and defaults/preserved-title assertions by inspection; maintained references point at 052/053. Current ontology diff changes evidence references only. Database execution not performed by this reviewer. |
| `g5.git.durability.no-implicit`, `g5.git.checkpoint.selected`, `.isolation`, `.retry`; accepted-file checkpoint and settled acceptance | Main Save/acceptance route flow, `write_workspace_file_and_maybe_push`, `commit_and_push_files`, runtime selected commit/pinned push APIs, guest changes operation; compared branch changes against main. Counterexample: PR integration turns explicit Save into all-files commit or changes acceptance failure to rejection. | Main selected paths/hash expectations, pinned returned commit push, private-index behavior and pending accepted result remain intact. Branch explicit PR preparation is still a separate requested save/push operation; automatic refresh never calls that preparation. Concurrent native-index or crash tests not executed here. |
| Current live workspace identity and authority | `workspace_runtime.rs`, live-file route checks, shared PR caller admission and `prepare_pull_request_source`. Counterexamples: stale VM PR token uses stopped workspace, selected Save silently crosses VM identity, coalesced Changes reads mix incarnations. | Main identity-sensitive runtime/Changes implementation is retained. Only branch delta in runtime is optional expected `(vm_id, epoch)`, rejecting stopped/replaced incarnations for VM callers. No additional identity bypass introduced by rebase found. Replacement-VM runtime execution remains unverified here. |
| `g5.architecture.pr_flow`, `g5.governance.pr.follow-push`; successful selected-file pushes refresh existing PRs | Native Git receive-pack accepted-ref path emits `WorkspaceBranchUpdated` after admission commit; existing maintenance subscriber plus durable open-PR inventory invokes shared refresh. Source/target current admission, PR row serialization, provider completion check and post-commit update event remain in shared owner. Counterexample: Save creates another PR, commits unrelated dirty work during refresh or advances on rejected ref. | Existing-PR committed-ref continuation composes with main selected pushes. Accepted-ref filtering and ordinary remote enqueue are retained; automatic service has no VM preparation/new-PR path. Same shared owner supports UI and both CLI identities. Actual live guest-to-provider sequence unexecuted here. |
| Proven divergence completion respects auto/manual provider push modes and exact Git versions | Main `remote_integrations/merge.rs` and worker completion flow; branch `workspace_pull_request_provider_lifecycle.rs::synchronize_after_push`. Counterexample: automatic resolution or PR refresh implicitly pushes a manual integration. | Main proof/CAS completion and bounded retry behavior remain; merge service is unchanged from main. Automatic completion does not enqueue explicit owner publication. After-push PR synchronization resolves read credentials and verifies the published remote head instead of pushing it, retaining pending recovery until policy-authorized publication. No live remote-mode execution here. |
| Authority, bounded recovery and scan ownership remain intact after rebase | Shared refresh and failure reconciliation; dedicated session scan lock/cancellation; typed route adapters and static audit. Counterexample: recovery reintroduces unbounded old error reads, drops target reauthorization, or restores the stale direct-route audit expectation. | CI repair commit is patch-identical; separate failure deadline, early transaction lock timeout, current authorization, terminal/completion reconciliation and leader connection lifetime remain present. No repair reversion found. No contention/cancellation injection rerun by this reviewer. |
| Inline tracking and PR UI retain both branches' truthful state | `BranchTrackingPopover`, main inline tracking/pill consumers, `WorkspacePage`, PR dialogs/query hooks and websocket invalidation. Counterexamples: popover returns as duplicate control, main color tokens lost, main authoritative-cut invalidation replaced by new PR events, stopped owner loses destination retry. | Main inline UI and design token survive; `merge_active` readable wording remains. New PR event is an added case, retaining main branch/review invalidation. Server target query and owner-gated error retry remain; no Save/acceptance UI overwrite found. Mounted/browser execution not performed here. |
| Preserve connected-user authorship and one account connection; CLI instructions respect configured execution policy | Auth/provider and Home connection integration patches, root AGENTS, runtime launcher/context changes; patch/range comparison for rebase effects. Counterexample: main Home navigation bypasses centralized connection, or rebase reintroduces deleted harness delivery/broader permissions. | Prior Home/harness/CLI patch behavior preserved; no second credential/publication flow or permission widening introduced. Linux image, actual model instruction delivery and live provider/auth checks unexecuted here. |
| Minimal implementation and accurate reconciliation | Compared backup/main/current and full seven-commit range-diff; inspected actual ontology diff rather than only its listed IDs. | Rebase changes are limited to preserving compatible main behavior and renumbering branch migrations. No new product decision, duplicated publication owner or unexplained consequential addition found. Historical receipts need not be rewritten as current execution evidence. |

## Executed checks and limits

Executed by this reviewer: Git/range/source comparisons, all current migration
SHA384 checks, uniqueness/byte-identity checks, maintained-reference searches and
`git diff --check` (clean). No build, API/database suite, native CLI execution,
browser suite, Linux guest, model/provider run, deployment or process-restart test
was run in this discovery pass. Tests were assigned to the main agent, so their
results should be recorded separately from these independent inspection results.
The previously run CLI fixture tests belong to the older review and are not
represented as new rebase execution evidence.

Review effort: one reused reviewer context, no delegation; no measured comparison
of review benefit is claimed.
