# IPM #281 D3+: model-led repair of the signed-admission change

D3+ continues the D2 candidate of issue #281 on the newer consolidated foundation. It repairs the intent, coverage, composition, correspondence and evidence defects of D2's models first, then changes the product to meet the repaired models, synthesizing the applicable fixes of the plain arm's B2 review. This file records the foundation milestone. Later sections record the implementation and the final verification.

```
                    f7f740f9a consolidated_uplift (coverage run, iterations 68-76)
                        |
   69fb851e4 ---------- + -- 30847cb2c prepare (D2 c1c05500, rebased)
   (old base)           |    2b31bd12f D1 snapshot (08078428, rebased)
                        |    bf123a78d D2 re-verification (755e6218, rebased)
                        |    108f0e30b guidance sync
                        |    <foundation milestone>   ontology, models, controls, census  (this section)
                        |    <implementation>          product changes bound to the models
                        v    <final verification>      proofs, adjacent models, bindings, native suites
```

## Revisions and pins

| Input | Value |
|---|---|
| Pre-PR product | `571e04a1`, the product source of `69fb851e4` and `f7f740f9a` |
| Foundation | `consolidated_uplift` `f7f740f9a` (coverage run iteration 76) |
| D2 candidate | `755e6218`, rebased as `bf123a78d`. Product diff byte-identical to D2's |
| B2 candidate | `experiment/ipm-281-plain-20261117` `9b7fa404`, plain arm after seven review passes |
| Requirements | experiment packet F01-F10 and C01-C07, issue #281 and its 13 and 14 March 2025 comments |
| Evaluator evidence | `review-round/audited-ledger.md`, `verification-audit.md`, `audit-native.json`, `audit-proofs.json`, `tla-comparison/`, B2 pass results |
| Guidance | G5-Lean-V2 `b1a4623a4` with its working-tree edits, synced into `.agents/guidance` by `108f0e30b` |
| Verus | `0.2026.09.27.3cf1832`, macOS arm64 build, tool hashes in [proofs/models.json](proofs/models.json) |

## Rebase onto the newer foundation

The three D-lineage commits were replayed onto `f7f740f9a`. The preparation commit's pending ontology edits were already in the uplift, so only its guidance copy and `AGENTS.md` overlay remain. The append-only `viewer-index.jsonl` conflicted in both later commits. Both sides were kept and D1's 18 relocated iteration 68 rows were deduplicated. Neither file had duplicate lines before. The product diff against the new base equals D2's product diff against the old one.

Iteration labels overlap. The uplift's coverage run numbers its iterations 68 to 76 under `coverage-20261117/`. The D lineage used `consolidated/iter69` and `iter70`. The coverage run's meaning of "Iteration70" (census chunks B to D) is kept. The D lineage's references now say "#281 consolidated iteration 70", and D3+ uses the unambiguous name `issue281-d3` for its directories, models and viewer concerns.

Two composition defects came from the rebase itself:

- The coverage census recorded seven blocks the D lineage rewrote as stale (`repo.oras.archive-boundary`, `repo.oras.signed-admission`, `deps`, `install.package`, `repo.manage`, `settings`, `settings.writes`), three units as unaccounted, and the seven new signed-admission children as missing. The census now has records for all of them, graded against the D3+ models (below).
- The coverage run added neighbour models of members the PR changes: `iter75/oras_integrity.rs` (`GetModule`), `iter75/r8a_oras.rs` (`TagForReference`), `iter75/r8a_settings.rs` (`UpdateOne`) and `iter74/actions.rs` (`LoadModuleReference` refusals). They join the adjacent set replayed on the final candidate.

## Expected outcomes

These come from the packet and the issue, before reading D2's or B2's success claims. The scoring ledger's row IDs are given where it split an outcome.

| ID | Expected outcome |
|---|---|
| F01 | `-trust-server` on an ORAS repository is stored, read back and used by the package service |
| F02 | An externally signed tag verifies against a real Docker Content Trust service with docker's semantics, including how the first root is trusted (F02b) |
| F03, F04 | Nothing is unpacked before verification, and the bytes consumed are the signed manifest's package |
| F05 | Missing, invalid and out-of-scope signatures refuse |
| F06 | Outage or an unavailable verifier refuses, also when another check of the same repository runs at the same time (F06b) |
| F07 | Install, reinstall, update, dependencies and a local load's dependencies all verify |
| F08 | Selection, planning and download use one authenticated manifest, and eligibility comes from signed data (F08a, F08b) |
| F09, F10 | Unprotected repositories and old records are unchanged; ordinary publish plus external signing installs |
| F11-F13 | Unsupported repository types refuse the option; deleting the verifier override restores the PATH lookup; a stalled verifier returns |
| C01-C07 | Selection, names, credentials (Basic, bearer, API key), configuration copies, dependency rules, refusal reporting, options and archives keep their existing behavior. C09: an offered alias belongs to the artifact installed on this platform |

