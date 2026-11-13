# Base re-verification from Home — plan

Governing:
- New: [g5.reverification](../../ontology/features/ontology_and_harness.g5.md#g5.reverification) and its children (run, scope, status, workspace); [g5.ui.home-reverification](../../ontology/features/ui.g5.md#g5.ui.home-reverification) and its dialog; [g5.git.conflicts-rebase.models](../../ontology/software_architecture/git.g5.md#g5.git.conflicts-rebase.models).
- Amended: [g5.documents.findings.files](../../ontology/features/documents.g5.md#g5.documents.findings.files) (extend in place, `Run:` line, run filter); [g5.workbench.review-prompts](../../ontology/features/ui.g5.md#g5.workbench.review-prompts) (the switch now also governs Home).
- Split to meet the writing rules, with no change of meaning: the switch into [g5.workbench.review-prompts.switch](../../ontology/features/ui.g5.md#g5.workbench.review-prompts.switch), a concept Home and the run start share; the evidence rules into [g5.documents.findings.evidence](../../ontology/features/documents.g5.md#g5.documents.findings.evidence).
- Unchanged and relied on: [fork state](../../ontology/features/workspaces.g5.md#g5.workspace.fork-state.formal), [Base](../../ontology/features/workspaces.g5.md#g5.workspace.base), [Base merge authority](../../ontology/features/workspaces.g5.md#g5.governance.pr.base-admin), [fork landing](../../ontology/features/ui.g5.md#g5.workspace.ui.fork-landing), [composer seeds](../../ontology/features/ui.g5.md#g5.workbench.chat-actions), [findings decisions](../../ontology/features/documents.g5.md#g5.documents.findings.decision), [verification artifacts](../../ontology/features/ontology_and_harness.g5.md#g5.harness.verification-artifacts), [deletion](../../ontology/features/workspaces.g5.md#g5.workspace.delete).

Intent: [chat intent](../../chat_intent/periodical-reverification/request.md). The owner answered all seven questions of the set `base-reverification` (Project Doc `base-reverification-design.md`) on 2026-11-06.

## Summary

Base counts as re-verified through the commits a Base re-verification run forked from, once that run's workspace merges into Base. Nothing else advances it.

- One table records the runs. Each run's state (in progress, completed, abandoned) is read from records G5 already keeps.
- Home shows the status from Git and G5's records, plus a button for a dialog that starts an incremental or a full run.
- A run is an ordinary fork of Base. It lands on a pre-typed prompt that carries the run's scope and asks the agent to write it into a Project Doc. The agent reports findings without fixing code, extending the existing findings files under a `Run:` label.
- Separately, an agent resolving a rebase keeps TLA+ models parsing and running when both sides changed them.

## Run record

**Table.** `base_reverification_runs`, one row per run. Only the run admission inserts a row, and no path updates it.
- Columns: `id`, `org_id`, `kind` (`incremental` | `full`), `fork_id` (unique, references `workspace_forks`), `workspace_id` (the child the fork allocated), `started_by`, `created_at`, and `base_heads` (each code member's frozen source commit, `[{repoId, rootPath, commit}]`).
- `since_run_id`: the completed run it continues from. It is required exactly when the kind is incremental.
- RLS and organization scoping follow `workspace_forks`. Foreign keys to workspaces restrict deletion; workspaces are only soft-deleted.

**Derived state.** Every reader computes the state with one shared query, owned by the run module:
- **completed** at the `merged_at` of the first pull request from `workspace_id` into Base with status `merged`;
- otherwise **abandoned** when the fork is `aborted` or the workspace has `deleted_at`;
- otherwise **in progress**.

The last completed run is the stale point. Its `base_heads` are S for the next incremental run.

**Admission.** The fork service owns the fork operation and its frozen manifest. The run module owns the run rules and the row. The fork admission coordinates the two, so they commit or fail together:
- `ForkWorkspaceRequest` gains `reverification: Option<RunKind>`, included in the request digest, so a retry with another kind is a different request.
- Inside `admit_operation`'s transaction, after the manifest is frozen, the fork calls the run module's `admit`. That function:
  - takes a transaction-scoped advisory lock per organization;
  - refuses unless the source is Base, the organization's `verification_action_enabled` is `true`, and the latest run is not in progress;
  - for an incremental run, refuses without a completed run, and otherwise sets `since_run_id`;
  - inserts the row with `base_heads` taken from the frozen manifest's code members.
- Fork eligibility is the fork's own: anyone who can fork Base.
- A replay or retry of the same fork finds the run by `fork_id`, unless the fork has failed (see Fixed after review).

**Direct merges.** A workspace can also reach its parent through the direct `POST /workspaces/:id/merge`, which its target's owner may run and which records no pull request. The UI never calls it, but a run merged that way would read as in progress forever. So `merge_workspace` refuses a run's workspace when the target is that run's Base (`base_reverification::is_run_into_base`), keeping completion derivable.

**Why derived, not a stored status.** A stored status needs writes inside three cross-owner transactions: the merge receipt reconciliation (`coordinated_repo_publication.rs`, the `status = 'merged'` update), deletion admission, and fork abort. If one of them is missed, the Home button stays greyed for good.

The derived form answers the owner's two questions the same way: when the last verification completed, and whether one is running. It reads at most the latest run and its pull requests. The owner chose this (question `run-state`).

## Status

`GET /api/v1/orgs/:orgId/home/base-reverification` returns:
- whether the feature is enabled;
- the state (`never`, `current`, `stale` or `in_progress`), the last completed run, and the in-progress run's workspace and current owner;
- the counts, and pages of merged pull requests and of runs.

Readers are members who can read Base. While the switch is off, the endpoint returns only "disabled".

- **Runs.** Each run carries `workspaceDeleted`, so the dialog names a deleted workspace without linking to it; completed runs' workspaces are usually deleted after they merge.
- **Pull requests.** G5 pull requests whose target is Base, merged after the last completed run's `created_at` (its fork admission), excluding a run's completing pull request (its workspace's first merge into Base). Each row shows title, number, merge date and who merged it.
- **Lines.** For each code member, use the g5-scm tree-to-tree diff stats (`repo.rs`, `line_stats`) between `base_heads` and Base's current head, skipping paths under `.g5/`. A member added to Base after the run counts from the empty tree. The docs repository is excluded.
- **Cache.** The diff is cached in memory per `(repo, from, to)`, bounded, and rebuilt on a miss. Home's polling then costs a lookup until Base moves.
- **Not counted apart:** lines in code that a model covers. The review guide's manifest lists no implementation files; only the developer helper `tools/tlc/scripts/model_evidence.py` declares them. An optional manifest field would make the count deterministic later.

## Home and dialog

- `HomePage` reads the organization switch with `useOrgScopedConfig`, as `WorkspacePage` does. While it is not exactly `true`, nothing renders.
- A status line (orange text token) and the button sit above the tiles, re-read with Home's existing focus and timer refresh.
- The button is greyed with a reason for someone who cannot fork Base, and for everyone while a run is in progress. In that case the line reads "Verification in progress in workspace X (by Y)", with X linking to the workspace when the reader can open it.
- The dialog has the explanation, the counts, and the two paged lists using Home's page size. Its choices post the ordinary fork, `POST /api/v1/workspaces/:baseId/clone`, with `uniquifyName: true` and `reverification: {kind}`; the response adds `reverificationRun`. A definitive refusal rotates the idempotency key, while a network error or 5xx keeps it so a retry replays the same fork.
  - The fork takes the first free name from `base-reverification-<UTC date>`.
  - On success, the client navigates to the workspace with `?view=chat`. The existing fork-landing state carries the run's prompt and a `base-verification` seed header, which shows the bold notice with Base wording in the same style.
  - A refusal, such as losing a race to another run or the switch being turned off, stays in the dialog with its reason.

## Run workspace landing and prompt

The prompt carries everything, and there is no other mechanism (owner's answer to `project-doc-writer`).
- `lib/reviewPrompts.ts` gains `baseReverificationSeed(run)`, built from the start response. Its wording is the owner's to tune. Content:
  - kind, S and F (each code repository's root path and commit), and the findings label `Base re-verification <start date> (<kind>)`;
  - the work: for incremental, apply [the PR incremental guidance](../../../.agents/guidance/TLC/pr_incremental_TLA_reverification.md) with Base at S as the pre-PR product and Base at F as the current product; for full, make every model current at F, add models for important behaviour without one, and run everything;
  - required: every model parses and runs at the end; findings go in the existing two files, in the [review guide's](../../../.agents/guidance/TLC/verification-review.md) format with `Run: <label>`; `link_verification.py` and `check_findings.py` run clean; no failing test is committed; rebase onto Base before the pull request;
  - optional: repaired copies of models to keep checking past a bug, never tagging a statement `design-verified`;
  - out of scope: fixing product code, and re-verifying pull requests merged after F;
  - first step: write all of this into a Project Doc and keep it as the checkpoint and final report.
- Pushing and opening the pull request work as in any workspace (owner's answer to `agent-opens-pr`). The PR guidance's "do not push" and "do not consult other tasks' findings" lines are for PR runs; the prompt says that a Base run extends the existing findings files.
- A reload before Send loses the draft, as with any fork landing.

## Findings

- `verification-review.md` documents the `Run:` line: its own paragraph after the status line, naming the run that first recorded the entry. A later run keeps it on entries it carries forward. Decided entries stay unchanged, so older entries simply have none.
- `check_findings.py` accepts an optional `Run:` line and still refuses changes to decided entries.
- `lib/findings.ts` parses the line. The reader adds a run selector beside the severity toggles:
  - All runs (the default);
  - each label, in order of first appearance;
  - "No run named" when some entries have none.

  The two filters combine, and the reader keeps the same entry-plus-introducing-headings display.
- The uplift's review step writes `Run: Uplift <date>`.
- The run's owner triages in the run's workspace before it merges (owner's answer to `triage-location`). Issues link into that workspace, so their links stop working once it is deleted.

## Rebase and models

- **Harness.** The merge-and-rebase paragraph of `.agents/AGENTS.md`, and the repository's `AGENTS.md`, gains one sentence. Only when the repository has TLA+ models and both sides changed them since the merge base, the agent brings the affected models into agreement, makes every affected model parse and run, fixes breakage the combination caused, and records changed outcomes; it does not re-verify. Agent-driven rebases and the resolution task both run with these instructions.
- **Resolution prompt.** `git_conflict_resolution.rs` gains one clause pointing to that rule, and `template_revision` becomes 5.
- **No new trigger.** The owner chose a hand-off on Git conflicts only (`clean-replay`). A clean replay therefore leaves models combined by Git unchecked, as a merge into a parent does. Each Base re-verification ends with every model parsing and running.
- **Affected models only** (`affected-models`): those either side changed, those using a changed module, and those linked from a reconciled statement.

## Choices and rejected alternatives

- **Per-PR verification tracking, receipts, verdict enforcement.** Rejected by the owner (decisions 2 and 3).
- **Stored run status.** Rejected by the owner, for the reasons in [Run record](#run-record).
- **A Project Doc written by G5, or a guidance file for the run.** Rejected by the owner: the prompt carries the run's scope and asks the agent to write the doc.
- **Starting the resolution agent on clean replays when both sides changed models.** Rejected by the owner: hand-off stays on Git conflicts only.
- **A re-seed of the prompt on later visits.** Dropped with "no extra mechanism".
- **Counting lines in modelled code.** Deferred until manifests list implementation files.

## Assumptions and relied-on facts

- The fork freezes each member's `source_oid` and allocates the child's id at admission (`ForkRepoManifest` in `workspace_fork_service.rs`). Its admission runs in one transaction that the run insert can join.
- Workspaces are only soft-deleted. The only `DELETE FROM public.workspaces` is the test reset in `g5-api-data/src/db.rs`.
- A merge sets `status = 'merged'` and `merged_at` once, in the receipt reconciliation transaction, for local and provider merges alike. To verify for provider merges during implementation.
- Members read the organization switch; only administrators write it.
- The agent's home has `$HOME/.agents/guidance/`, including the PR incremental and review guides the prompt names.
- The switch is a UI and API gate, not a policy on agent work: anyone may still ask an agent for the same work.

## Risks

- **Boundary race.** A pull request merging while a run's fork is admitted may be counted on the wrong side of the stale point. If that matters, compare ancestry with the merge's recorded target heads instead of times.
- **Conflicts for others.** The run's merge touches models, ontology metadata and findings, so open pull requests touching the same files will conflict. Report-only keeps code out of it; the guidance asks for narrow ontology edits.
- **Triage links.** Triage happens in the run's workspace, so issue links into it stop working once it is deleted.
- **Clean combinations.** Models that a clean replay or a merge into a parent combines stay unchecked until the next run, which ends with every model parsing and running.
- **Lost draft.** A reload before Send loses the long prompt. The run is still in progress and blocks others until the workspace is deleted or merged.
- **Long runs.** Base moves during a run. The rebase rule keeps models working, and the pull requests merged meanwhile stay counted.

## Verification plan

- **Run module (Postgres tests):**
  - each refusal: switch off, source not Base, not fork-eligible, a run in progress, incremental without a completed run;
  - a retry returns the same run, and two concurrent starts admit exactly one;
  - an aborted fork, or deleting the workspace, abandons the run and frees the next start;
  - a merge into Base completes it, and a pull request into another parent does not.
- **Status:**
  - pull requests are bounded by the fork time, and runs' own pull requests are excluded;
  - lines skip `.g5/` and the docs repository, and a new member counts from empty;
  - the `never`, `current`, `stale` and `in_progress` states;
  - read authority.
- **Frontend:**
  - Home line states and greyed reasons, with nothing shown while the switch is off;
  - the dialog's lists, choices and refusals;
  - a start navigates to the seeded prompt and header.
- **Findings:** `findings.ts` and `check_findings.py` handle the `Run:` line, and the run filter combines with severity.
- **Rebase:** the resolution prompt revision and its clause; the harness sentence in both `AGENTS.md` files.
- **End to end:**
  - Home, then the dialog, then a start in the `frontend-only` preview;
  - a full-stack start and merge where capacity allows.

## Implementation and review (2026-11-06)

Built as designed, with three changes the review forced. Three independent reviewers covered backend correctness, frontend behaviour against the ontology, and regressions in shared code.

**Fixed after review:**
- **Formal-review policies.** The run-record migration (now 155) lacked the restrictive formal-review policies every workspace-keyed table needs; `formal_review_policy_inventory` would have failed CI.
- **Failed forks.** A fork failing with a retryable error stayed `pending` with no workspace, which would have blocked every later run with nothing to delete. Such a fork now counts as failed after 30 minutes, and the next start aborts it first. Since the second review, that includes a replay or retry of the failed fork itself, so its run never turns from abandoned back to in progress. Admission refuses while any other run can still finish, not only the newest, and stamps `created_at` with `clock_timestamp()`.
- **Other fork options.** A run's fork refuses a merge target, a first chat, carried sessions and an issue; any of them could have bypassed completion or "never sends".
- **Start eligibility.** Since the third review, Home's `canStart` asks the fork admission's own rule (`fork_owner_is_eligible`) instead of a copy of its query, so the button cannot drift from who may fork Base.
- **Status read.** Home's status counts lines after the scoped transaction commits, one diff per commit pair at a time, and excludes only a run's completing pull request.
- **Direct-merge guard order.** It now runs after the owner check, so a non-owner still gets a 403.
- **Dialog.** It keeps its state and refusal when the switch goes off mid-start, shows clean refusal messages, falls back to the status when a start response names no run, and notes the run list's cap. Since the second review, its pull request rows name who merged them, as [Status](#status) says, and a failed fork's refusal shows without its doubled `Conflict:` prefix.
- **In-progress line.** It says "a workspace being created" and names the starter until the fork finishes.
- **Prompt.** It says it overrides the PR guidance (push, extend earlier findings, report code bugs) and asks for `link_verification.py`.
- **Preview default.** The frontend-only preview keeps the switch off by default (the mock journey turns it on in Admin), so other features' previews are unchanged.
- **Ontology metadata.** It names the implementing files and tests.

**Checked and unchanged:**
- **Fork digest.** It omits `reverification` when absent, so existing idempotent fork records still match.
- **Ordinary forks and merges.** They behave as before.
- **MSW guard.** An unmocked ordinary clone in vitest is still reported as unhandled (probed directly).
- **Template revision.** Nothing reads the old value.

**Known limits:**
- **Merges outside G5.** A run whose pull request is merged on GitHub outside G5 completes only once G5 records that merge.
- **Child workspaces.** A child of the run's workspace merging into Base does not complete the run.
- **Switch race.** The switch is checked before admission, not inside it: the fork service's context cannot read the agents context's configuration, so a start already past the check when an administrator turns the switch off is still admitted.
- **Owner limit.** The Home button does not anticipate the owner workspace limit.
- **Lost draft.** A reload before Send loses the long draft (owner's "no extra mechanism").
- **Unknown outcome after closing the dialog.** The dialog keeps a start's key after a network error or 5xx only while it stays open. If the fork then stays pending, the run reads as in progress, the button is greyed for everyone, and it blocks starts until the fork counts as failed after 30 minutes.

**Owner answers (2026-11-06):**
- **Agent path.** Starting a run is human-only for now, recorded in the UI action catalog (`g5.ui.api-parity`).
- **Rebase cost.** The cost is accepted. The rule and the resolution prompt now say to ignore them when the repository has no TLA+ models or one side changed none. The prompt names the `git diff --name-only <merge-base> <tip> -- .g5/TLA_verification` check, so a conflict task in a repository without models spends nothing on them.

## Results (2026-11-06)

**Rust**, against Postgres:
- `api_base_reverification` passes (11 tests).
- Guards pass: `formal_review_policy_inventory`, `service_context_layering`, route auth and route SQL, `current_reader_contract`, and the tenant and storage guards.
- The fork, merge-override, auto-merge, Home, workbench and org-admin suites pass, 255 tests.
- Unit tests pass: `base_reverification` and fork service in g5-api-core (13), and diff summary in g5-scm (29).

**Frontend:**
- The related vitest suites pass: 199 files, 3,137 tests.
- Contract tests pass (CSS variables, button test IDs, a11y). Typecheck, lint and prettier are clean.

**End to end:**
- `e2e/flows/base-reverification.spec.ts` passes on the rebuilt full-stack preview twice in a row (BR1 to BR3). It covers:
  - a start from Home;
  - the unsent landing with the Base notice;
  - one run at a time;
  - a refused direct merge;
  - completion by a real pull request merge;
  - an incremental run continuing from the completed run's commits;
  - deletion abandoning a run;
  - the switch turned off.
- `e2e/mock-preview/base-reverification.spec.ts` passes (off by default, on from Admin, then the journey).
- `e2e/browser/findings.spec.ts` passes (3 tests, including the run filter).

**Not run:**
- the full CI suite;
- e2e specs that need a running workspace VM, since the preview has no worker host;
- an agent actually performing a Base re-verification or a model-preserving rebase. Both are agent behaviour.

**Regression e2e on the same preview** (`alignment-shell`, `workspace-merge`, `issue-to-workspace`): 10 passed and 6 failed. All 6 failures are environmental:
- Four need a running workspace VM or an agent, and this preview has no worker host.
- One needs the hermetic OAuth stack.
- One expects the e2e user's onboarding tour to be finished, as CI seeds it. With that state set, it passes.
