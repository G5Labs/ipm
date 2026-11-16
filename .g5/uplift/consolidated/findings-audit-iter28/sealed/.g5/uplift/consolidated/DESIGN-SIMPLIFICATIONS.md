# Consolidated uplift: design simplifications and repair proposals

Proposals only. No IPM source was changed. Each names the behavior to preserve and a case that distinguishes a safe
change. Inherited designs stay in G's [design_simplification.md](../../Verus_verification/design_simplification.md)
and P's [DESIGN-SIMPLIFICATIONS.md](../../../playbook-uplift/DESIGN-SIMPLIFICATIONS.md).

| ID | Proposal | Why | Preserve | Distinguishing case | Related |
|---|---|---|---|---|---|
| C:S1 | `TempLocalRepoManager:%OnNew` always returns an object. On a creation failure it warns and becomes a no-op manager, and `CleanUp` deletes only a row that exists (`%ExistsId`). | Removes the `<INVALID OREF>` after the commit at both call sites (G:D13 family) and the false cleanup message (C:D2) without touching the load's commit points. | TR1-TR5 outcomes (skip, naming, removal, kept foreign repository, search order). | TR7 and Dtxn: status OK, module installed, no message, no repository left. | G S7, P D14 (post-commit work) |
| C:S2 | In `%RegExReplace`, escape the replacement value fully (or replace case-insensitively without regular expressions) and quote the name as a literal. | Fixes C:D4 and C:D5 with one change at the one shared helper used by parameters and system values. | E01-E17, E19-E28, E34. | E32 gives `$5`; E21 leaves `{$axb}` unchanged. | - |
| C:S3 | Make the cycle guard reachable: after 20 passes that still change values, throw the existing "Circular reference" error. | Fixes C:D3 and gives the error the code already names. | E22, E31, E36 (chains up to three levels). | E30 raises the named error. | - |
| C:S4 | Desugar partial versions once, before choosing a range kind: an operator with a partial version compares with missing parts as zero (node-semver), a bare or caret partial version takes the x or caret rule. The `20NN.N` special case and the unreachable caret branch go away. | Fixes C:D6 and C:D7 at the one place ranges are built. | Every corpus expression that the IPM tests cover (X3-X25, X30-X32, X37-X40). | X26-X29 and X20 change as the tests and node-semver require; S1 installs `1.5.0`, S2 installs `1.3.0`, S3 installs `1.5.0`. | C:Q3 decides X35 |
| C:S5 | Compare bounds with full version precedence (including the prerelease) in `CompareVersions`, so the tighter of two bounds always wins. | Makes AND commutative (C:D8) and removes the dependence of a solvable install on dependency names. | A5-A12. | A1 = A2, A3 = A4; S4ab and S4ba install `1.5.0` like S5. | P D2 premise becomes true for these inputs |
| C:S6 | In `ModuleVersion`, compare the requested version with the current one before the higher-version check, and report "unchanged" when they are equal. | Fixes C:D13 with the branch that already exists. | MV1-MV4, MV6-MV9 (bumps, -force, refusals, validation). | MV5 returns OK with "Version is unchanged". | - |
| C:S7 | Treat a manifest whose module cannot be saved (invalid name) like an unreadable manifest: warn and skip, so the repository and a load's `.modules` repository still build. | Removes one trigger of the post-commit failure (G:D13) and makes C:Q5's provisional answer true. | LK1-LK5, TR1-TR5. | TR7 loads with status OK and no temporary repository left; Dtxn installs both modules. | C:Q5, C:S1 |
| C:S8 | One literal-replacement helper for every `${...}` substitution (module parameters, system values and environment values), replacing `%Regex.Matcher.ReplaceAll/ReplaceFirst` with replacement text. | Fixes C:D4, C:D5 and C:D14 in one place; the three evaluators already share the `${name}` syntax. | E01-E36 except E21, E32; U1-U7 values without `$` or `\`. | E32 gives `$5`; U7 gives `a$1b` and `a\b`; U8 loads. | C:S2 |
| C:S9 | Decide a downgrade by comparing major.minor.patch first and only then prereleases, instead of `Follows` in both directions. | Closes C:D15 without changing resolution, which keeps its node-semver rule for ranges. | HR2, HM2 refused; HB1 and every load of a higher version accepted. | HB2 refused with "Cannot downgrade". | - |


| C:S11 | Share same-origin Artifactory authentication construction between metadata and byte requests. | Repairs C:D17 at the actual lost handoff. | Token precedence, complete Basic credentials, anonymous/public download and existing local/offline reuse. Do not forward credentials to a different origin without an explicit contract. | A protected local byte endpoint accepts the repaired request; public bytes still succeed. | [artifact.authentication](../../ontology/features/artifact-download.g5.md#artifact.authentication). Abstract request repair proved; product repair not applied. |
| C:S12 | Check each stream read's status before treating an empty tar header as completion. | Repairs C:D18 without inventing a ban on empty archives. | Successful empty/header reads, checksum refusal, valid file extraction and errors after partial writes. | Malformed gzip read becomes an error; a successful empty read still stops normally. | [artifact.archive-admission](../../ontology/features/artifact-download.g5.md#artifact.archive-admission). Abstract first-header repair proved; decoder/body changes not implemented. |
| C:S13 | Preserve the deployed-item helper's returned status before testing handled/error or falling back to source. | Repairs C:D19's dead error guard. | Successful deployed handling, legitimate source fallback and source-export errors. | Failing helper returns error without invoking source; successful helper produces the same old outcome. | [build.package.deployed-status](../../ontology/features/build-and-publish.g5.md#build.package.deployed-status). Abstract status repair proved; no full command or product repair claim. |

## Extension proposals, not applied

C:S10 (related to G:OBS-L3 direction): persist the intended module Root before lock-file generation and make its location an explicit input to the file writer. Preserve resolved dependency names/versions, credential removal, read-only repository behavior and existing developer-mode semantics. Distinguish a first plain load from a rooted developer load, and a write failure from a later lifecycle rollback. Native evidence is in [the extension report](extension/REPORT.md); the proposed repair itself has not been implemented or checked.

Shared-guidance proposals are separate in [the independent review](extension/INDEPENDENT-REVIEW.md#future-shared-guidance-proposals-only), assessed against the five mechanism principles. Shared guidance stays unchanged.

## Boundary proposals carried from iteration 2

[ORAS selection and integrity](../../ontology/concepts/repositories.g5.md#repo.oras.integrity.policy) still need a provider-order/unique-archive decision; [partial publication](../../ontology/features/build-and-publish.g5.md#build.publish.partial-effects) still needs an explicit compensation policy before any atomicity promise; [cache consistency](../../ontology/concepts/repositories.g5.md#repo.filesystem.concurrent-refresh) must account for the actual immediate-release constructor. These proposals were not implemented. Preserve the successful single-archive, allowed deletion, explicit deferred lock, ordinary publication and force-uninstall error behaviors already recorded.

A shared-guidance change is not proposed here: direct source tracing exposed both real handoff errors and false leads. Adding another generic instruction has no measured value from this run. The cost of the added formal/native checks is recorded in iteration 3's receipts; discovery is exploratory, not an independent panel score.

## Iteration 4 proposals, not applied

| ID | Proposal | Why | Preserve | Distinguishing case | Related |
|---|---|---|---|---|---|
| C:S14 | Build the download request with credentials only when its host and port equal the repository's; otherwise send none. | Repairs C:D20 under answer (b) of C:Q14 without changing which URL is fetched. | Same-origin download with credentials (R-same), token handling, hash check. | R-cross: the other origin receives no `Authorization` or `apikey`; R-same unchanged. | Verus `registry.rs::repaired_bytes_request` and `lemma_cross_origin_carries_credentials` prove only this request boundary. |
| C:S15 | In `RelPathToClassName`, strip the folder only when it is followed by `/` or the end, and return "" for paths outside it, as the comment says. | Repairs C:D21. | Inside paths, non-`.cls` files giving "". | `tests/unitX/A/B.cls` and `other/...` both give ""; `tests/unit/SyncTest/Tests/Trivial.cls` still gives `SyncTest.Tests.Trivial`. | Control `prefix-requires-boundary` shows the model distinguishes the boundary; the repair itself is not modeled. |
| C:S16 | Lowercase the registry hash too before comparing. | Repairs C:D22 with no change for lowercase registries. | Exact lowercase match, mismatch refusal, empty-hash behavior (separately owned). | R-upper accepted; R-mismatch still refused. | Not modeled as a repaired function. |

## Iteration 5 proposals, not applied

| ID | Proposal | Why | Preserve | Distinguishing case | Related |
|---|---|---|---|---|---|
| C:S14b (revises C:S14) | Handle registry redirects explicitly and attach credentials only to hops on the repository's own host and port. | C:S14 checked only the returned URL; a same-origin redirect to another host still leaked (X1). | Same-origin downloads and redirects that stay on the repository's host. | X1: the second hop gets no `Authorization` or `apikey`; R-same unchanged. | Verus `redirect.rs::{s14b_sends,lemma_s14b_never_leaks}` at the abstract hop boundary only. |
| C:S17 (= P RC-6 for uninstall) | Run the dependent and global-scope check before any processor's `OnBeforePhase("Clean")`, and return an error naming the dependents. | Repairs C:D25 at its cause. | Forced uninstall, unblocked uninstall, `-recurse` keeping needed dependencies. | W6: application kept, module kept, status error; W7 unchanged. | Verus `webapp.rs::repaired_uninstall`, control `repair-still-deletes`. P `DESIGN-SIMPLIFICATIONS.md` RC-6. |
| C:S18 | Generate `%ZLANGF00` from its own explicit body (or match `do ##class` without regard to case). | Repairs C:D24. | The command form and its namespace fallback text. | `$zpm("version")` returns a status; Z4 returns an error status instead of raising. | Verus `repaired_function_returns_value` is a stub for the intended outcome, not a proof of the generator. |
| C:S19 | Call each processor's `OnPhase("Validate")` from `%Validate` (or move the check into `OnBeforePhase("Validate")`). | Repairs C:D23 with the check that already exists. | Valid web application attributes, including those `generate` writes (`#806`). | W9 refused with "The following invalid attributes were found"; W1 unchanged. | Not modeled as a repaired function. |

