# Gate on named required checks instead of inferring them

## User instructions

- 2026-10-13: "Evaluate the complexity of explicitly naming the checks that the system
  should look for. Then we can avoid all this implicitly inferring whether the check is
  meaningful or posted yet." This followed the [issue #492](https://github.com/G5Labs/G5-Lean/issues/492)
  fix, which dropped a check suite (GitHub's per-App container for check runs) when it
  was still empty five minutes after GitHub created it.
- 2026-10-14: "Let's go with A." Option A, as presented: read GitHub's own required
  checks for the PR's base branch (classic branch protection and rulesets) rather than
  keep a G5 org setting listing them.
- 2026-10-14: "When showing the checks in the Prepare to Ship dialog, split them by
  required and not required."
- 2026-10-14: "It's fine to cache the set of checks for a repo - it's unlikely to change
  frequently and enforcement is redundantly applied by github."

## Agent interpretation

The user didn't answer the four follow-up choices presented with option A. The plan
adopts the recommendations that came with them:

1. A repository whose base branch requires no checks has nothing to wait for. It is
   passing unless a check fails.
2. Any definite failure still blocks, required or not, keeping the 2026-10-08 decision
   in the [merge-gate intent](../ben-show-PR-status/external-pr-status-and-merge-gate.md#decisions-resolved-2026-10-08).
   Only required checks make the gate pending.
3. A requirement is met only on the revision GitHub judges. Amended 2026-10-14 after
   review: GitHub's "Troubleshooting required status checks" page says the test-merge
   commit must pass once it has a status, and the head otherwise, so G5 does the same.
   Results on the other revision are still shown, and their failures still block under
   point 2.
4. This replaces the #492 empty-suite window on the same branch.
