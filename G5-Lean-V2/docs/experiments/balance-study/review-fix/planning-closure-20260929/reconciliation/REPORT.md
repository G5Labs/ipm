# Final bounded repair reconciliation

Both candidates remain below eight supported scopes. The recommended **source/product** labels are **c1: 5S, 1P, 2V** and **c2: 4S, 1P, 3V**. The only overall-label correction is c2 I8, from V to P. Its failing frozen synthetic fixture remains a V at the API/database-fixture boundary; the original assessment and 4S/4V tally are preserved unchanged. This is an explicit boundary correction, not removal of a test or a passing replacement result.

This reconciliation started at 2026-09-29 02:18:49 UTC and is bounded to 15 minutes. It reads the original PLAN, GAPS, scope map, original e1/e2 assessments and actual producer/consumer source. It performs no repair, worker feedback, ontology change, whole-suite rerun, or provider/VM/browser operation. The original reports remain at `../deliveries/e1/source/artifacts/source-assessment.{md,json}` and the e2 counterpart. Their byte hashes and candidate preservation are recorded in `inputs-before.json` and `preservation.json`.

## Uniform evidence rule

S means support at the stated source/local boundary, with ordinary material producer paths inspected. P means substantial support with a material unresolved condition or reachability gap. V means a demonstrated contract failure at the boundary being claimed. U means that the boundary itself was not established or executed. Fixture-only failures retain V for the fixture, while their applicability to current production populations can remain unresolved. Neither missing live-provider evidence alone nor any source/local S is converted into a real-provider claim.

A single source/product tally should not treat a supplied filesystem condition as conditional for c1 while silently treating a supplied source-population condition as established for c2. Conversely, the original c2 4S/4V is defensible as a **frozen API/fixture regression tally**. It is not a count of four reproduced current ordinary producer failures. These views must remain separately named.

## Each new finding applied to both candidates

| Finding | c1 / e1 | c2 / e2 |
|---|---|---|
| Upload-valid 255-byte ASCII filename | Does not transfer: ChatService derives a bounded physical basename (180 bytes), retained by its materializer. Adapted actual-helper test passes. | Applies: adapter uses `index-displayName`, producing a 257-byte component; preserved actual-helper failure is `ENAMETOOLONG`. Valid upload reachability is source-supported. I1 remains V. |
| Unknown settled usage blocks a healthy second workspace | Does not transfer: reservation catches its accounting-unavailable conflict, excludes that workspace and keeps searching. Exact e2 scenario transferred to e1 passes and selects the healthy workspace. | Applies: `spend_admission_allows(...).await?` returns before exclusion; maintenance breaks the tenant dispatch loop. Preserved actual writer/reservation test fails. I6/I7 remain V. |
| `/mnt/g5` requires writable guest preparation | Applies conditionally: actual materializer is called with root `/`; module runs under provisioned UID 1000; inspected guest path does not prepare this attachment root. Existing actual-helper unwritable-mount analogue fails. I1 remains P. | Exact condition does not transfer: materializer uses HOME/.g5/turn-attachments; dispatcher hands provider home to module UID and chmods HOME to 0700. No `/mnt/g5` attachment write occurs. Real guest execution remains U. |
| Existing matching file with mode 0666 is accepted as read-only | Applies to supplied existing-file condition: integrity checked but mode neither checked nor reset. Preserve local failure and unestablished ordinary population. | Exact reuse condition does not transfer: fresh generated directory produces a different path. However a fresh actual helper output was mode 0644, directory 0755, and no chmod is present. Thus c2 does not establish read-only inode materialization either. See qualification below. |

The permission contrast does **not** certify c2 privacy or read-only behavior. Its runtime tells the provider that files are “read-only copies” (`runtime.rs:839`), and shared `ChatAttachmentContext` documentation describes read-only materialization. Its fresh files are owner-writable in the local probe; the production HOME ownership/mode provides the enclosing privacy boundary. There was no cross-user access, mutation by a real provider, guest confinement test, or exploit demonstration. The stronger c1 0666 pre-existing-file condition and c2 ordinary fresh-file writeability are different mechanisms. No extra denominator row is added; c2 I1 is already V from valid filename failure.

A diagnostic assertion in the new c2 contrast probe incorrectly assumed TempDir itself would be 0700. The retained log shows **0755 directory / 0644 file**, and the assertion failed. The earlier assertions establish a fresh path distinct from the planted 0666 fixture with correct bytes. This result is preserved as a correction to the evaluator's directory assumption; it is not presented as a reproduced breach through production HOME, which the source sets to 0700. No test expectation was silently rewritten and no rerun hid the failure.

## I8 and G7

Both candidates' actual ordinary single-agent sync producer → fixture terminal source → retry → public progress tests pass. Their docs-sync route supplies typed sync context; JobService uses the sync profile to allocate one agent carrying the whole target context. Only uplift selects the bootstrap graph path. The ordinary agent has empty workspaceTarget and no selected src_files, so each repair explicitly retains the full authoritative context for that one-agent source. Progress keeps the retry executable task separate from completed contextual seeds and counts new tasks.

