# Pull requests with g5d

Workspace terminals and platform-launched agents receive `g5d` on PATH and the current
workspace's API context automatically, with no extra login. The shared root harness
instructs both providers to use this flow for workspace PRs. Its four ontology skills
remain unchanged; PR flags and recovery are documented here and in `g5d pr --help`.
Network-disabled Codex sessions cannot reach the API and must report the block for the
owner to run the command in the workspace terminal, without changing sandbox permissions.

Run from a G5 workspace with its normal G5 credentials:

```sh
g5d pr list                       # Open G5 PRs involving this workspace
g5d pr list --state all --limit 50
g5d pr status                     # Open incoming and outgoing G5 PRs
g5d pr view                       # Latest open outgoing G5 PR, or latest closed/merged one
g5d pr view PR3fa9c1              # any G5 PR you can read, from any workspace (or its UUID)
g5d pr list --all --state all     # every G5 PR you can read in the organization
g5d pr list --workspace dummy     # one workspace's G5 PRs
g5d pr view 42                    # Live GitHub PR in the attached repository
g5d pr view 42 --repo acme/api     # Choose an attached repository explicitly
g5d pr view https://github.com/acme/api/pull/42 --comments
g5d pr view 42 --json id,number,title,state,url
g5d pr view issue-816-landing     # a branch's PR: the open one, else the newest
```

### Find PRs across workspaces

Read-only requests answer for every PR of any workspace you can open
(`g5.git.pr-reads`), as `gh` lets anyone who can read a repository read its PRs:
`view` and `checks` by G5 ID, `list --all` (every readable G5 PR, filtered with
`--state`, `--author @me|EMAIL`, `--search` and `--limit`, at most 100) and
`list --workspace NAME|ID`. A repository named with `--repo` or a PR URL may belong to
any workspace you can open; a bare number still means this workspace's repository.
`--json body` works on every listing. Writes (comment, resolve, edit, close, merge)
keep their owners. An invite-only workspace you are not a member of stays invisible.

`g5d pr list` with any of `-R/--repo`, `-H/--head`, `-B/--base`, `-A/--author`,
`-l/--label` or `-S/--search` lists a repository's PRs as `gh pr list` does, instead of
this workspace's stored G5 PRs:

```sh
g5d pr list --head issue-816-landing --state all     # the PR(s) for a branch
g5d pr list -R acme/api --state merged --search "landing" -L 30
g5d pr list -R acme/api --author octo --label bug,ui
g5d pr list --head issue-816-landing --json number,state,url,g5PullRequests
```

- `--repo` may name a repository of any workspace you can open. Without `--repo` the
  workspace's only GitHub repository is used. A personal workspace usually has two (code and docs), so pass
  `--repo`.
- GitHub decides which PRs match, their state and their order (newest created first),
  including PRs opened outside G5. `--state closed` covers closed and merged PRs, as `gh`
  does. Results are capped at 100 per call (`-L` above 100 is refused), and the output says
  when there are more.
- `--author`, `--label` and `--search` use GitHub search. `--search` refuses `repo:`,
  `is:`, `state:` and similar qualifiers that G5 sets itself; use `--repo` and `--state`.
  An agent's search spends the shared agent search budget, as `g5d issue search` does;
  exit 3 means wait and retry. People's searches are not limited by G5.
- Each PR's `g5PullRequests` (also on `g5d pr view NUMBER`) names the G5 PRs whose stored
  GitHub link is that PR, newest first, with `id`, `status` and the source and target
  workspaces. Only G5 PRs whose source or target workspace you can open are named.
  Readable output lists them under each PR.
- JSON fields are `view`'s names that one listing returns: `id`, `number`, `title`,
  `state`, `url`, `repository`, `author`, `isDraft`, `headRefName`, `headRefOid`,
  `baseRefName`, `createdAt`, `updatedAt`, `mergedAt`, `closedAt`, `labels`, `source`,
  `body` and `g5PullRequests`. Fields that need one PR's full read (`comments`,
  `mergeable` and others) point to `g5d pr view NUMBER`.
