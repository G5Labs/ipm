# ORAS signed admission for IPM #281: implementation slice

This iteration binds the #281 implementation to the [iteration 68 admission contract](../iter68/HANDOFF.md). An ORAS repository configured with a Docker Content Trust server offers and installs only tags signed on it, and IPM checks the manifest and package bytes before anything unpacks them. Behavior and decisions are in the [signed admission ontology](../../../ontology/architecture/package-sources.g5.md#repo.oras.signed-admission).

```
repo -trust-server <url>                  (Definition.TrustServer)
        |
search / dependency planning / install / update / reinstall
        |
PackageService ── ContentTrust.SignedTargets ── notary list <gun> -r targets/releases,targets
        |               (kept for the shell command)      (cached timestamp removed first)
        |
        ├─ AddModulesFromTags   signed tags only; metadata from the signed manifest
        ├─ GetModuleManifest    module.xml from the signed manifest
        └─ GetModule ── GetTrustedModule
                          manifest by signed digest  -> digest and size checked
                          exactly one package layer
                          package by digest           -> digest and size checked
                          stream ──> LoadModuleFromArchive unpacks
```

## Pins

| Input | Value |
|---|---|
| Base revision | `c1c05500` (product source identical to `571e04a1`) |
| Verus | `0.2026.09.27.3cf1832`, tool hashes in [proofs/models.json](proofs/models.json) |
| Verifier | notary CLI v0.6.1 `notary-Linux-amd64`, sha256 `73353b2b…19879` (release notes checksum) |
| Trust server | `notary:server-0.7.0`, image `sha256:ad27fdef…278a4`, [test configuration](../../../../tests/notary/server-config.json) |
| Registry | zot `ghcr.io/project-zot/zot-linux-amd64:latest`, image `sha256:b68f727f…c4a02` |
| IRIS | `ipmcont20261116-iris:571e04a1-fresh`, image `sha256:7da4480b…3f903`, candidate classes compiled by the runner |

## Formal result

[implementation.rs](../../../Verus_verification/consolidated/iter69/implementation.rs) transcribes the decision path and includes [admission.rs](../../../Verus_verification/consolidated/iter68/admission.rs) unchanged. 20 obligations verify, including the 8 contract proofs. All 10 controls are rejected at the theorem named below.

| Theorem | Claim | Control rejected there |
|---|---|---|
| `stream_refines_admission` | A returned stream is a package the contract admits. Neither the provider nor the loader unpacks anything else. | `manifest-digest-unchecked`, `first-of-several-package-layers`, `package-digest-unchecked`, `package-size-unchecked`, `provider-pull-unpacks`, `trust-server-or-root-unchecked` |
| `admitted_artifact_installs` | A trusted signature on unchanged bytes installs. | none; completeness |
| `failures_refuse` | Missing, untrusted or unavailable evidence refuses with no unpacking. | `trust-server-or-root-unchecked`, `provider-pull-unpacks` |
| `outage_refuses` | An unreachable trust server refuses even with valid cached trust data. | `cached-timestamp-kept` |
| `only_trusted_roles` and `lemma_select_is_docker_row` | The filter equals docker's targets/releases-then-targets rule. | `untrusted-delegation-accepted` |
| `snapshot_pins_metadata_and_payload` | Selection, planning and payload use one listing per command. | `payload-looks-up-again`, `manifest-digest-unchecked` |
| `snapshot_keyed_by_server` | A listing for one trust server never answers for another. | `snapshot-ignores-trust-server` |
| `plan_and_payload_share_identity` | The contract's `continue_from_metadata` holds for an admitted payload. | via `stream_refines_admission` |

The contract side defines docker's rule (`docker_trusts`, `docker_row`) and the single package descriptor (`single_package`) separately from the transcribed filter, so a weakened filter cannot verify against itself.

Premises the model does not discharge:

- notary's exit 0 means it verified the TUF chain for the configured server under the pinned root, including thresholds, expiry and rollback checks. That is the reference client's behavior and is not verified here.
- sha256 is collision resistant, so a matching digest fixes the bytes and their size.
- The Python helpers hash exactly the bytes they write and return, and nothing else writes the temporary file before `LoadModuleFromArchive` reads it.
- `$$$RepoLookupState` lives for one shell command (`Main.cls::ShellInternal`). Calls outside a shell command check again each time, so selection and payload can see different signed listings there.
- The first check of a repository trusts the root its server presents, as docker does.

## Correspondence

[bindings.json](bindings.json) lists each formal symbol's production members and roles. [exact-bindings.json](exact-bindings.json) records the member spans and hashes the runner resolved. All bindings are transcriptions. No ObjectScript or Python body is verified, so the ontology tags are `design-verified`, never `implementation-verified`.

