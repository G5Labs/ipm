# Workspace rebase consistency

## Human instructions

On 2026-10-06, after investigating QA rebase and PR failures, the user requested:

> start a new branch for improving rebase.
>
> 1. If there's a running VM, any rebase must be done from the VM even if it could be done simply in the central git.
> 2. force pushes from the VM should not confuse the remote git integration.
>
> write a design that will address these challenges. the ontology statement here is that "Rebasing a workspace branch via the UI or in a workspace VM by the agent should not result in system divergence"

The user then specified: “branch new branch from main”.

The user supplied this Create PR failure from their workspace:

> Conflict: Could not push workspace changes for the PR: "5548c24b-f2f6-4adb-a810-03e6ffd4bc6f": ["RECONCILIATION_REQUIRED"]. Saved commits are retained; resolve the issue and retry Create PR.

## Investigation evidence

These are observations from the preceding read-only QA investigation, not additional human requirements:

- [preview-test](https://qa.g5labs.ai/g5/preview-test): the UI source rebase succeeded at 2026-10-05 22:10:42 UTC. Remote integration then opened `ResetBelowBaseline` / `watermark_violation`: workspace `acc064b6`, provider and baseline `d0f3450e`. The UI called this “Rebased publication needs approval — WAITING FOR HUMAN”. This establishes a workspace-side rewrite being classified into the rewrite checkpoint; it does not establish that the provider had independently rebased.
- [ben-home-fork-button](https://qa.g5labs.ai/g5/ben-home-fork-button?session=40ffcc8c-d075-43d3-ab39-efe8d4f71dbf&view=terminal): the UI rebase returned success at 2026-10-06 17:56:11 UTC; Create PR returned 409 twice. After saving, the VM held `6f6f2452` on `d5d4bb6b`, while its fetched G5 workspace branch was `03597493`; the left/right difference was one local commit and five G5 commits. The supplied error matches the member push ancestry guard. This failure preceded provider publication and also affected a local-only repository.
- The first incident additionally had a provider PR that later completed while G5 continued reporting automatic refresh conflicts. That lifecycle issue is distinct from the two rebase boundaries designed here.

## Agent interpretation and decisions

- A running VM owns execution of every explicit workspace source rebase, including clean replay and fast-forward/no-op cases. Admission must account for lifecycle transitions; an unreachable running VM does not permit central fallback.
- Successful rebase means the VM result has been accepted by G5. Provider publication is a separate outcome governed by the existing binding, push mode and credential rules.
- An accepted workspace rewrite against an unchanged provider synchronization point may be propagated with an exact provider lease. It must not be described as an external rewrite or demand the approval reserved for one. This deliberately amends the previous blanket separation of internal rewrite permission and provider rewrite permission, while retaining provider-change protection.
- Normal background provider fast-forward imports retain their existing behavior. The request changes explicit workspace rebases, not every possible difference between live files, saved recovery and published branches.
- Existing Git refs, operation inputs, integration queue and receipts should suffice; no new head registry or rewrite approval token is proposed. A plain, unpushed native rebase does not by itself establish a safe force-push lease for Create PR.
- Work requested here is a reviewed design and ontology changes. No implementation, deployment, provider force-push or QA repair is authorized by this design request alone.

## Artifact and base

Branch `codex/improve-workspace-rebase` was created from fetched `origin/main` at `d1e75d6398d9a84a672564bbb0292afe6639b4b9`.

[Design](../../pr_ontology_diff_design/improve-workspace-rebase/ontology-plan.md).
