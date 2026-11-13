# `g5d issue edit` and agent writes on any readable issue

Intent: [G5-Lean#808](https://github.com/G5Labs/G5-Lean/issues/808) and the
[chat intent](../../chat_intent/issue-808-g5d-issue-edit/request.md) of 2026-10-29. Listeners
are out of scope (the assignee's comment); comment editing stays a recorded limitation.

## Ontology changes

| Block | Change | Summary |
|---|---|---|
| [`g5.external-issues.edit`](../../ontology/features/issues.g5.md#g5.external-issues.edit) | New | The agent edits title, description and assignees as its owner: admission, one write, verified outcome, kept marker. |
| [`g5.external-issues.transition`](../../ontology/features/issues.g5.md#g5.external-issues.transition) | Amended | The agent's close/reopen reaches any issue `view` can read, not only linked ones. |
| [`g5.external-issues.cli`](../../ontology/features/issues.g5.md#g5.external-issues.cli) | Amended | `edit` with gh flags; one selection rule for `view`, `comment`, `edit`, `close`, `reopen`. |
| [`g5.issue.native.writes`](../../ontology/features/issues.g5.md#g5.issue.native.writes) | Amended | Drops the stale "agent gets no state change or edit": the agent closes, reopens and edits as its owner. |
| [`g5.issue.native.page`](../../ontology/features/issues.g5.md#g5.issue.native.page), [`.adapter`](../../ontology/features/issues.g5.md#g5.issue.native.adapter) | Amended | Agent edits go through `g5d issue edit`; issue edit is an adapter content operation. |

## Governing ontology

- [`g5.issue.comment-authority`](../../ontology/features/issues.g5.md#g5.issue.comment-authority)
  and [`g5.external-issues.creation.admission`](../../ontology/features/issues.g5.md#g5.external-issues.creation.admission):
  the GitHub write guards an edit reuses (switch, domain comments, non-public now, the owner's
  connected account, secret screen).
- [`g5.external-issues.cli.reads`](../../ontology/features/issues.g5.md#g5.external-issues.cli.reads):
  what "any issue `view` can read" means — the configured domains only, while the owner is a member.
- [`g5.issue.native.writes`](../../ontology/features/issues.g5.md#g5.issue.native.writes) and
  `.page`: native edits are conditional updates that append one event per assignee change.
- [`g5.external-issues.creation.recovery`](../../ontology/features/issues.g5.md#g5.external-issues.creation.recovery):
  the creation marker a later description edit must not drop.

## Design

```text
g5d issue edit [REF] [-R] [-t] [-b|-F] [--add-assignee L,..] [--remove-assignee L,..]
  └─ POST /api/internal/workspace-agent/:id/issue-edit      (audited like issue-state)
       own workspace, not a review copy, external issues on
       choose_agent_issue (shared with view/comment): link first, else any readable issue
       ├─ G5 issue:  one transaction: read row → resolve people → edit_issue(expected = row)
       └─ GitHub:    admit_person_write(Edit) → secret screen → check added logins assignable
                     → read issue → one PATCH {title?, body(+marker)?, assignees?}
                     → compare GitHub's assignees with the requested set
g5d issue close|reopen [REF] → issue-state, now resolved by choose_agent_issue too
```

**Owners.** The issue service keeps selection and admission; the provider adapters own the
write. `choose_agent_issue` moves from `internal.rs` to the shared issue routes so the agent's
reads, comments, edits and transitions all name an issue one way. The state change splits
`change_linked_issue_state_as` into link lookup (the person's route, unchanged: linked issues
from their workspace) and a target-based write that the agent route calls after selection.

**No edit ledger.** An edit sets values, and adding or removing an assignee is idempotent, so,
like the transition, no edit key or publication record is needed; a rerun after an unknown
outcome recomputes from a fresh read. Native assignee changes still append their normal
per-change events.

**One provider write per edit.** GitHub's issue `PATCH` carries title, body and the full
assignee set at once, so a request either applies or is refused as a whole, and the outcome
is one of: `applied`, refused (GitHub's error, nothing changed), `unknown` (exit 7, rerun) or
`partial` (GitHub applied the write but its answer lacks a requested assignee change, exit 1).
The three delta endpoints (`POST`/`DELETE …/assignees`, `PATCH`) were rejected: they avoid the
assignee read-modify-write but turn every combined edit into a multi-step partial outcome.

**Clear refusals before writing.** Unknown G5 people, a person both added and removed, an empty
edit, a secret in title or body, and GitHub logins that cannot be assigned
(`GET …/assignees/{login}` is 404) are refused before any write. GitHub may still drop an
assignee the owner may not set; the response check reports it as `partial`.

**Marker kept.** A description edit on GitHub re-appends a `g5-issue` creation marker the
current description carries, as comment edits keep their reference line, so a retried creation
with the same key still finds the issue.

## Assumptions and accepted limits

- GitHub's `PATCH` is last-writer-wins and G5 cannot make it conditional: an assignee change
  made between G5's read and its write (well under a second) can be lost, and a person's
  concurrent title or description edit is overwritten, as with `gh issue edit`.
- A native edit applies to the version read in the same transaction, not one the agent viewed
  earlier; a concurrent change between the two is refused as `issue_changed`, and a rerun applies.
- People get no new GitHub edit UI here; the native issue page keeps its own edit form.
- Agent edits and state changes are attributed to the owner; the agent's action is visible in
  the `workspace_agent.action` audit event, as for close/reopen. Native events carry no origin.

## Increment 2: GitHub issues edited on their G5 page; stateless issue comments

Intent: the owner's [later instructions](../../chat_intent/issue-808-g5d-issue-edit/request.md#later-instruction-2026-10-29-external-issues-edited-in-the-platform).
Follow-up for PR comments: [G5-Lean#950](https://github.com/G5Labs/G5-Lean/issues/950).

### Ontology changes

| Block | Change | Summary |
|---|---|---|
| `g5.issue.native.page` | Amended | The issue page offers every adapter's write operations: comment, own-comment edit, close/reopen, title/description/assignee edit, for G5 and GitHub issues alike. |
| `g5.external-issues.edit` | Amended | People edit from the issue page too; their edit is refused when a field they change differs from what they saw. |
| `g5.external-issues.transition` | Amended | A person closes or reopens from the issue page as well as from a linked workspace. |
| `g5.issue.comment-recovery` | Amended | Issue comments are reconciled by a key-derived marker, as creation is; no record is kept. PR comments keep records until #950. |
| `g5.issue.comment-entry`, `.adapter`, `.creation.recovery` | Amended | Comments from the issue page; "publication records" leave the issue adapter's guard list. |

### Stateless issue comments

The comment marker becomes `<!-- g5-comment <id> -->` with `<id>` a UUID-shaped digest of
organization, author and idempotency key (the agent's keys keep their `agent:` prefix), so
`existing_marker` and comment-edit marker keeping are unchanged. Posting a comment, for a person
or the agent, from a workspace or the issue page:

```text
validate body → resolve target → look for the marker among the issue's comments of the last
14 days (the composer's draft retention) (installation token, read) → found: same body → return it (200); other body → refuse
`idempotency_key_reused` → not found: admit (comment guards) → post → answer: published (201) or
outcome_unknown (200, rerun with the same key: it looks again before posting)
```

The lookup runs before the guards, so a replay still answers after the switch, visibility or
connection changed, as `g5.issue.comment-recovery` already requires. A refusal is an HTTP error;
there is no `failed` state or pending receipt. Comment edits are idempotent text replacement with
the existing observed-version check, recorded nowhere. No migration: existing `issue`
destination rows are left unread until #950 drops the table (the owner's decision of
2026-10-30). Removed: the
`reconcile` and `discard` routes, `myPendingComments`, and the Activity receipts.

Accepted cost: one GitHub read per comment; the rare duplicate when GitHub has not yet shown a
just-posted comment (as for creation); an unclear outcome is no longer listed across sessions.

### Issue page writes for GitHub issues

Routes under `/api/v1/orgs/:orgId/issues/github/:repositoryId/:number`, for members with write
permission (viewers read), each calling the operations the agent uses with no workspace:

| Route | Request | Response |
|---|---|---|
| `GET …/write-target` | — | `{state, githubLogin, contentTrusted, displayKey, destination, assignableUsers, error}`; `state` as the comment composer's: `ready`, `connection_required`, `comments_disabled`, `public_destination`, `visibility_unknown`, `unavailable` |
| `PATCH …` | `{title?, body?, assignees?, seen: {title?, body?, assignees?}}`; `assignees` is the complete new set of logins; `seen` holds what the form showed for each changed field | `IssueEditDTO`; 409 `issue_changed` when GitHub's current value of a changed field differs from `seen` |
| `POST …/state` | `{state: "closed" \| "open"}` | `IssueStateChangeDTO` |
| `POST …/comments` | `{body, idempotencyKey}` | GitHub comment result, below |
| `PATCH …/comments/:commentId` | `{body, expectedUpdatedAt}` | GitHub comment result; 409 `comment_changed`, `not_your_comment` |

GitHub comment result (the `CommentResultDTO` GitHub variant, for every issue comment route):
`{id, provider: "github", state: "published" | "outcome_unknown", kind: "comment" | "edit",
origin, destination: "issue", displayKey, githubLogin, body, payload, externalCommentUrl,
error: null, createdAt, publishedAt, message}`.

**Stale check.** GitHub bumps an issue's `updated_at` on every comment, so a version check would
refuse most edits of a busy issue. The form sends what it showed for the fields it changes; G5
rereads and compares those fields (assignees as a case-insensitive set, the description without
G5's creation marker). The window left is between that read and GitHub's write.

**UI.** One set of issue-page controls for both trackers (Edit form with an assignee picker from
the write target, Close/Reopen, comment composer, edit own comment). Own GitHub comments are
those whose author login is the viewer's connected login; the server checks by account id.
Untrusted domains keep the review step showing exactly what is sent. The workspace tab of a GitHub
issue stays read-only with a link to its G5 page. The Activity composer keeps an unconfirmed
comment's draft and key so posting again is safe.

## Verification

- Native: `api_native_issues.rs` `the_workspace_agent_edits_closes_and_reopens_any_g5_issue_as_its_owner`
  — title, description and assignees (email, `@me`, bare number) on an unlinked issue; a rerun
  records nothing; unknown person, add-and-remove, empty edit and a missing issue refused with
  nothing changed; close/reopen of an unlinked issue; a review copy's agent refused.
- GitHub (mock): `api_external_issues.rs` `the_workspace_agent_edits_and_closes_an_unlinked_github_issue_as_its_owner`
  — comments switch and public repository refused; secret, unassignable login and
  add-and-remove refused with no `PATCH`; one `PATCH` with the owner's token, kept marker and
  computed assignee set; a dropped assignee reported `partial`; a 502 reported `unknown`; close of
  an unlinked issue. The person's transition test still sees the review-copy refusal first.
- `workspace_agent_parity.rs` `issue_writes_without_a_reference_need_a_primary_issue`; `g5-scm`
  `issue_edit_sends_only_the_edited_fields_and_checks_assignees`; `issue_creations`
  `edits_name_a_change_and_keep_the_creation_marker`; `g5d` parsing, gh-flag guidance and
  rendering tests.
- Not exercised: live GitHub, and GitHub silently dropping an assignee for an account without
  push access beyond the mocked answer.

### Increment 2 verification

- API (GitHub mock): `a_person_writes_a_github_issue_from_its_page_as_themselves` — write
  target before and after connecting, stale `seen` refused with nothing sent, a missing `seen`
  value refused, one `PATCH` with the kept creation marker and the new assignee set, close of an
  unlinked issue, comment then replay found by marker, own-comment edit, viewer refused, a
  repository outside the domains refused.
- Stateless comments: `an_unclear_comment_is_found_by_its_marker_when_posted_again` (unclear
  send, nothing listed, a busy issue's second page found on retry, another author's same key is a
  new submission), `comments_post_as_the_person_only_to_approved_non_public_destinations`,
  `edits_replace_only_the_person_s_own_unchanged_comment`, the agent comment tests, and the
  `sec_h02_*_in_flight` tests (no lock or transaction held while GitHub answers); unit
  `keyed_markers_name_one_submission_and_keep_the_marker_shape`.
- Frontend: `IssuePage.test.tsx` (GitHub writes, native page, viewers, blocked states),
  `ExternalIssueDetailPanel.test.tsx` (workspace-tab link), `ActivityCommentComposer.test.tsx`
  and `ActivityCommentEditor.test.tsx` (draft and key kept on `outcome_unknown`).
- Static: `route_auth_static` with the manifest entries for the new handlers.
- Not exercised: a browser against the running stack, and live GitHub.
