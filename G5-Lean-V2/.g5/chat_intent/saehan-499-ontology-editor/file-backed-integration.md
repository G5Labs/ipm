# File-backed Core Ontology integration — 2026-10-19

The human selected Option 1 after code and ontology research: keep the Core Ontology sidebar, rich editor, Compare, All Changes and `.g5/ontology` visibility in Files; reconcile ontology, finish the rebase onto main, run relevant checks and push with force-with-lease. The preceding history instruction retains four feature commits and consolidates eleven repair commits into one.

The selected design uses workspace/repository/path identity and existing file access. Ontology parsing, rich presentation and bounded comparison remain specialized logic. Projection workers, storage and node APIs remain retired. This amends the earlier removal of the Core Ontology UI without restoring its node model.