## Iteration 6 disposition

No additional product simplification is proposed. Reuse of the existing producer's global relative path for export and import is behavior to preserve, not a new repair. C:Q16 needs an owner decision before changing generation's error policy. Existing proposals remain unimplemented.

## Iteration 7 disposition

No product change or new simplification is proposed. Keep the distinct processor sweep, module placeholder resolver, supplied-hash refusal and computed-hash skip as separate operations. Combining them without preserving their callers' observed outcomes would change behavior. The iteration5 formal missing-key predicate is narrowed by the current canonical owner, with its original artifact preserved.

## Iteration 8 disposition

No product repair or new simplification is implemented. Preserve configuration-only export rather than treating a WebApplication Path as a database document. Preserve raw package declarations across publication and keep registry naming separate from manifest identity. Python cleanup requires ownership evidence (C:Q18); deleting a shared target on every uninstall is not a supported simplification. Existing proposals and their evidence remain unchanged.

## Iteration 9 disposition

No simplification accepted or retired. C:Q19's alternatives need a concurrent-writer check and a behavior-preserving comparison before a repair recommendation. Existing proposals and decisions are preserved.

## Iteration 10 proposal

| ID | Proposal | Preserve | Distinguishing result | Check and limits |
|---|---|---|---|---|
| C:S20 | Capture ResolveChildren's returned status in Clean, propagate its error and do not delete module metadata after that error. | Successful child enumeration and ordinary class cleanup. | Failed single resource retains metadata and reports error rather than apparent removal. | `errors.rs::repaired_uninstall` and valid-neighbor equivalence checked in the abstract model; product repair not applied. Earlier resource deletions and external effects are not compensated by this comparison. |