## V-category findings and their foundation repairs

Every concrete finding of the verification audit is reopened here, including those D2 had repaired. "Model" is the D3+ crate that now carries it.

| Finding | Audit class on D2 | Foundation repair | Model and controls |
|---|---|---|---|
| Unauthenticated first use over HTTP (F02b) | V3 wrong intent | The configuration and verifier blocks now require https, as docker's trust-server rule does, and a stored non-https value fails every check. First use pins the root an authenticated server presents. `TrustBootstrap.tla`'s HTTP trace is the failing case | `implementation.rs::accepted_root_is_authentic` with premises `tls_authenticated`, `pins_authentic`; `configuration.rs::http_refused`. Controls `d2-http-trust-server-allowed`, `d2-http-accepted`, `verification-allows-http` |
| Competing verifier cache writers (F06b) | V4 incomplete model | The freshness block now gives each check its own trust directory, seeded from the pins without a timestamp, under an instance-wide lock that also serializes pin updates. D2's `implemented_list` supplied `cached_timestamp = false` as a constant | `invocations.rs::accepted_check_fetched_itself` over every interleaving, `step_inv`, `run_inv`. Witnesses `shared_directory_accepts_without_reaching_server` (the TLC trace's schedule) and `unlocked_pins_move_back`. Controls `d2-shared-unlocked-directory`, `timestamp-seeded`, `pins-written-on-failure`, `unlocked` |
| External name from the wrong platform (C09a/b) | V6 composition, V4 | The catalog block now says the name, external name and metadata come from the signed manifest of the artifact install retrieves. The model imports iteration 15's `metadata_tag` and `tag_for` instead of taking one tag for both reads | `implementation.rs::catalog_checks_installed_artifact`, `offered_name_is_installed_artifact`, `platform_alias_neighbours` (the audit's two-variant probe as a computed case). Controls `d2-metadata-from-first-platform`, `deployed-disagreement-accepted`, `name-not-checked` |
| Unsupported repo type silently accepts the option (F11) | V1 missing formal coverage | Covered | `configuration.rs::only_oras_takes_trust`, `refusal_changes_nothing`, `neighbours`. Control `any-repo-type-accepts` |
| Verifier override delete kept the first path (F12) | V5, fixed in D2 | Rechecked | `settings.rs::notary_set_then_delete_finds_on_path`. Controls `notary-reset-uses-stored-default`, `notary-not-configurable` |
| Who checks the listing's server and root | V5, repaired in D2 | Kept: IPM decides the transport and the server it passes; notary's checks are premises | `transport_allowed`, `invoked_server`, `notary_verified`. Control `wrong-server-invoked` |
| Catalog offers signed versions | V1, repaired in D2 for one tag | Composed with platform selection | `signed_manifest_is_authorized` with the catalog theorems above. Control `catalog-metadata-unverified` |
| Generic reset theorem covered ModuleRoot | V5, contributing V4 | `explicit_reset` gives TestReportFormat and NotaryExecutable the empty value and ModuleRoot the computed root. The generic theorems now exclude every key with its own rule. The settings ontology no longer says iteration 41's theorems describe ModuleRoot | `settings.rs::module_root_delete_computes`, `delete_restores_first_write`. Controls `d2-module-root-not-excepted`, `module-root-empty-accepted` |
| Required evidence and completion | V7 | Each control names the theorems it must be rejected at, and the runner checks that each is rejected through a postcondition or assertion, never a resource limit. The census grades every feature obligation, with partial and absent grades stated. Completion claims are limited to the obligations graded full with a passing check | [run.py](../../../Verus_verification/consolidated/issue281-d3/run.py), [census](../../coverage-20261117/census/COUNTS.md) |

Two other byte-check findings motivated a model change rather than a defect. D2 had two copies of the size and digest rule. `implementation.rs` now has one `bytes_match`, which both downloads apply. Controls `shared-check-ignores-digest` and `shared-check-ignores-size` break both at once, and `manifest-not-checked` and `package-not-checked` remove one call site each.

## Comparative simplification

Each choice was compared under the same required outcomes, inputs, concurrency and failures.

