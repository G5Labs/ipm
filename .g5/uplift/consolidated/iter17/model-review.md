# Ordinary test-report run identity

This lane represents which completed test run each reader selects, what selected result projection is written to a JSON report, and which stored results decide the command status. The product stays at `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`. The iteration17 baseline is `e8a99bb89488648fbda653d98f9665722309d31ad3ef27349dc2a5462d849824`. Parent owns native execution, canonical owners, findings, the project-wide model/source map and integration. This lane changes only its allowed model/config/artifacts/review; it runs no native/Docker jobs and changes no product or Git state.

## Prediction ledger and producer correction

The independent initial draw was the product's passing/failing assertion methods. Before deciding-body inspection, the prediction was passing status versus failing status, a written JSON file in both cases, and two passed assertions versus three assertions with two failures. The status/file prediction held. The count prediction was wrong: the pinned test framework adds a passed `LogMessage` duration row. `BuildResultTree` includes every numeric child without filtering by action, so the observed totals are three passed rows for the passing method and four rows (two passed, two failed) for the failing method. This miss is retained; assertion operations in the fixture are not equated with every stored numeric result row.

The file/device identity lead was discovered while tracing that draw: `ReportTestResults` resolves the process's current run index, forwards it to device output, but calls `ToFile(outputFile)` without forwarding it. `ToFile` defaults to the greatest global result index. This is a source-selected follow-up, not a claimed independent pre-source run-identity prediction. The initial conditional hypothesis was that an outer run allocates its ID before a nested child. Native ordinary Assert/Shell execution confirmed outer3/child4 allocation with the child completing first; no raw result globals were seeded or rewritten.

The parent retained trial sessions, including `20261115T201146549337Z`, and qualified the final fresh session `20261115T201334487452Z` with independent raw METHOD/ROW lines, raw globals, file JSON and device JSON. An initial observer split-length mistake was corrected by the parent and its prior output retained. None of those observer corrections was treated as product behavior.

## Actual neighboring results

| Ordinary command result | File projection | Final device projection | Command status |
| --- | --- | --- | --- |
| Passing outer method, run1 | outer1, one passed method, three passed numeric rows | outer1, same counts | OK |
| Failing outer method, run2 | outer2, one failed method, four rows/two failed | outer2, same counts | Error, two failure rows |
| Failing outer3 calls passing child4 first | child4, passed method, three passed rows | child4 is printed first, then failed outer3 with four rows/two failed | Outer error, two failure rows |
| Explicit public `ToFile(path,3)` after nested run | failed outer3, four rows/two failed | Existing outer device remains run3 | File API succeeds; this is not a new test-command status |

The omitted-index nested file equals the child's full native file, while the explicit file equals the stored outer tree. The model's `forwarded().ok` still describes the outer test command's failure; it is deliberately not the success status of the standalone public export call.

Parent records the reachable JSON discrepancy as C:D31 and the proposed explicit-index forwarding as C:S21. The repair relation preserves single-run output and makes the file select the same resolved index as the device. The product caller is not patched. Other output formats and failed-export behavior are unassessed in this lane.

## Representation and scope

`RawRun` has one selected case/method, raw header status/category/message fields and every numeric child row. `passed(status)` follows `status != 0`; formal neighbors cover positive2, negative-1 and zero. Error text requires both failed status and nonempty category. The aggregate messages with blank categories remain suppressed. Numeric rows preserve action/description/location, so `LogMessage` is counted like every other row. The framework's assertion execution, row creation, valid list/primitive decoding and allocation are explicit pinned-provider boundaries qualified by actual native rows; the model does not invent a proof of `%UnitTest.Manager` internals.

Decoded method status and counts feed the unchanged iteration2 reader contract. Decoded error/entry rows feed the unchanged iteration4 failure-count/accumulator contract. Its neutral `CaseOk`/`MethodOk` tags mean no separately counted error row, not that the raw method header passed: an assertion-failing method can have a failed header, blank error category, and two failed assertion rows. No summary count or command result is supplied as an unconstrained outcome.

`State` separates completed persistent runs from process-local tracked IDs. `complete` models provider completion followed by `SaveResult` appending its run ID. `consume` removes the phase-owned process slots while retaining persistent results. `file_index(None)` selects the global numeric maximum; `device_index` uses the last tracked ID and falls back to that maximum only when tracking is empty. `file_index(Some(id))` honors the explicit API argument. `report` writes its selected file snapshot independently of the later fatal failure count.

Named pass/fail/nested witnesses retain the earlier completed runs1/2, rather than treating the selected new runs as the whole global database. The child's intermediate projection omits still-running outer3, whose ID is below selected maximum4; it cannot change the selected latest ID or lookup of child4. `latest_older_frame` proves the corresponding older-prefix maximum relation. The final named state contains completed1,2,4,3 in completion order and chooses global latest4, not last-completed3. The generic symbolic two-run theorem covers either numeric order, but does not assert that both orders occur in the pinned provider.

