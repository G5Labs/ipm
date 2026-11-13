# Historical ontology migration fixture

`cli-implementation-map.semlift.md` is copied unchanged from the original G5
reference checkout at `.semlift/specs/software_architecture/g5d/cli/implementation_map.semlift.md`.
It is historical test data, not the current Lean CLI specification.

`backend/g5d/tests/ontology_v4_read_ontology_e2e.rs` copies it into an isolated
workspace and runs the actual CLI measurement and dry-run migration commands.
The test verifies block/token measurements, reference preservation, lint results,
and zero source, CRDT, Git, intent, or grounding writes.