Comparison: none. Verus-only comparison; no TLC run or state-count reduction is claimed. See iteration10 proof/control receipts.

## Iteration 11 comparisons, not applied

- Under a global shared-step identity policy, `shared_steps.rs::repaired_attempt` uses the shared identity as the lock key and excludes the second simultaneous entry. This checks entry exclusion only; busy still reports success and no completion/progress repair is proved. C:Q19 must settle identity/ownership before recommending this change.
- Under C:Q21's error-and-retain choice, `deletion.rs::refuse_on_error` retains metadata on the calibrated single-class refusal and preserves the unlocked neighbor. Earlier provider/hook effects and several-class cleanup remain outside that comparison. This extends C:S20's status-boundary theme without claiming an implemented or generally atomic repair.

Comparison: none. These are scoped Verus-only comparisons, with no TLC state-count or performance simplification claim.

## Iteration 12 disposition

No new simplification accepted, tested as a product repair or retired. Correcting the routine argument and early code activation are retained as semantic controls against the as-built contract, not new accepted designs. Ordering in a future lock-file installer remains outside this uplift's implementation scope.

## Iteration 13 reconciliation

No design simplification claimed. Sequential error-prefix effects and target-specific activation are represented rather than folded into successful atomic commands.

## Iteration 14 reconciliation

No simplification claimed. Requested delete-first and actual Unix provider effects are separated; a free arbitrary-provider-output premise was rejected in favor of explicit positional arguments and calibrated merge behavior. Protection is re-evaluated after marker writers. Earlier foundations remain recoverable.

## Iteration 15 reconciliation

No simplification claimed. Separate service/display first-row selection from manager same-base last overwrite. Decoding aggregation alone could reverse the later winner; no coordinated product repair is implemented or certified.

## Iteration 16 reconciliation

No simplification proposed or certified. Supplier identity is derived before snapshot, current and historical readers stay distinct, and successful-uninstall is a conditional observed projection. Existing-installed behavior is explicitly unknown rather than silently treated as an overwrite.

## Iteration 17 proposal

