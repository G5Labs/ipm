# IPM #281 signed admission: incremental re-verification and repair

This iteration re-verifies the delivered #281 change, an interrupted snapshot, against its base and the experiment requirement packet. It repairs product defects and model discrepancies within the PR's scope and reruns the affected proofs, controls and native suites on the repaired revision. It supersedes [iteration 69](../iter69/HANDOFF.md), whose records stay unchanged. Behavior and decisions are in the [signed admission ontology](../../../ontology/architecture/package-sources.g5.md#repo.oras.signed-admission).

```
repo -o -name <repo> -trust-server <url>     Oras/Definition.TrustServer      (other types refuse -trust-server)
        |                                        |
        |                              GetPackageService / GetPublishService
        v                                        v
search / dependency planning / install / update / reinstall
        |
PackageService ── ContentTrust.SignedTargets ── notary -d TrustDirectory(server) -s server list <gun> -r targets/releases,targets
        |               one outcome per shell command          (cached timestamp removed first, timeout program)
        |               a timeout marks the server stalled for the rest of the command
        |
        ├─ AddModulesFromTags   signed tags only; metadata from the manifest fetched by signed digest
        ├─ GetModuleManifest    module.xml from that manifest
        └─ GetModule ── GetTrustedModule
                          manifest by signed digest  -> digest and size checked
                          exactly one package layer
                          package by digest           -> digest and size checked
                          stream ──> LoadModuleFromArchive unpacks
```

## Revisions and pins

