# IPM iteration 4 handoff

Product unchanged at `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`. The canonical ontology owns every new decision; the consolidated [BUGS](../BUGS.md), [QUESTIONS](../QUESTIONS.md) and [DESIGN-SIMPLIFICATIONS](../DESIGN-SIMPLIFICATIONS.md) registers carry C:D20-D22, C:Q13-Q14 and C:S14-S16. No owner accepted any policy here and no repair was applied.

## How the work was chosen

Ontology coverage counts showed three consequential user workflows with almost no canonical text: what a CI job sees from `test` (one mention), installing from a remote registry (no mention of its hash or token), and sync's recompile step. Each was followed from the actor's command through the deciding member to what the caller or operator observes:

```
  W1  zpm "test m" / Shell(cmd,1) ─► ExecutePhases ─► %Test ─► each Test resource OnPhase
                                                      │            └─► RunTest ─► ReportTestResults
                                                      │                     ├─ report file
                                                      │                     ├─ summary + FAILED lines
                                                      │                     └─ GetAllTestsStatus (count, consume)
                                                      └─► status ─► Shell ─► Terminate(1) or exit 0
  W2  install m ─► search (_ping, packages/m) ─► GetModule: metadata ─► bytes (registry URL, credentials)
                                                 └─► SHA-1 check ─► token ─► archive load ─► analytics
  W3  sync -test ─► load changed files ─► SyncCompile (resolve each resource) ─► commit hashes ─► SyncRunTests
```

## What is new, repaired or strengthened

| Kind | Owner | Distinction a reader can now predict |
|---|---|---|
| New | [build.test.outcome](../../../ontology/features/build-and-publish.g5.md#build.test.outcome) | Count rule; every resource runs after a failure; report file written before the failure check; `.csv` fails after passing tests |
| New | [build.test.outcome.not-fatal](../../../ontology/features/build-and-publish.g5.md#build.test.outcome.not-fatal) | `FailuresAreFatal=0` turns failing tests green, exit 0 |
| New | [build.test.outcome.empty](../../../ontology/features/build-and-publish.g5.md#build.test.outcome.empty) | Empty package passes with a warning; a named missing Suite folder fails (C:Q13) |
| New | [build.test.outcome.exit](../../../ontology/features/build-and-publish.g5.md#build.test.outcome.exit) | Only `Shell(cmd,1)` gives exit 1; `zpm "..."` exits 0 on failure |
| New | [repo.remote.download](../../../ontology/concepts/repositories.g5.md#repo.remote.download) | Credentials reach a different download origin (C:D20, C:Q14) |
| New | [repo.remote.download.integrity](../../../ontology/concepts/repositories.g5.md#repo.remote.download.integrity), [.token](../../../ontology/concepts/repositories.g5.md#repo.remote.download.token) | Exact-case hash (C:D22), empty hash unchecked, mismatch sends no report, token kept only after the check |
| New | [devtools.sync.test-stop](../../../ontology/features/developer-tools.g5.md#devtools.sync.test-stop) | `sync -test` stops at the first failing resource; `test` does not |
| Repair (I2E09, I3F02) | [build.test.outcome.nested](../../../ontology/features/build-and-publish.g5.md#build.test.outcome.nested) | Nested reader counts and removes only its suffix; parent slot kept; display skips empty slots |
| Repair (I3F20) | [devtools.sync.compile-after-resolve](../../../ontology/features/developer-tools.g5.md#devtools.sync.compile-after-resolve) | Survivors compile; none left means no compile; status OK; warning kept only in a whole-namespace sync |
| Repair (I2C04) | [devtools.sync.test-paths](../../../ontology/features/developer-tools.g5.md#devtools.sync.test-paths) | Outside paths keep folders; sibling prefix strips wrongly (C:D21) |
| Repair (I2E14 Verus) | [module.deployed](../../../ontology/concepts/modules.g5.md#module.deployed) | The lifecycle class is now the model input; swapped defaults are rejected |
| Repair (I2E07 domain) | [build.test.selection](../../../ontology/features/build-and-publish.g5.md#build.test.selection) | Narrowed: only the request is lowercased; admitted declarations make iteration 3's reading correct |
| Repair (I2E21) | [repo.search-order](../../../ontology/concepts/repositories.g5.md#repo.search-order) | Keyword tokens: split on spaces, empties dropped, spaces-only means no keywords |
| Stronger proof | C:D18 ([artifact.archive-admission](../../../ontology/features/artifact-download.g5.md#artifact.archive-admission)) | Natural registry consumer: empty-hash junk ends as a manifest error, not an installed module |

The public prior cases above are repairs, not discoveries. I3F05 is also answered by the new outcome block. I3F11's token-after-SHA1 neighbor is covered by `repo.remote.download.token`; the registry workflow itself was chosen from coverage counts and the source, but it shares that reader with a published case. The [alias check](../BUGS.md#alias-check-for-iteration-4-claims) found related but distinct P:F-REPO-11, P:F-REPO-1 and C:D17.

## Corrections and withdrawals

`build.test.selection` previously said the phase list matches "without regard to case". That holds only for admitted declarations; the stored value is not folded. The statement was narrowed, and `lemma_admitted_agree` proves where iteration 3's reading still holds. No other earlier claim was weakened. Iteration 3 contracts, native receipts and withdrawals are byte-identical; their 35 units and 56 controls were re-run in a scratch extraction ([inherited-replay](inherited-replay/)).

## Evidence and commands

`python3 .g5/uplift/consolidated/iter4/run_all.py` runs 24 verified units with 38 wrong variants rejected, a fresh-container native suite of 44 cases plus 4 exit-code runs, 58 observed fields replayed as Verus assertions with 4 flipped controls rejected, and the validator. The [index](INDEX.json) tags each pinned member as direct decision or reader/caller. [validation.json](validation.json) shows that only eight declared canonical and register files changed.

## Blind spots and where this stopped

- A real `verify` nested inside a running test method; `verify` namespace cleanup errors; `ManagerClass` overrides; a Suite folder that exists but is empty.
- Registry TLS, redirects, proxies, non-default token methods, temporary-file cleanup after a mismatch, and dependency installs from a registry.
- The full `sync` command around the recompile step, sync file routing (whether C:D21 is reachable) and `sync -test` natively.
- Keyword tokenization has no native case.
- Not started: web application deploy/remove, uninstall of dependents, update steps with mirroring, and ZPM-to-IPM migration. These are the next consequential candidates by coverage count.

## Owner decisions needed

C:Q13 (should an empty test package fail CI), C:Q14 (registry credentials on another host), plus all earlier open questions, unchanged.
