# Credential and mutation authority (issue #660)

## Human instruction

The workspace owner asked (2026-10-22): "Work on G5Labs/G5-Lean#660: [Fix now]
Credential and mutation authority", following the design process before
implementing, verifying, filing a PR and making it ready to ship.

## Source

[G5Labs/G5-Lean#660](https://github.com/G5Labs/G5-Lean/issues/660), opened by the
owner against base commit `df1d099989`. Its scope statement:

> Identity services own credentials and current membership; the scoped database
> capability preserves the grant used by each transactional writer.

> Constrain delegated key creation and revocation, enforce the viewer ceiling,
> protect refresh credentials at rest, serialize OAuth invitation acceptance against
> revocation, and retain audience authority through review/vote commits. Preserve
> valid existing sessions through an explicit migration strategy; do not invent a new
> credential hierarchy unless required by the existing contract.

> The absolute refresh-token lifetime repair in #459 / PR #481 is already complete.
> Protecting stored bearers does not require introducing token rotation, token
> families or reuse-detection policy. For delegated key creation, preserve the
> existing scope, organization and revocation contract; a subset check alone does not
> address child survival after revocation.

> Fix all producers, consumers and sibling paths sharing each root cause. Keep
> unrelated refactors and new product policy outside the change.

Acceptance criteria and the 5,000-line budget are in the issue.

## Interpretation

- "Constrain delegated key creation and revocation" together with "do not invent a
  new credential hierarchy unless required by the existing contract" is read as:
  API-key management stays person-session administration. A delegated key obtains no
  credential at all, which satisfies "cannot obtain absent scopes or credentials
  outside its own lifetime/revocation boundary" without a parent/child key lineage.
  The bounded-child alternative is recorded and rejected in the design.
- "Address already-issued independent children explicitly": no stored fact
  attributes an existing key to the credential that created it, so the design
  records that disposition rather than inventing one.
- The viewer ceiling applies to additive grants for the viewer role only; members and
  administrators keep configured additive grants pending the open
  grant-lifecycle question.
- Replies, reader file suggestions, review rounds and votes remain read-scoped
  collaboration (the open viewer-collaboration question is not decided here).
