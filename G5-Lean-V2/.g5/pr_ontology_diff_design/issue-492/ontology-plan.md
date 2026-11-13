# Gate on GitHub's required checks instead of inferring them

The request and the decisions it carries are in the [intent record](../../chat_intent/issue-492/required-checks.md).
Ontology changes are in this branch's diff. After the ontology rebuild on main, they live in
the [Git ontology](../../ontology/software_architecture/git.g5.md); the section links below
name the pre-rebuild `governance.g5.md` and `git_governance.g5.md` sections this plan amended.

- new [g5.git.required-checks](../../ontology/software_architecture/git.g5.md#g5.git.required-checks)
  (was `g5.governance.pr.checks.required`, including the Prepare to Ship split and the check read architecture)
- new [g5.git.check-suites](../../ontology/software_architecture/git.g5.md#g5.git.check-suites)
  (was the revision-binding and check-retrieval amendments)
- amended [merge policy](../../ontology/software_architecture/git.g5.md#g5.git.external-check-policy)

## Problem

The merge gate and the Prepare to Ship verdict wait on every unfinished check suite.
GitHub creates a suite per installed App on each push, including Apps that never post a
run, so the verdict stayed pending forever (#492). The first fix on this branch guessed:
it ignored a suite still empty five minutes after GitHub created it. The repository already
says which checks matter, in its branch protection and rulesets. Reading that list
removes the guess.

## Design

**Owner.** The SCM check reader (`g5-scm/src/providers/checks.rs`) stays the single
owner of the GitHub protocol. It now also reads the base branch's required checks.
The API check read (`workspace_pull_request_checks.rs`) keeps admission, credentials and
the merge verdict. No new database state.

**Reading the required set.** The PR read already returns `base.ref`. For that branch the
reader merges two sources and removes duplicates:
- `GET /repos/{o}/{r}/branches/{base}` → `protection.required_status_checks.checks[{context, app_id}]`,
  ignored when `enforcement_level` is `off`;
- `GET /repos/{o}/{r}/rules/branches/{base}` (paged) → each `required_status_checks` rule's
  `parameters.required_status_checks[{context, integration_id}]`.

A requirement is `(context, app?)`. A null or missing App id means any source. Both shapes
were confirmed against live public repositories (kubernetes, cpython, next.js, github/docs).
The branch read needs Contents read and the rules read needs Metadata read, so the
`PullRequestChecksRead` grant adds `contents: read`. That matches what PR reads already
require since #486. Any failure to read either source fails the check read, so the
repository reports `unavailable`. A missing requirement list is never read as "nothing
required".

**Cache.** An in-process map from the branches endpoint URL to `(read at, requirements)`
keeps the set for ten minutes. The URL identifies provider instance, repository and branch.
Only successful reads are cached, and the cache is consulted only after the caller's own
credential has read the PR. Display and CLI reads use it; the merge gate refreshes it on
every decision (`PullRequestReadQuery::fresh_requirements`, never taken from a request),
so a requirement added within the window cannot be missed at merge. Every merge the gate
admits is completed by GitHub's own merge first (the route imports only after GitHub
reports each provider repository merged; local-only PRs are exempt from the policy).

**Classifying rows.** Each check row gains `required: bool`.
- Requirements are judged on one revision, as GitHub does: the test-merge commit once it
  has any check run or status, otherwise the head (GitHub's "Troubleshooting required
  status checks"). Rows on the other revision are never required.
- A check run on the judged revision is required when its name equals a requirement's
  context and any bound App equals the run's `app.id`.
- A commit status on the judged revision is required when its context matches. GitHub's
  REST status record does not identify the posting App, so an App-bound requirement
  matches a status by name only.
- Each requirement with no matching row adds an `expected` row on the judged revision:
  state `EXPECTED`, bucket `pending`, `required: true`, a description naming the base
  branch.
- The result gains `baseRefName` and `requiredChecks` (the requirement list) so readers
  can say "base branch `main` requires no checks".

**Suites are no longer reported.** The suite-level `PENDING` row only ever stood in for
checks that had not posted yet. Expected rows now do that by name. `abandoned()` and
`EMPTY_SUITE_GRACE` are deleted. Suites are still read: they map runs to their suite, and
the rule that a queued (rerequested) suite's completed runs read pending still uses them.

**Verdict** (one rule, mirrored in `external_checks_verdict` and the frontend's
`aggregateVerdict`):
1. any `fail`/`cancel` row, required or not → failing (keeps the 2026-10-08 decision);
2. else any repository not `ready`, or any `required` row still `pending` → pending;
3. else passing. Not-required pending rows are advisory.

The Ship button pre-gate and the server merge refusal both follow it. Ship-modal polling
stays "while the verdict is pending".

**CLI.** `g5d pr checks` keeps its own watch rule: any pending row or no rows means
pending. Watch needs no change. It loses the endless suite rows and gains expected rows,
so a watch now waits for required checks by name. `required` is added to the `--json`
field list. Readable output marks required rows. gh's `--required` filter stays out of
scope.

**Prepare to Ship.** `ExternalPrChecksSection` renders, per repository, a "Required"
list then a "Not required" list. An expected row shows as "Expected — not yet
reported". With no requirements it says "`main` requires no checks; results below are
advisory unless they fail."

## Choices and reasons

- **GitHub's required checks rather than a G5 setting (option A).** This was the user's
  choice. It needs no G5 configuration UI, can't drift from the repository, and matches
  GitHub's merge box.
- **Only required checks make the gate pending.** Not-required pending checks no longer
  hold a merge. This narrows the 2026-10-08 "pending blocks" decision to the checks that
  were named, which is what the request asked for. A repository requiring nothing is
  therefore passing as soon as nothing has failed, as on GitHub. The UI says so.
- **Failures still block regardless of requirement.** This keeps the human's 2026-10-08
  decision and is conservative: a flaky advisory check can still block until rerun.
- **Judge the revision GitHub judges.** First designed as head-only; review showed GitHub
  uses the test-merge commit once it has a status, so head-only matching could block a
  merge GitHub allows, or pass one GitHub would refuse.
- **Drop suite rows instead of keeping the grace window.** The window was the inference
  the user wants gone. Expected rows cover the case it protected: a fresh head whose
  checks have not posted yet.

## Assumptions and limits

- A status context matching an App-bound requirement is accepted by name. Any account
  that can write statuses could satisfy it in G5's gate; GitHub still checks the App at
  its own merge, which every gated merge goes through. Refusing unattributed statuses was
  rejected: GitHub reports no App for a status, and App-bound statuses are often created
  under a plain user account (cpython's `bedevere/*` by `bedevere-bot`), so such
  requirements would stay expected forever.
- A branch name containing `/` is sent as one path segment (`%2F`). This is unverified
  against live GitHub.
- Strict "branch must be up to date", required reviews and other ruleset conditions are
  not evaluated by G5.
- Not-required runs created later in an active suite can appear after a CLI watch has
  already ended. Required ones can't, because they are expected rows.

## Tests

- Requirement parsing (enforcement off, null/missing App ids, duplicates) and row
  classification (App-bound runs vs. statuses, test-merge never required, expected rows)
  are covered through the API tests below rather than separate SCM unit tests. Rules
  pagination reuses the bounded check enumeration and has no test of its own.
- API tests in `api_pull_request_reads.rs` with GitHub doubles, through the real user and
  VM routes:
  - required set from protection and rules;
  - an expected row while a required run is missing, and passing once it posts;
  - a silent App's empty queued suite never reported;
  - a not-required pending run advisory;
  - a not-required failure blocking;
  - a test-merge run with a required name not counting;
  - an unreadable requirement source making the repository unavailable;
  - a second read inside the window not re-requesting the branch or rules endpoints;
  - the grant minting `contents: read`.
  The existing suite-row and grace-window tests are rewritten to these expectations.
- A merge-route API test drives a mocked snapshot through `ensure_external_checks_merge_policy`:
  refused while a required check is expected, admitted once it passes. This closes the
  route-wiring gap recorded on the merge policy.
- Vitest: `aggregateVerdict` required-only pending; the section's required/not-required
  split, expected rows and the "requires no checks" note; the Ship pre-gate.
- CLI tests: the `required` JSON field and watch pending on an expected row.
- `docs/g5d-pr.md` updated for expected rows, the `required` field and the observed-checks
  caveat.
