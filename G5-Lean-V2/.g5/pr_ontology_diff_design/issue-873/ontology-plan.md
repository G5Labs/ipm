# Merge with override: G5 requirements (#873) and GitHub rules (#550)

Requests (both written by the owner):

- [#873](https://github.com/G5Labs/G5-Lean/issues/873): an org-admin override of
  G5's own merge requirements.
- [#550](https://github.com/G5Labs/G5-Lean/issues/550): stop G5 bypassing GitHub's
  rules silently, and add an explicit GitHub override under the person's own
  identity. It is linked to this workspace as related and was included at the
  owner's request (2026-10-27).

#550's two decisions are settled by the two issues together:

- **Who may override G5's failing-checks policy:** org administrators, through
  #873's override.
- **Replace Merge or sit beside it:** beside it, as both issues say.

## Blocks

Governing blocks (all in this diff):

- **The G5 override:**
  [`g5.workspace.merge.admin-override`](../../ontology/features/workspaces.g5.md#g5.workspace.merge.admin-override).
  Its [formal child](../../ontology/features/workspaces.g5.md#g5.workspace.merge.admin-override.formal)
  owns what both overrides share: the shown set, the re-check at execution, the
  record and the flagging. Also new: the story
  [`g5.story.merge-with-override`](../../ontology/features/workspaces.g5.md#g5.story.merge-with-override)
  and three Q blocks.
- **GitHub (#550):**
  - [`g5.git.provider-rules`](../../ontology/software_architecture/git.g5.md#g5.git.provider-rules):
    no merge bypasses GitHub's rules silently;
  - [`g5.git.provider-override`](../../ontology/software_architecture/git.g5.md#g5.git.provider-override):
    the GitHub override, with its
    [CLI question](../../ontology/software_architecture/git.g5.md#g5.git.provider-override.cli-question);
  - [`g5.git.provider-merged-outside`](../../ontology/software_architecture/git.g5.md#g5.git.provider-merged-outside):
    merges done on GitHub.
- **Browser-only:**
  [`g5.security.browser-session-actions`](../../ontology/concepts/security.g5.md#g5.security.browser-session-actions).
- **Restored:**
  [`g5.governance.pr.base-admin`](../../ontology/features/workspaces.g5.md#g5.governance.pr.base-admin),
  which the code already cites. Its old claim that the merge runs under the
  administrator's own provider authority is not restored: in App-bound orgs, ordinary
  merges call GitHub as the App, and only the GitHub override uses the person's identity.
- **Amended:**
  - approval [requirements](../../ontology/features/reviews.g5.md#g5.governance.approval.requirements)
    and [request status](../../ontology/features/reviews.g5.md#g5.governance.approval.request-status);
  - the [external-checks policy](../../ontology/software_architecture/git.g5.md#g5.git.external-check-policy);
  - [merge when ready](../../ontology/software_architecture/git.g5.md#g5.git.pr-auto-merge);
  - the [announcement](../../ontology/software_architecture/git.g5.md#g5.git.pr-merge-announcement);
  - the [recovery cut](../../ontology/software_architecture/git.g5.md#g5.git.pr-recovery-cut);
  - [credential classes](../../ontology/concepts/security.g5.md#g5.security.credential-classes)
    and [human decisions](../../ontology/concepts/security.g5.md#g5.security.agent-human-exceptions);
  - [Ship to parent](../../ontology/features/ui.g5.md#g5.workbench.ship-to-parent).

## Two overrides, one confirmation

| | G5 override (#873) | GitHub override (#550) |
|---|---|---|
| Waives | unmet approval requirements; the org's external-checks policy | GitHub branch protection and rulesets, for the repositories GitHub reports `blocked` |
| Authority | effective org admin who may already merge the PR (provisional scope Q) | anyone who may merge the PR in G5; GitHub then decides on that person's bypass rights |
| Provider identity | the ordinary merge's (the App installation; the acting person without the App) | the person's connected GitHub identity, for the blocked repositories only; no fallback |
| Human-only | browser session only | browser session only (provisional CLI Q) |

When both apply, they share one dialog, one reason, one confirmation and one record.
The action is offered only to a person who holds the authority for every group of
blockers present. If a non-admin hits a G5 requirement, the dialog names it and says
it needs an administrator. G5's requirements are never waived by a GitHub override,
and GitHub's rules never by a G5 override.

## Decisions

The owner answered the question set `admin-merge-override` on 2026-10-27
([intent](../../chat_intent/issue-873/request.md)). The former Q blocks are now D
blocks and keep their IDs.

- **Scope:** only where the admin can already merge: Base, or a workspace they own.
- **Self-override:** allowed, flagged `selfOverride`.
- **Opt-out:** none in v1.
- **Confirmation:** a checkbox ("I understand the listed requirements and rules will
  be waived"), not typing the workspace name.
- **GitHub-blocked:** every merge refuses a blocked repository, enforced. The
  rollout check of QA's repositories stays.
- **GitHub override:** browser-only. `g5d pr merge` has no `--admin`.

Agent decisions kept:

- A push after the dialog opened refuses the override.
- Pre-existing sessions count as not browser sessions.
- An uncomputed GitHub state is re-read briefly, then refused for retry.

### One authority rule

At the owner's instruction, one pure policy function in `g5-api-core` decides who
may override:

```text
merge_override_authority(subject) -> { g5: Allowed | Denied(reason), github: Allowed | Denied(reason) }
```

`subject` holds:

- `can_merge`: the caller passed the ordinary merge admission (`target_db`) for this
  PR;
- `org_role`: the caller's effective organization role;
- `browser_session`: the result of the session guard.

These callers ask it and never test roles themselves:

- the merge route's `Override` admission;
- the preview's `authority` field;
- through `authority`, the UI's offer.

Changing who may override, for example adding a dedicated org permission, an opt-out
or a two-admin rule, edits this function and its unit tests. The only other place
that would need a change is a new input fact. The route-auth static test pins that
the override handlers reach it.

## Ownership

| Concern | Owner | Change |
|---|---|---|
| Who may override | [`merge_override_authority`](#one-authority-rule), consulted by `routes/workspace_merge_override.rs::authority_for` | The person merges as the ordinary `PullRequestMerger::Person`; `authority_for` establishes the facts (browser session, the `Person` merge admission, `org:admin`) and asks the policy. `merge_pull_request_as` takes the confirmed `MergeOverrideRequest` as an extra argument, `None` for every other caller. |
| Browser-only | auth middleware and the auth service | New guard `require_browser_session`. See [browser sessions](#browser-sessions). |
| Approval verdict | `ApprovalMergeAdmission` | Unchanged rules and locks. |
| Checks verdict | `ensure_external_checks_merge_policy` | Evaluates only repositories G5 has yet to merge at the provider (shared with #696). For an intent that waives G5 requirements, it returns the blocking snapshot instead of refusing. |
| GitHub merge state | provider lifecycle (`merge_provider_pull_requests`) with a new `g5-scm` read | Reads `mergeable_state` for every merge. See [GitHub rules](#github-rules). |
| GitHub merge identity | `RepositoryCredentials` | New operation `MergePullRequestWithBypass`, which is `user_authored`. Ordinary `MergePullRequest` stays on the App. |
| "Admitted with override" | `WorkspaceMergeService` admission (`admit_target_merge`, `admit_prepared_pull_request_merge`) | Takes an optional `OverrideIntent`. Under the requirement locks, it compares the current set with the shown digest, then records `admitted: true` with an `override` record or refuses. |
| Bypassed requests | approval request store | New terminal state `bypassed`, set in the landing transaction. |
| Announcement | `auto_merge::announce` | Override recipients and wording. |

There is no new lifecycle or table. An override exists only as part of one merge
admission, which already has the right identity (one PR, one cut), recovery and
retention. A separate override table was rejected: it would need its own link to
the admission and its own recovery rule, and an override never outlives the attempt
that made it.

## GitHub rules

**Every merge** (`Person`, `WorkspaceAgent`, `WhenReady`, `Override`):

1. Before the first provider merge, read each provider PR's `mergeable_state`.
   `g5-scm` already reads it for `g5d pr view` (`pulls.rs`).
2. `unknown` is re-read a few times over a few seconds.
3. A repository still `blocked`, or still `unknown`, refuses the whole merge before
   any provider merge. The refusal names the repository, GitHub's state and the
   required checks already read. Other states (`clean`, `unstable`, `has_hooks`,
   `behind`, `dirty`) go on to GitHub's own merge as today.
4. Merge when ready records the refusal as its waiting reason.

**With a GitHub override** in the intent:

- The blocked set must equal the shown one.
- The blocked repositories merge first, with `MergePullRequestWithBypass`, which
  uses the person's GitHub App user-to-server token, as `CreatePullRequest` does.
  Orgs without the App already merge with the acting person's token, so there the
  override differs from an ordinary merge only by proceeding past `blocked`.
- A refusal maps to "Your GitHub account can't bypass these rules", with no
  fallback, and stops before any other repository merges.
- The remaining repositories merge as ordinary.
- A missing GitHub connection refuses up front with the existing
  connect-GitHub recovery.

**Merges done on GitHub:** `reconcile_provider_acceptance` already records a
provider PR GitHub reports merged.

- It now also marks the repository `mergedOutsideG5` in the PR's provider record,
  unless G5's own accepted merge produced it.
- The checks policy skips merged repositories.
- Approvals still gate the local finish. An unmet requirement there is exactly the
  case the G5 override serves.
- #696 owns recovery of G5's own accepted merges, including the same policy skip.
  Whichever lands first implements the skip, and the other reuses it.

**Rollout risk.** Repositories whose G5 merges succeed today only because the App
is a bypass actor will start refusing. Their people then need either GitHub bypass
rights for the GitHub override or a change to the repository's rules. Before
enabling this on QA, list the repositories G5 merges into and check their current
`mergeable_state` on open PRs. G5-Lean's own `main` is one of them.

**Acceptance condition (unverified).** GitHub must honour a person's ruleset or
branch-protection bypass when the merge uses their App user-to-server token. Verify
it live before building the GitHub override, against a ruleset with the person as
bypass actor and the App not. If GitHub refuses, the GitHub override cannot work
for App-bound orgs, and the design goes back to the owner. `g5.git.provider-rules`
does not depend on this.

## Operations

**Preview:** `GET /api/v1/workspaces/:target/pull-requests/:pr/merge-override`.
It requires the browser guard and admits the caller as the merge does. It returns:

```text
{ sourceHeads,
  g5: { approvals: [{definitionId, name, version, state, detail}],
        checks: [{repository, headRefOid, availability, checks: [{name, bucket, required}]}] },
  github: [{repository, mergeState, requiredChecks}],
  notOverridable: [reason…], authority: {g5, github}, selfOverride, digest }
```

- `approvals` lists the requirements `evaluate_merge_admission` reports as not
  approved, evaluated lock-free like the PR summary.
- `checks` is present only while the org policy is on. It holds the rows
  `external_checks_verdict` treats as blocking, read with
  `read_checks(fresh_requirements: true)`.
- `github` lists the repositories reported `blocked`.
- `authority` says which groups the caller may waive.
- `digest` hashes the canonical `sourceHeads`, the approvals (id, version, state),
  the checks (repository, head, name, bucket) and the GitHub states (repository,
  head, state).

**Merge:** `POST …/merge-override` with `{reason, confirmed, digest}`.
It refuses before any effect when:

- the reason is empty or `confirmed` is not true: `400`;
- the browser guard fails: `403 BROWSER_SESSION_REQUIRED`;
- the caller lacks the authority for a present group: `403`.

Then it runs `merge_pull_request_as` as the person, with the override request:

1. `reconcile_provider_acceptance`, as today.
2. The checks policy reads checks fresh for the not-yet-merged repositories. A
   waived policy captures the snapshot instead of refusing.
3. Admission, under `lock_merge_admission`:
   - evaluate;
   - read the GitHub states;
   - compute the digest;
   - on a mismatch, refuse with `409 MERGE_OVERRIDE_CHANGED` and the current
     preview, recording nothing;
   - with nothing to waive, refuse with `409 MERGE_OVERRIDE_NOT_NEEDED`;
   - otherwise record `MergeAdmissionEvidence { admitted: true, override: Some(…) }`
     and append `merge_overridden` in the same transaction.
4. Provider merge as in [GitHub rules](#github-rules), then local application,
   landing and announcement, unchanged.

**No other caller can reach an override:**

- The workspace-agent router has no override route.
- `WhenReady` and Ship with agent never construct `Override`.
- `g5d pr merge` has no override flag (provisional CLI Q).
- A g5d person login is a CLI session.

## Record and recovery

`MergeAdmissionEvidence` moves to `schema_version` 2. It gains
`override: Option<MergeOverrideRecord>`, and version-1 evidence reads as `None`.
The record holds:

```text
{ actorId, actorName, at, reason, selfOverride,
  g5: {waivedApprovals, waivedChecks} | null,
  github: {repositories: [{repository, mergeState, requiredChecks}]} | null }
```

It is stored wherever admission already lives:

- `workspace_pull_requests.merge_admission`;
- `workspace_merge_runs.approval_admission`.

**The override cannot outlive its attempt.** A later attempt that the provider has
not accepted re-evaluates admission in `admit_pull_request_merge` and overwrites the
evidence.

**Recovery keeps it.** `admit_pull_request_merge` with `provider_accepted`, and
`require_admission_for_provider_merge`, reuse the recorded admission for the same
heads, override included. That is the
[recovery cut](../../ontology/software_architecture/git.g5.md#g5.git.pr-recovery-cut)
rule. If GitHub accepted only some repositories, whoever finishes recovery completes
the overridden cut and makes no new override:

- With a GitHub override, the blocked repositories went first, so what remains are
  ordinary merges.
- Refusing recovery by anyone else was rejected, because it would strand an
  irreversible partial merge.

**Landing.** When the recorded override waived G5 requirements,
`apply_workspace_pull_request_merge_consumer` closes the PR's pending
`approval_requests` as `bypassed`. This needs a migration extending the state CHECK
and the transition trigger. It also collects their recipients for the announcement.

## Surfaces

- **PR summary DTO:**
  - `mergeOverride` (the record's public fields) or null;
  - `mergedBy`, already written but not projected;
  - per repository, `mergedOutsideG5`.
- **g5d:** `pr_json.rs` adds `mergeOverride` and `mergedBy`. `g5d pr view` prints
  "Merged with override by X: reason", saying whether it was G5, GitHub or both.
- **Activity:** the source's timeline gains `merge_overridden` and `provider_merged_outside`. The target's timeline, which listed only its own outgoing pull requests, now also lists `merged` and `merge_overridden` for pull requests merging into it, named "from {source}" (`workspace_activity::TARGET_PULL_REQUEST_EVENT_TYPES`). Both were missing in the first implementation; the owner found no override in Activity on 2026-10-28. Also labels for `merge_overridden` (attempts too), for an overridden
  `merged`, and for "merged on GitHub outside G5". Migration: extend the event-type
  CHECK.
- **Announcement:**
  - Recipients: the source owner, the target owner and the bypassed reviewers,
    minus the actor.
  - The text names the override kind, the actor and the reason, and marks a
    self-override.
  - The agent platform notice carries `override`.
- **Approvals:** a bypassed request reads "Bypassed by override" and never counts
  as a decision.
- **UI:**
  - `PrepareToShipModal.tsx` and `PullRequestList.tsx` show a warning-styled
    "Merge with override…" beside Ship to parent / Merge, when only overridable
    blockers remain and the caller holds every group's authority.
  - The GitHub-blocked state becomes a named Ship blocker.
  - `MergeOverrideDialog.tsx` lists the G5 group and the GitHub group. It shows
    `notOverridable` and disables Confirm while any are present, and requires the
    reason and the confirmation checkbox.
  - On `MERGE_OVERRIDE_CHANGED` the dialog shows the new set, keeps the reason and
    clears the checkbox.
  - A merged pull request's Prepare to Ship note shows the actor and reason. The pull-request list shows only open pull requests, so it has no merged-with-override badge (removed after review).

## Browser sessions

Today a browser login (`handle_callback`) and a CLI device login
(`issue_user_tokens`) insert identical `refresh_tokens` rows and mint identical JWTs.

**Storage and writers:**

- Migration: `refresh_tokens.client_kind text NULL CHECK (client_kind IN ('browser','cli'))`.
  Existing rows stay NULL, which means unknown and is refused.
- `handle_callback` and the E2E `issue_test_token` write `browser`.
- The CLI device flow writes `cli`.
- Refresh rotation copies the kind.
- `mint_jwt` stamps an optional `client` claim (`#[serde(default)]`).

**Guard:** `require_browser_session(auth, headers)` refuses:

- `Host`;
- the API-key issuer;
- any `token_use`;
- `client != browser`;
- a `Sec-Fetch-Site` other than exactly one `same-origin` or `same-site`.

`Origin` is not consulted. The first version also required an `Origin` from the
configured origins. That refused every real browser: browsers omit `Origin` on a
same-origin GET, so the preview request never carried one, and G5's preview relay
presents another origin. The owner hit this when the override didn't appear in
the workspace preview (2026-10-28).

The tests now send only what the web app's fetch sends. The UI asks a person
whose preview is refused with `browser_session_required` to sign in again,
instead of showing nothing.

The limit is in the ontology: this attests how the session was issued and that a
browser sent the request, not that a person is present.

## Extending to later gates

A future G5 merge gate is overridable unless its block says otherwise. To be
overridable, it reports its blockers into the preview and the digest, and accepts
the intent at its decision point.

## Capacity

Every merge adds one `mergeable_state` read per provider repository, plus a few
re-reads while GitHub computes it. A preview and an override also read checks once
per repository. There is no background work. Merge when ready already reads checks
each pass, so the extra read is small at tens of armed PRs.

## Tests (required before shipping)

Integration (`backend/g5-api/tests`, the live-Postgres `integration` target, with
the GitHub fake).

**G5 override:**

- An org admin's browser session overrides an unmet human requirement into Base.
  The evidence, the event, the `bypassed` request, the notifications (not to the
  actor) and `mergeOverride` in the summary and `pr/info` are all checked.
- It is refused, with no state change, for:
  - a member;
  - another organization's admin;
  - an admin's all-scopes API key;
  - an admin's CLI session;
  - an unknown-kind session;
  - a request missing either browser header;
  - an armed `WhenReady` PR, which keeps waiting.
- **Changed set:** a rejection or a source push between preview and confirm gives
  `MERGE_OVERRIDE_CHANGED`. **Nothing to waive:** gives `NOT_NEEDED`.
- **Mechanical:** a conflict still blocks.
- **Self-override:** flagged.
- **Policy checks:** an org-policy failing check is overridden.

**GitHub rules:**

- An ordinary merge of a `blocked` repository refuses before any provider merge,
  whatever the merger. An `unknown` state that settles to `clean` merges.
- Merge when ready records the blocked reason.

**GitHub override:**

- The blocked repository merges first with the person's token, and the others
  with the App. The fake asserts which token each call used.
- A GitHub refusal stops before the others and keeps the event.
- A missing connection refuses up front.
- A member with merge authority can use it, but not when a G5 requirement is unmet.
- A combined override by an admin records both groups.

**Merges done on GitHub:**

- A repository merged on GitHub is marked `mergedOutsideG5`.
- Failing checks on it no longer block the finish.
- An unmet approval still blocks, and an admin override then finishes it.
- A partial provider acceptance followed by another merger's retry keeps the
  override record.

**Sessions:** browser and CLI logins stamp their kinds, and refresh preserves them.

**Static:** `route_auth_static` pins the browser guard and admission on the new
handlers.

**Frontend:**

- `MergeOverrideDialog.test.tsx`: both groups, the authority gating, the reason and
  checkbox, the changed-set re-render, and the not-overridable disable.
- `PrepareToShipModal.test.tsx`: when the button shows, and the GitHub-blocked
  blocker.
- Activity and request-status labels.

**Live, before building the GitHub override:** the user-to-server bypass check in
[GitHub rules](#github-rules).

## Not in scope

- An org opt-out or two-admin rule (provisional Q).
- GitHub's async merge endpoint with `bypass_rules`. It could later replace the
  person-token call.

## Implementation

- **Sessions:** migration `132_session_client_kind.sql`; `JwtClaims::client` and
  `SESSION_CLIENT_*` (`g5-auth`); `auth_service.rs` (`handle_callback`,
  `issue_user_tokens(client)`, `refresh_token`, `issue_test_token`);
  `middleware/auth_middleware.rs::require_browser_session`.
- **Authority, blockers and record:** `services/git/merge_override.rs`
  (`merge_override_authority`, `OverrideBlockers::digest`, `blocking_checks`,
  `apply_override`, `MergeOverrideRecord`). `MergeAdmissionEvidence` is schema
  version 2 with `merge_override`.
- **Admission:** `WorkspaceMergeService::admit_target_merge(…, merge_override)`
  applies the override under the requirement locks. `evaluate_merge_admission` is
  the preview's lock-free evaluation of the source's committed cut.
- **Events and landing:** `coordinated_repo_publication.rs`.
  `insert_override_event` writes `merge_overridden` where the admission is
  persisted (provider-first: `admit_pull_request_merge`; local:
  `apply_prepared_workspace_pull_request_merge`). The landing consumer closes
  pending requests as `bypassed`. The summary gains `mergedBy`, `mergeOverride`
  and `providerRepos[].mergedOutsideG5`. Migration `133_merge_override.sql` adds
  the request state, the event types and the column.
- **GitHub rules:** `workspace_pull_request_provider_lifecycle.rs`:
  - `settled_merge_state`;
  - the blocked/unknown refusal in `preflight_one_provider_pr_for_merge`;
  - `RepositoryOperation::MergePullRequestWithBypass` for a covered repository;
  - bypass plans first, stopping on their failure;
  - `blocked_provider_repositories`;
  - `mark_merged_outside`.

  `GithubPull::mergeable_state` lives in `g5-scm`.
- **Routes:**
  - `routes/workspace_merge_override.rs`: preview and merge, `authority_for`,
    `override_intent`;
  - `routes/workspaces.rs::policy_checks_snapshot`, shared with the ordinary
    policy and skipping repositories already merged;
  - `auto_merge::announce`: override wording, target owner and bypassed
    reviewers.
- **g5d:** `pr_json.rs` adds `mergedBy` and `mergeOverride`. `g5d pr view` prints
  the override.
- **UI:**
  - `MergeOverrideDialog.tsx`: the dialog and `MergeOverrideAction`;
  - `lib/mergeOverride.ts`: the offer, read only from the server's `authority`;
  - `PrepareToShipModal.tsx`, `PullRequestList.tsx`, Activity labels, the
    `bypassed` request state.

## Evidence

Integration tests (live Postgres, real routes):

- `workspace_merge_override.rs`:
  - the administrator's override merges, records, bypasses and notifies;
  - refusal with no effect for a reader, a writer who owns nothing, another
    organization's administrator, a CLI session, an unknown-kind session, missing
    browser headers, an admin API key and a VM credential;
  - a changed set (rejection, push) refuses with the current set;
  - nothing to waive refuses;
  - merge when ready never overrides.
- `api_workspace_pull_requests.rs`:
  - a GitHub-blocked repository refuses the ordinary merge before any provider
    merge, and the GitHub override then merges it;
  - a repository merged on GitHub is recorded as outside G5 and skipped by the
    checks policy.
- `api_auth.rs::sessions_record_whether_the_web_sign_in_or_cli_login_issued_them`.

Static: `route_auth_static.rs::merge_override_authority_is_decided_in_one_place` pins the single authority rule. Only the override routes call `merge_override_authority` or `require_browser_session`, and they run the browser guard, merge admission and `org:admin` before asking it. `route_handlers_use_expected_api_scopes` covers the preview (read) and the merge (write).

Regression run in the VM: `api_workspace_pull_requests`, `workspace_pull_request_auto_merge`, `workspace_merge_override`, `route_auth_static`, `workspace_agent_parity`, `api_workspace_approvals`, `api_credential_authority` and `api_org_admin_authority` (184 tests) pass, as do the `api_auth` CLI and session tests. The full integration suite was not run locally; CI runs it.

Unit tests: `merge_override::tests` (authority, digest, apply, blocking checks,
bypass coverage), the `require_browser_session` guard, and the `g5d` view.
Frontend: `MergeOverrideDialog.test.tsx`, `PrepareToShipModal.test.tsx`,
`PullRequestList.test.tsx`, Activity and approval-history tests.

Gaps:

- In an App-bound org, the GitHub override's person token (user-to-server) is
  not exercised against a fake that distinguishes tokens. The fixture is a
  personal-token org, where every merge already uses the person's token.
- GitHub honouring a person's bypass through an App user token is unverified
  against live GitHub (the acceptance condition above).
- No browser end-to-end run.
