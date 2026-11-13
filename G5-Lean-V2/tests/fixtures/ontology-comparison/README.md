# Ontology statement comparison conformance corpus

One rule, two implementations
([comparison owner](../../../.g5/ontology/features/documents.g5.md#g5.documents.ontology.comparison-owner)):
the editor's `frontend/src/lib/ontologyComparison.ts` and the server's
`g5_semantic::ontology_comparison` must both pass every case unchanged.

Each directory holds `before.g5.md` and/or `after.g5.md` (an absent file is an
absent version) and `expected.json`:

- `before` / `after`: the statements of that version in source order (`id`,
  `kind`, `human`), or `{"error": ...}` with the first refusal diagnostic;
- `changes`: Added, Modified and Deleted statements sorted by ID; or `error`
  when the comparison is refused.
