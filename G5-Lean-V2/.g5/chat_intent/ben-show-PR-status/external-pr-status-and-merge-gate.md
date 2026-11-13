# External PR status in Prepare to Ship, with an org merge-block policy

## User instructions (2026-10-07)

- If a workspace branch has an external PR with commit status, checks, or external
  approvals, then G5 "Prepare to Ship" should display them.
- An organization-level knob governs whether or not failures block Merge.

## Agent interpretation

- "External PR" means the provider (GitHub) pull request already linked to the
  workspace's source branch through the existing `workspace_pull_request_provider_repos`
  record. A G5 PR can span several provider repos; status is per provider repo with an
  aggregate verdict across them.
- "commit status, checks" are GitHub commit **status contexts** and **check runs**, which
  G5 already reads for `g5d pr checks` (`workspace_pull_request_checks::read_checks`,
  buckets `pass|fail|cancel|skipping|pending`, availability `ready|pending|unavailable`).
  Surfacing them in the UI reuses that read; it is a new typed projection + display, not a
  new provider read.
- "external approvals" are GitHub PR **reviews / review decision** (APPROVED /
  CHANGES_REQUESTED / REVIEW_REQUIRED). G5 reads these **nowhere** today; adding them is
  net-new in `g5-scm`. Scoped here as part of the same display, flagged as the largest
  net-new piece (see the plan's open decision on splitting it into a follow-up).
- Display lives in the existing **Prepare to Ship** surface (`PrepareToShipModal`), beside
  the provider-PR links, and in the Base **Pull Requests** list provider column.
- "An organization-level knob" is an **org-admin boolean policy**, stored like the existing
  `external_issues_enabled` org-only setting (org `Runtime Config`, `org:admin`-escalated
  `PUT /orgs/:orgId/config`), **default off** so current behavior (checks are advisory) is
  unchanged until an admin opts in.
- "failures block Merge": when the policy is on and the PR's aggregated external state is a
  definite **failure** (any observed check/status in the `fail`/`cancel` bucket, or review
  decision CHANGES_REQUESTED), G5 refuses the Ship/Merge **server-side** and the UI
  disables Ship with that reason. Pending/unknown is shown but treated as advisory (it does
  not itself hard-block), matching the literal "failures block".
- The policy is additive to, not a replacement for, the provider's own branch protection:
  for a provider-bound merge GitHub still enforces its required checks/reviews at merge.
  The G5 knob gives earlier, G5-owned enforcement, covers the button gate and display, and
  also governs the local-application merge path where GitHub is not the gate.
- A checks read is a point-in-time observation bound to the current provider head
  ([g5.governance.pr.checks.revision]); the authoritative merge gate re-reads at merge time
  and never trusts a stale client snapshot or lets old passing checks satisfy a newer head.

## Decisions (resolved 2026-10-08)

1. **Land checks + commit statuses first**; external **review approvals** (the only net-new
   provider read) are a fast-follow increment.
2. The gate blocks on **any** failing check/status; branch-protection required-context
   integration is deferred.
3. Policy **default off**. When on, block on **pending or failure** (require passing) —
   otherwise a PR could be created and merged before checks have had time to fail.
4. The admin toggle lives on the org **Approval Definitions** page (`OrgApprovalsPageContent`).

## Increment-1 scope (this change)

Display external commit statuses + check runs per provider repo with an aggregate verdict in
Prepare to Ship; add the org-only `external_checks_block_merge` boolean (default off) with
its Approval-Definitions toggle; enforce it authoritatively in the server merge path (block
when on and the PR's external verdict is not passing) and reflect it in the Ship button.
External review-decision read/display and the Pull Requests list surfacing are the follow-up.
