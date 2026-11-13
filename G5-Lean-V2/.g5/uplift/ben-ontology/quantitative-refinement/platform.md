# Platform refinement

P01–P03 now have dedicated governing owners instead of nearest-owner candidates:
`g5.ui.home-attention`, `g5.documents.explainer-catalog`, and
`g5.documents.explainer.templates`. Their status changes from missing to adequate
intent for the audited behavior; this is not a full-code enforcement claim.

The [fresh campaign](https://github.com/G5Labs/G5-Lean/blob/345b546e9fed5477e6fdf3c77ad87002ed558dd6/.g5/TLA_verification/ben-quantitative/platform/README.md)
checks the three commitments using two concern models. Important refinement: Home
uses captured audience, with no instantaneous revocation claim; a saved template is
live until ordinary publication despite stale route comments saying “commit.”
Replacement consent names the template, while shared file-mutation ownership retains
the known native hash-check/rename limitation. No additional template-specific lock,
publication ledger or decision store is justified by these requirements.

Current source bindings add `backend/g5d-dispatch/src/files.rs:574–679` to the original
P01–P03 reads. The check/rename counterexample is a fresh finite confirmation of the
previous native-writer limitation, not a runtime reproduction or a new gap count.
Candidate minimal design uses the existing committed catalog, current owner live-file
mutation and normal flush. For atomic version preservation, every competing writer
must participate in the same exclusion or stronger filesystem protocol; simply adding
a template-level lock cannot establish the required guarantee.

Remaining partial enforcement: Home candidate producers/cursor consistency/revocation
and publication/storage contracts. No runtime test coverage added. Existing P04–P07
statuses and historical evidence are unchanged.
