# Iteration 3 contracts

Start from the canonical [artifact download](../../../ontology/features/artifact-download.g5.md), [module](../../../ontology/concepts/modules.g5.md), [settings](../../../ontology/concepts/settings.g5.md) and [build](../../../ontology/features/build-and-publish.g5.md) owners. The [generated index](../../../uplift/consolidated/iter3/INDEX.json) binds their exact clauses to pinned source members, contracts, controls and native observations.

From the IPM root:

```sh
python3 .g5/Verus_verification/consolidated/iter3/run_models.py
python3 .g5/Verus_verification/consolidated/iter3/run_native.py
python3 .g5/Verus_verification/consolidated/iter3/replay_native.py
python3 .g5/uplift/consolidated/iter3/replay_inherited.py
python3 .g5/uplift/consolidated/iter3/run_all.py
```

`run_all.py --native --inherited` also refreshes those two environment replays. The native driver creates or reuses only its dedicated `ipm-iter3-iris` container from the existing pinned fresh image; set `IPM_ITER3_CONTAINER` for another dedicated name. It uses a disposable loopback TLS provider and fixture credentials. It changes no repository product file and resets no shared container. Its source-pin receipt compares the image's on-disk product inputs with the immutable pin; it does not mechanically prove compiled-code identity.

`artifacts.rs` models selection, credential handoff, cache and copy boundaries and the first tar-header decision. `resources.rs` models ordered defaults, concrete admission/deployment rules and project/status readers. `public_repairs.rs` repairs now-public contradictions and omissions. Functions prefixed `repaired_` are proposals, kept separate from as-built behavior.

The native bridge derives predictions from fixture inputs and asserts the actual observed fields, then rejects a flipped byte-download outcome. [TRUST.md](TRUST.md) names every representation boundary. These are abstract proofs plus native correspondence; implementation proofs are zero.
