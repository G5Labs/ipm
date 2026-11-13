# Outbound email with invitation-created delivery

Design for [issue #621](https://github.com/G5Labs/G5-Lean/issues/621). The request and
its interpretation are in the [chat intent](../../chat_intent/issue-621/request.md).
The authorized branch is `issue-621-add-a-scoped-outbound-email-module-with`, and the
design starts from `4839e606` (main).

## Ontology changes

| Block | Change | Summary |
|---|---|---|
| [Outbound email page](../../ontology/software_architecture/email.g5.md) | New module page | Shared mechanism for any domain. Covers the boundary, the consumer extension contract, committed handoff, validity check, message identity, retries, ambiguity limits, observability, data, provider, sender mailbox and per-kind volume limits. Open question on future recipient preferences. |
| [`g5.organization.invitation-email`](../../ontology/features/organizations.g5.md#g5.organization.invitation-email) | New | Which invitations are emailed, the email's content and link, and the rule that email grants nothing and outages never undo an invitation. |
| [`g5.organization.invitation-email-validity`](../../ontology/features/organizations.g5.md#g5.organization.invitation-email-validity) | New | When the invitation domain still wants a queued email. |
| [`g5.organization.invitation-resend`](../../ontology/features/organizations.g5.md#g5.organization.invitation-resend) | New | Resend requests a new message, coalescing with a live one. |
| [`g5.organization.invitation-email-limit`](../../ontology/features/organizations.g5.md#g5.organization.invitation-email-limit) | New | At most 100 new invitation emails per organization per rolling 24 hours; beyond that the invite or resend is refused with 429 and nothing changes. |
| [`g5.organization.invitation-email-status`](../../ontology/features/organizations.g5.md#g5.organization.invitation-email-status) | New | Administrators see each pending invitation's latest email status. |
| [Root](../../ontology/root.g5.md) | Navigation | Links the new module page. |
| [`g5.workspace.owner-email`](../../ontology/features/workspaces.g5.md#g5.workspace.owner-email), `.owner-email-validity`, `.owner-email-limit`, `.owner-email-preference` | New | The workspace agent emails the workspace's current owner with `g5d workspace email-owner`; see [g5d owner email](#g5d-owner-email). |
| [`g5.email.identity`](../../ontology/software_architecture/email.g5.md#g5.email.identity), [`g5.email.volume-limits`](../../ontology/software_architecture/email.g5.md#g5.email.volume-limits) | Amended | Single-use purpose keys for retry-safe requests; optional per-recipient limits; volume limits may be turned off in local stacks. |

## Governing ontology

- [`g5.organization.invites`](../../ontology/features/organizations.g5.md#g5.organization.invites)
  and [`.invitation-authority`](../../ontology/features/organizations.g5.md#g5.organization.invitation-authority):
  acceptance happens at verified login against the current invitation. The email only
  leads there and changes nothing about it. The CI-G1 race (OAuth acceptance after a
  revocation) is a separate, unchanged gap.
- [`g5.organization.existing-user-invitation-question`](../../ontology/features/organizations.g5.md#g5.organization.existing-user-invitation-question):
  immediate membership for existing users is preserved and sends nothing.
- [`g5.user.admission-continuity-question`](../../ontology/features/users.g5.md#g5.user.admission-continuity-question):
  a pending invitation already lets the invitee past the email allowlist at login, and
  this design relies on that unchanged behavior.
- [`g5.design.minimal-mechanism`](../../ontology/concepts/system_design_principles.g5.md#g5.design.minimal-mechanism),
  [`.abstraction`](../../ontology/concepts/system_design_principles.g5.md#g5.design.abstraction)
  and [`.durability`](../../ontology/concepts/system_design_principles.g5.md#g5.design.durability):
  justify the new table and worker, and keep the provider replaceable.
- [`g5.security.tenant.rls`](../../ontology/concepts/security.g5.md#g5.security.tenant.rls)
  and [`g5.security.tenant`](../../ontology/concepts/system_invariants.g5.md#g5.security.tenant):
  the delivery table is system-only, and every request names its organization.
- [`g5.architecture.commands`](../../ontology/concepts/system_invariants.g5.md#g5.architecture.commands):
  the invite and resend operations of `OrgService` own the request; routes do not.
- [`g5.platform.report-delivery`](../../ontology/features/operations.g5.md#g5.platform.report-delivery):
  the nearest precedent, with a persisted request, a claim, reconciliation of uncertain
  outcomes and no duplicate after a lost claim.

## Current behavior the design relies on

- `OrgService::invite_member` (`g5-api-core/.../identity/org_service.rs`) runs inside
  one system-pool `ScopedDb` transaction. It does one of three things: returns an
  existing member or an existing pending invitation unchanged, inserts a membership for
  an existing user, or inserts a pending `org_invitations` row (7-day expiry).
  `UNIQUE (org_id, email, status)` makes a concurrent duplicate invite fail and roll
  back.
- Invitations have no token. The id is a UUIDv7 and is not secret. Revoke hard-deletes
  the row. An hourly sweep marks expired rows, and acceptance already checks
  `expires_at` itself.
- Resend (`resend_invitation`) only does `UPDATE … SET expires_at = now() + 7 days`
  for a pending row.
- The invitee accepts at OAuth login: every pending, unexpired invitation that matches a
  verified email (case-insensitively) becomes a membership. There is no acceptance page.
- The frontend's `/:org` route requires authentication. `RequireAuth` sends the visitor
  to `/login`, which remembers the return path across the OAuth round trip, then
  continues to the organization's home.
- `G5_EXTERNAL_URL` is the browser origin, already used for absolute links.
- `MaintenanceRunner` runs background loops on the `daemon` role (`indexer` before #632) (`all` in tests and
  CI E2E). No generic queue exists: `platform_issue_outbox` is GitHub-specific, and
  migration 088 removed the jobs system.

## High-level design

```text
POST /orgs/:id/members ─► OrgService::invite_member ─┐ one transaction
                                                     ├─ INSERT org_invitations
                                                     └─ Email::request(conn, OrgInvitation{…})
                                                          └─ INSERT email_deliveries (queued)
                                                   COMMIT
maintenance role: EmailWorker every 5 s
  claim due rows (SKIP LOCKED, lease 2 min, attempt+1)
  └─ gate.still_wanted(purpose)  ── invitation domain: exists ∧ pending ∧ unexpired ∧ same address
       ├─ no  → cancelled(reason)
       └─ yes → transport.send(message, idempotency key = delivery id)
                 ├─ accepted        → sent(provider id)
                 ├─ permanent       → failed(rejected)
                 ├─ transient       → queued(backoff)      ┐ until first attempt + 23 h,
                 └─ no definite answer → queued(backoff), uncertain ┘ then failed(window), keep uncertain
```

### Module and ownership

The owner plans to reuse email for many purposes unrelated to identity
([follow-up](../../chat_intent/issue-621/request.md#follow-up)). The module is
therefore its own service context, `g5-api-core/src/services/email/`, placed first
(lowest) in the `service_context_layering` order: `Email`, then identity, git, hosts,
workspace and agents. It names no other context, and every context may call it. A
consumer's validity check is a trait the module owns, implemented in the consumer's
context and installed in `services/mod.rs`, the composition root the layering rule
already exempts. No new crate is needed: Rust module privacy inside
`services/email` hides the provider just as well, and the stateful services, SQLx
query cache and migrations stay where they are.

Public items:

- `Email` (cloneable handle in `AppState`, built from `EmailConfig`):
  - `request(conn, EmailRequest) -> Result<RequestOutcome>`: records a request on the
    caller's connection. The outcome is `Queued(id)`, `Coalesced(id)` or `Disabled`.
  - `latest_status(pool, org_id, PurposeKind, &[key]) -> map of key to DeliveryStatus`,
    scoped to one organization.
  - `worker(pool, EmailGates) -> Option<EmailWorker>`, which is `None` when email is
    disabled. This is the only way to build a worker.
- `EmailRequest { org_id, sender: Sender, recipient, message: Message }`, where:
  - `Sender` is the consumer's mailbox name
    ([`g5.email.sender`](../../ontology/software_architecture/email.g5.md#g5.email.sender)).
    It is built only by `const fn Sender::new("invitations")`, optionally followed by
    `.with_display_name("G5 Approvals")`, so a consumer declares it as a `const`. A
    malformed or role mailbox name, or a display name outside the plain character
    set, panics during constant evaluation, which fails the build. The module renders
    `<display name or G5_EMAIL_FROM_NAME> <name@G5_EMAIL_FROM_DOMAIN>` and stores it on
    the row, so retries keep an identical payload. Each consumer chooses its own mailbox and the
    module keeps no registry of names; two consumers may share one.
  - The coalescing identity is the message's `PurposeKind` plus a key the message
    supplies; for invitations the key is the invitation id. A request carries no
    separate purpose field, so a message cannot be filed under another kind.
  - `Message` is an enum with one variant per message kind, each a typed data struct.
    The first is `Message::OrgInvitation { org_name, inviter_name, role, link_path }`.
  - A message fixes its `PurposeKind`, so a request cannot pair an invitation message
    with another domain's validity check.
  - Consumers supply data and app-relative paths only. The module renders the subject,
    plain text and HTML, escaping every value, and resolves links against
    `G5_EXTERNAL_URL`.
- `DeliveryGate` trait: `still_wanted(pool, purpose_key, recipient) -> Result<Validity>`,
  where `Validity` is `Wanted` or `Cancelled(reason)`.
- `EmailGates`: a struct with one field per `PurposeKind`. The composition root must
  fill every field, so adding a kind without its check fails to compile rather than
  at send time.
- `EmailWorker::run(cancel)` for the daemon, plus `process_due(limit)` and `purge_ended()`
  for tests. Its constructor is private.
- `CaptureTransport` handle (`messages()`), returned when the transport is `capture`.

These stay private: the transport trait, the Resend client and its error mapping,
backoff, lease constants, SQL and templates (`email/templates/<kind>.rs`). An
integration-test probe checks that no public path reaches a transport or provider type.

**Adding a later kind** touches only the module (one `Message` variant, its
`PurposeKind` and template) and the consumer (its `Sender` constant, request call and
`DeliveryGate`).
That kind's design decides any recipient preference
([`g5.email.preference-question`](../../ontology/software_architecture/email.g5.md#g5.email.preference-question))
and any volume limit. No migration, worker or retry change is needed.

**Not built now:**

- Messages without an organization: rows require `org_id`, and a platform-level kind
  would need an amendment.
- Several recipients per message: send one request per recipient instead.
- Unsubscribe headers and preferences.
- Per-kind priorities or separate queues.

### Data (migration 093)

The `email_deliveries` table has these columns:

- `id uuid` (PK, also the idempotency key)
- `org_id` (FK → organizations, ON DELETE CASCADE)
- `purpose_kind text`, `purpose_key text`: no database list of kinds, so a new kind needs no migration; code accepts only `PurposeKind` values
- `sender`, `recipient`, `subject`, `text_body`, `html_body`: bodies are nullable and cleared when a request ends. The sender is stored too, so a configuration change between retries cannot alter the payload under the same idempotency key.
- `status` (`queued|sending|sent|failed|cancelled`)
- `attempts`, `next_attempt_at`, `lease_expires_at`, `first_attempt_at`
- `outcome_uncertain bool`, `last_error text` (classified code only)
- `terminal_reason`: module reasons `rejected` and `retry_window`, or `cancelled:<consumer reason>` such as `cancelled:invitation_revoked`
- `provider_message_id`, `created_at`, `updated_at`, `completed_at`

Indexes:

- A partial unique index on `(purpose_kind, purpose_key) WHERE status IN ('queued','sending')`
  enforces one live request per purpose.
- Partial indexes on due `queued` rows and on expired `sending` leases.

Grants: CRUD to `g5_app`, nothing to `g5_tenant`. The catalog-wide RLS assertion admits
the table because the tenant role has no grant. A privileges regression mirrors
`platform_issue_outbox_privileges`.

### Request (inside the caller's transaction)

`INSERT … ON CONFLICT (purpose_kind, purpose_key) WHERE live DO UPDATE SET
next_attempt_at = LEAST(next_attempt_at, now())`, returning the row and whether it was
inserted. This works because the insert and the conflict target share one predicate.
A conflicting live row in `sending` keeps its lease; the update touches only the due
time. Content is rendered before the insert. When the transport is `disabled`, nothing
is written, the caller proceeds and the request returns `Disabled`.

### Worker

The worker is spawned only when the transport is not `disabled`. It polls every 5
seconds, in batches of 10.

1. **Claim, in one short transaction.** Select rows that are either `queued` and due,
   or `sending` with an expired lease, using `FOR UPDATE SKIP LOCKED`. Set `sending`,
   `lease_expires_at = now() + 2 min`, `attempts + 1` and
   `first_attempt_at = COALESCE(first_attempt_at, now())`. Set
   `outcome_uncertain = outcome_uncertain OR (old status = sending)`, because a reclaimed
   lease means the earlier attempt may have reached the provider.
2. **Gate.** Run the row's kind's `still_wanted` from `EmailGates` on a pool
   connection. Cancel leads to `cancelled`. A stored kind that this build does not know
   ends as `failed(unknown_kind)` and is logged at error level; this only happens after
   a rollback to an older build.
3. **Send.** The provider HTTP timeout is 30 seconds, less than the lease.
4. **Record.** Every completion is a conditional update:
   `WHERE id = $1 AND status = 'sending' AND attempts = $claimed`. A worker that lost
   its lease cannot overwrite the new owner's result. Its in-flight send, and the new
   owner's, carry the same idempotency key.

A transient or uncertain outcome sets the row back to `queued` with backoff
`min(30 s · 2^(attempts−1), 30 min)` and ±20% jitter. A provider `Retry-After` is
honored up to that cap. If the next attempt would fall after
`first_attempt_at + 23 h`, the request ends instead as `failed(retry_window)` and
keeps its uncertain flag. Terminal transitions null the bodies and set `completed_at`.
The same loop deletes ended rows whose `completed_at` is more than 30 days old.

### Resend classification

| Provider answer | Class |
|---|---|
| 2xx with `id` | sent |
| 400 `validation_error`, 422 | permanent: `rejected` |
| 400 `invalid_idempotency_key`, 409 `invalid_idempotent_request` | permanent: internal error, logged at error level (cannot occur with a fixed key and payload) |
| 401, 403 (key or sender domain) | transient, logged at error level as a configuration error |
| 409 `concurrent_idempotent_requests` / `resource_locked`, 429 | transient |
| 5xx, timeout or broken connection after the request was sent | uncertain |
| Connection refused, DNS failure (request never sent) | transient |

The capture transport always accepts the message. Integration tests reach the other
classes through the Resend client against `wiremock`.

### Invitation integration

- `invite_member`, pending branch: after the insert, read the organization's name and
  slug and the inviter's display name (falling back to email), then call
  `email.request(db.conn(), …)` in the same transaction. A database error there fails
  the invite as any other write would. No provider call happens in the request path.
  The member, existing-pending and immediate-membership branches request nothing.
- `resend_invitation`: after the expiry update, it requests delivery for the same
  purpose. The [resend contract](../../ontology/features/organizations.g5.md#g5.organization.invitation-resend)
  is a new message, or coalescing with a live one. The route's response is unchanged.
- `InvitationDeliveryGate` (identity context, installed in `EmailGates`) reads the row by id and returns a cancel reason when the
  invitation is missing (revoked), `accepted`, `declined`, past `expires_at`
  (`expired`, whether or not it has been swept), or no longer addressed to the
  recorded recipient.
- `list_invitations` adds `emailDelivery: { status, uncertain, updatedAt } | null` per
  invitation from `latest_status`. The Members page shows it beside the pending
  invitation ("Email queued", "Email sent", "Email failed", "Email may not have been
  delivered") next to the existing Resend button. With email disabled the field is
  `null` and nothing is shown.
- The message says who invited the recipient to which organization and as which role,
  and that signing in with this email address joins it. It omits the expiry date: a
  resend that coalesces into an already-rendered message would otherwise show a stale
  date.

### Configuration and deployment

| Variable | Meaning |
|---|---|
| `G5_EMAIL_TRANSPORT` | `disabled` (default), `capture` or `resend` |
| `G5_EMAIL_FROM_DOMAIN` | Verified sending domain, for example `mail.g5labs.ai`; required unless disabled |
| `G5_EMAIL_FROM_NAME` | Display name on every message (default `G5`) |
| `G5_EMAIL_RESEND_API_KEY` | Secret; required for `resend` |
| `G5_EMAIL_RESEND_API_URL` | Default `https://api.resend.com`; stubs override it |

`EmailConfig::validate` (run by `validate_startup`) makes startup fail on an
incomplete non-disabled configuration. `Debug` redacts the key. Both API and daemon pods read
the same values, because the API records requests and the daemon sends them.

Deployment:

- Terraform adds three Secret Manager containers, `g5-email-transport`,
  `g5-email-from-domain` and `g5-email-resend-api-key`. They are bootstrapped inert: the
  transport as `disabled`, the sending domain and key blank. An all-or-nothing
  `g5-email-secrets` ExternalSecret carries them, so the operator enables QA email by
  adding versions, with no chart change. A Helm value was rejected because it would
  have to change in the same deploy as the operator's secrets.
- Helm adds an optional `g5-email-secrets` `secretRef` to `g5.apiSecretEnvFrom`, so
  the API and the daemon both receive it.
- Self-hosted `env.template` documents the variables, defaulting to `disabled`.
- Compose dev and test, and CI E2E, set `resend` with the API URL pointed at
  `oauth-stub`. The stub becomes the stack's hermetic provider stub and gains
  `POST /emails`, which records the message and its `Idempotency-Key` and replays the
  same id for a repeated key, plus `GET /__e2e/emails`.
- `docs/email-delivery.md` documents sender and domain setup (SPF/DKIM via Resend
  domain verification), secrets, local capture, the stub, failure handling and
  diagnosis queries.

### Provider

Resend is the [chosen provider](../../ontology/software_architecture/email.g5.md#g5.email.provider)
because its `Idempotency-Key` header (24-hour retention, the same answer for a repeated
key) is what lets uncertain attempts retry without duplicates and without
provider-side reconciliation. Postmark documents that it has no idempotency key, and
SendGrid's and Amazon SES's send APIs have none. With those, uncertain outcomes would
need search-before-retry or would end as possibly sent after the first uncertain
attempt. Resend's account-level suppression list blocks repeat sends to hard-bouncing
or complaining addresses; G5 adds no suppression store. Transactional invitations need
no unsubscribe link. Provider-required compliance is otherwise the sending domain's
verification and the abuse question below.

## Decision notes

- **Why a new table and worker.** The issue requires that requests survive a restart
  and never send for a rolled-back invitation. That needs the request in the invitation
  transaction and a durable store. `ScopedDb::on_commit` alone would lose the request
  on a crash between commit and send. The platform-report outbox hard-codes GitHub
  columns, a marker search and tombstones; generalizing it would change a working
  delivery path for no gain. The new table is the minimum that meets the requirement,
  and the worker reuses the maintenance runner and its cancellation.
- **A shared context, not an identity leaf.** The first draft placed email inside
  identity because that was its only consumer. Since email will be reused across
  domains, a lowest context keeps it free of identity, and lets git, workspace and
  agents call it without depending on identity's internals. A separate crate was
  rejected: it would split the SQLx cache and migrations from the services that share
  the database, and gives no extra encapsulation over module privacy.
- **Gate rather than cascade.** A foreign key with cascade would drop the request on
  revoke and lose its observability, and it cannot see acceptance or expiry. The
  pre-send check keeps the invitation rules in the invitation domain. Its race window
  (a commit between check and send) is harmless because the email grants nothing.
- **Render at request time.** A fixed payload is required by the idempotency key: a
  re-rendered payload would get `409 invalid_idempotent_request`. It also makes retries
  deliberately unaffected by later organization renames.
- **Coalescing resend.** A resend during an outage must not stack messages. A resend
  after the previous message ended is the administrator's explicit choice to send
  again, even if the earlier one was possibly sent.
- **No synchronous send attempt.** Calling the provider from the request path would tie
  invite latency and failure to the provider. A 5-second poll is the accepted latency.
- **Limit counted from delivery rows.** The shared `rate_limit_windows` store is a
  fixed one-minute window and could not express a rolling 24 hours. Counting the
  module's own rows under a transaction-scoped advisory lock needs no new state, and
  refusal charges nothing because the transaction rolls back.
- **Not chosen.** Postgres `LISTEN/NOTIFY` wake-ups (an optimization only), a
  separate crate, multiple providers, SMTP, user-managed templates, membership-added
  notifications, and changing CI-G1.

## Owner questions

The owner answered the `outbound-email` set on 2026-10-17, and each answer reports
`decision: true`:

| Question | Decision |
|---|---|
| `provider` | Resend |
| `sender-domain` | `G5 <invitations@mail.g5labs.ai>`; the owner provisions the account, DNS and key |
| `invitation-email-cap` | Cap at 100 per organization per 24 hours |

The cap is recorded as [`g5.email.volume-limits`](../../ontology/software_architecture/email.g5.md#g5.email.volume-limits)
and [`g5.organization.invitation-email-limit`](../../ontology/features/organizations.g5.md#g5.organization.invitation-email-limit),
replacing the former abuse question.

### Volume limit

Each `PurposeKind` declares `daily_limit_per_org: Option<u32>`, which is `Some(100)`
for `OrgInvitation`. The check runs inside `request`, before the insert:

1. Take `pg_advisory_xact_lock` on a key derived from (org, kind). This serializes
   concurrent requesters until their transactions end, so two invites cannot both take
   the last slot.
2. Return the coalesced result if a live row exists, without counting it.
3. Otherwise count rows with that org and kind and `created_at > now() - 24 h`.
4. At or over the limit, return `EmailError::Limited { retry_after }`, where
   `retry_after` is the oldest counted row's `created_at + 24 h - now`.

The invitation service maps that error to `G5Error::RateLimited`. The route returns 429
with `Retry-After`, and the transaction rolls back, so neither the invitation insert nor
the resend's expiry extension survives. An index on `(org_id, purpose_kind, created_at)`
serves the count. The 30-day retention keeps every row the 24-hour window needs.

**Local development (owner, 2026-10-19).** `G5_EMAIL_VOLUME_LIMITS=off` skips every
kind's limit; the compose dev stack sets it, so repeated local E2E runs no longer hit
429. Startup refuses `off` while the `resend` transport uses Resend's public API, so a
deployed environment cannot drop the limit by mistake. Test stacks and CI keep
`enforce`, which keeps the limit's behavior tested. Evidence: the config unit test and
`email_delivery::a_local_stack_with_volume_limits_off_admits_invitations_past_the_cap`.

## Implementation notes

- **Unsendable addresses.** An invitation whose address G5 cannot send to (no domain
  dot, spaces, several addresses) is created without an email and a warning is
  logged. It is not refused, because email being unavailable never undoes a valid
  invitation. This is recorded in
  [`g5.organization.invitation-email`](../../ontology/features/organizations.g5.md#g5.organization.invitation-email).
- **Consumer-chosen sender mailbox** (owner request, 2026-10-18). A single
  `G5_EMAIL_FROM` mailbox was replaced by a configured domain and display name plus a
  build-time-checked `Sender` per consumer, which may also set its own display name
  (owner follow-up). Invitations use `invitations` with the configured name. On a
  self-hosted install a consumer's own display name replaces the operator's
  configured brand for that kind of mail, so G5's own kinds should leave it unset
  unless the kind needs it. The secret
  container `g5-email-from` became `g5-email-from-domain` before any deployment used
  it. No schema change was needed, because the rendered mailbox is already stored in
  `sender`.
- **Runtime SQL.** The module and the invitation changes use runtime `sqlx::query`,
  so the SQLx offline cache did not change.
- **Lost-lease fence.** A worker that loses its lease reports `lost` instead of
  overwriting the new owner's result. Both of their sends carry one idempotency key.
- **Gate errors.** A database error in the validity check puts the request back
  without marking it uncertain, because nothing was sent.
- **Late claims.** A retry the worker claims after `first_attempt_at + 23 h` (for
  example after a worker outage) ends as `failed(retry_window)` before the validity
  check and without a send, so no attempt leaves the provider's idempotency window.

## Validation results

Run on 2026-10-17 at the branch head, against live Postgres 17 in the VM:

- **Email integration suite.** `email_delivery::` has 25 integration tests, all
  passing. They cover:
  - the sender mailbox at the configured domain and display name;
  - committed handoff, rollback, the immediate-membership, existing-member and
    existing-pending branches, and a concurrent duplicate invite;
  - an unsendable address and disabled email;
  - restart recovery, a reclaimed lease marked uncertain, and the lost-lease fence;
  - 503-then-200 retries with one key and an identical payload, a permanent 422,
    401 and 429 with `Retry-After`, and retry-window exhaustion;
  - revoke, accept, decline, expiry before and after the sweep;
  - the resend contract, the volume limit including concurrency, the status listing
    and log redaction;
  - the system-only grants, retention, and the Rust end-to-end invite → message →
    link → OAuth login → membership.
- **Regression suites.** All passing: `api_orgs`, `api_org_admin_authority`,
  `api_auth`, `service_context_layering`, `migration_087_tenant_table_rls`,
  `platform_issue*`, `maintenance_integration`, `crud_lifecycle_integration`,
  `workspace_search_removal_guard` and the invitation filters (181 tests over the
  two runs).
- **Unit tests and doctests.** Email unit tests pass for rendering, classification
  and backoff, as do the config validation tests. The module doctests pass, including
  three `compile_fail` probes showing that the transport, the store and worker
  construction are unreachable from outside the module.
- **Static checks.** `cargo fmt --check`, and `cargo clippy -D warnings` over the four
  affected crates with all targets. The migration checksum check passes.
- **Frontend.** `tsc` (build config), ESLint on the changed files, and the label
  helper's vitest.
- **Infrastructure.** The Terraform/ESO contract tests pass, including the new
  `test_email_secrets.py`. The Helm template renders the optional `g5-email-secrets`
  reference for both deployments, and both compose files pass `config`.
- **Playwright.** The whole `invite-members.spec.ts` (34 tests in Chromium) passes,
  including the new end-to-end test. It ran against a local stack: the host-built
  API, the provider stub and Vite.

Gaps:

- Production delivery through a real Resend account is unverified until the owner
  provisions `mail.g5labs.ai`; the steps are in `docs/email-delivery.md`.
- Firefox and WebKit were not run locally, and the CI E2E lanes have not run yet.
- The 30-second client timeout path is classified as uncertain in code but is not
  exercised by a test. The uncertain outcome itself is exercised through 5xx
  responses and an expired lease.

## Validation plan

These are live-Postgres integration tests in `g5-api`'s `integration` target, plus
Playwright:

- **Committed handoff.**
  - Inviting over HTTP records one queued request.
  - A forced rollback after the request (a failing statement in the same `ScopedDb`)
    leaves no row.
  - The immediate-membership and existing-pending branches record none.
  - A concurrent duplicate invite records exactly one request.
- **Restart recovery.**
  - A request committed with no worker is sent by a fresh worker.
  - A row left `sending` with an expired lease is reclaimed and marked uncertain, and
    the stale worker's late completion is refused by the attempt fence.
- **Transient and permanent failures** (wiremock Resend):
  - A 503 is followed by a 2xx and the request ends `sent` with one idempotency key
    across both attempts.
  - A 422 ends as `failed(rejected)` after one attempt.
  - A 401 retries and is logged as a configuration error.
  - A 429 honors `Retry-After`.
  - Exceeding the 23-hour window ends as `failed(retry_window)`.
- **Ambiguity and duplicates.**
  - A timeout followed by success keeps the same key and payload.
  - The stub's replay returns the same provider id.
  - Requesting while a request is live coalesces, and requesting after it ended creates
    a new identity.
  - Retries never add rows.
- **Invalidated invitations.** Revoke, accept (HTTP and OAuth login), decline, and an
  `expires_at` in the past, each before the worker runs, end `cancelled` with the
  matching reason and no provider call.
- **Volume limit.** The 101st new invitation request in 24 hours returns 429 with
  `Retry-After` and leaves no invitation and no row. A coalescing resend at the limit
  still succeeds. Two concurrent invites at 99 admit exactly one. Other organizations
  are unaffected.
- **Resend contract.** Resend after `sent` queues a second message with a new key;
  resend while queued coalesces; resend still extends expiry; a non-pending resend is
  still a 409 and records nothing.
- **Observability.**
  - The invitation list exposes the status.
  - A log-capture test asserts that no key, recipient or body appears in delivery logs.
  - Privileges regression for `g5_tenant`.
  - Startup refuses incomplete configuration.
- **Encapsulation probe.** The outside crate can construct only `EmailRequest` and its
  public enums.
- **Shared-context boundary.** `service_context_layering` places `services/email` first
  and fails if any email file names identity or a later context. The invitation gate is
  reached only through `EmailGates` from the composition root.
- **Rust end to end with capture.**
  - Invite over HTTP and run `process_due`.
  - Inspect the captured recipient, sender, subject and body, and parse the link.
  - Assert the link equals `G5_EXTERNAL_URL/{slug}`.
  - Complete the OAuth login as the invitee against the mocked provider.
  - Assert membership in the organization named by the link.
- **Browser end to end** (`invite-members.spec.ts`).
  - The owner invites a new stub identity, and the test reads the message from
    `oauth-stub /__e2e/emails`.
  - `page.goto(link)` reaches login; signing in as that identity through the stub lands
    in the invited organization's home.
- **Regressions.** Existing `api_orgs`, `api_org_admin_authority`, `api_auth`
  invitation tests, `maintenance_integration`, `service_context_layering`, migration
  checksum, frontend unit tests for the status label.

Production verification is the owner's step: after provisioning the domain and key,
deploy to QA, invite a real address, and confirm receipt and the delivery's `sent`
row. Until then it is reported as not done.

## g5d owner email

Status: **implemented.** Requested by the owner on 2026-10-19, who then accepted both
recommendations and said "build it"
([chat intent](../../chat_intent/issue-621/request.md#local-cap-and-owner-email-command)).
See [owner decisions](#owner-email-questions) and [implementation notes](#owner-email-implementation).

### Purpose and scope

An agent in a workspace VM can email that workspace's **current owner**, for example
to report that a long task finished or needs a decision. The command has no recipient
argument. It cannot reach collaborators, other members or outside addresses, so it
cannot be used to spam anyone but the person who directs the agent. It is the first
consumer outside identity and the second [email kind](../../ontology/software_architecture/email.g5.md#g5.email.consumers).
Not included: attachments, HTML or Markdown authored by the agent, other recipients,
reading replies, and a status-polling command (the response already gives the
delivery id and status).

Governing: [`g5.email.consumers`](../../ontology/software_architecture/email.g5.md#g5.email.consumers),
[`.volume-limits`](../../ontology/software_architecture/email.g5.md#g5.email.volume-limits),
[`.preference-question`](../../ontology/software_architecture/email.g5.md#g5.email.preference-question),
[`g5.security.guest-authority`](../../ontology/concepts/security.g5.md#g5.security.guest-authority)
(the owner controls the VM, so the guest is no boundary against the owner),
[`g5.security.credential-classes`](../../ontology/concepts/security.g5.md#g5.security.credential-classes)
(a VM identity never acts as a person for person-account operations; emailing that
person is not one), and [`g5.external-issues.cli`](../../ontology/features/issues.g5.md#g5.external-issues.cli)
(non-interactive, gh-style agent commands with an idempotency key).

### Command

```text
g5d workspace email-owner --subject TEXT (--body TEXT | --body-file PATH|-) [--idempotency-key KEY] [--json]
```

- `-s/--subject`: 1–120 characters, one line.
- `-b/--body` or `-F/--body-file`: plain text, 1–20,000 characters, with line breaks
  kept.
- The command never prompts. It generates an idempotency key when none is given and
  retries transport failures with that same key.
- Output: `Queued email to the workspace owner (delivery <id>)`, or with `--json` the
  `{deliveryId, status}` record. The owner's address is not printed; the agent does
  not need it.
- Exit codes follow `g5d issue` and g5d's shared error mapping:
  - `0`: queued, or an earlier request with the same key exists;
  - `2`: the subject, body or key is invalid;
  - `3`: rate limited, with the retry time printed (wait as told);
  - `7`: G5 did not confirm the request. Rerun with the printed `--idempotency-key`.
  - `1`: any other refusal, including "email is not enabled in this deployment"
    (a 409, so the client does not retry it as an outage).

### Path and authority

The command follows the `g5d issue create` path end to end:

1. `g5d` calls g5-api directly with the workspace-incarnation token
   (`InternalApiClient::from_workspace_env`), at
   `POST /api/internal/workspaces/{workspaceId}/owner-email` with
   `{subject, body, idempotencyKey}`.
2. The route is added to the incarnation allowlist (`auth_middleware.rs`) and
   registered as an exempt route with the reason "the owner's agent emails its own
   owner".
3. The handler checks that the path names the token's workspace. It then opens a
   system-pool `ScopedDb` for the token's organization: the owner's login email and
   the delivery table are system data the tenant role cannot read.
4. A workspace-context service, on that one transaction:
   - proves the token's VM is the workspace's live incarnation with
     `current_incarnation_on_system` (`FOR SHARE` until commit). This also confirms
     that the workspace, organization and owner still match the token;
   - reads the owner's current login email and the workspace and organization names
     and slugs;
   - calls `Email::request`.

   A live workspace's owner is always an eligible member of its organization
   (`workspaces_owner_same_org_fk`), so membership needs no check of its own.

The recipient is never taken from the request, so a caller cannot redirect mail. A
stopped or replaced VM cannot send, because its incarnation is no longer live. Mail
already queued from a VM that then stops is still delivered.

### Email module additions

All additions are generic, and the invitation kind is unchanged:

- `PurposeKind::WorkspaceOwnerMessage` and its message
  `Message::WorkspaceOwnerMessage { workspace_name, org_name, subject, body, link_path }`.
- The sender constant `Sender::new("agent").with_display_name("G5 Agent")`, giving
  `G5 Agent <agent@mail.g5labs.ai>` ([`g5.email.sender`](../../ontology/software_architecture/email.g5.md#g5.email.sender)).
- **Single-use purpose keys.** The purpose key is `{workspaceId}:{idempotencyKey}`. For
  a kind that declares single-use keys, a repeated key returns the request that exists
  for it, live or ended, instead of creating a message. This uses a new outcome
  `Existing(id)`, so a CLI retry after a lost response cannot send twice. Without it,
  [`g5.email.identity`](../../ontology/software_architecture/email.g5.md#g5.email.identity)
  would treat a retry after the first message was sent as a new message. The lookup
  runs under the kind's per-(organization, kind) volume lock, so concurrent retries
  with one key are serialized; with limits off in a local stack, the live-purpose
  unique index still coalesces them. The guarantee lasts as long as the 30-day
  retention keeps the row.
- **Per-recipient limit.** `PurposeKind` gains `daily_limit_per_recipient`, counted
  under the same per-(organization, kind) advisory lock as the organization limit. One
  owner's runaway agents then exhaust only that owner's allowance, not the
  organization's. The count reuses the volume index and filters on `recipient`.
- **Template.** It needs a body renderer that keeps line breaks; the existing `plain()`
  collapses whitespace. The rendered message has these parts:
  - subject `[{workspace}] {subject}`;
  - the body as escaped plain text (`white-space: pre-wrap` in HTML);
  - a fixed footer: "Sent by the agent in your G5 workspace {workspace} ({org}).
    G5 did not write this message, and an agent never needs your password or keys."
    It ends with a link to `/{org}/{workspace}`.

  The footer marks agent-authored text so it is not read as G5's own voice, which
  limits phishing through a prompt-injected agent.

No migration is needed: kinds and keys are text, and the existing indexes serve both
counts.

### Validity

`WorkspaceOwnerEmailGate` lives in the workspace context and is installed in
`email_gates()`. A queued message is still wanted only while all of the following
hold:

- the workspace exists and is not deleted;
- its current owner's login email equals the recorded recipient.

A deletion cancels the message with `workspace_deleted`; a transfer or an owner's
email change cancels it with `owner_changed`. The message carries no
authority, so the remaining race between the check and the send is harmless
([`g5.email.validity`](../../ontology/software_architecture/email.g5.md#g5.email.validity)).

### Limits and preference

The agent controls this kind's volume, so it must carry limits
([`g5.email.volume-limits`](../../ontology/software_architecture/email.g5.md#g5.email.volume-limits)).
The limits are 50 per owner and 500 per organization in any rolling 24 hours (owner
decision). The route maps a refusal to 429 with `Retry-After`, and `g5d` exits 3. Local
stacks with volume limits off skip both.

The message is the owner's own agent writing to them, usually because they asked it
to. The owner settled [`g5.email.preference-question`](../../ontology/software_architecture/email.g5.md#g5.email.preference-question)
for this kind as "requested": no unsubscribe link and no preference store now. The owner silences it by instructing their agents, and a later notification
settings design ([CI-G4](../../../docs/code_gaps.md)) may add a per-user switch.

### Decision notes

- **Owner only, no `--to`.** Letting the agent choose recipients would turn a workspace
  token into a general mail relay on G5's sending reputation. Other recipients need
  their own kinds with their own authorization.
- **Through the email module, not a new path.** The module already gives the committed
  handoff, retries, ambiguity handling, status and logs without content. The command
  adds only a consumer.
- **Single-use keys rather than a consumer table.** Recording CLI idempotency in a
  workspace table would duplicate the delivery row. A kind-level flag on the module's
  existing purpose identity is the smaller mechanism.
- **Per-recipient rather than per-workspace limit.** The inbox being protected belongs
  to the owner, whatever the number of workspaces. A per-workspace cap would let an
  owner with many workspaces receive many times more mail, and it would need another
  scope in the module.
- **Plain text only.** Agent-authored HTML would let a prompt-injected agent imitate G5
  mail. Escaped text with a fixed footer keeps the source visible.
- **No MCP tool now.** Agents reach `g5d` through the shell; an MCP wrapper can follow
  with the same route.

### Validation plan

- **Integration** (`email_delivery::` plus a new `workspace_owner_email::`):
  - the token's owner receives one message with the footer and link;
  - a body with HTML and header-like lines is escaped and kept as text;
  - a repeated idempotency key after the message was sent returns `existing` and sends
    nothing;
  - a stale incarnation, another workspace's token and a person's session are refused;
  - transfer, deletion and an owner email change before sending cancel the message
    with the matching reason;
  - the 51st request for one owner and the 501st for one organization return 429 with
    nothing written, while concurrent requests at the edge admit exactly one;
  - disabled email is refused with 409.
- **g5d:**
  - the argument parser takes gh-style flags, needs a subject and one body, and has
    no recipient flag;
  - only answers without a definite outcome (network, timeout, 5xx after retries)
    exit 7 with the key; rate limits exit 3 and definite refusals pass through.
- **Regression:** `service_context_layering` (the gate lives in workspace, and email
  still names no consumer), the incarnation allowlist and route-auth audits, and the
  invitation suites unchanged.

<a id="owner-email-questions"></a>
### Owner decisions

The owner accepted both recommendations in chat on 2026-10-19 ("both of your
recommendations look fine. build it"):

| Question | Decision |
|---|---|
| `owner-email-limits`: daily limits for agent-to-owner email | 50 per owner and 500 per organization per rolling 24 hours |
| `owner-email-preference`: does this kind need an opt-out now? | No: it is owner-requested mail; revisit with notification settings |

<a id="owner-email-implementation"></a>
### Implementation notes

- **Disabled email is a 409.** The internal client retries every 5xx, so a 503 would
  be retried and then reported as unconfirmed (exit 7). A definite refusal is
  reported as such.
- **No membership check.** The design's "owner no longer a member" refusal and
  `owner_not_member` cancellation were dropped. `workspaces_owner_same_org_fk` makes
  that state impossible for a live workspace, and leaving the organization requires a
  transfer first, which cancels the message as `owner_changed`.
- **Links to workspaces.** Workspace names are Git branch names (for example
  `feature/login`). The public `email::link_path` percent-encodes each segment, and
  the module's link check now accepts well-formed `%XX` escapes.
- **The client retries.** `InternalApiClient::email_owner` uses the retrying POST,
  because a repeated key cannot send twice.
- **System pool, not the incarnation helper.** `scoped_db_for_workspace_incarnation`
  runs on the tenant pool, which cannot read `orgs`, `users` or `email_deliveries`.
  The system role cannot take `FOR SHARE` on `workspaces` directly, and without an
  organization context row-level security hides workspaces from it. The route
  therefore uses a system-pool `ScopedDb` with the SECURITY DEFINER incarnation
  lookup. The worker's validity gate reads the workspace through
  `authz.lookup_workspace_runtime_authority` for the same reason.
- **Agent guidance.** The workspace agent instructions in `.agents/AGENTS.md`, which
  are packaged into the worker runtime, describe `g5d workspace email-owner` next to `g5d issue`:
  when to use it, its bounds, limits and exit codes. The command's `--help` repeats
  the recipient rule, limits and exit codes.

### Owner email validation results

Run on 2026-10-19 against live Postgres 17 in the VM:

- **Owner email suite.** `workspace_owner_email::` has 7 integration tests, all
  passing. They cover:
  - the message, its attribution footer, escaped HTML and the workspace link;
  - a repeated key after sending;
  - stale, foreign and person credentials;
  - invalid input and disabled email;
  - cancellation on transfer, email change and deletion;
  - the per-owner limit with concurrency, and the per-organization limit.
- **Regression suites.** All passing (128 tests in one run): `email_delivery::`,
  `service_context_layering::`, `route_auth_static::`, `api_question_sets::`,
  `api_external_issues::` and `api_orgs::`.
- **Unit tests and doctests.** All passing:
  - email templates and link encoding;
  - the service's input bounds;
  - the `g5d workspace email-owner` parser, outcome classification and output;
  - the email module's doctests, including `link_path` and the per-recipient limit;
  - `auth_middleware` unit tests.
- **Static checks.** `cargo fmt --check`, and `cargo clippy -D warnings` over g5d, the
  internal API client and the four API crates, with all targets.
- **Binary.** `g5d workspace email-owner --help` prints the exit-status guide. Outside a
  workspace VM the command refuses with a configuration error.
- **Not run:** a send from a real workspace VM, and a real Resend send, which still
  awaits the owner's provisioning. Two `g5d` host-mode unit tests fail in this VM with
  "host block capacity: Permission denied"; they are unrelated to this change.