The frozen selected-subset fixture is different: it starts with a supplied multi-agent graph, changes job_type/sync_context by SQL, and supplies selected src_files without per-agent repository identity. In c2, the multi-agent filter requires `agent.workspaceTarget.repoId`, obtains no selected target pairs, and drops context. Its public progress direction becomes null. c1 instead maps unambiguous selected paths against authoritative target context; that same fixture passes. c1 declines ambiguous cross-repository same-path claims rather than inventing attribution.

The failing c2 fixture and causal source branch are real. The bounded source trace does not establish a current producer of that exact multi-agent sync shape. It also does not prove no such retained/custom source can exist. Accordingly:

- c2 actual ordinary sync producer support: **S**.
- c2 supplied selected-subset API/DB regression case: **V**, unchanged.
- current production reachability of that exact source shape: **U**.
- c2 broader I8 source/product coverage: **P**, recommended correction from original V.
- c2 G7: retain **partial**, with the ordinary-producer defect closed and conditional subset loss explicit.
- c1 I8: retain **S** at its stated source/local boundary; both the actual-producer and supplied subset tests pass. No full browser or alternative graph-population certification is implied.

This avoids describing G7's repaired ordinary producer as failing, while also avoiding a blanket I8 pass. Both actual-producer tests use fixture-supplied terminal failure; neither executes a provider-completed source sync.

## Recommended labels

| Scope | c1 original → recommended | c2 original → recommended | Reason |
|---|---|---|---|
| I1 | P → P | V → V | c1 guest materialization/environment and conditional permissions remain incomplete; c2 valid filename failure remains. |
| I2 | S → S | S → S | No new contrary evidence against fresh identities and unchanged originals. |
| I3 | S → S | S → S | Preserve exact request identity, replay and transaction evidence. |
| I4 | S → S | S → S | Preserve graph/specification refusal and rollback boundary. Valid filename failure is I1, not evidence of partial graph writes. |
| I5 | S → S | S → S | Preserve current admission/Stop/read-replay evidence. |
| I6 | V → V | V → V | Both lose an explicit unknown token dimension upstream of repaired accounting; original producer-to-reservation failures remain. |
| I7 | V → V | V → V | Same accounting failure; c2 additionally has cross-workspace selection starvation. Supported ownership, settlement and capacity subcases remain supported. |
| I8 | S → S | V → P | Conditional selected-subset source reachability is unresolved; ordinary producer passes. |

Thus c1's 5S/1P/2V remains fair. c2's original 4S/4V is fair only with its fixture boundary stated; use 4S/1P/3V for a comparable conservative source/product summary. The frozen independent backend results remain c1's original pass population and c2 **23 passed / 1 failed**. This reconciliation does not recompute a quality percentage or population reliability estimate.

## Executed checks, adaptations and limits

1. `probes/e1-accounting-isolation.rs` transfers e2's scenario and setup helpers, changing only helper path from e2 to e1 and omitting unrelated tests. It calls actual outcome writer, sets the affected workspace's cost threshold, queues one blocked and one healthy task, and calls actual reservation. **1 passed**, `logs/e1-accounting-isolation.log`. No unknown flag-schema adapter was necessary because this case does not query those flags.
2. `probes/e1-maximum-filename.rs` compiles actual unchanged e1 adapter modules. It adapts e2's input to c1's camelCase ChatAttachmentContext with readOnly and providerMountPath, calls c1's materializer signature, and uses the exact copied ChatService pure filename helper and its constants (also preserved separately). **1 passed**: 255 display bytes → 180 physical bytes. This is local helper composition, not an executed public upload/retry/provider journey.
3. `probes/e2-private-directory.rs` compiles actual unchanged e2 adapter modules and uses a short ordinary filename plus a planted matching 0666 file in another directory. It confirms fresh-path nonreuse and correct bytes, then fails the evaluator's unsupported TempDir-0700 expectation. **1 diagnostic assertion failed**; observed modes and limitation retained in `logs/e2-private-directory.log`.

All Rust commands sourced the candidate `.study/env.sh`, used at most two Cargo jobs, and one test thread. Only e1 private Postgres/Redis were started; they were stopped after the database probe. Final e1/e2 Postgres/Redis port checks are in `preservation.json`. New files in evaluator repos are test symlinks only. No tracked candidate bytes changed, and both original assessments and preserved candidate patches hash identically before/after. `source-evidence.md` records independently inspected line-numbered excerpts.

Remaining uncertainty includes real guest attachment preparation and confinement, provider-native exact-pin behavior, real browser recovery, real VM lifecycle races, complete retained/custom sync graph populations, and exhaustive concurrent schedules. Existing unrelated evaluator findings (including c2 generated-catalog drift and unresolved question/tool selection) remain as originally reported. Nothing in this reconciliation authorizes or performs another repair.
