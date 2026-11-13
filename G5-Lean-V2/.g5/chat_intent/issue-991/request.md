# Issue #991 — email reviewers when their approval is requested

Source issue: [G5Labs/G5-Lean#991](https://github.com/G5Labs/G5-Lean/issues/991) (primary).

## Human instructions

- Email the human reviewer when their approval is requested (web composer or
  `g5d workspace request-approval --reviewer`). One email per *newly admitted*
  request; a retried/`reused` admission sends nothing more.
- Filterable subject with a fixed prefix `[G5 Approval] <workspace>: <definition>
  requested by <owner>`, from a dedicated `G5 Approvals <approvals@…>` sender.
- Body links to the request in the web app and names who asked, for which
  workspace and what is being approved.
- Server-side per-user setting "Email me when someone requests my approval",
  default **on**, shown in Account settings, edited through `PATCH /users/me`.
  The service checks it before queuing; the delivery gate re-checks it.
- The requester is never emailed for their own request. Deployments without
  email still create requests normally. A per-recipient/org daily limit is
  defined for the kind.

## Owner decisions (2026-10-30, this chat)

Settled through an in-chat question set:

1. **Delivery durability.** The literal "same transaction as the request" is not
   achievable: approval requests are written on the tenant DB pool, the email
   queue (`email_deliveries`) is writable only by the system pool, and the
   approval tables are tenant-insert-only (so the request cannot move onto the
   system pool the way org invitations do). Owner chose **best-effort
   post-commit queuing**: after the request transaction commits, the email is
   queued in a short system-pool transaction, idempotent on the request id so
   retries/reuse never double-send, with the delivery gate still cancelling a
   request that is no longer pending. The Home attention list stays the durable
   source of truth; a crash in the narrow post-commit window loses only the
   email, not the request.
2. **Setting scope.** **Account-wide** (one boolean on the user row), not
   per-organization. Follows the issue recommendation over CI-G4's per-org lean.
3. **One-click `List-Unsubscribe`.** **Deferred** to a follow-up. This change
   ships the toggle in Account settings; the unsubscribe header and its signed
   unauthenticated endpoint are out of scope here.

## Owner follow-up (2026-10-31, preview check)

After confirming in the preview that one email was sent and an opted-out
reviewer's second request sent none: "fix the footer wording issue. And
improve the email body to include the review request message (if any), along
with some reasonable amount of the PR description". The footer had pointed
at "account settings", but the toggle now lives in the Settings page's
Notifications section.

## Owner follow-up (2026-10-31, Markdown description)

After reviewing a sample email in which the quoted PR description showed its
raw Markdown: "go ahead - just render the PR description in markdown". The
requester's note stays plain text.

## Owner follow-up (2026-11-02, security review)

The Ontology Alignment approval was rejected by a human reviewer: "there is a
security risk, worthy evaluate the minimal mitigation". The agent reviewer had
flagged the migration's `SECURITY DEFINER` lookup, which gave the `BYPASSRLS`
definer role a lasting table-wide `SELECT` on `approval_requests` (reversing
migration 102's "no runtime grants remain") so the delivery gate could check
that the request was still pending. Offered a column-limited grant or no grant
at all, the owner chose: "drop the 'still pending' check from the gate and keep
only the preference and membership checks ... I'm not concerned about a tiny
race in email sending if it buys a simple security story". This supersedes the
delivery-time pending check in decision 1; the queue-time check remains.

## Owner follow-up (2026-11-02, decision email)

"It would be good to also send email to the owner of the workspace when a human
approval occurs." Asked three questions, the owner chose:

- **Outcomes:** email on both approve and request changes (a withdrawal never
  emails).
- **Preference:** reuse the existing toggle rather than add a second one.
- **Delivery:** add it to PR #997 rather than a follow-up PR.

## Interpretation (agent)

- Per-recipient/org daily limits mirror the owner-email kind (500 per org, 50
  per recipient) — an owner controls both recipients and volume.
- Eligibility re-checked at delivery = recipient is still an active member and
  their current account email still matches the queued recipient, alongside the
  setting-on check (the pending-request check was dropped on 2026-11-02, above).
- Decision email (2026-11-02): the recipient is the workspace's *current* owner;
  the owner's own decision (self approval) is not emailed; the reviewer's comment
  is quoted; it reuses the request email's sender, subject prefix, limits and
  recipient-only delivery check, and keys on the settled request with a
  single-use key so a replayed decision never resends.
