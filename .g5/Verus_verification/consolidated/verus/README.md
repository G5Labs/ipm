# Consolidated Verus models

These input-driven hand translations extend the preserved G/P models at product571e04a1. They do not mechanically verify ObjectScript bodies.

Run the complete new gates from the IPM root:

```sh
python3 .g5/uplift/consolidated/extension/run_all.py
```

The [report](../../../uplift/consolidated/extension/REPORT.md) contains before/after member counts, separate obligation/native results, critical shortfalls, exact commands and links. Each `*.manifest.json` identifies exact product members, ontology IDs, Verus symbols/properties, premises, roles, receipts and open boundaries. The [binding index](../../../uplift/consolidated/extension/bindings.json) and [reverse index](../../../uplift/consolidated/extension/reverse-bindings.json) are generated from these and the inherited inventories.

`check_parent.py`, `check_env_attr.py`, `semver_check.py`, `owners_check.py` and `check_thin_surfaces.py` verify the models and reject wrong versions. Generated native assertion files consume native observations as assertions to prove, never axioms. Compiler, filesystem, HTTP, ICU and process/restart behavior stay explicit environment inputs/premises. `inherited_bridge.rs` invokes P's actual homogeneous-mode theorem without extending its domain. The seven Python families remain independent native oracles; only the known environment twin was corrected.