| Input | Value |
|---|---|
| Pre-PR product | `c1c05500`, product source identical to `571e04a1` |
| Delivered candidate | `08078428` "Snapshot interrupted IPM #281 Verus implementation" |
| Repaired candidate | the commit that adds this file |
| Requirements | experiment packet F01–F10 and C01–C07 (issue #281, its 13 and 14 March 2025 comments and the 12 November 2026 status comment) |
| Verus | `0.2026.09.27.3cf1832`, tool hashes in [proofs/models.json](proofs/models.json) |
| Verifier | notary CLI v0.6.1 `notary-Linux-amd64`, sha256 `73353b2b…19879`, checked in the container |
| Trust server | `notary:server-0.7.0`, image `sha256:ad27fdef…`, [test configuration](../../../../tests/notary/server-config.json), container `ipm281-d1-notary` on `ipm281-d1_default` |
| Registry | zot image `sha256:b68f727f…c4a02`, container `ipm281-d1-oras` |
| IRIS | image `sha256:7da4480b…3f903` (baseline IPM `571e04a1`), container `ipm281-d1-iris`, candidate classes compiled by the runner |

The foundation's earlier evidence describes `571e04a1`. That revision equals the pre-PR product source, so no pre-existing mismatch is carried in.

## Scope and expected outcomes

The expected outcomes come from the packet, independently of the delivered handoff. "Native" names the test methods in [native/observations.json](native/observations.json).

| ID | Expected outcome | Disposition on the repaired candidate | Evidence |
|---|---|---|---|
| F01 | `-trust-server` on an ORAS repo is stored, read back and used by the package service | Established | `TestRepoConfiguration` (also refusal on other types, old records) |
| F02 | Externally signed content verifies against a real Docker Content Trust service | Established, conditional on notary's TUF checks | notary 0.6.1 against notary server 0.7.0. `TestSignedInstall`, `TestTrustedRoles`, `TestReplacedRootRefused`. Premise `notary_verified` |
| F03 | Nothing unpacked before verification | Established | `stream_refines_admission`, control `provider-pull-unpacks`. `TestNothingUnpackedBeforeVerification` with the unprotected positive control |
| F04 | Consumed bytes equal the authenticated identity | Established, conditional on sha256 | `stream_refines_admission`, controls on digest, size and layer count. `TestMismatchedContentRefused`, `TestWrongBytesRefused`, `TestSignedArtifactIsTheOneInstalled` |
| F05 | Missing, invalid and untrusted signatures refuse | Established | `failures_refuse`. `TestSignedInstall` (including another repository's signature), `TestTrustedRoles`, `TestReplacedRootRefused` |
| F06 | Outage or missing verifier refuses | Established | `outage_refuses`, `stalled_server_only_refuses`. `TestVerifierUnavailableRefused`, `TestStalledTrustServerRefused` |
| F07 | Install, reinstall, update and new dependencies all verify | Established for shell commands | `TestUpdateAndReinstall`, `TestProtectedDependency`, `TestExternalNameDependency`. Independent caller sweep found no unverified path |
| F08 | Metadata and payload consistently authenticated | Established within a shell command | `catalog_offers_only_authorized`, `catalog_and_payload_read_one_manifest`, `snapshot_pins_metadata_and_payload`. `TestSnapshotWithinCommand`. Outside a shell command each step verifies separately |
| F09 | Old records and unprotected repos unchanged | Established | `TestRepoConfiguration` reads a record without slot 7. `Oras` 28/28 and the regression table below |
| F10 | Ordinary publish, external signing, protected install | Established | Every positive test publishes with `publish` and signs with the notary CLI |
| C01 | Version, range, build and platform selection | Established | `TestBuildAndPlatformSelection`, `OrasDeployedPlatformFilter` 7/7 |
| C02 | Namespaced and external names | Established | `TestExternalNameDependency`, `TestSignedInstall` (GUN includes the namespace) |
| C03 | Credentials and configuration copies | Established for the registry. Trust-server credentials are an open question | `TestBasicAuthentication`, `TestRepoConfiguration` |
| C04 | Dependency rules | Established | `TestProtectedDependency` |
| C05 | Failure status, history, no lifecycle, old code usable | Established | `TestSignedArtifactIsTheOneInstalled`, `TestReplacedRootRefused`, `TestVerifierUnavailableRefused`, `TestUpdateAndReinstall` |
| C06 | Options do not suppress the check | Established | `-force` and `-dev` in `TestReplacedRootRefused`, `TestProtectedDependency` |
| C07 | Valid archives load, malformed content errors | Established | `TestMismatchedContentRefused` (non-archive blob), `TestPackageLayer` |

## Reverse lookup and its calibration

[inherited/reverse-lookup.json](inherited/reverse-lookup.json) lists the 43 production members the final candidate adds or changes in five classes, and the [iteration 67 project map](../iter67/project_map.py)'s claims on each. [inherited/freshness-candidate.json](inherited/freshness-candidate.json) is the shared [freshness gate](../common30/freshness.py) on the final candidate.

| Affected obligation, derived from the requirements and code reading | Shared freshness gate | Project-map member lookup |
|---|---|---|
| Signed admission (all children), new `ContentTrust.cls` | Added file only | No bindings (new file) |
| ORAS selection and payload, iter15 claims on `GetModule`, `AddModulesFromTags` | Missed (gate covers iterations 30 onward) | Found |
| Repository configuration, iter16 and iter22 claims on `Repo/Definition.cls::Configure` | Missed | Found |
| Settings, iter41 claims on `UniversalSettings` | Found | Found |
| Archive boundary for repos without a trust server, iter68 | Missed (its receipt refuses any change by design) | No member changed (`Pull`, `PullOras`, `VerifyModuleHashAndSize` bodies unchanged) |
| Loader and snapshot lifetime, `Utils/Module.cls::LoadModuleReference`, `Main.cls::ShellInternal` (unchanged callers) | Missed | Not in the iteration 67 map |

Neither selector alone covered the PR. This iteration therefore used the whole `Repo/` subsystem, the loader and the shell lifetime as the invalidation boundary. An independent reviewer swept every retrieval caller in `src/cls/IPM` (`GetModule`, `GetModuleManifest`, `HasModule`, `ListModules`, `Pull`, `PullOras`, `GetModuleXML`, `GetPackageMetadata`, `GetAllTags`, `GetLatestTag`, `GetPackageService`). The search was limited to `src/cls/IPM` and the bundled oras-py 0.1.30 wheel.

## Findings and repairs

Origin is relative to the pre-PR product. "Live" means reproduced in `ipm281-d1-iris` before the repair.

| ID | Finding | Origin | Found by | Reproduced | Repair and recheck |
|---|---|---|---|---|---|
| R1 | `-trust-server` on a remote or filesystem repo was accepted and silently ignored, so an administrator could believe that repo checked signatures | PR-introduced: the modifier parses for every repo type | this pass | Live: `repo -r -name remote1 ... -trust-server ...` returned OK and stored nothing | `Repo/Definition.cls::Configure` refuses it for types without `TRUSTSERVERSUPPORTED`, before anything is saved. `TestRepoConfiguration` |
| R2 | `config delete NotaryExecutable` restored the first path ever set instead of the documented PATH lookup, and `config set NotaryExecutable ""` is rejected by the CLI, so no command could return to the PATH | Inherited first-write-default defect ([P:F-REPO-4](../../../Verus_verification/bugs_issues.md#p-f-repo-4)), newly exposed by the PR's key | this pass | Live: after `config set NotaryExecutable /opt/first/notary` and `config delete`, the value stayed `/opt/first/notary` | `ResetToDefault` empties the key, as it does `TestReportFormat`. [settings.rs](../../../Verus_verification/consolidated/iter70/settings.rs). `TestRepoConfiguration`. The inherited defect remains for the other keys |
| R3 | A trust server that accepts connections and then stalls cost a full 130 s timeout for every repository in the registry catalog, because each lookup ran notary again | PR-introduced | independent reviewer | Derived from the code. The stall itself is reproduced by `TestStalledTrustServerRefused` | `SignedTargets` marks the repo's trust server stalled for the rest of the command. Later repositories fail at once (fail-closed). `stalled_server_only_refuses`, `timeout_stalls_server`. `TestStalledTrustServerRefused` |
| R4 | `GetPublishService` did not pass the trust server, although `PublishService` inherits every retrieval method | PR-introduced latent gap; no production caller retrieves through it | independent reviewer | None, inspection only | `GetPublishService` sets `TrustServer` |
| M1 | iteration 69's `trusted_row` compared the listing's server and root with the configuration. IPM executes no such check, notary does. Its control `trust-server-or-root-unchecked` mutated a line with no product counterpart | Model transcription error in iteration 69 | this pass | Source comparison of `TrustedTarget`, `SignedTargets`, `LookUpSignedTargets` | `invoked_server` models IPM's actual decision (which server it passes with `-s`). `notary_verified` states notary's part as a premise. Control `wrong-server-invoked` replaces the withdrawn one |
| M2 | The catalog block was tagged `design-verified`, but no theorem covered "offers only signed versions, with metadata from the signed manifest". The cited theorem was about the payload | Assurance claim in iteration 69 | this pass | Proof statement review | `offered`, `catalog_manifest`, `catalog_offers_only_authorized`, `catalog_and_payload_read_one_manifest`. Controls `catalog-offers-registry-tags`, `catalog-metadata-by-tag` |
| M3 | iteration 41's settings model fixes 12 configurable keys and uses index 12 as an unknown key. The PR made index 12 `NotaryExecutable` | Stale correspondence caused by the PR | freshness gate | Source comparison | [settings.rs](../../../Verus_verification/consolidated/iter70/settings.rs) with 13 keys. The iteration 41 theorems stay valid for the other 12 keys |
| E1 | No test showed a signature in another repository's trust data being refused, an item on iteration 68's PR evidence list | Evidence gap | this pass | n/a | `TestSignedInstall` signs the same manifest digest for a sibling repository and checks the install is still refused |
| E2 | iteration 69's handoff, viewer rows and model manifest describe 10 controls and 13 native cases, while its receipts hold 11 controls and 16 cases | Records of the interrupted snapshot | this pass | `run.py --check` on iteration 69 passes against the receipts, not the prose | Left unchanged as history. Superseded here |

The reviewer also reported these items. They are not repaired:

- Trust-server credentials: open question, fails closed.
- Supplier steering when a protected repo offers nothing: added to the open question.
- Snapshot outside a shell command: no production caller.
- No deadline without the `timeout` program: documented.
- A stale cached delegation file with an unsafe target name keeps refusing a repository until its trust folder is deleted: fails closed.
- Protected `HasModule` ignores the platform suffix as the unprotected one does: no production caller.

## Formal result

[implementation.rs](../../../Verus_verification/consolidated/iter70/implementation.rs) includes [admission.rs](../../../Verus_verification/consolidated/iter68/admission.rs) unchanged. It has 24 verified obligations, and all 15 controls are rejected at the named theorems. [settings.rs](../../../Verus_verification/consolidated/iter70/settings.rs) has 4 verified obligations, and both of its controls are rejected. There is no `assume`, `admit` or `external_body` in either model or the contract.

| Theorem | Claim | Controls rejected there |
|---|---|---|
| `stream_refines_admission` | A returned stream is admitted by the contract, and nothing else is unpacked | `wrong-server-invoked`, `manifest-digest-unchecked`, `first-of-several-package-layers`, `package-digest-unchecked`, `package-size-unchecked`, `provider-pull-unpacks`, `unsafe-target-names-accepted` |
| `admitted_artifact_installs` | A trusted signature on unchanged bytes installs | none (completeness) |
| `failures_refuse` | Missing, untrusted or unavailable evidence refuses with no unpacking | `wrong-server-invoked`, `provider-pull-unpacks`, `unsafe-target-names-accepted` |
| `outage_refuses` | An unreachable server refuses despite valid cached trust data | `cached-timestamp-kept` |
| `only_trusted_roles`, `lemma_select_is_docker_row` | The filter equals docker's targets/releases-then-targets rule | `untrusted-delegation-accepted` |
| `catalog_offers_only_authorized` | An offered version, and the manifest its metadata comes from, are authorized for that tag | `catalog-offers-registry-tags`, `catalog-metadata-by-tag`, `wrong-server-invoked`, `unsafe-target-names-accepted` |
| `catalog_and_payload_read_one_manifest`, `snapshot_pins_metadata_and_payload` | Selection, planning and payload use one manifest per command | `payload-looks-up-again`, `manifest-digest-unchecked`, `catalog-metadata-by-tag` |
| `snapshot_keyed_by_server` | A listing for one trust server never answers for another | `snapshot-ignores-trust-server` |
| `stalled_server_only_refuses`, `timeout_stalls_server` | The stall short cut only refuses and is set by a timeout | `stall-reuses-other-listing`, `stall-not-recorded` |
| `plan_and_payload_share_identity` | The contract's `continue_from_metadata` holds for an admitted payload | via `stream_refines_admission` |
| `notary_set_then_delete_finds_on_path` | `config delete NotaryExecutable` returns verification to the PATH | `notary-reset-uses-stored-default`, `notary-not-configurable` |

Premises the models do not discharge:

- notary's exit 0 means it verified the TUF chain for the server IPM passed, under the root pinned in that server's trust directory, including thresholds, expiry and rollback checks (`notary_verified`). This is the reference client's behavior and is not verified here. Iteration 70 checked one part of it natively. A `trust_pinning` entry in the IRIS user's notary configuration is enforced alongside IPM's `-d` and `-s`, and a wrong pin is refused.
- sha256 is collision resistant.
- The Python helpers hash exactly the bytes they write and return, and nothing else writes the temporary file before `LoadModuleFromArchive` reads it.
- `$$$RepoLookupState` lives for one shell command (`Main.cls::ShellInternal`).
- The first check of a repository trusts the root its server presents, as docker does.

## Correspondence and native evidence

[bindings.json](bindings.json) has 8 rows and 52 member bindings, and [exact-bindings.json](exact-bindings.json) holds their spans and hashes. All are transcriptions with native agreement, or inspection where a row says so. No ObjectScript or Python body is verified, so no tag is `implementation-verified`. The repository-type refusal in `Configure` is tested natively but not modeled.

[native/observations.json](native/observations.json) holds the runner's own run. `Test.PM.Integration.OrasContentTrust` passed 16/16 methods and 357/357 assertions, and `Test.PM.Unit.OrasContentTrust` passed 5/5 and 31/31. None were skipped, which the runner asserts from `%UnitTest_Result.TestAssert`. Each method maps to requirement IDs. Positive and refusal cases observe the installed version, the callable code and the newest history row.

Before the repairs, the delivered candidate passed the same integration class 16/16 with 345 assertions, also with none skipped. Its unit suite passed 157/157, and the adjacent integration classes passed except two `LockFile` failures. Both were fixture-permission errors on the bind mount, and `LockFile` passed 10/10 after making the fixtures writable, as CI does ([logs](regressions/delivered/)).

## Inherited evidence

| Evidence | Status | Basis |
|---|---|---|
| [iter68 admission contract](../iter68/HANDOFF.md) | Reused unchanged | `admission.rs` hash is an input of this run, refined by `implementation.rs` |
| iter68 as-built archive boundary | Still applicable to repos without a trust server | `Pull`, `PullOras` and `VerifyModuleHashAndSize` are unchanged. The unprotected control in `TestNothingUnpackedBeforeVerification` reproduces provider unpacking on the candidate. iter68's own `--check` refuses any product change by design |
| iter15 ORAS claims on `GetModule` and `AddModulesFromTags` | Reopened, rechecked natively | Only a protected branch was added, and the unprotected branches are byte-identical. Theorems unchanged. `Oras` and `OrasDeployedPlatformFilter` in the table below |
| iter16 and iter22 claims on `Repo/Definition.cls::Configure` | Reopened, rechecked natively | The new branch fires only for `-trust-server`, which no fixture of theirs passes. Theorems unchanged. `History`, `InstallModule` and `Update` below |
| iter41 settings claims | Reopened, revised | Bodies other than `ResetToDefault` are unchanged. `settings.rs` here covers the 13th key. The unit suite includes `Test.PM.Unit.UniversalSettings` and `CLI` |
| iter69 implementation slice | Superseded | M1, M2 and E2 above |

## Final regression runs

Each class ran on its own after `repo -delete-all`, with the final product classes compiled. All 22 integration classes that configure repositories ran, because `Configure` changed for every repo type. The full integration suite was not run as one pass (see AGENTS.md).

| Suite | Methods passed | Assertions passed | Note |
|---|---|---|---|
| Unit, all classes (`test zpm -only`) | 157/157 | 1183/1183 | Includes `Test.PM.Unit.OrasContentTrust`, `UniversalSettings`, `CLI` |
| `OrasContentTrust` (runner) | 16/16 | 357/357 | [native/observations.json](native/observations.json) |
| `BuildDependencyGraph` | 19/19 | 287/287 |  |
| `BatchActivate` | 5/5 | 57/57 |  |
| `FileBinaryTar` | 5/5 | 55/55 |  |
| `FileCopy` | 10/10 | 69/69 |  |
| `LoadModuleWithDeps` | 2/2 | 20/20 |  |
| `LockFile` | 10/10 | 49/49 | Fixtures made writable, as CI does. Before that, the delivered candidate failed here only on fixture permissions |
| `PublishModuleWithDeps` | 1/1 | 12/12 |  |
| `EnvironmentConfig` | 2/2 | 15/15 |  |
| `InstallModule` | 7/7 | 102/102 |  |
| `QualifiedModInfo` | 2/2 | 10/10 |  |
| `Update` | 15/15 | 310/310 |  |
| `PublishExternalName` | 1/1 | 8/8 |  |
| `DeployedItems` | 3/3 | 45/45 |  |
| `ModuleRoot` | 3/3 | 23/23 |  |
| `Oras` | 28/28 | 324/324 |  |
| `OrasDeployedPlatformFilter` | 7/7 | 67/67 |  |
| `ProcessPythonWheel` | 8/8 | 100/100 | First run 5/8: irisowner could not write `pip download` output and a built wheel into the bind-mounted fixtures. Passed after making `python-deps-tests` writable, as CI does ([log](regressions/final/int-ProcessPythonWheel-writable.log)) |
| `Scopes` | 6/6 | 68/68 |  |
| `StandaloneFormBuilder` | 1/1 | 1/1 |  |
| `SelectiveDeployResource` | 2/2 | 26/26 |  |
| `WebApplicationExport` | 1/1 | 16/16 |  |
| `History` | 13/13 | 450/450 |  |

Logs are in [regressions/](regressions/).

## Freshness gate

`python3 .g5/Verus_verification/consolidated/iter70/run.py --check` compares the saved evidence with the models, controls, contract, runner, bound member spans, every changed product file and its recompiled subclasses, both test classes and the fixtures. [freshness-demo.txt](freshness-demo.txt) runs it on a disposable copy of the final tree:

- A formal edit reopens the proof.
- A source-only edit inside a bound span (`TRUSTEDROLES`) reopens the correspondence, while the theorem stays reusable.
- A new writer outside every bound span of a bound file reopens the native receipt.
- A new writer in a production file the runner does not list passes `--check`. Only the shared census in `common30/freshness.py` reports it, so run both.

## Replay

From the IPM checkout root, with the oras and notary services on the IRIS container's network:

```sh
python3 .g5/Verus_verification/consolidated/iter70/run.py               # proofs, controls, bindings, native
python3 .g5/Verus_verification/consolidated/iter70/run.py --proofs-only
python3 .g5/Verus_verification/consolidated/iter70/run.py --check
python3 .g5/uplift/consolidated/common30/freshness.py                  # census of added or removed production files
```

`VERUS` and `IPM_IRIS_CONTAINER` override the verifier and the container.

## Completion and limits

The authorized re-verification is complete on the repaired revision:

- Every affected model, binding and native suite named above was rerun on the final product classes.
- Repairs R1–R4 and M1–M3 were applied.
- No required check was skipped.

Guarantees:

- **Established:** F01, F03, F05, F06, F07 and F08 within shell commands, F09, F10 and C01–C07, with C03 covering registry credentials.
- **Conditional:** F02 and F04, on the notary and sha256 premises above.
- **Violated:** none found.
- **Unestablished:**
  - F08 for direct class-API callers outside a shell command. No production caller was found.
  - A trust server that requires authentication.
  - An https trust server with a private CA (not exercised).
  - Windows hosts and Linux arm64. notary v0.6.1 has no Linux arm64 build, so the tests skip without notary.

Remaining reliance and open decisions:

- The notary client, oras-py's HTTP session and IRIS's `$zf(-100)` are trusted.
- Two processes checking one repository at once share notary's cache.
- The CI workflow step and the Dockerfile download step were not run in CI.
- Owner questions remain open: [supplier fallback](../../../Verus_verification/open_questions.md#repo.oras.signed-admission.supplier-policy), [trust-server credentials](../../../Verus_verification/open_questions.md#repo.oras.signed-admission.trust-server-credentials) and [root pinning](../../../Verus_verification/open_questions.md#repo.oras.signed-admission.root-pinning).
- No external blocker stopped any check.
