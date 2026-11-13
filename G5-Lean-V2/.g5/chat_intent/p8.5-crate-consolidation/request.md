# P8.5 — delete unused code first, then consolidate crates

## Human request

On 2026-10-07 the user, who owns P8.5 per issue 342, asked for a reading of the
current code and a proposal:

> can you read the latest code in g5-lean and proposal how to proceed?

After the proposal to re-scope P8.5 (remove pass-through crates now, regroup the API
core into bounded contexts after P7.2 and the P6.4 decision, instead of the plan's
two-crate merge) the user asked for a target crate set:

> can you propose future crates needed?

and accepted the 17-crate end state:

> i would go as you propose

Before implementation started the user widened the scope to deletion:

> can you also check if the code/crates are useless or not in the scope of g5-lean, we
> should really delete them instead of keeping them without real usage

and, after reviewing the audit's tiers, said:

> go

## Agent interpretation

- P8.5 is kept but re-scoped. Deleting unused and out-of-scope code precedes any crate
  move, because it shrinks the crates that would be merged and removes the risk of
  moving code that is about to be deleted.
- "Useless" means no production caller anywhere in the repository, verified by a
  repository-wide search for every public symbol and command string. "Not in scope"
  means a subsystem design v9 lists as removed (§3.2 cut list, §8.8, §10.5, §11.3).
- Tier 1 (no decision needed) is deleted in independent slices. Tier 2 items that are
  live but outside v9 scope wait for the owner decisions named in the plan: governance
  legacy (another teammate), external issue links, the intent-store query engine,
  persona views and the local validation package.
- The user's standing scope guidance applies: G5-Lean has no external users, so no
  compatibility shims or rollout machinery are kept for removed commands.