`Snapshot` is a modeled tree projection, not serialized bytes. Named fixtures abstract variable duration text, source locations, timestamps and namespace metadata; the generic snapshot relation preserves supplied row values. The parent's native byte/tree comparisons independently include those real variable fields. Full JSON encoding/escaping, arbitrary suite/case/method enumeration, malformed rows, read/export failures, unsupported formats, nonfatal settings and concurrent global writers are not covered by the current result relation. Applicability is successful framework execution, selected IDs present with one valid selected case/method, valid JSON format/streams, successful serialization/filesystem providers and ordinary fatal result handling. Native numeric/string JSON ID representations are normalized to run identity; equality of primitive JSON types is not asserted.

## Scoped source roles

| Model role | Source decision/writer/reader slice |
| --- | --- |
| ordinary invocation | `ResourceProcessor.Test.OnPhase:192-210`; real Case/Method selectors and public manager call; selector parsing is not re-proved |
| framework delegation | `Test.Manager.RunTest:7-12`, unchanged forwarding to the pinned external framework; this is a shared provider role, not executable-body verification |
| tracked completion | `Test.Manager.SaveResult:16-23`; super saves the provider result, then appends `i%LogIndex` |
| tracked scope | `ResourceProcessor.Test.OnPhase:100-108` and `Test.Manager.GetAllTestsStatus:103-115`; inherited accumulator contract reused |
| report ID choices | `ResourceProcessor.Test.ReportTestResults:247-250,266,280`; omitted file argument versus explicit device argument |
| default/explicit file ID | `Test.Abstract.ToFile:9-19`; default global greatest index, explicit index forwarded to writer |
| raw-row decoder | `Test.Abstract.BuildResultTree:135-188`; stored status, category gating, all numeric children and copied fields |
| summary | `Test.Abstract.GetSummary:211-233`; method status and all row counts; inherited count relation reused |
| JSON file writer | `Test.JsonOutput.WriteToFileStream:4-8`; selected tree serialization under provider contract |
| JSON device reader | `Test.JsonOutput.OutputToDevice:18-64`; selected summary and case/method/failed-row detail |
| command result | `Test.Manager.GetAllTestsStatus:74-100`, then `ResourceProcessor.Test.ReportTestResults:287-291`; failure rows derive from the tracked phase, not the exported file |

These bindings constrain the stated slices and delegated roles. They do not cover whole ObjectScript members. Parent incorporates the roles in the project-wide map required by the revised guides; a resolvable locator alone supplies no coverage credit. Direct ObjectScript implementation proofs remain zero.

## Verification and retained attempts

The complete final baseline verifies39 units with zero errors: 17 inherited units and 22 new units. Seven unique focused positive witnesses each verify exactly one unit. All21 negative controls reject semantically in their selected witness, with no timeout/resource-limit credit. The complete final direct suite took37.611 seconds. Source SHA256: `fdccd49cf40475c2b471314d44eebc9f996271d2a8cb677d6df4b2ed77c8b04c`. Exact commands, selector scope, source/import/tool hashes and logs are recorded in `model-lane/controls/receipt.json`; `model-lane/final-summary.json` summarizes the counts. Config proposes106 replay fields and11 setup qualifiers; parent integration owns the full native/replay receipt.

The controls challenge nonzero status decoding, inclusion of duration rows, the provider duration status, both error-category gates, stored method status, file default/explicit IDs, device last/fallback selection, global maximum versus minimum/last completion, selected-run lookup, persistent versus process cleanup, file-independent fatal status, write-before-failure behavior, nested device output and both inherited count contracts. The explicit-forwarding variant is a candidate repair expected to disagree with the as-built nested witness; its rejection is not evidence that forwarding is a bad design.

The initial `tracked` field spelling was a Verus keyword parse failure; its exact source/log remain as `model-lane/baseline-1.*` and earn no semantic verdict. Later baselines preserve the pre-prefix and pre-proof-split sources. Five broad negative attempts remain under `model-lane/controls-attempt1/` through `controls-attempt5/`, preserving exact mutant sources/logs. They found intended semantic failures together with unrelated proof limits; none of those mixed verdicts is credited. Proof factoring, intermediate assertions and first-error search reduced but did not eliminate broad mutant instability. The final controls therefore use explicit independent witness selectors rather than unrelated fixture proofs. All21 controls have `control_functions` metadata. Every selected proof directly unfolds producer/reader definitions and calls no proof lemma or broadcasted invariant, including imported or mutated proofs. Each unique selector has a same-source focused passing neighbor (verified1). Mutants use `--verify-root --verify-function <selector> --multiple-errors 0`; the full positive candidate baseline and parent full replay remain unrestricted. Thus the negative result is a distinguishing witness, not a full-mutant proof audit. Selected scopes and the earlier broad failures are explicit, and no resource result is treated as a counterexample. No assumption, admission, external body or new trusted declaration was added.

No repository-wide convergence is claimed. Ordinary same-version supplier changes, filesystem duplicate first-admission provenance and other functional workflows remain queued. Archive paths are expressly UNASSESSED/UNPURSUED by user direction: this lane performs no investigation, modeling, reproduction or probing of that boundary.
