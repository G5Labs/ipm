# G5 PRs over per-repository pull requests, behind adapters

Source: owner request in the #830 workspace chat, 2026-10-28 and 2026-10-29
([chat intent](../../chat_intent/pr-adapter/request.md)). The owner asked for a design for
"the middle option": a thin workspace-level record grouping per-repository PRs, with a
GitHub adapter and a local adapter behind one API that both `g5d pr` and the UI use. On
2026-10-30 the owner asked to build the whole plan together with the deletion and
visibility rules; [Implementation](#implementation) records what was built.

Governing intent, unchanged:
[g5.git.pr-identity](../../ontology/software_architecture/git.g5.md#g5.git.pr-identity),
[g5.git.pr-providers](../../ontology/software_architecture/git.g5.md#g5.git.pr-providers),
[g5.git.multi-repo-merge](../../ontology/software_architecture/git.g5.md#g5.git.multi-repo-merge),
[g5.git.pr-recovery-cut](../../ontology/software_architecture/git.g5.md#g5.git.pr-recovery-cut),
[g5.git.pr-cli](../../ontology/software_architecture/git.g5.md#g5.git.pr-cli),
[g5.governance.approval.version](../../ontology/features/reviews.g5.md#g5.governance.approval.version),
[g5.git.pr-auto-merge](../../ontology/software_architecture/git.g5.md#g5.git.pr-auto-merge),
[g5.workspace.merge.admin-override.formal](../../ontology/features/workspaces.g5.md#g5.workspace.merge.admin-override.formal),
[g5.architecture.scm_provider_client](../../ontology/software_architecture/git.g5.md#g5.architecture.scm_provider_client)
and the model being copied,
[g5.issue.native.adapter](../../ontology/features/issues.g5.md#g5.issue.native.adapter).

## Ontology changes

- [git](../../ontology/software_architecture/git.g5.md): new
  `g5.git.pr-repository` (what a repository pull request is and which adapter it binds) and
  `g5.git.pr-adapter` (who owns what). They expand the human block `g5.git.pr-providers`
  and leave it as written.
- [git](../../ontology/software_architecture/git.g5.md): `g5.git.pr-list`,
  `g5.git.pr-list.local`, `g5.git.pr-no-github`, `g5.git.pr-g5-identity` and
  `g5.git.pr-cli.selectors` are amended to use repository pull requests.
- [git](../../ontology/software_architecture/git.g5.md): `g5.git.pr-local-number` and
  `g5.git.pr-local-conversation`, proposed as open questions and accepted with the
  implementation request.
- Deletion and visibility (owner decisions 2026-10-29 and 2026-10-30, recorded in the
  [#830 intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#deleted-workspaces-2026-10-29)):
  new [g5.workspace.delete.pull-requests](../../ontology/features/workspaces.g5.md#g5.workspace.delete.pull-requests)
  and [g5.ui.deleted-workspace-prs](../../ontology/features/ui.g5.md#g5.ui.deleted-workspace-prs);
  [g5.git.pr-reads](../../ontology/software_architecture/git.g5.md#g5.git.pr-reads) amended.
- [ui](../../ontology/features/ui.g5.md): the rows of `g5.workbench.landing.implementation`
  show each changed member's repository pull request.

## What changes, in one paragraph

Today's G5 PR already holds everything that has to be workspace-wide: one record per source
and target pair, the recorded version vector, approvals pinned to that vector, merge
admission, merge when ready, override and events. GitHub content (body, state,
conversation, threads, checks, mergeability) is already read live. What is missing is the
per-repository half. A G5 PR's local-only members have no pull-request identity, no number,
and no place in `checks` (a mixed PR silently drops them). Each surface also decides for
itself whether to ask GitHub or the stored records: `g5d`'s `request_path`, the listing's
`list_stored` fallback, `checks`' `noExternalChecks` branch, and the selectors' `classify`.
The design keeps the G5 PR as the unit of work. It gives every changed member a
**repository pull request** served by an adapter, and routes every surface through one
service that dispatches by adapter. So most of the change is restructuring, not new state.

## Repository pull requests

A G5 PR has one repository pull request per member it changes. Publication already decides
"changes" (`repo_needs_provider_pr`: the source and target heads differ and the preflight is
neither `Identical` nor `SourceAlreadyMerged`). A member with a recorded GitHub PR keeps it,
because `synchronize_provider_pull_requests` keeps recorded rows today.

**Binding.** A repository pull request is GitHub-bound when its member has a GitHub link or
would be published to GitHub. That is exactly the test `g5.git.pr-no-github` already calls
"expects GitHub". Every other repository pull request is local. So "GitHub is expected" and
"the GitHub adapter is bound" become one fact, not two rules that could drift.

**Local repository pull requests are derived, not stored.** Each one is a projection of its
G5 PR:

| Field | Taken from |
|---|---|
| identity | (G5 PR, repository) |
| state | the G5 PR's open, merged or closed |
| head and base | the recorded source and target heads and branches |
| body | the canonical description (`g5.git.pr-description`), so a local one always "matches" |
| author | the PR's creator |
| checks | none apply |

`list_stored` already computes this projection. The one missing piece is a number
(question below).

**The binding follows the member, not the PR's creation.** Example: the docs repository is
local when the G5 PR opens, and the owner then runs Make Remote. Publication now expects
GitHub, so the next refresh or `g5d pr update` creates its GitHub PR. Creating a
repository's first provider PR during update is already allowed by
`g5.git.pr-write-identity`. The local projection ends because the member now has a GitHub
link, and nothing is lost because it held no state.

Fixing the adapter when the PR is created was rejected. Then a member that became published
would keep merging as local into a remote-bound target, or would need a "superseded"
lifecycle. Losing a connection never rebinds a member; it stays an error, as
`g5.git.authorization-loss` and the current fallback rule require.

## The adapter

### Owners

| Owner | Owns | Decides |
|---|---|---|
| PR service (G5 PR) | identity, recorded vectors, publication decision and binding, canonical description, approvals, merge admission, merge when ready, override records, events, announcements, read/write authority, selection | G5 PR lifecycle (open → merged/closed), admission, which members get repository PRs |
| GitHub adapter (on `g5-scm` `RepositoryClient`) | GitHub protocol, credentials, publication pushes and refusals (`PROVIDER_PERMISSION_REQUIRED`, workflow files), required checks, provider rules and merge state, GitHub-override identity, comment receipts, agent search budget | whether GitHub accepts a create, update, merge or comment, and what GitHub reports |
| Local adapter | no state of its own; reads the G5 PR record and Git | nothing beyond the G5 PR's own transitions |
| Workspace merge (`workspace_merge_service`) | the atomic target cut under the commit fences | whether the cut commits |

The PR service coordinates. The adapters decide their own transitions. The workspace merge
owns the commit. This is the same split as issues: the issue service keeps links and
authority, and adapters own content and external guards.

### Operation sketch

An adapter serves a repository pull request with:

- reads: `read`, `list/search`, `checks`, `conversation`
- writes: `comment/reply/resolve`, `publish`, `edit`, `close`
- merge: `merge_state`, `merge(at the admitted head, as an identity)`, `observe_merge`

The local adapter answers reads from the projection. It reports "no external checks apply"
and refuses conversation writes before any effect (question below). Its `merge` adds no
provider effect: it contributes its member's clean-merge plan to the G5 cut, as local-only
members do today. Dispatch can follow the issue code's style (a set of optional backends
chosen per access) or use an enum. That is an implementation choice.

### Merge, walked through the adapters

The sequence and its guarantees are today's (`routes/workspaces.rs` merge route,
`coordinated_repo_publication.rs`, `workspace_pull_request_provider_lifecycle.rs`). Only the
call sites change.

1. **Reconcile.** The PR service asks each GitHub-bound repository PR's adapter to
   `observe_merge`, which records merged-at-pinned-head or merged-outside-G5.
2. **Admission.** The PR service evaluates approvals on the full recorded vector, the
   external-checks policy (the adapters' `checks`; local ones need none) and any override.
   It records `merge_admission` before any provider effect.
3. **Provider merges.** Since #697 the GitHub side first durably reserves each repository
   and target remote branch
   ([target serialization](../../ontology/software_architecture/git.g5.md#g5.git.provider-merge-target-serialization)).
   For each GitHub-bound repository PR, the adapter's `merge_state` must be ready. If any reports blocked (without the GitHub override) or not ready, the
   whole merge refuses before any effect. The adapters merge override-bypass repositories
   first, then the rest, each at its pinned head. The adapter picks the acting identity:
   the App, or the person for a GitHub override.
4. **Import and cut.** The PR service hands the accepted provider tips and the local
   members' plans to workspace merge, which applies one cut under the commit fences and
   re-reads the admission.
5. **Land.** The G5 PR moves to merged. Its local repository PRs read merged with it.

Partial provider merges recover as today: the merged rows persist, admission is reused, and
`merge_state` reports already merged. Merge when ready calls the same operation, and its
"provider heads unchanged for two minutes" check reads the GitHub adapters' heads.

### Stored state

`workspace_pull_request_provider_repos` stays the GitHub adapter's record. No migration is
needed for it.

- **Kept as facts:** link identity (repository, number, node id, URL), the pinned published
  source head, the expected target, merged evidence and `merged_outside_g5`.
- **Kept as observations:** `provider_status`, `provider_head_sha` and `last_synced_at` stay
  the last observation. Lifecycle refusals and listings use them, and a listing that shows
  them says so, as `g5d pr view`'s "(stored state)" heading already does. A single PR's
  provider content is read live, as it already is.
- **Local:** local repository PRs add no table.

## Surfaces

`g5d pr` and the UI call PR-service operations. Neither chooses between GitHub and stored
paths.

| Command | Acts on | Change |
|---|---|---|
| `view`, `view <G5 id>` | G5 PR, with its repository PRs | each row's status from its adapter; local rows instead of absent ones |
| `view N -R repo`, `view <URL>` | repository PR | a local repository's number resolves through the local adapter (needs `g5.git.pr-local-number`) |
| `view -c`, `comment`, `resolve` | repository PR | local: refused with the alternative, under `g5.git.pr-local-conversation` |
| `checks` | G5 PR → each repository PR | local members reported as "no external checks apply" instead of dropped |
| `list` (workspace) | G5 PRs | unchanged |
| `list -R …` | repository PRs | GitHub's rows plus the repository's local rows, merged newest first, each naming its tracker; `list_stored` becomes the local adapter and the no-integration fallback |
| `create`, `update`, `edit`, `close`, `merge` | G5 PR | unchanged; a repository-PR selector maps to its G5 PR |

**UI.** The landing's repository rows show each changed member's repository PR with its
adapter's status and link. A local row links to the G5 PR; it will link to its own URL once
#838 adds G5 PR URLs. An unchanged member says so. Prepare to Ship's checks section takes
the same per-repository checks, so a mixed PR's local member shows "no external checks
apply". The PR list and Home signals are unchanged.

**Capacity.** There are no new GitHub calls. A single view reads each GitHub-bound
repository PR once, as now. Combined listing adds one tenant query, bounded by the existing
100-row cap.

## Rejected alternatives

- **Per-repository PRs with no group** (the original proposal taken alone). This breaks
  `g5.git.multi-repo-merge`, approvals pinned to the full vector, one admission per merge,
  and one merge-when-ready decision and override record per change. Restoring them
  reintroduces the group, so the group is kept from the start.
- **A stored local repository PR record.** No guarantee needs it while local PRs carry no
  conversation. The projection already exists in `list_stored`. A store becomes necessary
  only if `g5.git.pr-local-conversation` is answered yes.
- **Mirroring with surfaces left as they are.** This keeps the per-surface GitHub-or-stored
  branching that #830 Parts 3–5 kept patching, and leaves local members without identity.

## Implementation

Built on the branch of PR #854, after rebasing onto main (which added #697's provider
merge-target serialization; compatible with step 3 above).

- **Binding and repository PRs.** `services/git/pull_request_adapters.rs` owns the binding:
  members come from the recorded heads, a member changes as publication decides
  (`changes_member`, shared with publication's `repo_needs_provider_pr` and the SQL
  `CHANGES_REPOSITORY`), and the GitHub publication rule (`github_publication_target`) is
  the one the lifecycle's publication uses. Every G5 PR summary carries
  `repositoryPullRequests`, attached by the summary loaders in
  `coordinated_repo_publication.rs`, so the UI and `g5d` get the same rows.
- **Numbering.** Migration `145_pull_request_numbers.sql` adds `number`, backfills it per
  organization in creation order, and allocates new numbers in a trigger under a
  per-organization transaction lock (unique per organization). A trigger rather than
  application code keeps every insert, fixtures included, numbered.
- **Reads by number.** `read_repository_pull_request` and `local_selection`
  (`workspace_pull_request_read.rs`) pick the repository with the listing's selection
  rule (GitHub preferred for a bare number) and answer a local one with its G5 PR. The PR
  info route returns that as `source: g5` plus the repository. `checks` reports local rows
  as ready with no checks (`noExternalChecks`), so `external_checks_verdict` and `g5d` read
  them as passing.
- **Listing.** `stored_rows` is the local adapter's listing; a GitHub repository's listing
  merges its local rows (no GitHub record, and the two workspaces do not both track the
  repository remotely) newest first under the same cap. The no-integration fallback is
  unchanged.
- **Conversation.** `conversation_target` refuses a local repository before any effect. The
  GitHub comment-receipt and thread-resolution protocol moved from the route module into
  the core conversation module; `conversation_client` is private to it, so no route
  obtains a GitHub client for PRs.
- **Deletion and visibility.** `close_deleted_workspace_pull_requests` runs in the
  tombstone transaction; PR creation holds both live workspace rows `FOR SHARE`, so no PR
  opens beside a deletion. The summary queries no longer require live workspaces and
  report `sourceWorkspaceDeleted`/`targetWorkspaceDeleted`; `pr_audience` applies the
  audience rule to a deleted workspace's retained visibility, owner and invitations.
  Migration 145 closes PRs left open by earlier deletions.
- **Surfaces.** `g5d pr view` prints the G5 number, local rows and deleted sides, `--json`
  adds `g5Number`, `repositoryPullRequests` and the deleted flags, and a branch or number
  resolving to a local row maps to its G5 PR. The landing shows a local row's status and
  G5 number and marks unchanged members; Prepare to Ship lists a local row's "no external
  checks apply".

### Follow-up: tightening the adapter boundary (2026-10-30)

An audit the owner asked for found surfaces and services reading the GitHub adapter's records
directly. At the owner's request (fixes 1, 2, 3 and 5 of the audit):
- Each GitHub repository pull request carries the adapter's record (`github`) and the head
  and base branch names. The UI (`githubPublications`) and `g5d` (`github_publications`) read
  GitHub publication facts only from there; the raw `providerRepos` stays in the API for
  older `g5d` binaries.
- Activity's labels and remote tracking's import hold call PR-service functions in
  `pull_request_adapters.rs` instead of their own SQL.
- `checks` iterates the repository pull requests; the listing uses the binding's
  repository-level rule and a shared per-PR SQL rule, so it cannot drift from the binding.
- The binding no longer depends on the PR's status: a never-published GitHub member of an
  ended PR stays GitHub's (shown with the PR's end), instead of reading as local.

Not done (fix 4): a compile-time boundary around the GitHub client.

## Testing

System tests through the real routes, workers and Postgres, with the existing GitHub double
(its assumptions as today):

- **Mixed PR (code on GitHub, docs local).** Run create, view, `checks` (exit 0 when
  GitHub passes, docs row says no external checks apply), `list -R` for each repository,
  merge (the target sees both members or neither), and a forced partial provider merge
  that then recovers.
- **Make Remote on the local member mid-PR.** The next refresh creates its GitHub PR, the
  local row ends, and the merge goes provider-first for both.
- **Connection lost on a GitHub-bound member.** It stays an error with its recovery step and
  never reads as local.
- **Local-only organization.** No GitHub wording anywhere (`g5.git.pr-no-github`
  regressions).
- **Selectors.** A local number with and without `-R`, ambiguity refusals, and write verbs
  mapping to the G5 PR.
- **Ownership probe.** A compile-time check that route modules cannot name the GitHub
  adapter's client.
- **Regressions.** Existing `api_pull_request_*`, auto-merge, override and landing tests.

## Discrepancies found while designing (not part of this change)

- The Rust code checks `provider_status == "closed"`
  (`workspace_pull_request_provider_lifecycle.rs` near lines 739, 1222, 1316 and 1976), but
  the table's CHECK constraint does not allow `closed`, so that state can never be stored.
  Those checks are dead code. A PR closed on GitHub is noticed only by the live read near
  line 1325, and G5 never records it. The GitHub adapter's `observe_merge` should report
  closed-on-GitHub explicitly.
- `g5.architecture.scm_provider_client` describes clients "behind ScmProvider". In code,
  `ScmProvider` is an origin enum, and PR calls go to the concrete `RepositoryClient`.
- Write commands map a GitHub PR only to *open* G5 PRs (`pr_write.rs`), which
  `g5.git.pr-cli.selectors` does not state.

## Decided questions

- [g5.git.pr-local-number](../../ontology/software_architecture/git.g5.md#g5.git.pr-local-number):
  an organization-wide G5 PR number that local repository PRs share.
- [g5.git.pr-local-conversation](../../ontology/software_architecture/git.g5.md#g5.git.pr-local-conversation):
  no conversation on local repository PRs for now.

## Assumptions

- A repository PR's lifecycle is its G5 PR's. G5 does not close or merge one repository PR
  on its own. A GitHub PR closed or merged on GitHub is handled by the existing reconcile
  and merged-outside paths.
- Local repository PRs exist only inside G5 PRs. PRs opened directly on GitHub stay
  readable through the GitHub adapter without a G5 PR, as today.
- Stacks (#818) and G5 PR URLs (#838) build on this design and are not part of it.
- Local repository PRs, being projections of retained G5 PR records, stay readable after
  their workspaces are deleted, under
  [g5.git.pr-reads](../../ontology/software_architecture/git.g5.md#g5.git.pr-reads) and
  [g5.workspace.delete.pull-requests](../../ontology/features/workspaces.g5.md#g5.workspace.delete.pull-requests),
  the same as GitHub-bound ones.  Neither adapter needs anything extra for this.
