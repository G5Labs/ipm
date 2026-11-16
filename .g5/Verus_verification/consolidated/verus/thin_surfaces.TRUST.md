# Thin boundary contracts

`thin_surfaces.rs` is a hand translation at IPM `571e04a1`, not proof of ObjectScript bodies.
The check is `python3 .g5/Verus_verification/consolidated/verus/check_thin_surfaces.py`.
It verifies the input-driven functions, executes them against held-out native observations,
and verifies wrong variants fail. Wrong executables are compiled only with `--no-verify`
for comparison; these are controls and never proof evidence.

The source-only independent predictions were saved before inspecting consolidated model
predictions in `extension/independent-source-cases.json` and
`extension/independent-additional-source-cases.json`. Every native receipt records inputs.
All native runs use ARM64 IRIS 2026.1 build 234U. Loopback HTTP services supply actual
HTTP status/header responses to the unmodified ORAS adapter; they are environment fixtures.
No shared instance restart or CPF merge was performed.

## Boundary premises

- Paths are already normalized and disjoint. FileCopy/Utils.File reject overlap, but path
  resolution, aliases, symlinks, normalization, directory-chain details, concurrent filesystem
  writers, permissions and the full contents of partial copies remain environment inputs.
- `copy_step` permits partial target changes on failure, and preserves foreign paths only
  under the normalized disjoint-path premise. `%File` may create directories before failing.
- File copy and lockfile byte writes are outside IRIS database rollback. This was observed
  through the real load consumer and separately at the direct FileCopy/LockFile boundary.
- CPF's child `iris merge` may update configuration independently of the database. The
  model represents this freedom and exit-status conversion. No native CPF side effect is
  claimed in this extension; full correspondence remains open.
- ORAS `IsAvailable` checks transport/status/challenge only. It does not prove credential
  acceptance, artifact pull success, digest integrity, retries or network fairness.
- Reporting format classes, export and summary write success are inputs. A completed export
  is distinct from status success; failure to export does not imply absence of partial bytes.
- Inspection delegates Name/ExternalName collation and lookup to IRIS. Only selection and
  frame are modeled; detailed resource formatting and dependency query completeness remain open.

## Prior representation retained

P `uninstall.rs::clean_step` already models FileCopy KeepOnUninstall, protected directories
and single-file versus directory cleanup. Its `RKind::Files` distinguishes those branches.
The new `clean_removes` is a checked executable projection and native reconfirmation, not a
newly discovered Verus representation. P `dep_resolve.rs::{name_order,needs_first,
dep_witness_lockfile_name_order}` already represents the lockfile dependency-order defect.
It lacks file location, write exception and rollback outcome inputs. P
`sync_baseline.rs::{sync_state,sync_ok,sync_lemma_sync}` already distinguishes successful
file/hash commit followed by test failure from compile failure before commit; do not claim a
new sync representation from the held-out source case. P `load_general.rs::Db` and G's load
state distinguish database module/history outcomes but have no file or external configuration
store. No import replaces these inherited statements or changes their premises.

## Open findings

The real load consumer confirms files can survive error and database rollback; this is an
observed boundary, not an automatically normative defect. The working policy is the existing
`transaction.rollback-scope` Q. First normal load writes a relative `module-lock.json` in
process cwd because the stored module root is empty; this upgrades the prior OBS-L3 suspicion
on these fixtures, not all load modes. Lockfile path/content and whole-process cwd capture
need dedicated cross-platform correspondence before broader claims.

Concurrent history delete then finalize was exercised in two processes. The delete removes
the unfinished persistent twin; finalization and PersistToTwin return success and the row stays
absent after close. This observation refutes the resurrection hypothesis on this schedule.
It does not settle the owner policy Q or prove all interleavings.
