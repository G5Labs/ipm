# Issue #1016: GitHub read budget (Tier 1)

Request and decisions: [chat intent](../../chat_intent/issue-1016/request.md). Ontology changes:
- [provider read budget](../../ontology/software_architecture/git.g5.md#g5.git.provider-read-budget), with [completed provider PRs](../../ontology/software_architecture/git.g5.md#g5.git.pr-refresh.completed-provider), [settled check observations](../../ontology/software_architecture/git.g5.md#g5.git.check-observation) and [bounded check waits](../../ontology/software_architecture/git.g5.md#g5.git.check-waits);
- amended [merge-when-ready readiness](../../ontology/software_architecture/git.g5.md#g5.git.pr-auto-merge.readiness).

## Problem

All workspaces of an organization share its GitHub App installation's REST budget (5,000/hour baseline). QA exhausted it on 2026-10-31. The audit (workspace Project Doc `github-api-audit.md`) found these per-source costs:

| Source | Cost | Behavior |
| --- | --- | --- |
| `checks=true` read | ~14 GETs on an open, mergeable PR | Half of it re-reads the same observation |
| `g5d pr checks --watch` | ~3,500–5,000/hour per watcher | 10 s polls; no time limit, no backoff; does not stop on merged/closed |
| Prepare to Ship | ~3,400/hour per open modal | 15 s polls |
| Merge-when-ready pass | ~1,700/hour per armed PR | Reads checks every 30 s, even while only approvals are missing |
| Automatic push refresh | 240/hour per completed provider PR (1,680/hour on QA) | Re-reads GitHub every 15 s for a PR it already found completed (#1016) |

The design rule: repeated reads scale with provider change, not with time or with the number of waiters.

## Owners and changes

1. **Completed provider PRs: provider PR lifecycle (`workspace_pull_request_provider_lifecycle.rs`).**
   - `ensure_automatic_refresh_allowed` already refuses a completed provider PR, but its caller rolls back the observation, so the next scan re-reads GitHub.
   - Each provider row now carries `provider_completed_pr_number`: the provider PR number automatic refresh observed merged or closed.
   - The owner writes it under the PR row lock. The refresh commits that write and returns the same refusal, which records the Activity failure once (existing deduplication).
   - While the marker equals the row's provider PR number, refresh refuses before preparing evidence and makes no GitHub read.
   - Recording the row `open` clears the marker. That happens when Update observes the PR open again, or when a replacement provider PR is created.
   - Merge recovery is unchanged: rows with an unresolved merge keep the existing reconcile-and-refuse path, uncommitted.
   - *Why a new column:* `provider_status='merged'` belongs to merge recovery (pinned admission, `merged_outside_g5`, a source-head match), and the status constraint has no closed value. Reusing it would let refresh make merge recovery's decision.
2. **Merge-when-ready pacing: the scan owner (`auto_merge.rs`).**
   - Pending checks, settling heads and a merge that is due keep the 30-second read.
   - Any other waiting outcome backs off the next checks read, from 60 seconds doubling to 5 minutes. These outcomes are: failing or unreadable checks (including a rate-limited read), unmet approvals, an attempt inside its ten-minute retry window, or a refused merge. Skipped passes keep the recorded progress and reason.
   - Repositories whose GitHub PR is already merged leave the checks gate and the settle window, so the merge (which records them merged outside G5) is reached on the first pass. Without this, a merged PR re-read its checks every pass for the whole settle window.
   - A change in the PR's source or target heads or its admission resets the backoff (read from G5's own records, no GitHub call).
   - The pacing lives in the scan owner's memory, like refresh's settling budget. Losing it to a restart or leader change only costs one extra read.
   - *Rejected: moving the approval gate before the checks read.* The settle timer would start only after the last approval, adding two minutes of latency, and the waiting reason would change precedence.
3. **Check waits: `g5d pr checks --watch`, Prepare to Ship, VM guidance.**
   - The checks reader adds per repository `prState` (`open`/`closed`/`merged`), and `retryAt` when GitHub rate-limited the read.
   - `--watch`:
     - starts at `--interval`, which now defaults to 15 s with a 5 s minimum;
     - doubles while the observation is unchanged, up to 2 minutes (or the interval if larger);
     - sleeps until `retryAt` on a rate limit instead of exiting 1;
     - stops once every watched provider PR is merged or closed;
     - stops after `--timeout` (default 30 minutes) with exit 8.
   - Exit codes keep their meanings.
   - Prepare to Ship polls pending checks every 30 s. While rate-limited it waits until `retryAt`, clamped to between 1 and 5 minutes; while unavailable for another reason it waits 1 minute.
   - `.agents/AGENTS.md`, `g5d pr --help` and `docs/g5d-pr.md` say the watcher waits out rate limits and stops by itself; agents do not rerun it immediately. The merge-agent prompt is the owner's own text and already asks for a bounded wait, so it is unchanged.
   - A failure already observed when the timeout passes still exits 1, as a single read would.
4. **Settled observations: SCM checks reader (`g5-scm/src/providers/checks.rs`).**
   - The reader reports a single observation when it finds a required check pending or expected, or a test-merge revision GitHub is still computing.
   - Only a result that could settle the merge gate's verdict repeats the snapshot and PR-revision reads, and it reports pending if anything changed. That includes a result where only advisory checks are pending, since the verdict passes it.
   - No caller flag is needed: a pending result only causes another read. The merge gate's admission, the watcher's exit and Prepare to Ship's verdict all still rest on a confirmed observation.

## Acceptance

- A completed provider PR with a persistent head mismatch costs one GitHub PR read across repeated automatic scans and one Activity failure. Update after a GitHub reopen clears the pause, and the next automatic refresh proceeds.
- An open provider PR still refreshes after a source push. A transient settling failure still retries. Merge-pending rows keep reconciling.
- Merge when ready reads checks once for successive passes that are waiting on approvals or failing checks. It reads again on the first pass after an approval or push, and still merges after the two-minute settle.
- A read with a required check pending issues no re-reads. A settled one, including one where only advisory checks are pending, still detects a change between its two observations.
- `--watch` stops on a merged PR, on timeout (exit 8), and after waiting out a reported rate limit. Its interval grows while the observation is unchanged.

## Gaps and follow-ups

- **Remaining polling.** Watchers and UI tabs still read separately. A shared per-head snapshot with single-flight, conditional requests and outbound-call telemetry is Tier 2.
- **Webhooks.** `pull_request`, `check_run`, `check_suite` and `status` events are Tier 3. They need a public signed route, deduplicated deliveries, fan-out across replicas, reconciliation of missed deliveries and polling fallback for self-hosted installations. Tracked as [#1023](https://github.com/G5Labs/G5-Lean/issues/1023) (shared snapshot, conditional requests, telemetry) and [#1024](https://github.com/G5Labs/G5-Lean/issues/1024) (webhooks).
- **Merge-pending rows.** A provider row with an unresolved merge still reads GitHub on each refresh pass until the merge is retried (merge recovery's existing behavior; rare).
- **Replica-local state.** Merge-when-ready pacing is per scan owner. Event hints and caches remain process-local.
