# Ontology amendments

## Source and scope

Apply the 57 items in the owner's supplied
[ontology-changes.md](g5://file/docs/1c6941cc-a49f-462e-a7ae-87c94d2923bb/ontology-changes.md).
The owner asked to work on these proposed ontology changes on 2026-10-29. The
original document is preserved unchanged in docs commit `dd62a7f`; its item and
question identifiers are cited from the affected statements' metadata.

This change updates the ontology and its evidence descriptions. It does not claim
that the application already satisfies every amendment. Historical model results
keep their original scope and limits; no new TLA+/TLC work was requested or run.

## Governing decisions

Use the existing statement owners and preserve their IDs. Five questions become
decisions: [additive grants](../../ontology/features/organizations.g5.md#g5.organization.grant-lifecycle-question),
[existing-account invitations](../../ontology/features/organizations.g5.md#g5.organization.existing-user-invitation-question),
[viewer participation](../../ontology/features/organizations.g5.md#g5.organization.viewer-collaboration-question),
[Base branch creation](../../ontology/software_architecture/git.g5.md#g5.git.base-create-exception)
and [logical VM uniqueness](../../ontology/software_architecture/workspace_vm.g5.md#g5.vm.physical-uniqueness-question).
Their historical `-question` IDs remain valid for existing references. Their
former uncertainty and contradictory qualifications are removed from current
metadata, while the original evidence remains linked.

The other amendments retain the proposal's ownership and failure boundaries:
delivery evidence belongs to its reservation; cancellation preserves the actual
outcome; membership removal revokes keys and audience grants atomically; accepted
merges retain the admitted repository versions and preserve the live target;
forced host removal records VM loss with logical custody release. No additional
lifecycle or store is introduced. The actual diff supplies the complete set of
amended commitments instead of duplicating them here.

The broader rules now name the exceptions they depend on: viewers can discuss
and suggest, existing accounts join on invitation, Base's sole provider write
requires administrator confirmation, and VM uniqueness concerns authority. Chat
Stop distinguishes ordinary turns from held platform work. Item 54's assertion
that command and preview tasks are "short" is omitted: a preview is already a
[long-running service](../../ontology/software_architecture/workspace_vm.g5.md#g5.preview.service).
Its nonresumable failure outcome is retained. Item 2's "left of unknown delivery"
is corrected to "left with unknown delivery" without changing its meaning.
Agent profiles link to the [installed guidance](../../ontology/features/ontology_and_harness.g5.md#g5.harness.guidance-delivery).
Quota refusal is distinguished from a later report-validation refusal, as item 17
requires. Merge recovery retains any admitted administrator override and the
parent's provider-target serialization. Session refresh retains the parent's
host-bound cookie issuance and unambiguous cookie selection alongside the
memory-only access-token rule.

## Open scope and implementation limits

[Spend scope](../../ontology/features/chat.g5.md#g5.chat.spend-scope-question)
remains open; offered thresholds must have enforcement meanwhile.
[Telemetry retention](../../ontology/features/operations.g5.md#g5.platform.client-telemetry.retention-question)
remains a question with no invented period. Logical VM uniqueness remains
conditional on [independent cleanup progress](../../ontology/software_architecture/hosts.g5.md#g5.runtime.cleanup.progress),
and never certifies physical termination of a partitioned VM.

Targeted source inspection found follow-up work; this is not a full implementation
audit. In particular:

- [Click labels](../../../frontend/src/lib/telemetry.ts) still fall back to visible
  control text, contrary to item 18.
- [Organization role changes](../../../backend/g5-api-core/src/services/identity/org_service.rs)
  reject promotion to owner and changing an owner's role, so item 24 needs an
  ownership operation with concurrent last-owner protection.
- [Workspace settings](../../../backend/g5-api-core/src/services/workspace/workspace_service.rs)
  accept the three spend thresholds, while
  [spend admission checking](../../../backend/g5-api-core/src/services/identity/spend_service.rs)
  has no dispatch caller. Hiding UI fields alone cannot establish the promised
  enforcement for settings accepted by the API.

The existing [task outcome owner](../../../backend/g5-api-core/src/services/agents/task_lifecycle.rs)
supports the proposed successful/failed outcome precedence, the unknown-delivery
cancel exception and nonresumable command/preview failures. This inspection does
not certify their integration or all races. Existing code-gap records and runtime
qualification limits remain applicable unless the amended policy explicitly
settles their underlying question.

## Validation

All 57 proposals were checked against their final owning statements, including
the documented wording and reference corrections. All pre-existing statement IDs
are retained, and the five resolved questions have their requested placement.
The repository's structural/local-link checker passes across all 25 ontology
pages and 830 statements with zero issues. The design's local links and
`git diff --check` also pass.

Three metadata ordering defects already present in the touched security,
organization and UI pages are repaired so evidence follows statement order.
Application behavior is unchanged; no runtime tests or new model checks were run.