| Decision | Alternatives | Chosen and why |
|---|---|---|
| Verifier state across checks | (a) D2's shared directory with the timestamp deleted, no lock. (b) The same directory under a lock per trust server and repository. (c) A private directory per check, no lock. (d) A private directory under the lock | (d). (a) fails freshness (`shared_directory_accepts_without_reaching_server`). (b) has the fewest states and would be fresh, but freshness would then depend on the lock name resolving across namespaces. A failed run would also leave notary's partial writes in the pins. (c) is fresh but lets a slower check move the pinned root back (`unlocked_pins_move_back`). With (d), freshness follows from directory ownership alone (`step_inv`), and the lock is needed only for pin monotonicity. Only a successful check replaces the pins |
| First trust | Allow http only with a pre-pinned root, or require https | Require https, docker's rule. It removes a configuration state instead of adding a conditional one. Pre-pinning remains available through notary's own configuration |
| Byte checks | Keep two copies (D2), or share one helper (B1, B2) | One helper, `read_verified`. A future digest or size rule changes once |
| Alias and metadata source | Read the first platform's manifest (D2). Check the unsigned annotation and then the signed module.xml of the install tag (B2). Read metadata from the current platform's artifact directly | Keep the first-platform deployed flag that the unprotected path uses (C01), then read names and metadata from the signed manifest of the tag `TagForReference` will request, and skip a version whose two manifests disagree on the deployed flag. B2's rule drops the current platform's own alias when another variant lists a different one first (C09b) |
| Pin storage | Namespace-local persistent rows, mapped into other namespaces (B2) | Keep D2's instance-level directory. It already covers the verify namespace, `enable -map -repos` and namespaces mapped before an upgrade (C10-C12) without mappings |

## B2 fixes by applicability

| B2 change | Applies to D3+? | Disposition |
|---|---|---|
| Pass 1: trim module.xml as `GetModuleXML` does | Already met | D2's `ModuleXMLAnnotation` strips the same whitespace for planning and the alias check |
| Passes 2 and 4: check the external name on the installed artifact | Yes, in a different form | Catalog repair above. D3+ reads the deployed flag from signed data, which B2 did not, and keeps the current platform's own alias |
| Pass 3: refuse `-trust-server` for other repository types | Already met | D2 `TRUSTSERVERSUPPORTED`; now modeled |
| Passes 5 and 6: share pins with the verify namespace and `enable -map -repos` | Not applicable | D2's pins are instance-wide. The audit credited this as N for C10-C12 |
| Pass 7: check `-trust-server` before `-publish 1` saves another repo | Already met, now atomic by model | D2 validates first. `refusal_changes_nothing` covers every refusal, and the https rule adds the http refusal of the B2 test |
| B1/B2: https only, test CA in the image | Yes | First-trust repair |
| B1/B2: private trust directory per check under a lock | Yes | Verifier-state repair, keeping D2's pins and its stall and deadline handling |
| B1/B2: one `read_verified` for manifest and package | Yes | Byte-check repair, keeping D2's credential headers (C03c) |
| B1/B2: notary run with an empty configuration file | No | It would disable `trust_pinning` and `remote_server.root_ca`, which the verifier block relies on for pinning before first use and private CAs |
| B2: cross-check notary's rows against hash-chained metadata files | No | D2's `UnsafeTargetName` refusal already makes printed rows faithful (`faithful`), with control `unsafe-target-names-accepted` |
| B2: full-cleanup uninstall deletes pins | No | Not a requirement of the issue; the instance-level pins outlive a namespace's IPM metadata as before |

## Foundation proof results

`python3 .g5/Verus_verification/consolidated/issue281-d3/run.py --proofs-only` and `--adjacent`, with `VERUS` set to the pinned verifier:

| Model | Verified | Controls rejected at their named theorems |
|---|---|---|
| `implementation.rs` (with iteration 68's contract and iteration 15's selection) | 54 | 19 |
| `invocations.rs` | 7 | 4 |
| `configuration.rs` | 7 | 5 |
| `settings.rs` | 7 | 4 |

The 13 adjacent crates in [adjacent/models.json](adjacent/models.json) verify unchanged. At this milestone no model is bound to product code: the product still has D2's behavior, which the controls named `d2-*` reproduce as counterexamples.

## Implementation plan

The models fix what the product must do. Each change below is bound to a model symbol after it is written.

- `Oras/Definition.cls::NormalizeTrustServer` accepts only https. `ContentTrust.cls::LookUpSignedTargets` refuses a stored value that is not https before running notary.
- `ContentTrust.cls` runs notary in a new private directory seeded by `SeedTrustDirectory` with the pinned metadata except `timestamp.json`. The run is held under an instance-wide lock on the trust server and repository, longer than the notary deadline. `KeepTrustData` replaces the pins only after a successful run.
- `PackageService.cls` gets one `read_verified` helper used by `FetchTrustedManifest` and `DownloadTrustedBlob`, keeping the configured credential headers on both requests.
- `PackageService.cls::AddModulesFromTags` reads names and metadata from the signed manifest of the tag `TagForReference` will request, and skips a version whose manifests disagree on the deployed flag.
- Tests and environment: an https test trust server with a test CA (also for the stalled-server stub), http refusal without side effects, concurrent checks that do not share a timestamp, and two signed platform variants with different external names.
