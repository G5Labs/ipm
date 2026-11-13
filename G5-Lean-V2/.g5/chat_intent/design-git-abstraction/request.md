# Tighten the Git abstraction

## Human instructions

The user first asked for a review of abstraction strength:

> review the G5 codebase for strength of abstractions. For core concepts of the platform (eg chat, tasks, preview, git, etc) how well encapsulated are they (one interface for interacting with the component - other components use only the interface without inventing other paths). List the top 5 abstractions in the code base and how "leaky" they are.

After reading the review, the user asked:

> draft a design for tightening up the git abstraction

## Agent context

The original review (saved outside the repository as
`~/G5/abstraction-leakiness-review-2026-10-01.md`, against `main` at `05dddc47`)
motivated consolidation of Git execution, credential injection, repository locations
and GitHub access. The supplied audit corrects its counts and classification of
legitimate owners; the [design's evidence section](../../ontology-plan-git-abstraction.md#evidence-and-scope)
records those qualifications. The review's numeric leakiness rating is not a design
requirement or proof of missing enforcement.

The design is the agent's interpretation of "tighten up", except where the human
instructions below supply a specific requirement.

## Clarifications

The user asked of the first design:

> Can github login be consolidated as well?

The agent proposed three things:

- Move the SCM-token calls that AuthService makes (organization/group listing and PAT validation) into the provider client, leaving AuthService's exception as OAuth login identity only.
- Have the provider client, login and bug-report clients share one GitHub HTTP transport, sharing configuration and transport but never credentials or operations.
- Make the guard allow GitHub hosts only in that transport.

Login itself stays outside `ScmProvider` because of `g5.security.login` and `g5.architecture.identity_owner`. The user replied:

> go ahead

The user then asked:

> read https://github.com/G5Labs/G5-Lean/pull/268 and ensure the plan assumes that this work lands shortly

The agent's understanding of PR 268 ("Unify workspace PR creation, user authorship, and automatic refresh", head `b1d3342e`) as it affects this design:

- New PRs are created with the initiating user's GitHub credential, selected by `auth/repository_credentials.rs`.
- A new PR read service (`workspace_pull_request_read.rs`) comes with its own GitHub HTTP client.
- Git entry points signal accepted ref changes so that open PRs refresh.
- The guest `GitRuntime` takes its PATH from the `g5d` launcher setup.

The design treats PR 268 as landed first and builds on these owners. Its current
dependency snapshot and obligations are maintained in the design.

## Validation and retry clarification

The user asked to validate whether the plan results in a tight Git abstraction, then
supplied [another evaluation](https://claude.ai/artifact/G1VgAmoseT5X5yagrYrthQ):

> only pay attention to the git related ones

After the design review and comparison with that audit, the user instructed:

> update the design with your findings. On the subject of retries, do not add any new tracking tables: provide clear errors and the agent/human can determine how and when to retry

Agent interpretation: amend this design and its owning ontology, keeping native Git
and existing policy owners. Specify ref effects, authorized reading, credential scopes,
clear failure outcomes and guards that reject actual bypasses. The new abstraction
adds no automatic replay or retry persistence; it reports failures for an agent or human
to assess. Existing independently governed durability and lifecycle recovery remains
with its current owners. This instruction authorizes design edits, not implementation.

## Implementation authorization

The user asked to rebase the branch on main for newly landed PR functionality, then:
“commit and then start implementation.” The reviewed design was committed before
code changes. The initial implementation and its review continue in the existing
[design](../../ontology-plan-git-abstraction.md); the retry constraint above still governs.

## Workspace Git read isolation

The user added this correction during implementation:

> currently, when G5 clones a repo for a workspace, it clones the whole repo - this includes branches for other workspaces for which the user might not have read access. This leaks the contents of these repositories if the user runs 'git switch'.

The user asks to correct this as part of the work and asks whether Git can limit
checkout to relevant workspace, parent and child branches.

The user then asked whether Git is practical for that requirement and instructed:

> If not, stop working here and continue with your original task

Feasibility conclusion: clone/ref filtering is useful selection but cannot establish
branch-level confidentiality within the current shared object-store model. Isolated
repositories could satisfy it, but require a separate storage/transport redesign.
The security extension is therefore parked and its unimplemented ontology amendment
withdrawn; the original abstraction implementation continues. Sibling-object visibility
remains a known accepted-MVP limitation, not a security guarantee supplied by this work.


## Main simplification integration

The user requested main's concurrent Git simplification changes and ultimately
authorized: “fine, pick up changes from main in a way approved by agents.md.”
The integration therefore appends a merge commit on `design/git-abstraction`.
The user reserves local development for another project: subsequent runtime
verification belongs in CI; local services must remain untouched.

## Second abstraction audit

The user asked for an audit of the abstraction this branch strengthens:

> audit the abstraction that the branch is strengthening.  Are there duplicate paths to accomplish the same thing?  Is the test coverage solid?

After reading the audit, the user instructed:

> go ahead with implementation: fix identified issues and resolve test gaps

Agent interpretation: implement the audit's repairs inside the existing design and retry
constraint (no new retry state), retire dead competing APIs rather than migrating them,
and add executed evidence for the acceptance outcomes the audit found untested.