| ID | Proposal | Preserve | Distinguishing result | Evidence/limits |
|---|---|---|---|---|
| C:S21 | Forward the already-selected testIndex into the ToFile call, using the same run identity as device output. | Single passing/failing run results, failure status and file-before-fatal-check ordering. | Nested failed outer export contains outer failed method/two failure entries rather than passing child results. | Verus as-built/explicit-ID comparison and actual direct ToFile(path,outerID) export; [iteration17](iter17/HANDOFF.md). Product caller not patched; serializer/stream errors, other formats and complete nesting lifecycle are not a certified repair. |

Comparison: none. Verus-only design comparison plus native explicit-ID API result; no TLC run, state-count reduction or implemented product repair. C:D31 stays open.

## Iteration 18 reconciliation

No simplification proposed or certified. Equal-version reload remains required behavior; a hypothetical equal-version skip is a rejected control, not a recommended optimization. Selected supplier/module/code/history writers compose to the unchanged inherited relation under declared successful-provider premises. Earlier proposals and owner decisions remain unchanged.

## Iteration 19 reconciliation

No simplification proposed or certified. Retain first admission's complete manifest/directory pair and its warning. Last-wins, mixed-field overwrites and silent/fatal duplicates are counterfactual controls, not accepted designs. The complete map now merges mirrored boundary metadata without duplicating semantic claims; existing proposals remain unchanged.

## Iteration 20 reconciliation

No simplification proposed or certified. Do not replace home lookup with reference existence alone or remove the naming pass: both alter supported orphan/owner outcomes. Normalizing the extension before branching is a rejected counterfactual for the as-built contract, not a product repair proposal. Existing proposals remain unchanged.

## Iteration 21 reconciliation

No simplification or product repair proposed/certified. A cache-only version selection cannot establish the version of a later mutable directory load. Earlier proposals and owner decisions remain unchanged; source snapshot/concurrency and generic identity-guard design have not been checked. Archive-path boundary stays unassessed/unpursued including investigation, reproduction and probing.

## Iteration 22 reconciliation

No simplification or product repair proposed or certified. Preserving the stored supplier, falling back to its root, or choosing the latest version would change the checked caller behavior. They are contrasting models or unresolved policy alternatives, not accepted changes. Snapshot dependency refresh and successful developer-mode behavior remain unassessed. Archive-path behavior remains unassessed and unpursued, including investigation, reproduction and probing.

## Iteration 23 reconciliation

No simplification or product repair proposed or certified. Replacing raw detail status with decoded error text would change the established contract; it is a wrong-rule comparison, not an accepted repair. Byte capture and escaped observer logs preserve evidence without changing product output. Earlier proposals and owner decisions remain unchanged. Archive-path behavior remains unassessed and unpursued, including investigation, reproduction and probing.

## Iteration 24 reconciliation

No simplification or product repair proposed or certified. Dropping the installed-version guard or always permitting downgrades would change a separate constraint. The explicit permission neighbor isolates the C:D33 failure and qualifies downstream reload; it does not authorize that change. Inherited semver code and protected owner decisions remain unchanged. Archive-path behavior remains unassessed and unpursued, including investigation, reproduction and probing.

## Iteration 25 reconciliation

No simplification or product repair proposed or certified. Source mappings, declared resource paths and external source-control consumers are separate contracts. Replacing the selected import directory with the declared default would alter the valid fallback mapping; treating a descriptor as a file finder would add an unestablished obligation. No deferred queue is invented from the trigger-suppression local variable, and no external consumer is replaced with a synthetic successful reader. Earlier proposals and owner decisions remain unchanged. Archive-path behavior remains unassessed/unpursued, including investigation, reproduction and probing.

## Iteration 26 reconciliation

No simplification or product repair proposed or certified. A set of classes would lose repeated skipped entries, a single shared counter would change per-module sequence values, and sorting modules afresh would change the caller-supplied report order. These are wrong-rule comparisons under the existing contract, not accepted redesigns. Begin/End lifetime and direct stored-field readers are preserved. Archive behavior remains unassessed; an unintended verify priming traversal was discarded and will not be repeated.

## Iteration 27 reconciliation

No new simplification. P:D11 already proposes implementing or removing the incomplete `uninstall -purge` option for P:F-UNIN-2. The new parameter/callback/SQL closure adds evidence to that existing disposition, without changing the product, selecting a repair or claiming repair certification. Preserving the intended table-data deletion rule is compatible with the checked as-built retention. [Handoff](iter27/HANDOFF.md).

## Iteration 28 reconciliation

No new simplification or product repair. The scripting model composes existing settings behavior and new preparation/sequence rules rather than changing execution or error policy. P:F-RES-5/DEC-RES-2 remain the existing documentation disposition; no repair certification follows from correct native neighbors. Earlier simplifications are preserved. [Handoff](iter28/HANDOFF.md).
