# Iteration 4 contracts

Start from the canonical owners: [test outcome](../../../ontology/features/build-and-publish.g5.md#build.test.outcome), [remote registry download](../../../ontology/concepts/repositories.g5.md#repo.remote.download), [sync recompile and test stop](../../../ontology/features/developer-tools.g5.md#devtools.sync.compile-after-resolve), and the repaired [deployment](../../../ontology/concepts/modules.g5.md#module.deployed), [test selection](../../../ontology/features/build-and-publish.g5.md#build.test.selection) and [search order](../../../ontology/concepts/repositories.g5.md#repo.search-order) blocks. The [evidence index](../../../uplift/consolidated/iter4/INDEX.json) binds each owner to the pinned member that decides the behavior and to the members that call or read it.

From the IPM root, in a clean checkout or an extracted seal:

```sh
python3 .g5/Verus_verification/consolidated/iter4/run_models.py     # contracts + wrong variants
python3 .g5/Verus_verification/consolidated/iter4/run_native.py     # fresh disposable IRIS container
python3 .g5/Verus_verification/consolidated/iter4/replay_native.py  # observed fields as Verus assertions
python3 .g5/uplift/consolidated/iter4/validate.py                   # index, links, pin, before image
```

Or `python3 .g5/uplift/consolidated/iter4/run_all.py` for all four. The model step needs pinned Verus `0.2026.09.27.3cf1832` (set `VERUS` to override its path). The native step needs Docker and the local image `ipm-consolidated-iris:571e04a1-fresh`. It creates and removes its own `ipm-iter4-native-<stamp>` container (`--keep` retains it) and touches no shared container.

| File | What it represents |
|---|---|
| `test_outcome.rs` | Failure rows, the per-process result list with nested start positions, per-resource outcome, `test` versus `sync -test` aggregation, process exit code |
| `registry.rs` | Metadata and byte requests with credentials, the hash decision, the install token and what `install` reports; Artifactory's byte request for contrast; proposed same-origin credential rule |
| `sync_compile.rs` | Which resources sync recompiles after a resolution failure, and the warning it prints and keeps |
| `repairs4.rs` | Lifecycle deploy default from the lifecycle class, phase admission plus as-built matching, keyword tokens, test path conversion |
| `native/` | Fixture module, probe classes and the two-origin registry provider the native step copies into the container |

[TRUST.md](TRUST.md) states each representation boundary. Implementation proofs: 0.