- When the repository has no GitHub integration, G5's stored PRs that change the repository are listed instead, each with its recorded head (`headRefOid`).
  That covers a repository not on GitHub (listed without a notice, since GitHub was never
  expected), and a GitHub repository with no GitHub App binding for the organization and
  no GitHub connection for you (stderr gives that reason). Each row has `source: "g5"`, and `id` is the G5 PR. `--head` and `--base` match
  the recorded branches, `--author` takes `@me`, an email or its local part, `--search`
  matches every word in the title or saved description, and `--label` is refused. An
  integration that exists but fails, such as an expired connection or a missing
  permission, is an error, not a fallback.

In a personal workspace, `view`, `view -c` and `checks` also read Base's repositories.
Comments and thread resolution stay limited to the workspace's own repositories.

Use `--workspace-id UUID` to select an accessible workspace, including from outside a
checkout. Workspace VM credentials remain restricted to their own workspace. The existing
`g5d --workspace PATH pr ...` flag selects a local workspace directory.

`--json FIELDS` selects comma-separated fields, following `gh` syntax:

```sh
g5d pr view 42 --json number,title,state,headRefOid,baseRefName
g5d pr list --json id,title,state,providerRepos
g5d pr status --json=title,state,localMergeStatus
g5d pr view 42 --json              # List available GitHub fields without authentication
g5d pr view --json                 # List available G5 fields without authentication
```

`view`, `create`, and `update` emit a plain object; `list` and `status` emit arrays of selected objects.
There is no `pullRequest`/`pullRequests` wrapper. Unknown fields and empty entries
(such as `title,,state`) fail before fetching data. Explicitly empty values
(`--json=` or `--json ""`) also fail. Other G5 commands retain their boolean
`--json` flag. A root-level
`g5d --json pr ...` also lists fields; put `--json FIELDS` after `pr` for selection.

Select `source` to distinguish stored `g5` records from live `github` information.
`id` is a G5 UUID for workspace PRs and GitHub’s node ID for live GitHub PRs;
`number` is GitHub’s repository-local PR number. G5-specific fields include `status`, `localMergeStatus`, `providerStatus`,
`providerRepos`, and workspace IDs/names. `state` uses `OPEN`/`CLOSED`/`MERGED`;
`body` is the G5 PR's canonical description: its saved description, else the
workspace's PR-body Project Doc, or null for neither. It comes from the same server read
as the workspace landing's Implementation description, and readable output also says when
a GitHub description differs or could not be compared. `descriptionOverride` is the raw
saved description. `list` and `status` read `body` for the PRs they show in one request,
without the GitHub comparison. Singular provider
fields (`number`, `url`, `repository`, head/base refs and SHAs) are null unless
exactly one provider record supplies them. Stored provider state may lag GitHub.
Stored records carry no comments or reviews: selecting a GitHub-only field such as
`comments` without a GitHub number or URL fails with the `g5d pr view <number|url>`
command that provides it.

GitHub field support covers the data below; it is not the entire `gh` field catalog.
`mergeable` uses `MERGEABLE`/`CONFLICTING`/`UNKNOWN`; `mergeStateStatus` uses uppercase
names. Nested author objects currently expose `login`; comments contain REST IDs,
author login, body, creation time and URL, rather than the full `gh` nested schema.
`repository`, `source`, `commentCount`, `reviewCommentCount`, `commentsHasMore`,
`reviewThreads`, `reviewThreadsHasMore`, `reviewsHasMore` and `latestReviewsHasMore` are
G5 extensions. `--jq`, `--template` and `--web` are not supported, and `-q` means
`--quiet` rather than gh's `--jq`; using them fails with guidance to pipe `--json` output
into `jq` (`g5d pr view 42 --json title | jq -r .title`) or select `url`.
Unlike `gh pr status`, G5 status remains a flat array of open workspace PRs.

Lists default to 30 results (`--limit` controls this); JSON arrays do not contain a
pagination envelope. Human output reports truncation. `pr view` without a selector
prefers the newest open outgoing PR, then the newest outgoing PR in any state.
Use the UUID from `pr list` to select an incoming or older G5 PR.

GitHub numbers are ambiguous when multiple GitHub repositories are attached: supply
`--repo OWNER/REPO` or a PR URL. A URL or `--repo` may name a repository of any workspace
you can open. Any PR in a selected repository can be read, whichever workspace published
it.

