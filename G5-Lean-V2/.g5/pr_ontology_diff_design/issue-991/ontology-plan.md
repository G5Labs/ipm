# Issue #991 — approval-request reviewer email

Design for emailing a human reviewer when their approval is requested, with an
account-wide per-user opt-out. Settles the approval-request case of
[`g5.email.preference-question`](../../ontology/software_architecture/email.g5.md#g5.email.preference-question);
extends [`g5.user.notifications`](../../ontology/features/users.g5.md#g5.user.notifications)
(gap CI-G4) and [`g5.governance.approval.request`](../../ontology/features/reviews.g5.md#g5.governance.approval.request).
Owner decisions and the human request are in
[`../../chat_intent/issue-991/request.md`](../../chat_intent/issue-991/request.md).

## Owners and why

- **Approvals is the email consumer.** Per
  [`g5.email.consumers`](../../ontology/software_architecture/email.g5.md#g5.email.consumers)
  the consumer owns recipient selection, the per-recipient opt-out, and the
  decision that a message is warranted. The email module stays unaware of
  approvals: a new `PurposeKind::ApprovalRequest`, a `Message::ApprovalRequest`
  variant with its template, the `approvals` sender, and a `DeliveryGate`
  installed at the composition root. No module-boundary change.
- **The user account owns the preference** (`g5.user.notifications`): one boolean
  `users.email_on_approval_request`, default true, read by the approvals
  consumer before queuing and re-checked by the gate. Account-wide, so one row
  answers "may I email this person about approval requests" everywhere, and a
  later unsubscribe link has a single target. This closes the approval-request
  slice of CI-G4; a general notification-preference framework stays open.

## Why best-effort, not atomic (the one real tension)

[`g5.email.handoff`](../../ontology/software_architecture/email.g5.md#g5.email.handoff)
lets a consumer record the email in its own transaction so a committed change
never loses its mail. Org invitations get this by running the whole invite on
the **system** pool. Approvals cannot: `create_human_requests` runs on the
**tenant** pool (`g5_tenant`), `email_deliveries` is denied to that role, and the
approval tables grant INSERT/UPDATE only to `g5_tenant` — so neither the request
nor the email can cross to the other pool inside one transaction. Granting
`g5_app` write on the approval tables, or `g5_tenant` write on the email queue,
would dissolve the tenant/system split in
[`g5.security.tenant.rls`](../../ontology/concepts/security.g5.md#g5.security.tenant.rls);
a second tenant-owned email outbox would duplicate the queue the email module
already is.

Chosen (owner-approved): the request transaction commits first, then the email
is queued in a short **system-pool** transaction. This is sound because:

- **Exactly once / never on reuse.** The purpose key names the request (a lasting
  object, not a single-use key) as `<request id>:<org id>:<reviewer id>`. We queue only for admissions the service
  reports as *newly admitted* (`reused == false`). A re-POST that reuses a
  pending request returns `reused == true` and queues nothing; and because the
  key is the request id, any accidental re-queue coalesces instead of sending a
  second message.
- **No spurious mail.** Mail is queued only after the request commits and only
  while it is pending. The delivery gate does not re-read the request (owner
  decision, 2026-11-02), so a request decided in the seconds before delivery
  may still be emailed; its link shows the settled state.
- **Degradation.** A crash between the two commits loses only the email; the
  request exists and the pull-based Home attention list still surfaces it, which
  is the whole pre-#991 behavior. Queuing failures (disabled email, volume
  limit) never fail the already-committed request — matching "deployments
  without email still create requests normally."

Email is therefore explicitly **not** the system of record for an approval
request; it is a best-effort latency reducer over the Home list.

## Operation sketch

Both callers (`POST …/approvals/requests` and the agent
`request-approval` route) already funnel through `create_human_requests_with`,
which commits the tenant transaction. Immediately after that commit it opens a
system-pool org-scoped `ScopedDb` and calls a new consumer function
`request_approval_request_emails(email, &mut db, org_id, source_workspace_id,
requested_by, &admissions)`, then commits. For each admission with
`reused == false` and `actor_user_id != requested_by` it, in that one
transaction:

1. reads the request row + source workspace name + org name/slug + requester
   display name + recipient's current email and `email_on_approval_request`
   (RLS is satisfied by the org scope);
2. skips the recipient when the request is not pending, the setting is off, or
   the recipient is not an active member;
3. otherwise calls `email.request(…)` with `Sender::new("approvals")
   .with_display_name("G5 Approvals")`, `Message::ApprovalRequest{…}`, purpose
   key = request id.

The **gate** runs on the worker's system pool with no org context. It takes the
organization and reviewer from the purpose key and reads only `users` and
`org_memberships` (no RLS on either), returning `Wanted` only when the
recipient's current email equals the queued recipient, the setting is on and the
member is active; otherwise `Cancelled("setting_off" | "recipient_changed" |
"ineligible" | "recipient_gone")`. It never reads `approval_requests`, so the
change adds no definer lookup or grant on tenant approval tables. (Superseded
draft: a `SECURITY DEFINER` lookup with a table-wide definer `SELECT` on
`approval_requests` to also cancel settled requests; dropped on 2026-11-02
after a security review, by owner decision.)

## Volume limits

A new kind on the shared sending reputation, with recipients and volume the
owner controls, so per
[`g5.email.volume-limits`](../../ontology/software_architecture/email.g5.md#g5.email.volume-limits)
it carries a per-organization limit (500 / 24h) and a per-recipient limit
(50 / 24h), mirroring owner email. Not a single-use key.

## Ontology amendments

- `g5.user.notifications` — gains the account-wide approval-request email
  preference (default on), consumer-checked and gate-rechecked; names CI-G4 as
  partly closed.
- `g5.governance.approval.request` — a new sibling `…request-email` commitment:
  creating a human request best-effort-emails the reviewer after commit, with
  the fixed subject prefix and approvals sender, honoring the account
  preference, never the requester, email not the system of record.
- `g5.email.preference-question` — metadata records the approval-request case as
  settled (preference on the user account, consumer-checked, account-wide, no
  unsubscribe header yet); the general question stays open.
- `g5.email.volume-limits` — metadata notes the approval-request limits.

## Decision email (2026-11-02 follow-up)

[Owner follow-up](../../chat_intent/issue-991/request.md#owner-follow-up-2026-11-02-decision-email):
also email the workspace owner when a human reviewer approves or requests
changes, under the same preference, in this PR. Governing block:
[`g5.governance.approval.decision-email`](../../ontology/features/reviews.g5.md#g5.governance.approval.decision-email).

- **Owners.** The approvals service still owns the decision and its one-time
  settlement of the request; the email module owns delivery; the decision email
  consumer (beside the request email's) owns *whether* a decision warrants mail.
  The route only says which request it settled; the consumer reads the settling
  human decision back on the system-pool org scope (`g5_app` may read
  `approval_statuses` and `approval_requests` under RLS), so callers cannot
  misstate the outcome, reviewer or owner.
- **Reuse over new state.** No new table, column or lifecycle. The existing
  preference column (`users.email_on_approval_request`, unmerged name kept) now
  governs both kinds; the Settings row "Approval requests" says so and both
  footers name it. The delivery gate is shared: both kinds key on
  `<request>:<org>:<recipient user>`, so the same recipient-only check (active
  member, unchanged address, preference on) applies, and the
  [2026-11-02 security decision](../../chat_intent/issue-991/request.md#owner-follow-up-2026-11-02-security-review)
  holds for both — the system worker never reads tenant approval or workspace
  records.
- **Once per request.** A request settles once, and the service's replay path
  returns without telling the route whether it settled now. Rather than widen
  the service result, the new kind declares single-use keys
  ([`g5.email.identity`](../../ontology/software_architecture/email.g5.md#g5.email.identity)):
  a replay returns the existing mail, sent or not, and a replay after a lost
  queue attempt re-tries it.
- **Accepted race.** Ownership is read when the mail is queued; a transfer in
  the moments before delivery may still email the previous owner (who must still
  be an active member). Same trade the owner accepted for the request email.
- **Not emailed.** Withdrawals (`deferred`), agent decisions (agents settle no
  human request) and the owner deciding a request addressed to themselves.
- **Limits.** 500 per organization and 50 per recipient in 24 hours, mirroring
  the request email; decisions are bounded by requests already limited.

## Tests

`new request sends once`, `reuse sends nothing`, `setting off skips`,
`gate cancels before delivery when the setting is flipped off / the reviewer leaves the organization`, `a request settled before delivery is still emailed`,
`email-disabled deployment still creates the request`, `requester is never
emailed for their own request`, plus render coverage of the fixed subject and
approvals sender, and `PATCH /users/me` round-trips the boolean with default on.
See [the testing reference](../../../.agents/skills/ontology-planning/references/implementation-testing.md).

Decision email: `email::render::tests::approval_decision_names_the_outcome_and_links_the_approvals`
and `approval_request_footer_names_the_shared_setting`; integration
`each_decision_emails_the_owner_once` (approve + request changes, replay before
and after delivery), `a_withdrawal_sends_nothing`,
`the_owners_own_decision_sends_nothing`,
`the_owners_preference_governs_decision_emails` (queue-time skip and gate
cancel), `the_gate_cancels_when_the_owners_email_changes`,
`email_disabled_still_records_the_decision`; `system_scope_binding_guard`
justifies the new post-commit system-scope step.
