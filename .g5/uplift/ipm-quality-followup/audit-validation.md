# Independent validation of audit findings R01–R12

Each audit claim is checked against the original sources before any artifact is revised. A claim can be CONFIRMED, NARROWED (true in part, with the part that does not hold stated), or REFUTED. Paths are relative to `.g5/` unless they start with `src/`.

## R01 — evaluator versus Verus clause

Probe: `Verus_verification/followup/probes/audit_r01_r03_probe.py`, run with

```sh
cd .g5/Verus_verification
python3 -I followup/probes/audit_r01_r03_probe.py native/evaluate_replay.py results/native/20261113T193952Z-replay-return-b cases/cases.json
```

It loads the original evaluator, parses the saved `raw.log` and evaluates in memory. No product code runs and no file is written.

| Claim | Source checked | Observed | Result |
|---|---|---|---|
| `slot_installed` accepts a module without resources; `installed_ok` requires `m.res` | `native/evaluate_replay.py:89-96`; `crates/ipm-load-history/src/contract.rs:291` | B1-C01 with `modules.top.resources_loaded=0` still gives `module_alt_ok = True` and no L1 violation | CONFIRMED |
| the manifest-only rule omits the document-version check | `evaluate_replay.py:109-114`; `contract.rs:296` (`m.doc == e.ver`) | a synthetic observation with `zpm_document_version=9.9.9` passes `manifest_only 2.0.0` | CONFIRMED |
| `slot_unchanged({}, 'top')` returns true | `evaluate_replay.py:80-86` | `True` | CONFIRMED. In addition, `unchanged` in Verus is `m == i` over all six `Mod` fields including `wr` (`model.rs:52-64`); the Python comparison omits the `LastUpdated` proxy that the same evaluator uses for `kept` (`evaluate_replay.py:175`) |

## R03 — evidence completeness (first part)

| Claim | Source checked | Observed | Result |
|---|---|---|---|
| `parse` overwrites repeated measurements | `evaluate_replay.py:63-77` | two `status` lines for one case yield only the last value | CONFIRMED |
| changed inputs are recorded but not used in `complete` | `evaluate_replay.py:229-234, 277-278` | `changed_inputs` only feeds the output field `cases_with_changed_inputs_need_rerun` | CONFIRMED by reading |
| only observed or begun IDs are evaluated | `evaluate_replay.py:297-300` | `not_realized` lists only binding entries whose `realized` is not `yes`, so a realized case absent from the log has no entry | CONFIRMED by reading |

The remaining rows are filled in as each finding is worked.
