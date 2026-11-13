# Publish workflow-file changes through the GitHub App

Design for [G5-Lean#612](https://github.com/G5Labs/G5-Lean/issues/612)
([request](../../chat_intent/issue-612-fix-workflow-edit-publication-github-app-tokens/request.md)).
Baseline `02d4cb76`.

## What happens today

- `RepositoryCredentials` maps `WriteGit`, `AutoPush` and `MergePullRequest` to
  `GithubAppCapability::ContentsWrite`, and `mint_repository_token` narrows the
  installation token to exactly `{"contents": "write"}`. GitHub refuses any push whose
  new commits create or update `.github/workflows/*` with
  `[remote rejected] (refusing to allow a GitHub App to create or update workflow … without workflows permission)`.
  Granting the permission on GitHub cannot help: G5 never asks for it.
- g5-scm classifies that refusal as a generic policy refusal; `From<NativeGitError>`
  makes every push failure `G5Error::GitError`, which is HTTP 500 `INTERNAL_ERROR`
  with the message sanitized away. PR creation, Publish, Make Remote, tracked-source
  push and remote forwarding all push through `GitRepo::push_commit_native` and share
  this mapping.
- `create_or_update` logs "Workspace endpoint succeeded: create_workspace_pull_request"
  after inserting the PR row but before source publication and the commit that make it
  durable.

## Ontology change

[`g5.git.workflow-files`](../../ontology/software_architecture/git.g5.md#g5.git.workflow-files)
(new). Governing blocks:

| Block | Implementation must |
| --- | --- |
| [`g5.git.protections`](../../ontology/software_architecture/git.g5.md#g5.git.protections) | Never bypass GitHub's refusal; the installation grant decides |
| [`g5.git.authorization-loss`](../../ontology/software_architecture/git.g5.md#g5.git.authorization-loss) | Same installation or same personal credential; no fallback identity on refusal |
| [`g5.architecture.repository_credentials`](../../ontology/software_architecture/git.g5.md#g5.architecture.repository_credentials) | Permission mapping, minting and caching stay inside the credential service |
| [`g5.git.pr-save`](../../ontology/software_architecture/git.g5.md#g5.git.pr-save), [`g5.git.save-publication`](../../ontology/software_architecture/git.g5.md#g5.git.save-publication) | Refused source publication saves no new G5 PR; post-save failures stay reported as saved-but-incomplete |
| [`g5.governance.pr.follow-push`](../../ontology/software_architecture/git.g5.md#g5.governance.pr.follow-push) | Reasons are actionable; internal details stay in logs |

## Design

### Token permissions follow the recorded grant (credential owner)

`ContentsWrite` keeps its required `contents: write` and gains one *optional*
permission, `workflows: write`. Selection (`classify_selection`) resolves the exact
token permission set from the binding snapshot: required permissions, plus each
optional permission the snapshot grants. `GithubAppRepositorySelection` carries that
set and `mint_repository_token` requests exactly it for the single repository.

- Without the grant the request is unchanged (`{"contents":"write"}`), so ordinary
  pushes keep working and existing installations see no change.
- Eligibility is unchanged: the optional permission never makes a selection ineligible.
- The installation-token cache is already keyed by `(installation, repository_ids,
  permissions)`. A refresh that records a new grant changes the request, so the next
  write mints a new token; tokens without the grant are never reused for it, and a
  removed grant stops requesting it. No new invalidation is needed.
- G5 observes a grant only through the binding snapshot. After an App owner adds the
  permission and the installation owner approves it, an administrator refreshes the
  binding (`POST /orgs/:id/github-app/binding/refresh`, or any coverage change).
- Merge shares the capability, so provider merges of workflow-changing PRs also work
  when granted. PR reads/writes and issue capabilities are unchanged.

Rejected: inspecting each push's new commits for `.github/workflows/` paths and minting
a wider token only then, or retrying with a wider token after a refusal. The token is
server-held, single-repository and used for one push of an already-authorized head, so
per-push escalation narrows nothing that matters, while it would duplicate GitHub's
path policy in every push path (create-only, tracked, forwarding, merge) and add a
retry protocol to each caller.

Known limit: if the recorded snapshot claims a grant GitHub has since withdrawn,
minting fails for every write until the binding is refreshed — the existing behavior
for any stale snapshot permission.

### Refusals become typed publication errors (Git execution owner)

`g5-scm`'s push classifier recognizes GitHub's workflow refusals as a
`FailureKind::ProviderPermission` (`Refused` effect):

- GitHub App: "refusing to allow a GitHub App to create or update workflow … without
  `workflows` permission" (installation and App user tokens).
- OAuth/personal: "refusing to allow an OAuth App … without `workflow` scope".

`From<NativeGitError>` maps it to `G5Error::ConflictDetails` (HTTP 409, code
`CONFLICT`) whose reason names the workflow file, the missing permission, that nothing
was published, and the recovery; details carry
`{"code":"PROVIDER_PERMISSION_REQUIRED","provider":"github","permission":…,"credential":…,"path":…}`.
Every push path inherits it. On PR create/update the error still passes through
`source_save_failed`, so `details.kind = pull_request_source_save_failed`, the PR
transaction rolls back, and `g5d pr create|update` exits with the Git error status and
prints the reason. The forwarding worker records the same reason in its push-failed
audit.

### Success log

The "succeeded" log moves after the PR transaction commits and says the PR was saved
after source publication; provider publication is reported separately afterwards.

## Verification

The fixture remote models GitHub's rule with a `proc-receive` hook
(`g5_scm::native::emulate_github_workflow_permission_for_tests`): it refuses new
commits under `.github/workflows/` with GitHub's wording unless the newest minted
contents-write installation token carried `workflows: write`.

- g5-scm `workflow_permission_refusal_is_typed_actionable_and_changes_nothing` and the
  classifier cases: typed refused error for fast-forward and lease pushes, ref
  unchanged, sanitized 409 with details; succeeds once the token is granted.
- g5-api-core binding units: permission resolution with and without the grant, minted
  body, eligibility unaffected; other capabilities never request `workflows`.
- `api_workspace_pull_requests::create_pr_publishes_workflow_changes_only_under_a_recorded_app_grant`
  (UI create, create-only `g5/feature` publication): two refused attempts reuse one
  contents-only token and save no PR; a GitHub-side grant alone changes nothing; refresh
  records it; a competing remote ref is refused and kept; after a source-head change the
  retry publishes exactly the new head under one new contents+workflows token and saves
  one PR; a further retry reuses it.
- `remote_tracking_background_loop::automatic_forwarding_publishes_workflow_changes_only_under_a_recorded_grant`:
  ordinary forwarding with contents write only; the workflow commit records an
  actionable `push_failed` and leaves the provider tip; after refresh it is pushed.
- Both API tests fail with the optional permission removed (refused at the post-grant
  step). g5d `pr_commands`: the refusal exits 4 and prints the reason.

Remaining gaps:

- QA: the App registration must request Workflows (read and write), the installation
  owner must approve it, and an administrator must choose Refresh from GitHub; then
  validate UI and `g5d pr create` on a workflow-changing branch. Not doable from the
  VM; the deployed revision and current installation grant are unverified.
- Merge: covered by the same grant; GitHub's merge-API refusal text for a missing
  Workflows permission is not classified and keeps the existing merge failure path.
- A snapshot that still records a grant GitHub withdrew fails every App write until
  refreshed (existing stale-snapshot behavior).
