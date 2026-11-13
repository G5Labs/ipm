# Outbound email with invitation-created delivery (issue #621)

## Human instruction

The workspace owner asked (2026-10-17): "Work on G5Labs/G5-Lean#621: Add a scoped
outbound email module with invitation-created delivery".

## Source

[G5Labs/G5-Lean#621](https://github.com/G5Labs/G5-Lean/issues/621), opened by the
owner. Requirements quoted from it:

> Provide a small application-facing interface for requesting transactional email
> delivery. Keep provider SDKs, credentials, sender configuration, transport errors,
> and delivery mechanics inside the module.

> Start with one configured delivery provider and a fake/capture transport for tests
> and local development. Choose the provider during implementation design; do not
> build multi-provider routing.

> Keep invitation authorization, recipient selection, lifecycle, and acceptance rules
> in the invitation domain. That domain supplies the invitation message data; the
> email module owns rendering and delivery.

> Define a reliable handoff from committed application state to delivery. Evaluate
> reuse of existing delivery infrastructure before adding a new queue or worker.
> Avoid sending for rolled-back invitations or losing requests during process restarts.

> Handle transient failures with bounded retries/backoff, distinguish permanent
> failures, and expose enough status/logging to diagnose failure without logging
> credentials or invitation secrets. Define duplicate suppression and the limits of
> delivery guarantees when provider outcomes are ambiguous.

> Request delivery when a new pending invitation is committed, addressed to its invitee.
> Include organization and inviter context and a link into the existing authenticated
> invitation acceptance flow. Preserve current authorization, expiry, and acceptance
> behavior; the email must not itself grant access.

> Provider outages must not roll back an otherwise valid invitation. Retain a
> recoverable delivery request and make terminal failures observable.

> Define how queued delivery handles invitations revoked, expired, or accepted before
> sending, and how the existing resend operation interacts with delivery identity and
> duplicate suppression.

> Preserve immediate membership behavior for existing users. Membership-added
> notifications are outside this initial slice.

Acceptance criteria and exclusions are in the issue; out of scope: inbound email,
marketing, bulk mail, a general notification framework, user-managed templates,
attachments, multi-provider failover and bounce/complaint analytics. "Any
provider-required suppression or compliance behavior remains part of making the
initial integration operational."

## Interpretation

- The provider choice, resend contract and failure visibility are delegated to the
  design by the issue; the design records them as agent decisions.
- "Existing authenticated invitation acceptance flow": there is no acceptance page and
  no invitation token. Pending invitations are accepted when the invitee completes an
  OAuth login with a matching verified email
  ([invites](../../ontology/features/organizations.g5.md#g5.organization.invites)). The
  link therefore leads to the organization's route, which requires that login.
- "Production delivery is verified with the chosen provider" needs a provider account,
  a verified sending domain and an API key that only a person can provision; the
  deployed environment is QA (`qa.g5labs.ai`).
- Owner decisions still needed (provider confirmation, sender domain and provisioning,
  abuse cap) are in the workspace question set `outbound-email`.

## Follow-up

After reading the design, the owner said (2026-10-17):

> I plan on re-using the email delivery mechanism for a variety of things that are not
> related to identity / invites.

Interpretation: email is a shared platform mechanism rather than part of the identity
context. Later message kinds must be added without touching invitation code or the
delivery schema, and the module must not depend on any consumer. The future kinds are
not named. Questions they raise (recipient preferences, unsubscribe, per-kind limits)
are left to their own designs, with the shared rule recorded as
`g5.email.preference-question`.

## Owner decisions

On 2026-10-17 the owner answered the `outbound-email` question set (each answer
`decision: true`): provider Resend; sender `G5 <invitations@mail.g5labs.ai>` on a
dedicated subdomain, with the owner provisioning the account, DNS records and key;
and a cap of 100 invitation emails per organization per 24 hours. The owner then
instructed: "implement it".

## Sender mailbox

On 2026-10-18 the owner asked:

> Can we update the API to allow the calling module to specify (within reason) the
> username that the email comes from? eg invitations uses 'invitations' which comes
> from 'invitations <invitations@mail.g5labs.ai>', but approval requests could use
> 'approvals <approvals@mail.g5labs.ai>'

Interpretation: the caller chooses only the mailbox name (the part before `@`). The
sending domain stays configured, so no caller can send from an unverified domain.
The display name stays configured as `G5`, consistent with the earlier sender answer
`G5 <invitations@mail.g5labs.ai>`. The example's `invitations <…>` is read as
shorthand for the address. "Within reason" is read as a build-time-checked constant
with a conservative character set and no RFC 2142 role mailboxes.

Asked whether each caller should also set its own display name, the owner answered
(2026-10-18): "caller can set its own display name too". A caller may therefore set
a display name, checked at build time; without one the configured `G5_EMAIL_FROM_NAME`
is used. Invitations keep `G5`.

## Local cap and owner email command

On 2026-10-19 the owner asked:

> lift the 100 per org per day cap for local dev.
> Also design a 'g5d' cli command for sending email to the workspace owner.

Interpretation: "local dev" is the compose dev stack, whose mail goes to the provider
stub. Test stacks and CI keep the cap so its behavior stays tested, and deployed
environments must not be able to drop it by mistake. The `g5d` command is a design
request only; its design is in the [plan](../../pr_ontology_diff_design/issue-621/ontology-plan.md#g5d-owner-email).

After reading the design, the owner said (2026-10-19): "both of your recommendations
look fine. build it". That accepts 50 per owner and 500 per organization per day,
and no opt-out for agent-to-owner email.
