# Outbound email

G5 sends transactional email through one shared module
(`backend/g5-api-core/src/services/email/`). The governing rules are in
[the email ontology page](../.g5/ontology/software_architecture/email.g5.md). Its first
consumer is the organization invitation email
([invitation email](../.g5/ontology/features/organizations.g5.md#g5.organization.invitation-email)).

This page covers configuration, local capture, provisioning the provider and
operating delivery.

## How delivery works

1. A domain operation (inviting a person, resending an invitation) records a request
   in `email_deliveries` **inside its own transaction**. A rolled-back invite records
   nothing, and a committed one survives restarts. The message is rendered at this
   point, so retries send identical content.
2. The email worker runs in the background daemon (`G5_ROLE=daemon`, or `all`). It
   polls every 5 seconds and claims one due request at a time under a 2-minute lease.
3. Before each attempt the consumer decides whether the message is still wanted.
   Revoked, accepted, declined or expired invitations cancel their email.
4. The worker sends the message to Resend with `Idempotency-Key: g5-email-<request id>`.

One request is one message identity. Retries reuse the identity, and Resend
deduplicates a repeated key for 24 hours. Retries stop 23 hours after the first
attempt, inside that window.

| Provider answer | What G5 does |
|---|---|
| 2xx | `sent`; Resend's message id is kept |
| 400 `validation_error`, 422, 409 `invalid_idempotent_request` | `failed` / `rejected`, no retry |
| 401, 403 | Retry with backoff; logged at error level as a configuration problem |
| 409 `concurrent_idempotent_requests`, 429 | Retry; `Retry-After` honored (capped at 30 minutes) |
| 5xx, timeout, broken connection | Retry with the same key; marked `outcome_uncertain` |
| Connection refused / DNS | Retry; nothing was sent |

Backoff is 30 seconds doubling to a 30-minute cap, with ±20% jitter. A request that
reaches the window without a definite answer ends `failed` / `retry_window`. If it had
an uncertain attempt, the UI shows it as **possibly sent**. G5 never retries such a
request automatically. An administrator's Resend creates a new message, which may
duplicate one that was in fact delivered.

"Sent" means Resend accepted the message. Later bounces, spam filtering and Resend's
suppression of addresses that bounced or complained are not tracked in G5. Look a
message up in the Resend dashboard by its `provider_message_id`.

### Invitations

- **When an email is sent.** Only a newly committed *pending* invitation is emailed.
  Adding an existing user (immediate membership), re-inviting a member and
  re-inviting an address that is already pending send nothing. An address G5
  cannot send to (for example one with no domain dot) keeps its invitation without
  an email, and a warning is logged.
- **What the link does.** The link opens `<G5_EXTERNAL_URL>/<org-slug>`. That page
  requires login, and logging in with the invited, verified email accepts the
  invitation. The email carries no token and grants nothing.
- **Resend.** Resend extends the expiry and requests a new email. If the previous
  email is still queued or sending, it is made due now instead.
- **Volume cap.** One organization may request at most 100 new invitation emails per
  rolling 24 hours, from invites and resends together. Beyond that the invite or
  resend is refused with `429` and `Retry-After`, and nothing is changed. The compose
  dev stack turns the cap off (`G5_EMAIL_VOLUME_LIMITS=off`).
- **Status.** The Members page shows each pending invitation's latest email status:
  queued, sent, failed, may not have been delivered, or not sent (cancelled).

### Agent email to the workspace owner

An agent in a workspace VM runs `g5d workspace email-owner` to email that workspace's owner:

```bash
g5d workspace email-owner -s "Tests pass" -b "All 42 tests pass; the PR is ready for review."
g5d workspace email-owner -s "Needs a decision" -F notes.txt      # or -F - for stdin
g5d workspace email-owner -s "Daily digest" -F digest.md --format markdown
```

- **Recipient.** Always the workspace's current owner, at their login email. The
  command takes no recipient. Only the workspace's live VM can send.
- **Content.** A one-line subject of up to 120 characters, and a body of up to 20,000
  characters. Mail comes from `G5 Agent <agent@…>`, and the subject is prefixed with
  `[<workspace>]`. A notice before the body and a footer after it say the workspace's
  agent wrote the message, not G5. The footer links to the workspace.
- **Formats.** By default the body is plain text and is never rendered as HTML. With
  `--format markdown`, the HTML part renders a fixed subset: headings `#`–`###`,
  paragraphs (a line break is kept), bold, italic, lists nested at most 2 levels, block
  quotes nested at most 2 levels, inline and fenced code, `---` rules, and tables of up
  to 10 columns and 200 rows. Raw HTML, images, deeper headings, footnotes, task lists
  and strikethrough appear as the literal source text. A link is clickable only when it
  is `https`, leads to G5's public host or into one of the organization's GitHub or
  GitLab repositories, and its text is not a different URL. Any other link shows its
  URL in parentheses and is not clickable; bare `https` URLs follow the same rule. The
  text part is always the body as written. A body over a bound, or one with a
  reference-style link definition (`[x]: url`), is refused with exit 2 and the reason
  and line. Nothing is sent in part. Rules:
  [`g5.email.markdown`](../.g5/ontology/software_architecture/email.g5.md#g5.email.markdown)
  and [`g5.email.link-policy`](../.g5/ontology/software_architecture/email.g5.md#g5.email.link-policy).
- **Retries.** Each request carries an idempotency key, generated unless
  `--idempotency-key` is given. Repeating a key returns the earlier request and sends
  nothing. When G5 does not confirm a request, the command exits 7 and prints the key
  to rerun with.
- **Cancellation.** A queued message is cancelled if the workspace is deleted, its
  ownership moves to someone else, or the owner's login email changes.
- **Limits.** 50 messages to one owner within an organization, and 500 per organization,
  in any rolling 24 hours.
  Beyond either, the request is refused with `429` and the command exits 3. The
  compose dev stack turns the limits off.
- **Exit status.** 0 queued or existing, 2 invalid input, 3 rate limited, 7
  unconfirmed, 1 for any other refusal, including email being disabled.

## Configuration

| Variable | Meaning |
|---|---|
| `G5_EMAIL_TRANSPORT` | `disabled` (default: nothing recorded, no worker), `capture` (kept in memory and logged), `resend` |
| `G5_EMAIL_FROM_DOMAIN` | Verified sending domain, for example `mail.g5labs.ai`. Required unless disabled |
| `G5_EMAIL_FROM_NAME` | Display name on every message (default `G5`). Quote it in `.env` files if it has spaces: `G5_EMAIL_FROM_NAME='Acme Platform'` |
| `G5_EMAIL_RESEND_API_KEY` | Resend API key (secret). Required for `resend` |
| `G5_EMAIL_RESEND_API_URL` | Default `https://api.resend.com`; stacks point it at the provider stub |
| `G5_EMAIL_VOLUME_LIMITS` | `enforce` (default) or `off`. `off` lifts per-organization limits for local stacks; startup refuses it while `resend` uses Resend's public API |

Links resolve against `G5_EXTERNAL_URL`.

**Sender mailbox.** Each kind of mail names its own mailbox at the sending domain, so
invitations come from `G5 <invitations@mail.g5labs.ai>`. It may also set its own
display name; otherwise `G5_EMAIL_FROM_NAME` is used. The sender is a code constant
checked when G5 is built:

```rust
const APPROVALS: Sender = Sender::new("approvals").with_display_name("G5 Approvals");
// -> G5 Approvals <approvals@mail.g5labs.ai>
```

- **Mailbox name:** 1–32 lowercase letters, digits, `.`, `-` and `_`, starting and
  ending with a letter or digit. Role mailboxes such as `postmaster` and `abuse` are
  refused.
- **Display name:** 1–64 letters, digits, spaces, `.`, `-` and `_`. A kind's own
  display name replaces the operator's `G5_EMAIL_FROM_NAME` for that kind of mail,
  including on self-hosted installs, so leave it unset unless the kind needs it.
- Callers cannot change the domain. Resend accepts any mailbox at a verified domain,
  so a new name needs no provider change. These mailboxes do not receive replies.

Both the API and the daemon need the same values: the API records requests and the
daemon sends them. An enabled transport without its sender or key **fails at
startup**, so mail is never dropped silently.

## Local development and tests

- **Compose dev stack.** The stack (`just dev-up`) sends through the provider stub's
  Resend-compatible API. Read what was "sent" with:

  ```bash
  curl -s http://localhost:9876/__e2e/emails | jq '.emails[] | {to, subject, text}'
  curl -s -X DELETE http://localhost:9876/__e2e/emails   # forget them
  ```

  The stub replays the same id for a repeated `Idempotency-Key` and refuses a reused
  key with a different payload, as Resend does. Set `G5_EMAIL_TRANSPORT=capture` to
  log messages instead; they appear in the delivery worker's logs (the daemon
  container in the dev stack). Set `disabled` to turn email off.
- **Real Resend from a dev stack.** Put the values in the repository-root `.env`
  (ignored by Git, sourced by `just dev-up`), then rerun `just dev-up`:

  ```bash
  G5_EMAIL_RESEND_API_URL=https://api.resend.com   # the stack defaults to the stub
  G5_EMAIL_RESEND_API_KEY=re_...                   # a key for a verified domain
  G5_EMAIL_FROM_DOMAIN=mail.example.com            # that verified domain
  G5_EMAIL_VOLUME_LIMITS=enforce                   # required: `off` is refused against Resend
  ```

  Links in the mail use `G5_EXTERNAL_URL`, which a dev stack leaves at its local edge
  (`http://localhost:3000`, or `:13000` in a workspace preview).
- **Test stacks.** `docker-compose.test.yml` and the CI E2E stack use the same stub.
  `frontend/e2e/flows/invite-members.spec.ts` reads `/__e2e/emails` and follows the
  link through login.
- **Rust tests.** These use the `capture` transport, or the Resend client against
  `wiremock` (`backend/g5-api/tests/email_delivery.rs`).

## Provisioning Resend (QA)

The owner provisions the account. G5 does not hold Resend credentials in Terraform
state. Mail comes from the dedicated subdomain `mail.g5labs.ai`, so transactional
reputation stays separate from company mail. Invitations come from
`G5 <invitations@mail.g5labs.ai>`.

1. Create the Resend team. Add the domain `mail.g5labs.ai` and add the DNS records
   Resend shows (SPF and DKIM; a DMARC record for `g5labs.ai` is recommended). Wait for
   **Verified**.
2. Create an API key with **Sending access** restricted to `mail.g5labs.ai`.
3. The `g5-email-*` Secret Manager containers are created by Terraform with an inert
   bootstrap version: transport `disabled`, sending domain and key blank. Add the real values
   as new versions, the transport **last**:

   ```bash
   printf '%s' 'mail.g5labs.ai' | gcloud secrets versions add g5-email-from-domain --data-file=- --project=g5-qa-1ccc
   gcloud secrets versions add g5-email-resend-api-key --data-file=- --project=g5-qa-1ccc   # paste the key, then Ctrl-D
   printf '%s' 'resend' | gcloud secrets versions add g5-email-transport --data-file=- --project=g5-qa-1ccc
   ```

4. External Secrets polls Secret Manager hourly. Force the `g5-email-secrets` refresh,
   then restart the API and daemon pods so they read it:

   ```bash
   kubectl -n g5 annotate externalsecret g5-email-secrets force-sync="$(date +%s)" --overwrite
   kubectl -n g5 get secret g5-email-secrets -o jsonpath='{.data.G5_EMAIL_TRANSPORT}' | base64 -d   # resend
   kubectl -n g5 rollout restart deployment/g5-api deployment/g5-daemon
   ```
5. **Verify production delivery.**
   - Invite a real address you control to a test organization.
   - Confirm it arrives, then follow its link and log in.
   - Check that its row is `sent`:

     ```sql
     SELECT status, attempts, provider_message_id, terminal_reason, last_error
       FROM email_deliveries ORDER BY created_at DESC LIMIT 5;
     ```

   - Check the message in the Resend dashboard.

To turn email off again, add a `disabled` version of `g5-email-transport`. To rotate
the key, add a new key version, restart the pods, then revoke the old key in Resend.

Self-hosted installs set the same three variables in `.env`; see `env.template`.

## Operating and diagnosing

**Logs.** Search the `g5_api::email` target. Delivery logs carry the delivery id,
purpose (`org_invitation` and the invitation id), organization id, attempt and a
classified error code. With the `resend` transport they never carry the API key,
recipient address or message content. The `capture` transport is the exception: it
exists for tests and local development and logs each message, recipient and text
included, instead of sending it. Do not use it where addresses are real. These log
lines are worth knowing:

- `Email provider refused G5's credentials or sender` (error): the key or the sending
  domain is wrong.
- `Email permanently rejected by provider` (error).
- `Email delivery failed: retry window exhausted` (error).
- `Email delivery lease expired` (warn): a worker died mid-send. Its retry is
  deduplicated by the idempotency key.

**Queries.**

```sql
-- Backlog and failures by state
SELECT status, terminal_reason, outcome_uncertain, count(*)
  FROM email_deliveries GROUP BY 1, 2, 3 ORDER BY 1;

-- Stuck or retrying requests
SELECT id, purpose_key, attempts, last_error, next_attempt_at, first_attempt_at
  FROM email_deliveries WHERE status IN ('queued', 'sending') ORDER BY next_attempt_at;
```

**State.** Statuses are `queued`, `sending`, `sent`, `failed` and `cancelled`. The
terminal reasons are:

- `rejected`
- `retry_window`
- `cancelled:invitation_revoked`, `cancelled:invitation_accepted`,
  `cancelled:invitation_declined` and `cancelled:invitation_expired`
- `unknown_kind`, which appears only after rolling back to an older build

**Retention.** Rendered content is cleared when a request ends. Ended requests are
deleted after 30 days.

**Recovery.** After a credential or DNS fix, queued requests retry on their own within
their backoff. An administrator can press Resend on an invitation whose email failed.

## Adding another email kind

Add these:

- a `Message` variant and its template in the email module, and a `Sender` constant
  for the consumer's mailbox;
- its `PurposeKind`, with a purpose key, optional per-organization and per-recipient
  daily limits, and whether its keys are single-use (a repeated key returns the
  existing request instead of sending again);
- a `DeliveryGate` in the consumer's context, installed in `services::email_gates()`.

Recipient choice, authorization and any preference or opt-out stay with the consumer;
see the preference question on the ontology page. No migration is needed.
