# Override authority follows ordinary merge authority

## Owner decision

In the follow-up to issue #1116, the owner requested:

> I suggest explicitly allowing admins to waive G5 requirements wherever they
> already have ordinary merge authority, and allowing other authorized mergers
> to request GitHub bypass under their own GitHub permissions. Document and test
> those rules, then resolve #1116 through that policy decision.

The owner also required consistent execution authority and permitted/refused
cases, including authority changes during a merge, if retaining the previous
restrictions instead.

## Interpretation

Implement the requested broader policy. Source-owner merge authority can qualify
an administrator to waive G5 requirements into another person's non-Base parent.
An authorized merger need not be a G5 administrator to request GitHub bypass;
GitHub decides using that person's own identity. Neither override creates
ordinary merge authority. Browser confirmation, current authority, the exact
confirmed revision and blockers, audit records, and accepted-provider recovery
continue to apply.
