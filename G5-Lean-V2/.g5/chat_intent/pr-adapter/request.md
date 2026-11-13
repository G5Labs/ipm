# PR adapter request (2026-10-28, 2026-10-29)

## Human instructions

2026-10-28, in the #830 workspace chat:

> Consider a PR design that more closely models how issues work: an adapter API that
> establishes what can be done with a PR. Two implementations: one that goes to github,
> one that is implemented locally. When a workspace makes PRs, it makes one per repo with
> changes, selecting the implementation based on whether the repo has a remote integration
> or not. 'g5d pr' and the UI would both use the adapter. Tell me about the pros and cons
> of this approach. An alternative is mirroring - we don't follow this approach for issues.
> Do not implement anything yet.

The agent replied with pros and cons, and proposed a middle option: a thin workspace-level
record that groups per-repository PRs and owns approvals, merge admission and atomic
coordination, merge when ready, overrides and Activity, plus a per-repository adapter that
owns PR content and operations.

2026-10-29:

> show me a design for the middle option.

## Interpretation

- The design is requested, not authorized for implementation.
- "Mirroring" is read as today's model: a stored G5 PR whose GitHub PRs are publications of
  it. The middle option keeps the G5 PR as the reviewed, atomic unit and moves per-repository
  content and operations behind adapters, as `g5.issue.native.adapter` does for issues.

## Implementation request (2026-10-30)

> go build the whole plan - pr-adapter plus teh deletion and visibility rules

### Interpretation

- This authorizes implementing the saved [plan](../../pr_ontology_diff_design/pr-adapter/ontology-plan.md)
  and the deletion and visibility rules (`g5.workspace.delete.pull-requests`,
  `g5.git.pr-reads`, `g5.ui.deleted-workspace-prs`).
- "The whole plan" includes its numbering phase, so the plan's proposed answers to its two open
  questions are taken as accepted: every G5 PR gets an organization-wide number that its local
  repository PRs share (`g5.git.pr-local-number`), and local repository PRs carry no
  conversation for now (`g5.git.pr-local-conversation`).

## Adapter boundary audit (2026-10-30)

> audit the API - does the pr adapter mechanism tightly gate access to pr content? eg no code
> paths to access pr state that should go through the adapter but don't

The agent reported that authorization was tight but the adapter boundary was not, and proposed
five fixes. The owner answered:

> do 1,2,3 and 5

### Interpretation

- Fix 1: first-party surfaces (UI, `g5d`) read GitHub publication facts from the repository
  pull requests. The raw `providerRepos` stays in the API, unused by them, because a VM's
  `g5d` can predate the API.
- Fix 2: Activity labels and remote tracking's import hold go through PR-service functions.
- Fix 3: `checks` iterates the repository pull requests, and the listing uses the binding's rules.
- Fix 5: the binding does not depend on the PR's status.
- Fix 4 (a compile-time boundary around the GitHub client) was not requested.

## Random hex PR names (2026-11-02)

The owner asked how hard it would be to move G5 PR numbers from a counter to hashes or
random numbers. The agent answered that the `bigint` column fits short hex, that a random
pick needs a retry on a concurrent collision, and that switching was nearly free while this
branch was unshipped. The owner answered:

> update to use PR###### (0 pad to ensure it's always 6 digits).  We can remove the sequence
> because we're using random numbers.  This branch hasn't shipped yet, so you can modify the
> existing db migration.

The agent first read `#` as a decimal digit; the owner corrected it:

> I meant hex

### Interpretation

- Numbers from 1 to 0xffffff, unique per organization, named `PR` and six zero-padded
  lowercase hex digits (`PR3fa9c1`). Selectors take exactly six hex digits in any case.
- Migration 139 is edited in place: no identity sequence; a trigger picks an unused random
  number, and the creation path retries if a concurrent creation took the same one.