Native evidence is the product's own integration test, [`OrasContentTrust.cls`](../../../../tests/integration_tests/Test/PM/Integration/OrasContentTrust.cls), run against the real notary server and zot registry. [observations.json](native/observations.json) maps its 13 methods to the requirement IDs, all passed with 290 assertions ([session](native/session.log), [compile](native/compile.log)). It signs with the notary CLI from the test, outside IPM, after the ordinary `publish`. The unprotected control in `TestNothingUnpackedBeforeVerification` shows that the check sees unpacking: the ORAS pull of the same manifest does unpack its directory layer.

## Inherited evidence

| Evidence | Status | Basis |
|---|---|---|
| [iter68 admission contract](../iter68/HANDOFF.md) | Reused unchanged | `admission.rs` hash is an input of this run, now refined by `implementation.rs` |
| iter68 as-built archive boundary | Still applicable to repositories without a trust server, rechecked | `Pull`, `PullOras` and `VerifyModuleHashAndSize` bodies are unchanged ([member-bodies.json](inherited/member-bodies.json)). `run.py --check` refuses its pinned receipt by design ([iter68-check.txt](inherited/iter68-check.txt)). The unprotected control above reproduces its provider-unpacking observation on the candidate. |
| iter2 registry, iter8 catalog and iter15 ORAS claims on `GetModule` and `AddModulesFromTags` | Reopened, rechecked natively | Both bodies gained a protected branch and their unprotected branches are unchanged. Isolated `Oras` 28/28 and `OrasDeployedPlatformFilter` 7/7 ([r2](regressions/r2-isolated.summary)). Theorems unchanged. |
| iter41 settings claims on `UniversalSettings` | Reopened by the freshness gate, rechecked | Member bodies are unchanged. `CONFIGURABLE` gained `NotaryExecutable`, which `config set` and `config list` now exercise in `TestVerifierUnavailableRefused`. Unit suite 152/152 ([log](regressions/unit-r1.log)). |
| Other mapped members of the changed files | Still applicable | 65 project-map entries changed only by file hash. 4 member bodies changed: `Display`, `OnConfigure`, `GetModule` and `AddModulesFromTags` ([project-map-diff.json](inherited/project-map-diff.json)). |
| Census | New file bound here | `common30/freshness.py` reports `Repo/Oras/ContentTrust.cls` as added ([freshness-candidate.json](inherited/freshness-candidate.json)). |

The first regression pass ran classes one after another in shared namespaces ([r1](regressions/r1-shared-namespace.summary)). `OrasDeployedPlatformFilter` failed because the new test had not yet unpublished its packages, and other ORAS tests search the whole registry. The test now unpublishes them. `InstallModule` and `History` failed because an earlier class left an `oras` repository behind. `LockFile` failed because `irisowner` could not write the bind-mounted fixtures, which CI makes world-writable. The second pass deleted repositories before each class and made the lock fixtures writable, and every class passed.

## Gate

`python3 .g5/Verus_verification/consolidated/iter69/run.py --check` compares the saved evidence with the model, controls, contract, runner, bound member spans, changed product files, test class and fixtures. [freshness-demo.txt](freshness-demo.txt) shows it reopening the proof after a formal change, the correspondence after a source-only change to `TRUSTEDROLES`, and the native receipt after a new writer outside the bound spans. A new writer in a production file outside the runner's list, such as `Main.cls`, is caught only by the census of added files.

## Replay

From the IPM checkout root, with the oras and notary services on the IRIS container's network:

```sh
python3 .g5/Verus_verification/consolidated/iter69/run.py               # proofs, controls, bindings, native
python3 .g5/Verus_verification/consolidated/iter69/run.py --proofs-only
python3 .g5/Verus_verification/consolidated/iter69/run.py --check
```

`VERUS` and `IPM_IRIS_CONTAINER` override the verifier and the container.

## Limits

- Model correspondence is by transcription and native tests. The notary client, oras-py's HTTP session and IRIS's `$zf(-100)` are trusted.
- Two processes checking one repository at once share notary's cache. One can briefly use a timestamp the other just downloaded.
- A trust server that requires a login, a private CA for an https trust server, Windows hosts and registries whose repository names contain a port on Windows are not exercised. notary v0.6.1 has no Linux arm64 build, so the test class skips without notary.
- The full integration suite was not run as one pass. The classes above ran in isolation.
- The CI workflow step and the Dockerfile download step were not run in CI. The Dockerfile step was built alone on the IRIS base image.
- Policy questions left open: [supplier fallback](../../../Verus_verification/open_questions.md#repo.oras.signed-admission.supplier-policy), [trust-server credentials](../../../Verus_verification/open_questions.md#repo.oras.signed-admission.trust-server-credentials), [root pinning](../../../Verus_verification/open_questions.md#repo.oras.signed-admission.root-pinning).
