# Organization member deactivation

Source: direct user request, 2026-10-17, workspace
`d3f7643b-472b-4f8b-aa13-0465a89231a4`.
The authorized repository branch was initially `member-removal`, at `52de4512`.

## Human instructions

An administrator currently cannot remove a person who owns a workspace. Introduce
organization-scoped deactivation instead:

- An admin can deactivate another admin, member or viewer; the protected organization
  owner role is outside this change.
- The person cannot enter or use that organization, including through existing
  sessions, CLI credentials or API keys. Other organizations are unaffected.
- Membership, role, workspace ownership and history remain intact. Nothing is deleted
  or reassigned, and existing foreign-key references remain valid.
- Stop their running workspace execution and prevent queued work from starting.
  Shutdown retries must not restore access.
- An admin can explicitly reactivate them. Workspaces stay stopped until deliberately
  started.
- Admins can separately transfer eligible workspaces to another active member.
- Enforce active membership throughout authorization and runtime admission, beyond
  the login screen.

## Design interpretations

The [design](../../pr_ontology_diff_design/member-removal/ontology-plan.md) treats
this as membership activity, independent of global identity and organization role.
Existing owner/admin administrative authority applies; self-deactivation is refused.
Reactivation may restore organization access while prior shutdown is pending, but
retains a durable compute barrier until cleanup completes. Previously issued
credentials can work again after reactivation if otherwise valid; expired or separately
revoked credentials are not revived. Existing transfer eligibility is retained.

This turn prepares the design and ontology. The repository's `AGENTS.md` places
implementation after the user's instruction to implement the design.
