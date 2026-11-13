# Cross-workspace PR reads and discovery in `g5d pr` (issue #830)

Source: [G5Labs/G5-Lean#830](https://github.com/G5Labs/G5-Lean/issues/830) and its
sub-issues [#831](https://github.com/G5Labs/G5-Lean/issues/831) (personal-workspace reads)
and [#832](https://github.com/G5Labs/G5-Lean/issues/832) (`gh pr list`-style listing).
Shareable PR URLs, including the `g5Url` field, are [#838](https://github.com/G5Labs/G5-Lean/issues/838)
and stay out of scope. `g5d pr status` is unchanged.

Governing intent: [g5.git.pr-cli](../../ontology/software_architecture/git.g5.md#g5.git.pr-cli)
says `g5d pr` closely follows `gh pr`. Other governing blocks, unchanged:
[g5.git.pr-identity](../../ontology/software_architecture/git.g5.md#g5.git.pr-identity),
[g5.git.pr-providers](../../ontology/software_architecture/git.g5.md#g5.git.pr-providers),
[g5.git.repository-operation](../../ontology/software_architecture/git.g5.md#g5.git.repository-operation),
[g5.architecture.scm_provider_client](../../ontology/software_architecture/git.g5.md#g5.architecture.scm_provider_client),
[g5.workspace.audience](../../ontology/features/workspaces.g5.md#g5.workspace.audience),
[g5.workspace.identity.reference](../../ontology/features/personal_workspace.g5.md#g5.workspace.identity.reference),
[g5.personal.refusals](../../ontology/features/personal_workspace.g5.md#g5.personal.refusals),
[g5.security.credentials.delegated-read](../../ontology/features/personal_workspace.g5.md#g5.security.credentials.delegated-read)
and [g5.external-issues.cli.search-budget](../../ontology/features/issues.g5.md#g5.external-issues.cli.search-budget).

## Ontology changes

- [git](../../ontology/software_architecture/git.g5.md): new `g5.git.pr-read-repositories`
  writes down the existing repository-selection rule, which was implicit until now, and
  extends it to personal reference repositories. New `g5.git.pr-list` and
  `g5.git.pr-list.search` cover repository listing, and new `g5.git.pr-g5-identity`
  covers how a GitHub PR names its G5 PRs.
- [git](../../ontology/software_architecture/git.g5.md): new `g5.git.pr-list.local`
  covers the stored-PR fallback when there is no GitHub integration.
- [personal workspace](../../ontology/features/personal_workspace.g5.md): new
  `g5.personal.pr-reads`, with the exposure the owner accepted.
  `g5.security.credentials.delegated-read` now says that provider PR reads are not
  delegated reads.

Owner decisions (2026-10-27, [chat intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md)):
personal reads are allowed read-only; listing uses GitHub plus G5 links and falls back to
stored PRs when there is no GitHub integration; implement now.

## Personal workspace reads

Cause, confirmed in code: `attached_repository`
(`g5-api-core/src/services/git/workspace_pull_request_read.rs`) selects only from
`workspace_repos`, and a personal workspace's only member is its providerless private
repository. View, checks and conversation (comment, resolve) all share it.

Change: give `attached_repository` the purpose of the selection, read or write.

- **Read** (view, view `-c`, checks, repository list) adds the current Base members when the
  workspace is personal (`personal_user_id IS NOT NULL`). This is the same set
  `load_reference_repos` checks out at Start and `personal_reference_readable` admits for
  Git reads: Base's members, including the docs repository. Like the Git rule, it is read
  per request, so a repository added to or removed from Base takes effect immediately.
- **Write** (comment, resolve) stays member-only. Create and update don't select a
  repository this way, and the personal refusal of PRs is unchanged.

No new credential is needed. `RepositoryCredentials::resolve(…, ReadPullRequests)` already
requires only organization membership plus the repository's binding; it never checked
workspace membership. Workspace authority stays where it is: the user route's
`scoped_db_for_workspace`, which applies the owner and administrator read rule, and the
internal route's live-incarnation check. Both open an organization-scoped connection that
can read Base's member rows.

Rejected: making Base repositories members of the personal workspace. That would pull
them into flush, receipts, Changes and every other member consumer that
`g5.workspace.identity.reference` deliberately keeps them out of.

Exposure (decided by the owner): any process in the personal VM can call `g5d pr view` on
Base repositories. Code from Base's reference checkouts can already do the same from any
ordinary fork of Base. The data is GitHub's, which the owner can read anyway, and G5's
stored PR records show only under the audience-filtered identity rule. The owner accepted
this, read-only.

## Listing

`g5d pr list` gains `-R/--repo`, `--head`, `--base`, `--author`, `--label` (repeatable or comma-separated, as `gh`) and
`--search`, alongside the existing `-s/--state` and `-L/--limit`.

- **Workspace mode** (none of the new options): unchanged. It lists stored G5 PRs whose
  source or target is this workspace, with the same output.
- **Repository mode** (any new option): lists the PRs of the repository chosen by
  [read selection](#personal-workspace-reads). Without `--repo`, the workspace's single
  readable GitHub repository is used; if there are several, it fails as `view NUMBER` does.
  This includes a personal workspace with Base code and docs repositories. If no candidate
  is on GitHub, the single readable repository is used. `--repo` names a GitHub repository
  by `OWNER/NAME` and a local one by its G5 repository name or slug. Only listing selects
  local repositories, because only listing has a [local fallback](#local-fallback).

Source of truth for repository mode: GitHub decides which PRs exist and match, their state
and their order, as `gh pr list` does. The issue proposed answering from stored G5 PRs
first and asking GitHub only for PRs created outside G5. That was not chosen:

- Only GitHub can tell which PRs were opened outside G5, so a stored-first answer would
  still need a GitHub call to be complete.
- Merging two sources breaks a single ordering, `--limit` and "more results" accounting.
- Stored provider state may lag GitHub, and the acceptance criterion asks for `gh pr list`
  behavior with the same flags.

The stored records are still used, but only to name each GitHub PR's G5 PRs (below), which
costs no provider call.

GitHub operations, which belong to the GitHub client (`g5-scm/src/providers/pulls.rs`)
under `g5.architecture.scm_provider_client`:

| Options | GitHub request | Budget |
|---|---|---|
| `--state`, `--head`, `--base` only | GraphQL `repository.pullRequests(states, headRefName, baseRefName, orderBy: CREATED_AT DESC)` | the read credential's ordinary limit, as `view` |
| any of `--author`, `--label`, `--search` | GraphQL `search(type: ISSUE)` with `repo:OWNER/NAME is:pr`, the state (`is:open`, `is:closed`, `is:merged`), `head:`, `base:`, `author:`, `label:` and the user's text, sorted newest created first | one agent search (`reserve_agent_search`, same keys as issue search); people are not limited |

This follows `gh`, which sends author, label and search filters to search and uses the plain
listing otherwise. `--state closed` covers closed and merged PRs, as `gh` does. A
`repo:`, `is:` or `type:` qualifier in `--search` is refused rather than silently
overridden, as agent issue search does. The repository's read capability
(`PullRequestRead`) gates both requests. Search results that come back as issues or
from another repository are dropped.

Capacity: at most 100 results per request (one GraphQL page). `-L` above 100 is refused in
repository mode with guidance to narrow the filters; workspace mode keeps its current
1–1000 range. `hasMore` reports GitHub's `hasNextPage` or a search count above the page.
An agent search costs one unit of the per-workspace (6/min) and per-organization windows.
It charges the organization window as though it searched every configured issue domain.
That over-counts slightly but keeps one window and one meaning.

API: new routes `GET /api/v1/workspaces/:id/pull-requests/github` (user) and
`GET /api/internal/workspaces/:id/pull-requests/github` (incarnation; this one reserves the
agent budget for search mode). Query: `repo`, `state`, `head`, `base`, `author`, `label`
(comma-separated), `search`, `limit`. A separate route is used because `pull-requests/info`
already uses `head` for the checks commit SHA. The workspace-incarnation auth middleware
admits the internal route for VM tokens.

JSON: `list --json` in repository mode uses `view`'s field names where GitHub returns
them: `id`, `number`, `title`, `state`, `url`, `repository`, `author`, `isDraft`,
`headRefName`, `headRefOid`, `baseRefName`, `createdAt`, `updatedAt`, `mergedAt`,
`closedAt`, `labels` and `source` (`github`), plus `g5PullRequests`. Fields that need a
per-PR read (comments, reviews, mergeability) fail before any request is made and point to
`g5d pr view NUMBER`, as stored records already do. Human output is a table of number,
state, title, head branch and G5 PR id, followed by a truncation note.

## Local fallback

The owner asked to fall back to local listing when there is no GitHub integration. That
means the selected repository has no GitHub access path:

- the repository is not on GitHub (no GitHub name from its clone URL);
- credential resolution reports `no_scm_connection` (no App binding and no connection for
  the owner); or
- it reports `github_app_unavailable` (the deployment has no GitHub App client).

Every other failure stays an error with its recovery step: expired token, inactive or
uncovered binding, permission, provider errors and rate limits. Swapping in stored records
there would hide the reconnection that `g5.git.authorization-loss` requires.

The stored listing has the same audience rule as [G5 identity](#g5-identity) and returns
the G5 PRs that change the repository, newest created first. *Amended in Part 5: originally every PR whose `source_heads` include it, which listed unchanged members too.* It applies
the state (`closed` includes merged, as `gh`), `--head` against the source branch
recorded for that repository, `--base` against the target branch, `--author` against the
creator (`@me`, email or email local part, case-insensitive), and `--search` with every
word in the title or saved description (case-insensitive). `--label` is refused. Rows use
the repository-mode field names: `id` is the G5 UUID, `number` and `url` come from a
stored provider link for that repository if there is one, and the rest are null. Each row
has `source: g5` and `g5PullRequests` with itself. The response carries `fallback` with
the reason, and the CLI prints it on stderr. An agent's fallback spends no search budget.

## G5 identity

`g5PullRequests` on a listed or viewed GitHub PR: the G5 PRs with a
`workspace_pull_request_provider_repos` row for (repository, number), newest first, each
with `id`, `status`, `sourceWorkspaceId`, `sourceWorkspaceName`, `targetWorkspaceId` and
`targetWorkspaceName`. It is a list because a GitHub PR can stay open across a closed G5 PR
and a later one that reuses it. A G5 PR is included only if the reader can open its source
or target workspace: the same predicate as `scoped.rs` `workspace_access_for_role`, run as
one set query for the reader's role (owner or administrator: every live workspace;
otherwise organization visibility, current ownership or an invitation). Review workspaces
are excluded, as there. One query runs per response, after the provider read and outside
its transaction. Adding the field to `view` gives agents the reverse lookup from a number
(for example, a "(#834)" merge subject) to the G5 PR.

## Interactions walked through

- Personal VM: `g5d pr list -R G5Labs/G5-Lean --head issue-816-… --state all`. The internal
  route checks the live incarnation, read selection admits the Base member, credentials
  resolve `ReadPullRequests` as the owner, and the GraphQL listing returns the PR. The
  stored-link query names the G5 PR, because the owner can read the source workspace.
- Personal VM: `g5d pr comment 42 -R G5Labs/G5-Lean …` is refused by write selection:
  "No matching GitHub repository is attached to this workspace".
- Ordinary workspace: `g5d pr list` with no options returns the same stored list as today.
- A Base repository is removed: the next personal read of it is refused, with no restart.
- A GitHub PR linked to an invite-only workspace the reader cannot open: the PR is listed
  from GitHub, and `g5PullRequests` omits that G5 PR.

## Tests (implementation)

- API integration (`g5-api` `integration`, wiremock GitHub, following the existing
  `pull-requests/info` tests):
  - Personal view, checks and repository list on a Base member succeed (user and internal routes).
  - Personal comment and resolve on that repository are refused.
  - A repository outside Base is refused.
  - Ordinary-workspace behavior is unchanged.
  - The listing's GraphQL variables match the options (state mapping, head, base, newest first).
  - Search mode builds the query and spends the agent budget. A refusal returns 429 and
    sends nothing; the user route spends nothing.
  - Reserved qualifiers are refused.
  - `g5PullRequests` is filtered by audience: an invite-only workspace is hidden from a
    non-member and shown to an administrator.
  - The limit cap and `hasMore`.
- g5-scm: query construction and response parsing for both modes, including foreign-repo
  or issue nodes being dropped.
- g5d: argument parsing and mode selection, JSON field validation for repository mode,
  human rendering, and workspace mode unchanged.
- Docs: `docs/g5d-pr.md` and `g5d pr list --help`. Add the branch-to-PR lookup to the
  `.agents/AGENTS.md` PR section.

## Assumptions and acceptance conditions

- G5 publishes a workspace's branch to GitHub under the branch name recorded in
  `source_remote_branch`, so `--head <workspace branch>` finds G5 PRs. GitHub matches
  `headRefName` exactly, without the owner prefix, so it also finds a PR from a fork with
  the same branch name, as `gh` does.
- #832 acceptance: `g5d pr list -R G5Labs/G5-Lean --head issue-816-improve-workspace-landing-for-non-stacked-prs-at --state all`
  returns that branch's PRs with GitHub number and G5 PR id. `--state merged --search
  "<text>" -L 30` matches `gh pr list` with the same flags. Workspace-mode output is
  unchanged.

---

# Part 2: aligning `g5d pr` with `gh pr` (follow-up, 2026-10-27)

Request (owner, this workspace): audit where `g5d pr` does not match `gh pr` and design
a better alignment. The owner pointed especially at `g5d pr update`, which they think is
mostly irrelevant now, although many agent instructions treat it as necessary. In their
words, "pushing to the branch is usually sufficient to update the pr automatically, just
like github" ([chat intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#gh-alignment-request-2026-10-27)).
This part is design only; nothing below is implemented.

Governing intent: [g5.git.pr-cli](../../ontology/software_architecture/git.g5.md#g5.git.pr-cli)
(human). Constraints kept:
- [g5.git.pr-save](../../ontology/software_architecture/git.g5.md#g5.git.pr-save) (human);
- [g5.git.pr-refresh](../../ontology/software_architecture/git.g5.md#g5.git.pr-refresh) (human);
- [g5.git.auto-publication](../../ontology/software_architecture/git.g5.md#g5.git.auto-publication) (human);
- [g5.git.save-publication](../../ontology/software_architecture/git.g5.md#g5.git.save-publication) (human);
- [g5.git.pr-auto-merge](../../ontology/software_architecture/git.g5.md#g5.git.pr-auto-merge);
- [g5.security.agent-human-exceptions](../../ontology/concepts/security.g5.md#g5.security.agent-human-exceptions).

## What a push does today (verified)

The claim holds wherever the workspace auto-pushes. It was tested on #854 on 2026-10-27:
1. A rebased branch was published with a plain `git push --force-with-lease` to the G5
   origin, with no `g5d pr update` and no `g5d workspace refresh`.
2. GitHub's PR head moved to the new commit about 60 s later.
3. G5's stored PR (`sourceHeads`, provider `sourceHeadSha` and `providerHeadSha`)
   followed, and no SHA was rewritten.

The path:

1. The push lands on the workspace branch in G5.
2. Remote integration (`branch_integration_service`) pushes it to the provider branch when
   the workspace's `push_mode` is `auto`. That is the default since migration 040, and a
   workspace's owner can switch Auto-push off in the UI. An accepted rewrite
   (`WorkspaceRewritten`) follows the same policy as a fast-forward. Automatic pushes need
   the organization's GitHub App with write access (`RepositoryOperation::AutoPush`,
   `g5.git.auto-publication`).
3. The PR refresh scanner (`workspace_pull_requests::run_loop`) is woken by the branch
   event or its 15 s tick. It sees the published heads differ from the PR's evidence
   (`needs_push_refresh`), checks that the provider branch already holds the pushed commit
   (`synchronize_after_push` with `after_push`), re-prepares merge evidence, and records
   completion. It also opens a provider PR for a repository that newly needs one, using
   the owner's connected identity.

So for code changes, `git push` already behaves like pushing to a GitHub PR's head
branch. `g5d pr update` still differs from a push in four ways:

| `update` does | Push does | Matters when |
|---|---|---|
| Runs the workspace Save first: commits every uncommitted change in every repository and pushes | Publishes only what was committed and pushed | Always. In a shared checkout it sweeps up another session's edits (memory `commit-only-own-paths-in-shared-checkout`). It was seen re-authoring commits on 2026-10-22 and was not seen doing so on 2026-10-27 |
| Pushes to the provider itself with the caller's credential (`WriteGit`) | Waits for remote integration | Auto-push is off, or the organization has no App write access |
| Applies `--title`/`--body` to the G5 PR and its provider PRs | Changes nothing but code | Editing PR text (`gh pr edit`) |
| Retries now after a recorded refresh failure | Retries on the next push or tick, within the settling budget | Recovery |

It also refuses rewritten history, because Save's push is fast-forward only, while a
`--force-with-lease` push works. Guidance that pairs a rebase with `update` therefore
fails exactly when it matters (`merge-with-agent` prompt, memory
`g5-publication-refuses-rewritten-history`).

## Where guidance treats `update` as required

| Location | Says | Proposed |
|---|---|---|
| `.agents/AGENTS.md`, "Watching a PR's CI" (packaged into every VM's instructions) | "publish the head first (`g5d pr update`)" | "push the branch first (`git push`)" |
| Root `AGENTS.md`, "Workspace pull requests" | create/update/view as the PR verbs; "inspect pending changes before create/update" | `git push` updates an open PR; `g5d pr edit` for text; `update` only where a push is not forwarded |
| `g5d pr --help` (`PR_HELP`) | `g5d pr update && g5d pr checks --watch --head …` | `git push && g5d pr checks --watch --head "$(git rev-parse HEAD)"` |
| Ship with agent prompt (`auto_merge::merge_with_agent_prompt`) step 3, and `g5.git.pr-auto-merge.with-agent` | publish fixes with `g5d pr update` | publish with `git push`, the same path step 2 already uses for a rebase |
| IE persona (`workbench_builtins/personas/ie.md`) | PR "created or updated with `g5d pr create` / `g5d pr update`" | created with `g5d pr create`, updated by pushing |
| Server messages: checks (`workspace_pull_request_checks.rs`, `g5-scm/providers/checks.rs`), refresh failures (`workspace_pull_request_create.rs`), lifecycle (`workspace_pull_request_provider_lifecycle.rs`) | "run g5d pr update" | push again, or retry with the republish command where pushes are not forwarded; name which one applies to this workspace |
| `docs/g5d-pr.md` "Create or update through the UI flow" | update as the routine verb | push first; `edit` for text; republish as the exception |
| Agent memories (`g5d-publish-after-rebase`, `g5-publication-refuses-rewritten-history`, `g5d-pr-update-rewrites-shas`) | force-push, then `workspace refresh`, then `update` | superseded by the experiment above; to be rewritten when this design is implemented |

The UI's Update PR button keeps its role for people (Save, then publish); the CLI and UI
stay one operation underneath.

## Audit: `gh pr` and `g5d pr`

Legend: **=** aligned; **≈** aligned with a documented G5 difference; **✗** missing or
different.

| `gh pr` | `g5d pr` today | Gap | Proposal |
|---|---|---|---|
| (no `update`); `git push` updates the head | `update` = Save + publish | ✗ different verb and side effects | Push is the update path; see decisions 1–3 |
| `create -t -b -F -B -H -d -f -r -a -l --dry-run --web` | `create -t -b -F --target-workspace-id --new`; Save first; reuses an open PR | ≈ create; ✗ `-B`, `--dry-run`; reuse instead of "already exists" | `-B/--base WORKSPACE` (target by name); `--dry-run` on the existing preview route; Save is decision 3; reuse stays as a documented difference |
| `edit -t -b -F --add-label …` | none (title/body only through `update`) | ✗ | `g5d pr edit` (decision 2) |
| `view [<n>\|<url>\|<branch>] -c --json --jq --web` | number, URL or G5 UUID; `-c`, `--json`; `g5PullRequests` (#854) | ✗ branch selector; ✗ `--jq`, `--web` | Selectors (decision 4); `--web` prints the G5 PR URL once #838 lands; `--jq` stays refused |
| `list` filters | workspace G5 PRs, or repository mode (#854) | ≈ default lists this workspace | Keep (#832 decision) |
| `status` sections | flat array of open G5 PRs | ≈ | Keep |
| `checks --watch --fail-fast -i --required --json` | all of these except `--required`; adds `--head`, `--details` | ✗ `--required` | Add `--required` (the rows are already marked) |
| `merge <n> --auto --disable-auto --rebase --merge --squash -d --admin --match-head-commit` | `merge [G5 UUID] --auto --disable-auto` | ✗ number/URL/branch selectors; ✗ strategy flags | Selectors (decision 4); `--rebase` accepted as a no-op (G5 lands rebase only); `--merge`/`--squash`/`-d`/`--admin` refused with the reason; `--match-head-commit` stays deferred by the owner |
| `close [-c] [-d]`, `reopen` | `g5d workspace pr-close` only | ✗ | `g5d pr close [<sel>] [-c TEXT]`, the same operation as `workspace pr-close`; `reopen` refused: run `g5d pr create` |
| `comment -b -F --edit-last --delete-last` | `comment -b -F --reply-to --idempotency-key` | ≈ adds thread replies; ✗ edit/delete-last | Keep; edit/delete deferred |
| (none) | `resolve` | G5 extension | Keep |
| `diff [<n>] --name-only --patch` | none (docs say `git diff origin/<base>...HEAD`) | ✗ | Deferred: own PR = local diff against the target head; a number = provider diff |
| `review --approve/--request-changes/--comment` | none | ✗ by design: an agent never decides an approval | Refuse with guidance: `g5d pr comment`; `g5d workspace request-approval` |
| `ready [--undo]`, `create --draft` | none | ✗ G5 opens no drafts | Refuse with guidance |
| `checkout` | none | ✗ the VM's branch is fixed | Refuse with guidance: read other branches with `git fetch` |
| `update-branch [--rebase]` | none; agents rebase by hand | ✗ | Deferred (decision 5) |
| `--jq`/`--template`, `-q` | refused; `-q` is `--quiet` | ≈ | Keep |
| exit codes (4 = auth) | 3 = auth; 4, 5, 6, 7 G5-specific | ≈ documented | Keep |

## Decisions

1. **A push updates the PR.** Where the workspace auto-pushes, `git push` (plain or
   `--force-with-lease`) to the workspace branch is how an agent updates a PR's code. No
   guidance requires another step after a push. `g5d pr checks --watch --head SHA`
   already waits until GitHub has the commit. Where pushes are not forwarded (Auto-push
   off, or no App write), `g5d pr view`, `checks` and the push-refresh failure say so and
   name the republish command. Owner: the CLI and the guidance; the refresh scanner and
   remote integration stay as they are. Reason: this is `gh`'s model and it already works;
   `update` broke rebases and swept up other sessions' edits.
2. **`g5d pr edit`.** `-t/--title`, `-b/--body`, `-F/--body-file` change the G5 PR's
   title and saved description, and apply them to its provider PRs, with no Save and no
   push. This uses the existing provider write for explicit title/body
   (`ProviderPullRequestWrite`). `--base` and label, reviewer, assignee and milestone
   flags are refused with the reason. Reason: `gh pr edit` is where agents look for this,
   and it separates PR text from publishing code.
3. **Save stays out of the agent's PR verbs.** This one is open; see Q2.
4. **One selector rule.** Every `g5d pr` command that takes a PR accepts a G5 UUID, a
   GitHub number or URL, or a branch name, and defaults to this workspace's outgoing PR.
   A command that acts on a G5 PR (`merge`, `close`, `edit`) maps a GitHub PR to its G5 PR
   through the stored link (`g5.git.pr-g5-identity`), refusing when none or several match.
   A branch resolves through the head-branch lookup from #832. Reason: `gh` accepts all
   three forms, and agents usually hold a number from a merge subject or a URL.
5. **Deferred:** `diff`, `update-branch` (G5's rebase-onto-target), `comment
   --edit-last/--delete-last`, `--jq`, and grouped `status` output. Each needs its own
   operation design, and none blocks the update problem.

The `gh` verbs G5 does not provide (`review`, `ready`, `checkout`, `reopen`, `--draft`,
merge strategies other than rebase) fail before any request, with a one-line reason and
the G5 alternative. This extends the existing `gh_habit_hint` for `--jq`, `--template`
and `--web`.

## Open questions for the owner

- **Q1. What happens to `g5d pr update`?** (a) Recommended: keep it, hidden from the
  main help, as "republish now" for workspaces whose pushes are not forwarded and to retry
  a failed refresh. It does no Save; with `-t`/`-b` it behaves as `edit`; it prints
  that a push is the normal path. (b) Remove it (exit 2 with guidance). Workspaces
  with Auto-push off then use `g5d pr create`, which reuses the open PR. (c) Keep it
  unchanged and only fix the guidance.
- **Q2. Should `g5d pr create` stop saving?** `g5.git.pr-save` (human) says creating or
  updating a PR saves pending work first. (a) Recommended: in the CLI, `create` publishes
  committed work only. It pushes unpushed commits as `gh pr create` does, and refuses with
  the list of uncommitted paths when a repository is dirty. The UI's Create PR and Update
  PR keep Save, and `g5.git.pr-save` is scoped to the UI. (b) Keep Save in the CLI too.
- **Q3. Should an open PR forward pushes when Auto-push is off?** Opening a PR could be
  read as consent to publish that branch. That would make `git push` sufficient
  everywhere, but it would let a manual-mode workspace publish without a click, and
  without App write access it would need the owner's personal credential. That conflicts
  with `g5.git.auto-publication`. Recommended: no. Report "not forwarded" and the
  republish command instead.

- **Q4. Ship with agent step 3.** Your 2026-10-25 replacement prompt
  ([record](../../chat_intent/merge-when-ready/request.md#follow-up-the-merge-with-agent-prompt))
  publishes a rebase with a force-with-lease push but keeps `g5d pr update` for appended
  fixes. Under decision 1 the fixes would be published with `git push` too, so step 3
  becomes "commit, then `git push`". `g5.git.pr-auto-merge.with-agent` is already worded
  that way in this diff, but the prompt text changes only with your confirmation.

Answers (2026-10-27): the owner accepted all four recommendations
([chat intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#owner-answers-2026-10-27)).
They are now decisions in the ontology, and everything except the deferred items is
implemented. Additions found while implementing:
- Internal VM routes for PR targets and preview, so `create -B NAME` and `--dry-run` work
  from the VM.
- `NativeGit::uncommitted_changes` and `checkout_branch` in `g5-scm`, so `g5d` runs Git
  only through the shared runner.
- `sourceAutoPush` on the stored PR summary.

`close` closes the G5 PR only, as the UI's Close does; its GitHub PRs stay open, which is
a documented difference from `gh pr close`.

## Sequencing and tests (when implemented)

1. Guidance and message changes (table above) plus `edit`, under Q1/Q2 as answered.
   Tests:
   - a `g5d` help snapshot without `update` in the push-then-watch example;
   - the auto-merge prompt test (`workspace_pull_request_auto_merge.rs`) expecting `git push` for fixes;
   - API tests that `edit` changes the stored title/body and the provider PR without a Save or a provider branch push;
   - an integration test that a fast-forward push and a force-with-lease push each reach a provider fake and the PR evidence with no explicit write (the #854 experiment, automated);
   - a manual-mode workspace reporting "not forwarded".
2. Selectors and `close`, `checks --required`, `create -B/--dry-run`, refusals with
   guidance. Tests: one `g5d` parser/route test per selector form and per refusal; API
   tests for merge/close by GitHub number, including the none and several cases.
3. Deferred items as separate designs.

# Part 3: one description read for the UI and `g5d pr view` (2026-10-28)

The owner asked that the landing's Implementation description and `g5d pr view` "go
through the same code path so they are aligned and remain aligned"
([intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#one-description-read-for-the-ui-and-g5d-pr-view-2026-10-28)).
Governing block: [`g5.git.pr-description`](../../ontology/software_architecture/git.g5.md#g5.git.pr-description),
amended in this diff.

## What diverged (verified)

- The landing reads `GET /api/v1/workspaces/:id/pull-requests/:prId/description`
  (`read_pull_request_description`), whose `canonical_description` picks the trimmed saved
  description, else the PR-body Project Doc read at the docs repository's preflight head.
  When that read failed, the component fell back to the stored `descriptionOverride` itself.
- `g5d pr view` without a GitHub selector lists the stored PRs (`…/pull-requests/info`),
  picks one in the CLI, and printed `descriptionOverride`; `--json body` mapped to it. A PR
  whose text lives only in the Project Doc showed no description, and whitespace-only saved
  text was printed although the landing treats it as none.

## Decision

The description read stays the only resolver. Its owner is the PR provider lifecycle
service; the route is a projection of it.
- **Internal route.** `GET /api/internal/workspaces/:id/pull-requests/:prId/description`
  calls the same service function as the user route, as the owner, for the workspace
  incarnation, as the other internal PR reads do.
- **`g5d`.** For a single stored G5 PR (`view`, and the outputs of `create`, `edit` and
  `update`), `g5d` fetches the description from that route when the output needs it: plain
  text, or `--json` with `body`. `body` is the read's `description`. The plain text prints
  it, plus the landing's notice when publications differ or could not be compared. The
  extra read happens only then, so `--json state,…` loops cost nothing more. After a write
  that already succeeded, a failed description read is a warning, not a failure: the write's
  exit status stands. `descriptionOverride` stays available as the raw stored field (the
  editor's value).
- **Stored-PR listings** (`list`, `status`) with `--json body` make one batch read,
  `GET …/pull-requests/descriptions?ids=…` (user and internal), for the PRs they show
  after their own state and limit filtering. *Amended 2026-10-28; the first version
  refused `body` here.* The batch is the single read's text half: the same record loader,
  Project Doc reader and `canonical_description`, with no provider comparison or rendering,
  which are the costly part and which `gh pr list --json body` does not offer either.
  Records are read in one transaction; Project Docs are read after it, eight at a time,
  and only for PRs without a non-blank saved description (it would win anyway). At most
  100 IDs per request (`g5d` chunks); an ID on neither side of the workspace is 404.
- **Landing.** When the read fails (a server, network or permission error) and the PR
  record carries non-blank saved text, the landing shows that text with a note that it may
  be out of date; otherwise "The description could not be loaded". *Amended 2026-10-28:
  the owner asked to keep showing the saved description on a server error ("we have it we
  should show it - a note it could be stale is ok"); the first version removed the
  fallback.* This is a last copy on failure, not a second resolver: it never reads the
  Project Doc or decides precedence.
- GitHub PR reads by number or URL keep GitHub's body (`gh pr view` parity); that is
  GitHub's record, not the G5 description.

Rejected: putting the resolved description on every stored PR summary. The list is polled
every 15 s by the landing, read by every `g5d pr` command, and returned whole before `g5d`
filters it, and each resolution may read the docs repository. The opt-in batch for chosen
IDs avoids both.
Rejected: a CLI-only mode that skips provider comparison or rendering. That would be a
second code path; the cost (one provider read per publication plus one render, only when the
description is shown) is acceptable.

## Tests

- API: the internal route returns the same view as the user route for a PR whose text is
  only in the Project Doc, and refuses a PR that is not on the workspace's side.
- `g5d`: `view` plain text and `--json body` print the read's description (Project Doc
  case, blank-saved case), the differs notice, a write's warning on a failed read, and
  `list --json body` through one batch request.
- API (batch): a committed Project Doc, saved, blank-saved and absent cases equal to the
  single read; no GitHub request although one PR is published; the VM route; 404 for a PR
  off the workspace, 422 for an invalid ID or more than 100.
- Frontend: the landing shows the error state, not the stored text, when the read fails.

# Part 4: no complaints where GitHub is not expected (2026-10-28)

Owner request ([intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#no-github-expected-2026-10-28)):
don't complain about missing GitHub data when we can prove GitHub isn't expected. New block
[`g5.git.pr-no-github`](../../ontology/software_architecture/git.g5.md#g5.git.pr-no-github);
[`g5.git.pr-list.local`](../../ontology/software_architecture/git.g5.md#g5.git.pr-list.local)
amended so its notice applies only when GitHub was expected.

## The expectation test (one owner, existing facts)

- **PR scope:** GitHub is expected iff the PR has a provider record or
  `has_missing_provider_pull_request` is true. That function already applies publication's
  gate (`load_provider_candidate`: a GitHub repository with a remote integration on both
  sides), so "expected" means exactly "publication would publish here". No new state.
  Where a surface only has the PR summary, an empty `providerRepos` is enough: a PR that
  expects GitHub but lacks a record is already reported as "publication incomplete" by
  `checks` and refresh.
- **Listing scope:** the selected repository's clone URL is under the GitHub origin
  (`repository_name_from_clone_url`), which `select_repository` already decides.

A repository with a non-GitHub remote counts as not expecting GitHub, since no G5 PR surface
reads another provider.

## Audit and decisions

Already quiet (gated on provider records): the landing's comparison hints, Prepare to Ship's
external checks and provider notices, the "GitHub PR not created" toast, publication exit 5
when nothing was touched, the description warnings in `g5d pr view`, merge's check policy and
Merge when ready's checks gate.

| Surface | Today | Decision |
|---|---|---|
| `g5d pr checks` on a PR that expects no GitHub (server `workspace_pull_request_checks.rs`) | 422 "No matching published GitHub PR. Run g5d pr update…", i.e. a failure that sends agents to a recovery that cannot work | Succeeds with no repositories and `noExternalChecks`; text "No external checks apply: this PR's repositories are not on GitHub." Exit 0, including with `--watch`, `--head` and `--required`. This is the rule merge already applies |
| `g5d pr list` repository mode, repository not on GitHub | stderr "…not on GitHub; listing G5's stored PRs instead of GitHub's." | No notice; the header already says "G5 stored PRs". The notice stays for "GitHub is not connected" |
| `--label` on that listing | "--label needs GitHub…" | Still refused (the flag can't be honored), reworded without GitHub: "G5 PRs have no labels" |
| Branch selector where the listing is the stored fallback | NotFound "GitHub pull request for branch X" (exit 6) | Resolves to the matching G5 PR, open first, else the newest, as for GitHub |
| `g5d pr view`: Auto-push-off line | Printed for any open PR from an Auto-push-off workspace | Only when the PR has provider records |
| `g5d pr close` | "…its GitHub PRs stay open on GitHub." always | That clause only when the PR has a published provider PR |
| Automatic refresh failure for a PR without provider records | Message has the stages "checking the GitHub PR" and "synchronizing with GitHub" and "Check provider access"; Activity labels it "GitHub publication failed" | Neutral stage names and advice. The record stays `provider_failed` (no migration, at the owner's request): Activity entries already carry the PR's `repositoryCount`, and a `provider_failed` entry with 0 repositories is labelled "PR refresh failed". The existing failure-run rules (resolved by the next completed refresh) apply unchanged |
| Merge with agent prompt (`g5-api-routes/src/auto_merge.rs`) | Always describes provider forwarding and check watching | Without provider records: no forwarding sentence. The checks step stays because `checks` now succeeds at once |
| `create`/`update` post-save failure reported as exit 5 "GitHub publication incomplete" | Can come from non-GitHub phases | When the PR expects no GitHub, report the save or refresh error itself, not GitHub publication. Implement only if the phase result can say so without a second query path; otherwise record it as a gap |

Unchanged on purpose:
- A GitHub number or URL given in a workspace with no GitHub repository still fails. The
  caller asked for GitHub.
- `--json` with a GitHub-only field on a stored PR still fails, for the same reason.

## Tests (when implemented)

- API: `checks` for a local-only PR returns success with `noExternalChecks` for the user and
  the VM. A GitHub-candidate PR without a record still returns "publication incomplete".
- `g5d`:
  - `checks` exit 0 with `--watch`/`--head` on that response;
  - the listing fallback with no stderr for "not on GitHub" and with it for "not connected";
  - a branch selector resolving to a stored G5 PR;
  - view, close and update text without GitHub lines for a PR without provider records.
- Refresh: a local-only PR's failure message and Activity label carry no GitHub wording. A
  GitHub PR's message is unchanged.
- Merge with agent prompt snapshot for a local-only PR.

# Part 5: PR reads follow workspace read access (2026-10-28)

Owner request ([intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#pr-reads-follow-workspace-read-access-2026-10-28)):
if the reader can read a workspace, every read-only `g5d pr` request about its PRs works,
with all information that is not secret by design. New block
[`g5.git.pr-reads`](../../ontology/software_architecture/git.g5.md#g5.git.pr-reads);
[`g5.git.pr-read-repositories`](../../ontology/software_architecture/git.g5.md#g5.git.pr-read-repositories)
amended. It replaces the narrower rules of Parts 1 and 3 (stored PRs readable only from
their source or target workspace; other workspaces' PRs only named in listings).

## Found in the preview (verified)

A personal workspace's agent listed Jordan's open PR `dummy → Base` through the stored
repository listing, then could not read it:
- `view <G5 id>` returned NOT_FOUND, because view selects among the asking workspace's own
  PRs;
- the description read requires the path workspace to be a side;
- the repository listing refuses `--json body`.

Separately, the stored listing showed the PR under `docs`, which it does not change.
That was #2, fixed in this part: list only PRs that change the repository, with
`headRefOid`.

## The rule (one owner)

**Readable PR:** a G5 PR whose source or target workspace the reader can open under the
audience rule. The audience predicate already exists (`readable` in
`workspace_pull_request_list.rs`, mirroring `workspace_access_for_role`); it becomes the
only read check.
- A person reads as themselves. A workspace agent reads as its owner, on an
  organization-scoped read connection, as repository listing already does.
- **Readable GitHub PR:** any PR in a GitHub repository that is a member of a workspace the
  reader can open (Base's included), under the owner's per-repository read authority as
  today.

The rule has one server-side owner: a `readable_pull_requests` read in the PR read service
(by ID, by IDs, or filtered list) that every read route uses. No route has its own
side-of-the-PR check for reads.

## Commands

| Request | Today | With this part |
|---|---|---|
| `g5d pr view <G5 id>` | only this workspace's PRs | any readable PR, with its description |
| `g5d pr view NUMBER\|URL\|BRANCH [-c]` | members (+ Base when personal) | any readable workspace's repository |
| `g5d pr checks <sel>` | this workspace's PRs | any readable PR |
| `g5d pr list` | this workspace's PRs | unchanged (`gh`'s current-repository default) |
| `g5d pr list --workspace WS` | — | that workspace's PRs, if readable |
| `g5d pr list --all` | — | every readable PR in the organization: state, author, search and limit filters; at most 100 per call, saying when more exist |
| `--json body` on any listing | own list only; repository mode refused | every row, through the batch description read |
| description read and batch | PR on the path workspace's side | any readable PR |

`view` of another workspace's PR also shows its history and publications, which the stored
record already carries. Text output names the source and target workspaces, so an
auditor sees whose PR it is.

Writes are unchanged: comment and resolve select member repositories; edit, close and
merge act only from the source workspace (or as Base admission allows).

## What stays withheld

Credentials and tokens (never in PR records), check output G5 already redacts, and
workspaces the reader cannot open. The last is the audience rule itself: an invite-only
workspace stays invisible to non-members. No other field is filtered.

## Exposure (owner's choice)

Any process in the workspace VM can then read every PR its owner can open, including code
run from a checkout, as the owner accepted for Base's repositories in Part 1. The
alternative would separate the agent's reads from the owner's; it is not proposed, because
the owner asked for the agent to audit broadly.

Implemented after the owner's "yes, implement" (2026-10-28). Two details settled while
implementing:
- Without a name, a GitHub read keeps meaning this workspace's repository. Widening
  applies only to a repository named by `--repo` or a URL, so `view 42` stays
  unambiguous.
- GitHub listing bodies are fetched only when `--json body` asks for them (`with_body`),
  as `gh` does, rather than on every listing.

## Tests (when implemented)

- API:
  - a personal workspace's VM reads another readable workspace's PR by ID (view,
    description, checks);
  - `--all` lists across workspaces, and an invite-only workspace the owner is not a
    member of stays hidden in every read;
  - the batch description read accepts readable PRs from other workspaces;
  - writes from a non-source workspace are still refused.
- `g5d`: `list --all`/`--workspace` routes and JSON, `view <other id>` text, and
  repository-mode `--json body`.
