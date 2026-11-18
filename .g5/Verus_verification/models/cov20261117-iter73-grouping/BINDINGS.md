# grouping · coverage iter73

generate resource grouping, UnitTest entries, the module.xml one-entry-per-name collapse and which directory wins it (C:D72), generate -export file placement and the export-then-load round trip (C:D73), and the module.xml backup. General theorems over every file sequence; native m1-m8 and x1-x4, 139 fields.

- Groups are keyed by directory and top-level package, and named by the deepest package their classes share. Files under the tests folder, unsupported extensions and unlisted files never become resources. Proved for every file sequence (`group_key_separates`, `group_key_joins`, `group_characterized`, `tests_folder_files_never_written`, `unsupported_never_written`, `unlisted_never_written`). Native m1, m3, m4, m5.
- When no two directories produce the same group name and files sit in package folders, every class is written and found by `load` (`class_loads_when_no_clash`, `layout_loads`).
- UnitTest entries carry the tests folder plus directory as Name, Package, Phase "test" and the UnitTest processor (`test_class_has_unittest_entry`). Native m1, m4, read back after `load`.
- The writer keeps one entry per name. The kept one is the last in `$order` of the `pkg:/<directory>:<package>` keys (`collapse_earlier_key_loses`), so one directory's classes are lost (C:D72). Native m3, m7, m8.
- `generate -export` writes classes into package folders and routines and include files flat (`export_layout`). A dotted routine or include file then fails `load` (`export_dotted_item_fails_load`, C:D73). Native x1-x4.
- An existing module.xml is renamed before the new one is written (`save_keeps_previous`). Native m8.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declarations](../../coverage-20261117/iter73/grouping.rs)

[Native instances](../../coverage-20261117/iter73/instances.rs)

[Product correspondence](../../../uplift/coverage-20261117/iter73/bindings.json)

[Proof and controls](../../../uplift/coverage-20261117/iter73/proofs/models.json)

[Iteration handoff](../../../uplift/coverage-20261117/iter73/HANDOFF.md)
