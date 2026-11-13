# Override policy follows ordinary merge authority

The [owner decision](../../chat_intent/issue-1116/override-policy.md) resolves
[issue #1116](https://github.com/G5Labs/G5-Lean/issues/1116) by explicitly permitting
an administrator's override wherever they already have ordinary merge authority.
It supersedes this PR's earlier Base-or-owned-target restriction.

## Governing commitments

- [Merge authority](../../ontology/features/workspaces.g5.md#g5.workspace.merge)
  and [source-owner admission](../../ontology/features/workspaces.g5.md#g5.governance.pr.source-owner-merge)
  determine eligible destinations; an override creates no new merge authority.
- [G5 override](../../ontology/features/workspaces.g5.md#g5.workspace.merge.admin-override)
  adds current organization administration to ordinary merge authority.
- [GitHub override](../../ontology/software_architecture/git.g5.md#g5.git.provider-override)
  lets an authorized merger request bypass as themselves; GitHub decides.
- [Current authority](../../ontology/concepts/security.g5.md#g5.security.current-authority),
  [browser decisions](../../ontology/concepts/security.g5.md#g5.security.browser-session-actions)
  and [confirmation and recovery](../../ontology/features/workspaces.g5.md#g5.workspace.merge.admin-override.formal)
  govern both overrides through their effects.

## Design

Retain one shared override policy. Its inputs are browser admission, current
ordinary merge authority and current organization administration. G5 waivers
require all three; GitHub bypass requests require the first two. Reuse existing
ownership, Base-administrator and source-owner-with-attached-requirement rules
rather than introduce another permission or target restriction.

The merge service reopens transactions. Re-establish ordinary merge authority
and the additional G5 administrator condition in every transaction that admits,
records or consumes a new override: preparation, local application, provider
reservation and dispatch. Use the existing membership/permission, ownership and
approval-requirement locks to retain the relied-on facts until commit. The existing
merge-admission interface retains the requirement set, keeping its lock protocol
inside the approvals owner. For source
owners, retiring the last attached requirement revokes merge authority. Preserve
commit-fence order before taking the new authority row locks.

A target transfer is permitted when the caller retains another ordinary merge
path. Loss of the last merge path or G5 administrator status refuses the affected
waiver before new effects. A GitHub-only request can remain authorized after
admin demotion if ordinary merge authority remains. GitHub always receives the
acting person's credential for bypass; a refusal never retries as the App or
another user. Ordinary approved shipping continues without an override.

Reuse existing provider-acceptance evidence for recovery. Completing an already
accepted cut retains its recorded admission and does not originate another
waiver. Before acceptance, a refused dispatch releases undispatched reservations.
Local refusal leaves refs, PR settlement, approvals and override records unchanged.
After settlement, reading the committed response uses read authority so a later
transfer cannot turn a completed merge into a write-authorization error.

## Validation and evidence

Replace the earlier cross-owner refusal expectations with the accepted policy.
Exercise real preview and execution routes for source-owning administrators and
ordinary members, Base and owned targets, browser restrictions and audit records.
Cover target transfer while source authority remains; source transfer, membership
revocation, administrator demotion and requirement retirement when they revoke
applicable authority; and changes between preparation, reservation and dispatch.
GitHub tests verify member bypass requests use their own credential, refuse on
GitHub denial without identity fallback, and cannot waive unmet G5 requirements.
A denied bypass (including 401/403) stops a multi-repository batch before any
ordinary sibling merge; review found that the generic action-required path
previously continued the batch.
Retain the competing-writer lock-order regression and accepted-provider recovery.

Local validation on 2026-11-05 passed 62 focused Rust tests: 14 override API
tests; three provider override/recovery tests (including the refusal and
revocation cases above); 16 automatic-merge tests; 12 route-authority guards;
seven service-boundary guards; two route-SQL guards; two ordinary provider
refusal regressions; and six shared override-policy unit tests. The affected
cases were rerun after review fixes. The multi-repository refusal regression
first reproduced an unintended partial provider merge before the fix.

Eighty frontend tests passed across the override dialog, Prepare to Ship,
pull-request list and PDF view (the latter investigated a CI-only failure).
The UI already consumes the server's separate group allowances and requires no
policy-specific role check. Lint, formatting and full CI results are tracked in
[PR #1118](https://github.com/G5Labs/G5-Lean/pull/1118).

No open product decision remains; implementation is authorized by this request.
