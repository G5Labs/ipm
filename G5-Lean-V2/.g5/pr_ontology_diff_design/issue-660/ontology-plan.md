# Credential and mutation authority (issue #660)

Five credential and transactional-authority gaps share two root causes: a credential
or grant is checked at one moment and then discarded, or a bearer is stored in a form
that database reads can replay. The change keeps every repair inside the owner that
already holds the fact, adds no durable lifecycle, and preserves valid person-session
administration and existing sessions.

Request: [human intent and interpretation](../../chat_intent/issue-660/request.md).
Designed against main `015310db` on the authorized branch
`FVB-i660-F1-credential-and-mutation-authority`.

## Governing contracts

- [Credential classes](../../ontology/concepts/security.g5.md#g5.security.credential-classes)
  and [API-key authority](../../ontology/features/users.g5.md#g5.user.api-key-authority)
  own what a delegated key may acquire. Amended here: key management is a person
  operation.
- [Invitation authority](../../ontology/features/organizations.g5.md#g5.organization.invitation-authority)
  already requires acceptance and revocation to resolve against the same current
  invitation; the OAuth path did not implement it.
- [Current authority](../../ontology/concepts/security.g5.md#g5.security.current-authority)
  requires the authority relied on by a transactional mutation to be preserved until
  commit; [audience](../../ontology/features/workspaces.g5.md#g5.workspace.audience)
  defines the reader grant that reply, suggestion, round and vote writers rely on.
- [Viewers](../../ontology/features/organizations.g5.md#g5.organization.viewers) and the
  provisional answer to the
  [grant-lifecycle question](../../ontology/features/organizations.g5.md#g5.organization.grant-lifecycle-question)
  give the viewer ceiling. Added here:
  [viewer ceiling](../../ontology/features/organizations.g5.md#g5.organization.viewer-ceiling).
- [Session authority](../../ontology/features/users.g5.md#g5.user.session-authority) and
  [encryption](../../ontology/concepts/security.g5.md#g5.security.encryption) own the
  refresh credential. Amended here: it is stored only as a digest.

## 1. Delegated API keys cannot mint or revoke credentials

Owner: the API-key route is the credential-class boundary; `ApiKeyService` owns rows.

Decision: `POST`/`DELETE …/api-keys` require a person session
(`extract_session_user_id`), as the account routes already do. Listing keys stays a
read admitted by `org:admin`. A delegated key therefore acquires no credential, so no
child can outlive its parent's revocation or carry scopes the parent lacks, and the
existing organization, scope and active-membership checks are unchanged for people.

| Candidate | Guarantee or limit |
| --- | --- |
| Scope-subset check only | A child still survives the parent's revocation (the issue's reproduction). |
| Bounded children: record the issuing key, cap scopes and expiry, cascade revocation, verify the chain at every authentication | Closes the gap, but adds a durable key lineage, a recursive revocation and a chain walk on every request, for a capability no product flow uses; the contract does not require it. |
| Person-session management (chosen) | No credential hierarchy; the key's boundary is exactly "nothing new"; administrators keep the full surface. |

Already-issued keys created by a key cannot be identified: `api_keys` records the
attributed user, not the creating credential, and the audit stream records only the
auth kind of the request. They stay ordinary organization keys bound to their user's
current active membership, visible to administrators in the key list and revocable
there. The design records this disposition instead of inventing one.

Operation sketch: the route refuses with `403 user_session_required` before any
database work; the service keeps its active-membership check for the attributed user.

## 2. OAuth acceptance resolves against the current invitation

Owner: `OrgService` owns invitation transitions; `AuthService::handle_callback`
coordinates login and calls into it inside the login transaction.

Decision: move the auto-acceptance step into
`OrgService::accept_pending_invitations_on_conn(conn, user_id, verified_emails)`.
It selects the matching pending, unexpired invitations `FOR UPDATE`, then for each one
performs `UPDATE … SET status='accepted' WHERE id=$1 AND status='pending' AND
expires_at > now()` and inserts the membership only when that update reported a row.
Row locking makes a revoke (`DELETE … WHERE status='pending'`) or the expiry sweep
wait behind an in-flight acceptance and see `accepted`, and makes an acceptance that
waited behind a committed revoke find no row. The person-session
`accept_invitation` already locks this way; both now resolve against the same current
row. Implementation found one more sibling: `decline_invitation` updated by id alone
and could overwrite a concurrent acceptance, so it now declines only a still-pending
row and otherwise reports a conflict, like revoke and resend. Normal pending acceptance still succeeds exactly once (membership insert stays
`ON CONFLICT DO NOTHING`, and never reactivates a retained inactive membership).

## 3. Reader writes hold their audience grant until commit

Owner: the writer that opens the transaction takes the hold through the existing
`ensure_user_workspace_access_until_commit` (`AccessHold::UntilCommit`: workspace row
and the reader's `workspace_members` row share-locked on the writing connection).

Writers repaired, each before its first write and before the thread-policy read, in
the established order workspace share lock → policy → review advisory lock → row locks
(the same first step the owner's decisions and member removal take):

| Writer | Grant relied on |
| --- | --- |
| `add_reply_as` (reply; owner, reviewer, reader, owner's agent) | read access to the source |
| `publish_file_suggestions` recording transaction | read access to the source |
| `withdraw_suggestion` | read access to the source |
| `publish_review` publication on the source | reviewer's read access to the source |
| `QuestionVoteService::vote` / `withdraw` | read access to the workspace |

Denial before the hold leaves no write; a revocation already in flight makes the writer
wait and then refuse. Reads keep momentary admission. Comment threads, forks and
approval reviews already hold; pull-request creation re-admits per phase under its own
rule and is unchanged.

## 4. Viewer ceiling on additive grants

Owner: `PermissionService` owns effective authority; `AuthService` resolves issued
token scopes from the same rule.

Decision: one function composes effective scopes from a role and its additive grants
(organization defaults, individual grants, workspace overrides). For the viewer role,
additive scopes are kept only when read-only (`*:read`); other roles keep their
configured grants. Every consumer uses it: locked route/service admission
(`require_user_scopes_on_conn`), runtime predicates (`user_has_effective_scope`,
`_on_conn`), effective-scope DTOs, and token issuance (`resolve_effective_scopes`
for login, refresh, CLI exchange and test tokens). The SQL lock function is unchanged:
it returns the role and locked grants; the ceiling is a pure function on top.

Owner and administrator actions are unaffected: their role defaults already include
the administrative scopes.

## 5. Refresh credentials are stored as digests

Owner: `AuthService` owns every refresh credential path.

Decision: store `sha256(token)` and look up by digest, as API keys already do. The
plaintext is returned to the client once (cookie or body) and never stored. Paths:
OAuth login, `issue_user_tokens` (refresh and CLI exchange), E2E test token, refresh,
preview re-auth resolution, logout. Idle and absolute lifetimes, organization
re-resolution, concurrent sessions and the cookie-origin gate are unchanged.

Migration (`105_refresh_token_digest.sql`): rename `token` to `token_hash` and rewrite
every existing row to the hex SHA-256 of its stored value in the same statement set.
Existing sessions remain valid because the browser's cookie hashes to the rewritten
primary key; no re-login is required. The migration is one-way by design (plaintext is
not recoverable). An API instance running the previous code against the migrated
schema fails its refresh queries until it is replaced; the deploy already runs
migrations before the new release serves. No rotation, family or reuse detection is
introduced.

## Verification

Regressions that fail before the fix, through real HTTP, authentication and
PostgreSQL:

- API keys: an `org:admin` key cannot create or revoke keys (403, no row, target key
  still valid); a person session still can; wrong-organization and deactivated
  callers stay refused.
- OAuth: a revoke left uncommitted while the callback runs commits first; login
  succeeds with no membership and the invitation stays deleted. A normal pending
  acceptance creates one membership and a later revoke is refused.
- Audience: with a member removal or visibility change in flight, a reply, FILES
  push, withdrawal, round publication and vote wait, then refuse with nothing written;
  admitted writes still commit.
- Viewer ceiling: a viewer holding `org:admin` and `workspaces:write` grants (the
  populated fixture) cannot invite, change roles or write; issued tokens omit the
  grants; owner and administrator actions pass.
- Refresh digests: stored values are digests the client never sees; login, refresh,
  logout, CLI exchange and preview re-auth work; a database migrated through 104 with
  plaintext rows upgrades and the old cookie still refreshes within its lifetimes.

Repository checks: `cargo fmt`, `cargo clippy --workspace --all-targets -D warnings`,
SQLx offline data, migration checksums, and the affected `g5-api` integration modules.

## Assumptions

- Refusing key management to delegated keys is the intended reading of "constrain
  delegated key creation and revocation" (no caller depends on key-minted keys; the
  only reference is a comment in a canary script that mints once from a session).
- A single-deploy switch to digests is acceptable; dual-read compatibility is not
  needed because migrations precede the new release.

## Validation results (2026-10-22)

Local checks on the pinned 1.97.1 toolchain against a throwaway Postgres 17
container (`pgvector/pgvector:pg17`), all passing:

- `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`.
- `SQLX_OFFLINE=false cargo sqlx prepare --workspace -- --package g5-api --all-targets`
  (CI's `sqlx-check` form) regenerated the offline data for the four changed
  `refresh_tokens` and `org_invitations` queries; `scripts/check-migration-checksums.py`
  accepts migration 105.
- `cargo test -p g5-api --test integration` over `api_credential_authority`,
  `migration_105_refresh_token_digest`, `api_auth`, `api_orgs`, `api_token_management`,
  `api_org_admin_authority`, `api_permissions_e2e`, `api_member_activity`,
  `api_question_votes`, `api_comments`, `email_delivery` and the review
  audience-hold and FILES-push cases of `api_workspace_reviews`: 204 passed, 0 failed.
- The full `g5-api` integration suite under nextest after the review fixes: 1664 run,
  1663 passed; the one failure (`api_intent_log::…git_fallback…actor_filter…`) is the
  known VM-only Git identity mismatch (`GIT_AUTHOR_*` exported by the workspace) and
  passes with those variables cleared and in CI. CI's first run surfaced two more
  historical viewer-grant fixtures (`api_git_push_policy`, `api_external_issues`),
  now moved to the member role with the viewer asserted refused.

The new regressions failed on the unrepaired code in the expected way during
development (key creation by a key succeeded; the in-flight revoke was not awaited;
viewer grants admitted administration; rows held plaintext). Five
`api_org_admin_authority` fixtures that raised a viewer through additive grants were
moved to the member role; the first of them now also asserts that the viewer stays
refused with the same grant.

Migration 105 measured on this VM (Postgres 17, while the test suite ran): one million
synthetic rows take 33.6 s inside the transaction, about 34 µs per row, so the
`ACCESS EXCLUSIVE` pause is milliseconds at realistic session counts (one row per
live session per client). A staged backfill is not needed at this scale; the
measurement is the bound to recheck before a deployment with millions of sessions.

Gaps: no browser E2E was affected (the frontend is unchanged). Production
deployment of migration 105 is a single switch, as the eight earlier column-dropping
and renaming migrations were: the Helm `pre-upgrade` migration Job runs before the API
rollout, so an instance still running the previous code fails refresh and login
inserts (HTTP 500) during the rollout window until it is replaced; existing cookies
stay valid and succeed on new instances. A two-release expand/contract schema would be
new deployment practice for this repository and is outside this fix.
