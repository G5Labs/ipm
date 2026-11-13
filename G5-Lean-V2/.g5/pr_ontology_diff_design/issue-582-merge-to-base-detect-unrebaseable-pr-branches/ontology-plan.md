# Merge to Base verifies the target at merge time

Request: [#582](https://github.com/G5Labs/G5-Lean/issues/582) and the
[owner's follow-up](../../chat_intent/issue-582-merge-to-base-detect-unrebaseable-pr-branches/request.md).
Governing block: [`g5.git.pr-recovery-cut`](../../ontology/software_architecture/git.g5.md#g5.git.pr-recovery-cut),
amended in this branch. Related: [`g5.git.base-import`](../../ontology/software_architecture/git.g5.md#g5.git.base-import),
[`g5.git.pr-refresh`](../../ontology/software_architecture/git.g5.md#g5.git.pr-refresh),
[`g5.workspace.approval-cut-eligibility`](../../ontology/features/workspaces.g5.md#g5.workspace.approval-cut-eligibility).

## Problem

Each provider PR row records `target_head_sha`, Base's head when the PR was last
published. Provider-first merge refuses unless both Base's current head and GitHub's
live target tip equal that record. Only a republication (Update PR, `g5d pr update`)
rewrites it, and automatic refresh runs only on source pushes. So whenever Base moves
after publication, every merge attempt fails until someone republishes. That
republication changes nothing on GitHub: the PR head is the unchanged source branch
and its base is a branch name. It only rewrites G5's record.

## Why the record is unnecessary

Everything the record protects is checked at merge time already, or can be:

- Readiness: `merge_workspace_pull_request` already runs a fresh
  `TargetMergeExecution` preflight of the current source against Base's current
  committed heads, and blocks unless it is `ready`.
- Approval: merge admission and approval eligibility pin the source head vector only.
  A target-only move invalidates neither.
- Import safety: accepted-merge import requires the accepted merge and Base's head at
  import to descend from the fetched tip. It does not read the record.
- Live target safety (#551): what matters is that GitHub's target tip equals the Base
  head G5 classified, so GitHub rebases onto the history G5 checked. The record is
  only a stale proxy for that.

## Decision

Provider-first merge verifies the target against the heads its own fresh preflight
classified, then records them:

1. `merge_workspace_pull_request` carries the target heads its fresh preflight
   classified into `ProviderFirstMergeRequest`. The field is serde-defaulted because
   the request is also persisted as a consumer input.
2. `merge_provider_pull_requests`, under its existing PR row lock, requires Base's
   current committed head for each repository to equal the classified head. Otherwise
   it refuses: "Base changed while merging; merge again."
3. The provider preflight requires GitHub's live target tip to equal the classified
   head instead of the publication record.
4. The provider row records the verified head as `target_head_sha` with the merge
   outcome, so the PR names the target it was merged onto.
5. The comparisons with the publication-time record (local "Workspace target changed
   since PR publication" and the live-tip check) are removed. No source save, push,
   GitHub write or new admission is involved.

When GitHub's tip differs from Base's head, GitHub has target commits that Base has not
imported yet (Base imports and never pushes). Merge refuses with a plain retry message:
Base pulls automatically, usually within a minute; merge again after it has, or fetch
on Base. Merging anyway would let GitHub rebase onto history G5 never classified.

## Rejected alternatives

- **Automatic refresh also republishes on target moves.** This adds provider calls on
  every Base move for every open PR, and still leaves a window in which merge refuses.
- **Merge republishes, then retries.** Republication is a source-owner continuation
  with save/push semantics and its own transactions. Merge actors are target owners or
  Base admins. It would rewrite the same record the merge can verify directly.
- **Merge pulls Base itself when GitHub is ahead.** Base import has its own owner and
  pause/divergence policy, and auto-pull lag is normally under a minute. Revisit if the
  retry proves frequent.

## Unchanged guarantees and limits

- GitHub may still advance the target after the live check and before it merges. This
  is outside G5's lock, as before; import still checks ancestry.
- The GitHub PR description derived at publication may describe the diff against the
  older target. It is informational and refreshed on the next publication.
- The unrebaseable-branch refusal and the 405 explanation from the first part of #582
  are unchanged.

## Tests

- Replace `provider_merge_refuses_a_target_that_moved_after_the_last_publication` with
  two tests:
  - The target moved and Base imported it, while the provider row still records the
    old head: merge succeeds without republication and records the new head.
  - GitHub moved and Base has not imported it: merge refuses with the retry message
    before any provider merge.
- Keep the existing republished-after-move, retargeted-PR and admission tests passing.
