# Owners outside the load transaction

Product pin: `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`. The model imports `semver.rs`; the verification totals include those imported checks. `owners.manifest.json` binds the actual members and owning ontology IDs. This is a translated state machine with finite native correspondence, not a verified ObjectScript body.

`module_version` separates the stored module record, stored manifest and file on disk. Main owns `set_record`; the `update_version` callee changes only the stored manifest and preserves the record and disk. Its admission order includes module/stream availability, lexical version validation, modifiers, the force/strictly-higher guard, the unchanged branch, and checkout/save/rewrite failures. The transaction returns to the caller's initial depth and failure preserves the prior state. The version comparison is the default `SemVerPostRelease=false` branch. The CLI argument parser, source-control hook implementation and XML encoding are boundary contracts. In particular the disk-preservation theorem describes the IPM writer; a source-control hook may export independently. Whether the command should update the original file is still C:Q4.

`build_cache` takes purge, row existence, another lock holder, caller transaction depth, enumeration/manifest outcomes and save status as inputs. Only an existing purged repository acquires the modeled object lock. It preserves old rows on failure and can preserve an outer lock after its local commit. `scan` represents the directory depth boundary. `add_cache` distinguishes unreadable manifests from invalid-name save failures. The filesystem/decoder and IRIS nested transaction/lock lifetime are environment contracts, not proven platform implementations. C:Q5 remains a policy question.

`lock_class_id`, `unlock_class_id` and `unlock_all` operate on held object identities. They do not justify restoring a `LockManager.%OnNew` credit. The real-transaction LK5 receipt demonstrates the lock remains held inside a caller transaction; it does not distinguish constructor release modes. That item stays open.

`zlang_one` scans tags and replaces generated blocks while copying outside lines. Its input distinguishes generated and user content. Native cases check one and repeated updates with exact surviving user identity, plus test-only behavior in the Python oracle. A universal inductive user-line preservation theorem for arbitrary raw routines remains open; the current checks establish the concrete native cases and the input-driven specification.

The existing P `settings::get_value` represents configuration selection, not the filesystem cache writer. P `load_general::step_guard` and G's load machine represent a different transaction owner. Reusing those symbols would lose this writer's record/manifest/disk distinction, lock scope, scan result or routine content.

Run from the IPM root:

```
python3 .g5/Verus_verification/consolidated/verus/owners_check.py
```

The checker tests typed command transitions from each observed prestate to the actual poststate. It records Verus commands, stdout/stderr, native receipts and the complete check input hashes in `owners-results.json`; generated native assertions remain in `owners_native.rs`. The five controls change disk writes, guard ordering, invalid-name behavior, scan depth and user-line preservation. Only logical failures count as rejected.