### Repository pull requests and deleted workspaces

A G5 PR has one repository pull request per repository it changes
(`g5.git.pr-repository`): GitHub's PR where publication publishes the repository to
GitHub, otherwise a local one that is the G5 PR itself. Every G5 PR is named `PR` and
its random number as six hex digits (`PR3fa9c1`; `g5Number` in `--json`, where `number`
stays GitHub's), so it never reads as a GitHub number. `g5d pr view PR3fa9c1`, `checks`,
`edit` and `close` select it, in any case, from any workspace you can read; its local
repository pull requests are named by it and have no number of their own. A bare number
(`42`, `#42`) is always GitHub's; in a repository not on GitHub it is refused, pointing
to the `PR` form. G5 PRs and local repository pull requests have no comments or review
threads: `view -c`, `comment` and `resolve` refuse them. `--json repositoryPullRequests` lists a G5 PR's repository pull
requests.

Deleting a workspace first closes its open PRs, then removes it; if the deletion fails
afterwards, delete again. A PR opened meanwhile closes when the workspace is removed. Its PRs stay readable to whoever could read the
deleted workspace, and `view` and `--json` (`sourceWorkspaceDeleted`,
`targetWorkspaceDeleted`) mark the deleted side.
GitHub tokens stay on the server; the configured GitHub App read permission or personal
SCM connection supplies repository access. Missing access is an error.

Live details include title, body, state, author, draft status, head/base refs and SHAs,
change counts and provider mergeability. Null fields remain unknown. GitHub reports
`mergeable` as null while it computes it, so an open PR is re-read for about 3 seconds
before `UNKNOWN` is reported; readable output says when GitHub is still computing and
shows "not applicable" for a merged or closed PR. `--comments`
requires a GitHub number or URL and includes the first 100 discussion comments, inline
review threads and reviews; select `commentsHasMore` in JSON to report additional
discussion pages. The readable header shows how many were fetched of the total
(`Discussion comments (100 of 140)`). Selecting any comment, review-thread or review field also fetches them
without requiring `--comments`.

Readable output adds a `Reviews` section (author, state, body) and a `Review threads`
section listing unresolved threads first, each headed by `path:line`, marked
`unresolved`, `resolved by LOGIN` and/or `outdated`, and followed by its thread ID for
`comment --reply-to` and `resolve`. In JSON, each `reviewThreads` entry
has its node `id` (`PRRT_…`), `diffHunk`, `path`, `line`/`startLine` (null when the thread no longer maps onto the current
diff), `originalLine`/`originalStartLine`, `diffSide`, `subjectType`, `isResolved`,
`isOutdated`, `resolvedBy` and `comments` (REST `id`, `author`, `body`, `createdAt`, `url`,
`commit{oid}`, `originalCommit{oid}`) with a per-thread `commentsHasMore` above 50 replies.
`reviews` entries follow gh: node `id`, `author`, `state`, `body`, `submittedAt` and
`commit{oid}`, plus `url`. Up to 200 threads and 200 reviews are read;
`reviewThreadsHasMore` and `reviewsHasMore` report more.

`reviewDecision` (`APPROVED`, `CHANGES_REQUESTED`, `REVIEW_REQUIRED` or null) and
`latestReviews` (each reviewer's latest review, in the `reviews` shape, up to 100 with
`latestReviewsHasMore`) are a separate, comment-free read selected by those fields;
`--comments` includes them and readable output shows `Review decision` and `Latest
reviews`. Review data comes from GitHub GraphQL, the only API reporting thread
resolution. A GitHub App installation needs Contents read as well as Pull requests read
for any live PR read, because review and comment `commit{oid}` fields are Git objects;
the read is refused before reaching GitHub when the installation lacks either, and a
GitHub refusal names the field it refused. Diffs are not supported by
`view`; use `pr checks` for CI observations.

Stored G5 PRs carry the merge evidence G5 computed when the PR was last created,
updated or refreshed. The `mergeEvidence` field (and readable output) says whether it is
current: `stale` is true once a target branch has moved since then, false when none has,
and null when a target branch could not be read. Each entry in `repos` gives the
repository, `targetBranch`, `targetMoved`, `outcome` and `conflictPaths`, and readable
output lists the conflicting paths. Reads never recompute the evidence: a push (while
Auto-push is on), Refresh in the UI or `g5d workspace pr-refresh` does. For GitHub's own answer, read the live PR by number.

Reads do not publish branches, refresh persisted PR state, create PRs, merge or start a VM.

`refs/remotes/provider/g5/<branch>` in the checkout is GitHub's copy of this workspace's
branch (the PR head) as G5 last observed it, refreshed about every 30 seconds; compare it
with `HEAD` to see whether a push reached GitHub. There is no `pr diff`: use
`git diff origin/<base>...HEAD`.

### Exit codes

`g5d pr` agrees with `gh pr` on 0, 1 and 8 only; `g5d pr --help` lists these too.

| Code | Meaning | gh |
|---|---|---|
| 0 | Success; checks passed or were skipped | 0 |
| 1 | Failed, cancelled or unavailable checks; provider refusal; other errors | 1 |
| 2 | Usage error or invalid input | 1 (gh's 2 means cancelled) |
| 3 | Authentication, permission, network or rate limit | 4 for authentication |
| 4 | create/update: refused before publishing (uncommitted changes, not on the workspace branch) or a push failed; G5 published nothing | — (gh's 4 is authentication) |
| 5 | create/update: the G5 PR is saved, but GitHub publication is incomplete or pending | — |
| 6 | Not found | 1 |
| 7 | comment: outcome unclear; rerun with the printed `--idempotency-key` | — |
| 8 | Checks pending, absent, or not yet reported for `--head`; `--watch` timed out | 8 pending; 1 when absent |
| 130 | Interrupted (Ctrl-C) while reading or watching checks | 2 |


## Create, update by pushing, edit and close

```sh
git commit -am "Fix login" && g5d pr create          # committed work only; pushes for you
g5d pr create -t "Fix login" --body-file description.md
g5d pr create -B main                                # target workspace by name (gh's --base)
g5d pr create --dry-run                              # show title, description and target only
git push                                             # updates the open PR, as on GitHub
git push --force-with-lease                          # after a rebase; the PR follows it too
g5d pr edit -t "Fix login and reconnect" -F body.md  # PR text only; nothing is pushed
g5d pr close 42 -c "Superseded by #43"               # by number, URL, branch or G5 PR ID
g5d pr create --new                                  # replace a GitHub PR merged or closed outside G5
```

`g5d pr` follows `gh pr`, so a push updates the PR, as on GitHub. In each member checkout,
`create` refuses before doing anything when there are uncommitted changes (it lists
them; commit or discard them first). Otherwise it pushes unpushed commits to the
workspace branch, fast-forward only, and publishes that pushed head. It never commits
for you. The UI's **Create PR** and **Update PR** buttons still save first, because
people don't commit by hand. `create` reuses an open G5 PR for the same source and target. It may open a GitHub PR for a
repository that has none yet, for example after a failed first publication. A GitHub PR
that was merged or closed outside G5 is never replaced silently; `create --new` opens a
replacement. The current workspace is the source. The server gives the UI and CLI the
same targets: the readable live parent first, otherwise the first readable live Base
workspace ordered by name, within the source organization. `-B NAME` chooses another
by name, and `--target-workspace-id UUID` by ID. If there is no default, creation fails
before the G5 PR is created; commits it already pushed stay pushed.

`-t/--title TEXT` sets the G5 title and each GitHub PR title. Blank titles are refused.
Omitting it keeps existing titles, and new PRs derive one from commits.
`--body TEXT` or `--body-file PATH` (`-` reads stdin) sets the description and each GitHub
PR's body. A description GitHub cannot accept is refused before the G5 PR or any GitHub
PR changes.

After the PR exists, push to update it. While the workspace's **Auto-push** is on (the
default), a push, including a `--force-with-lease` rewrite, reaches GitHub. G5's stored
PR follows within about a minute, with no extra command, save or VM start. Approvals stay
tied to the reviewed commits. `g5d pr checks --watch --head "$(git rev-parse HEAD)"`
waits until GitHub has the commit.

`g5d pr edit [PR] -t … -b …/-F …` changes the G5 PR's title and saved description and
applies them to its GitHub PRs, as `gh pr edit` does, without saving or pushing. An empty
`--body ""` restores the derived description. Labels, reviewers, assignees, milestones
and retargeting are refused with the reason. `g5d pr close [PR] [-c TEXT]` closes the
G5 PR as the UI's Close does. Its GitHub PRs stay open on GitHub, and `-c` is posted on
each of them first. Commands that take a PR accept a G5 PR ID, a GitHub number or URL, or
a branch (resolved to its open G5 PR, refusing when none or several match).

With **Auto-push off**, pushes stay in G5. The PR's history then says once that the push
was not forwarded, `g5d pr view` notes it, and `g5d pr checks --head` stays pending.
`g5d pr update` (hidden from the main help) republishes the pushed head now. It refuses
uncommitted changes and pushes unpushed commits like `create`, and never commits. It is
also how to retry right away after a recorded refresh failure. With `-t`/`-b`/`-F` it
is `g5d pr edit`.

The result lists each GitHub repository with its PR number and whether that PR was
`created`, `reused` or `replaced`, needs action (`action_required`, `failed`, `merged`)
or was `unchanged`; whether GitHub shows the pushed head (`published` or `pending`); and
whether an explicit title or description was `applied`. JSON callers select
`publication` (`complete` and `repositories`). Exit 0 means every GitHub PR this request
touched is open with the pushed head and any explicit text applied. Exit 5 means the G5
PR is saved but publication is incomplete or pending, with the reasons listed. Exit 4
means the command refused (uncommitted changes, a rejected push) or publishing the
source failed, and later phases did not run.
A GitHub refusal for a missing provider permission, such as a change under
`.github/workflows/` when the organization's GitHub App lacks Workflows write, is
exit 4 with `details.code` `PROVIDER_PERMISSION_REQUIRED`. The message names the file,
the permission and how an administrator grants it, and the refused branch was not
updated. The CLI retries gateway errors (502/503/504 and lost connections) a few times;
rerunning is safe because the operation reconciles current state and reuses the open PR.
If a request with `--title` failed, include that title again to retry it.

Bare `--json` and invalid fields never create or update a PR. `gh pr` verbs G5 does not
provide (`review`, `ready`, `checkout`, `reopen`, `update-branch`, `diff`) and flags such
as `--draft`, `--reviewer` or `merge --squash` fail with the reason and the G5
alternative. `merge --rebase` is accepted, since G5 always rebases.

## Comment, reply and resolve

```sh
g5d pr comment 42 -b '@coderabbitai review'
g5d pr comment --reply-to PRRT_kwDO... -F reply.md
g5d pr resolve PRRT_kwDO...
g5d pr resolve PRRT_kwDO... --unresolve
```

`comment` posts to a GitHub PR's discussion, or with `--reply-to THREAD` replies in a
review thread (the PR may then be omitted); `resolve` resolves a review thread or reopens
it with `--unresolve`. Thread IDs come from `g5d pr view <number> -c` or the
`reviewThreads` `id` field. They take the same PR selectors as `view`, so they reach only
PRs in repositories attached to the workspace, and they act as the workspace owner's
connected GitHub account (the App user grant, or the personal connection), never a bot.
Bodies pass the same secret screen as other public output. These writes confer no
approval or merge authority.

A comment is recorded before it is sent, under an idempotency key (`--idempotency-key`,
default a new one). Rerunning with the same key never posts twice; if the outcome was
unclear, G5 looks for the posted comment instead of sending it again. An unclear outcome
exits 7 and prints the key to reuse. Resolution is idempotent. Replying and resolving
need the GitHub App's Pull requests write permission.

## Observe CI checks

`g5d pr checks` reads live GitHub check runs and commit statuses for the latest outgoing
G5 PR. A G5 UUID selects a stored PR; a GitHub number or URL selects a live provider PR
in a repository of any workspace you can open. G5 PRs include all linked provider
PRs; `--repo OWNER/REPO` selects one. Missing publication or read permissions are errors.
A G5 PR none of whose repositories are on GitHub has no external checks: `checks` says so
and exits 0, also with `--watch` and `--head`, as merge does. A branch whose PR exists only
in G5 selects that G5 PR.

```sh
git push
g5d pr checks --watch --head "$(git rev-parse HEAD)"
g5d pr checks --required          # only the checks the base branch requires
g5d pr checks --details
g5d pr checks 42 --repo OWNER/REPO --json name,state,bucket,link,headRefOid,details
```

`--head` accepts a full commit SHA and requires one selected repository. A mismatch stays
pending, so old passing checks cannot settle a newer push. Watch follows head changes
unless an expected head is specified; Ctrl-C cancels. `--interval` defaults to 15 seconds
(5–3600 allowed) and doubles while the observation is unchanged, up to 2 minutes (or the
interval if larger); any change resets it. `--fail-fast` stops watching on the first
observed failure. Watch stops once every watched GitHub PR is merged or closed, exiting by
that observation, and after `--timeout` minutes (default 30, 1–1440) with exit 8; it never
sleeps past the timeout.

JSON is an array of check rows. Bare `--json` lists supported fields without API access.
Selecting `details` requests diagnostics. Watch progress and errors go to stderr; stdout
contains one final JSON document. Repository/head identity are available as fields.
Exit 0 means a complete nonempty observed set passed or was skipped/neutral; exit 8 means
pending, no checks, an expected-head mismatch, or a watch timeout; exit 1 means
failed/cancelled checks or unavailable/incomplete data. Ctrl-C exits 130. Output projection never changes the exit.

Details include GitHub-published summaries, text and up to 100 file/line annotations,
with an explicit truncation flag above 32,768 characters. Check/status enumeration is
limited to 10 pages of 100 entries each and HTTP bodies to 2 MiB; hitting an enumeration
limit reports incomplete data. Provider requests have deadlines and the complete API
read is bounded. When GitHub rate-limits a repository's read, the checks API marks it
`rateLimited` with GitHub's reset as `retryAt` when supplied. A single read exits 1; watch instead waits until the latest
reset (a minute when none is given) and reads again, unless another repository is
unavailable for a different reason.

Rows are marked `required` when the PR's base branch requires them on GitHub, through
classic branch protection or an active ruleset. As on GitHub, requirements are judged on
the test-merge commit once it has any check or status, and on the head otherwise. A
required check that has not reported on that revision yet is listed as an `expected` row
with a `pending` bucket, so watch waits for it by name. A status context matches by name
even when GitHub binds the requirement to an App, because GitHub does not report which
App posted a status. The required set is cached for up to ten minutes per repository and
branch; the merge policy reads it fresh. These are observations, not proof that
merging is approved. jq/templates, rerun/cancel and
`pr view --json statusCheckRollup` are not supported. Direct Cloud Build/gcloud and raw
build-log retrieval are separate capabilities: a link-only failure may need external
log access before an agent can diagnose it. The reader never follows diagnostic links.
Credentials stay server-side and require Pull requests, Contents, Checks and Commit
statuses read access for a GitHub App installation; Contents read covers the base
branch's protection. An installation granted only Pull requests, Checks and Commit
statuses must add Contents read (PR reads already need it); until then check reads report
the repository unavailable, which keeps the merge policy pending. There are no CI-state migrations or server monitors.

Each check row includes the source `headRefOid`, actual `commitOid`, and `revision`
(`head` or `test_merge`). Open PRs include their current test-merge CI; while GitHub
computes that revision, the observation remains pending. Queued rerequests invalidate
earlier completed attempts. Suites are not rows: an App that never posts a run leaves
nothing pending. A change to the PR itself during a read (head, state or base branch)
ends it at once; when only GitHub moved around an unchanged head (the base advanced, the
test merge was recomputed, an attempt changed), the read is retried, up to three reads,
and a read that never settles stays pending and names what kept moving
(`g5.git.check-read-settling`). Enumeration is bounded to 1,000 suites, runs and status entries per revision;
a larger set reports incomplete access. An annotation error preserves available failure
text, marks details partial, and stops the read. Provider rate limits also stop remaining
repository reads; watch waits for the reported reset rather than polling sooner.
