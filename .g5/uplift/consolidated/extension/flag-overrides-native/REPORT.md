# Native flag override correspondence

The shipped consumer confirms that an explicitly false inherited flag differs from an absent flag. This validates the existing Verus flags and transaction-entry model; it adds no member credit and does not prove an ObjectScript body.

Product source: IPM `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`; all six relevant source files match their pinned content (`results/20261114T160126Z/source-audit.json`). Native platform: IRIS 2026.1 build 234U, ARM64; namespace USER in the existing `ipm-consolidated-iris` snapshot. The container was not reset, and no product class was changed.

Reproduce with `./.g5/uplift/consolidated/extension/flag-overrides-native/run.sh`. The driver invokes `/usr/bin/python3` (3.9.6), copies uniquely named fixture modules and an outside-loaded helper, then calls the real `%IPM.Main.Shell("load ...")`. Preflight refuses any collision with the helper, marker global or twelve module names. The wrapper removes its helper afterward. The immutable initial receipt is under `results/20261114T160126Z/`, with `raw.log`, `script.cos`, `receipt.json`, `checks.json`, `preflight.cos`, `preflight.log`, and separate helper cleanup evidence. Subsequent runs get new timestamped directories.

## Source-derived inputs

`Utils.Module.LoadNewModule` lines 1164–1172 obtains defaults from the module and merges `pParams` over them. It then opens its own transaction only if neither effective flag is true; `NoJournal=1` instead disables the process journal flag at lines 1191–1197. `Storage.Module.GetDefaultParameters` lines 799–832 only reads `ModuleSetting.Parameter`, so the fixtures use the actual `<Defaults><Parameter Name="NoJournal">1</Parameter></Defaults>` syntax (and likewise NoTransaction), rather than a `<Default>` element. `Storage.InvokeReference` dispatches the explicit Activate hook and checks its returned `%Status`.

The child helper only reads journaling via `'($zboolean($zjob,1024,1)=1024)`, exactly the `JournalManager.Enabled` formula. It never creates a JournalManager, changes the flag, or starts a transaction. Each child writes a private global marker and reports the ambient transaction depth. The root helper reports the restored flag, writes its own marker, and deliberately returns a named error in the four failure cases. Both helpers are compiled before any module load, so a database rollback cannot make the observer disappear.

## Checked observations

Every load starts and returns at transaction level 0 with journaling enabled. During each root's Activate, the parent remains at transaction level 1 with journaling enabled. All four failing commands return the deliberately injected parent Activate error.

| Case | Child default | Parent command key | Child depth | Child journal | Root stored | Child stored | Child marker stored |
|---|---|---|---:|---:|---:|---:|---:|
| JA | NoJournal=1 | absent | 1 | 0 | 0 | 1 | 1 |
| J0 | NoJournal=1 | `-DNoJournal=0` | 2 | 1 | 0 | 0 | 0 |
| TA | NoTransaction=1 | absent | 1 | 1 | 0 | 0 | 0 |
| T0 | NoTransaction=1 | `-DNoTransaction=0` | 2 | 1 | 0 | 0 | 0 |
| JS | NoJournal=1 | absent, successful root | 1 | 0 | 1 | 1 | 1 |
| TS | NoTransaction=1 | absent, successful root | 1 | 1 | 1 | 1 | 1 |

`checks.json` checks 15 named fields in each case, with no mismatches. In the failure cases, the root marker is removed. In the two successful neighbors, both module records and both markers remain until cleanup. The two NoTransaction cases demonstrate that skipping a child's own transaction does not cancel its parent's already open transaction. The NoJournal pair additionally demonstrates the surviving-write distinction at the real parent rollback boundary.

## Existing formal correspondence

In `.g5/Verus_verification/consolidated/verus/loadtx.rs`:

- `merge_key`, `effective`, and `explicit_false_parent_overrides_child` express absence versus explicit false.
- `own_transaction`, `entry_depth`, and `no_transaction_child_does_not_cancel_parent` express the observed child depths.
- `nojournal_survives_parent_rollback` and `journaled_child_cannot_survive_uncommitted_parent` express the JA/J0 surviving-write distinction on the model's stated write domain.

The previous Python `MODES` presentation cannot directly encode an explicit false key. These native observations check that missing premise at the real CLI-to-defaults-to-dependency consumer. They do not extend the model to arbitrary concurrent rollback histories or claim complete flag-policy intent.

## Preservation and remaining effects

After recording the results, cleanup uninstalls only the five surviving, uniquely named fixture modules and kills only the private marker global. The before/after snapshot of every foreign module's ID/name/version and repository's ID/name/enabled state has the same eight rows, with no additions or removals. Separate helper cleanup confirms the helper class and marker global are absent. The product's own audit records for these loads and uninstalls remain; repository/module ID counters may advance. No foreign repository, module or fixture is removed, and staged/untracked checkout work is unchanged outside this new directory.

The checks establish these six matched cases, the native journaling premise and the transaction-boundary distinction. They do not settle whether child defaults should override inherited parent settings as policy.
