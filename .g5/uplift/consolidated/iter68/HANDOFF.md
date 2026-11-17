# ORAS archive admission foundation for IPM #281

This iteration covers the boundary between the configured ORAS repository, dependency planning and the first extraction effect. Product revision `571e04a1` still consumes unsigned artifacts. The proposed contract is an input to the signing experiment.

## Observed on pinned IPM and the bundled ORAS wheel

| Case | Outcome |
|---|---|
| Same-version reinstall with a size mismatch | The real ORAS provider unpacked a benign directory-layer marker in staging. IPM returned its archive integrity error. The previous callable revision, module row, and installed-root sentinel survived. The newest install history row was finalized, failed and uncommitted. |
| Same version, valid replacement | The revised archive installed, its callable code returned revision 2, and the old installed-root sentinel disappeared. |
| Exact dependency, tag changes during one parent load | The catalog and the dependency-manifest request returned revision 1. The provider's payload request and IPM's later validation request used revision 2 of the same tag. The parent load succeeded and the installed child row and callable code were revision 2. The two revisions have distinct observed manifest digests. |

[Native observations](native/observations.json) · [IRIS output](native/session.log) · [OCI request trace](native/registry.log) · [Source bindings](exact-bindings.json)

[As-built Verus model](../../../Verus_verification/consolidated/iter68/as_built.rs) reuses iteration 2's pull status projection. It distinguishes provider extraction from the status gate that lets the loader replace the installed root. Its successful-root effect assumes an existing writable root and no subsequent filesystem failures. The dependency trace qualifies one exact, fresh dependency. Fuzzy reads have source links, while multi-node graphs and concurrent local writers remain unqualified.

## Contract to carry into the experiment

The [proposed Verus model](../../../Verus_verification/consolidated/iter68/admission.rs) binds authorization to the selected registry, namespaced repository key, exact tag, signer, policy revision, and immutable manifest digest. The descriptor's digest and size must match the bytes at extraction. A dependency plan carries the authenticated manifest identity into payload retrieval. Authorization for a newly retagged manifest cannot validate a plan built from the earlier one. The plan may restart against the new artifact if the PR's retry policy allows it.

The production premises still to establish are the verifier's trust and signature checks, canonical OCI identity, the parser's authorized package descriptor and manifest annotation, and that the bytes hashed are the bytes actually opened by the extractor. If the verifier authenticates an OCI manifest, dependency metadata read from the annotation must come from that confirmed manifest. If the implementation uses metadata from inside the verified archive, qualify the consumer at that boundary. An enabled verifier should not inherit the existing pull adapter's file-selection ordering by accident. The model uses one selected descriptor and rejects a missing one. It leaves handling of extra layers and descriptor choice to the design decision.

The repository definition should carry its policy to the package service. Either pin a configuration snapshot for the command or revalidate when its policy changes. The cached-evidence theorem applies at the point where the design supplies the active policy. Trust format, authorization and revocation, selection across protected and unprotected repositories, and reliance on installed dependencies remain open. The contract does not require verification for unsigned-policy repos, local archives, local development or already installed modules. If experiments adopt a stronger whole-tree policy, record that as a new requirement.

### Existing obligations that the PR touches

| Obligation | Evidence to reuse and correspondence to reopen |
|---|---|
| [Requested version and supplier](../../../ontology/concepts/repositories.g5.md#repo.install.version-request), [platform and build selection](../../../ontology/architecture/package-sources.g5.md#repo.oras.selection-modes) | Reuse the iteration 15 selection and payload contrasts. Reopen the handoff to the retrieval adapter if the PR adds a new reference type or candidate filter. |
| [Dependency resolution before installation](../../../ontology/features/dependencies.g5.md#deps) | Reuse the exact-child trace here. The protected adapter must consume the identity retained by the resolver. The fuzzy branch remains source-inspected. |
| [Install extracts the selected package](../../../ontology/features/install.g5.md#install.package), [archive integrity](../../../ontology/architecture/package-sources.g5.md#repo.oras.integrity) | Reuse the accepted archive and size-refusal neighbors. Show that both the provider and IPM's extractor respect the protection decision. |
| [Load outcomes](../../../ontology/features/load.g5.md#load.outcome), [persistent history](../../../ontology/features/history.g5.md#history.survives-rollback) | Reuse the scoped rollback and history contracts. Confirm actual files, module rows, callable code and history independently when a protected load is refused. |

### PR evidence to collect

- Preserve requested version, platform/build tag, external repository name, repository selection order and ordinary unconfigured installs. Carry forward the existing platform, build and install/history controls under their recorded scope.
- Pair signed success with unavailable verifier, missing signature, unauthorized signer, wrong repository scope, and wrong digest or size. Check that the provider never extracts an unaccepted layer and that the product does not alter an already installed root on refusal. Record failure status and history independently.
- Exercise a protected child via the real resolver. Distinguish the metadata used for the dependency plan from the bytes loaded by the final consumer, and check the behavior after retagging. Check qualified install, update and reinstall, including flags such as `-force` that change other admission rules.
- Keep the existing rollback and history obligations for later failures. A trust rejection of a dependency does not undo earlier filesystem or developer-mode commits. The inherited child-history survival defect remains documented in [the load/history coverage map](../../../Verus_verification/coverage-map.md).

## Replay

From the IPM checkout root, run `python3 .g5/Verus_verification/consolidated/iter68/run.py`. Use `--proofs-only` with the pinned Verus build or `--native-only` with the pinned IRIS image. `VERUS` and `IPM_IRIS_IMAGE` override those paths. `--check` checks saved evidence against its source and fixture hashes without rerunning Docker. The native run starts a disposable container with external networking disabled and serves synthetic artifacts over its own loopback. It checks the image's product inputs and the live Python provider sources against the repo's shipped wheel. The proof verifies specification code. Future PR correspondence must be established on the PR's source and its actual verifier.
