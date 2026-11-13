# API GC execution review — reviewer 2

Review of the increment after `99f7961d`, including the new
`backend/g5-scm/src/native/maintenance.rs`, on `design/git-abstraction`.
Discovery was read-only; no implementation, test or ontology edits were made.
Investigative source, executables and receipts are in `/tmp/g5-api-gc-review-2/`.

## Review provenance and scope

Started with the pinned design's **Ontology changes and governance** inventory, its
recorded ontology diff, original saved user request and the **API garbage-collection
execution** section; read the applicable contracts and paired metadata, repository
AGENTS, ontology-review skill and design-review guide. Scope covers ordinary
correctness, ontology conformance, architecture/minimal implementation, the shared
runner's actual policy and affected consumers, including CLI error rendering.

Independence qualification: the frozen patch contains the main review narrative. An
early unfiltered display of that patch inadvertently included its 28-line API-GC
summary before this report was saved. No peer reports were read. Findings below come
from source tracing and independently executed checks, not that summary's assertions;
this is a fresh reviewer context but not a fully unexposed independent discovery.
The supplied handoff also named a cached SCM test binary and the exact-source guard
build option; these were treated as setup hints, not passing evidence.

The frozen SHA-256 values for all seven changed/new Rust files matched the checkout
before execution. SCM test executable
`target/debug/deps/g5_scm-08d6ca130345ed39` had SHA-256
`e02c3c33ee531f93b38ecff5629f5e8ca4672fa7e1fa389e87397d44b65d4a2f`, listed 192 tests,
and was newer than every SCM source; its Cargo fingerprint includes native/libgit2.
CLI executable `target/debug/deps/g5d_cli-4b8cae290564ff67` had SHA-256
`847d10f39f76fc942e1e411de34c2d41ac5a8258d594321953117c0a3595050d`, was newer than
all SCM/CLI source and had a current Cargo fingerprint. These are refreshed cached
artifact identities, not a claim that this reviewer rebuilt those crates.

## Initial findings

### F1 — P2: preserve confirmed GC completion in API sweep accounting

**Rule:** `g5.architecture.git_outcomes`, `g5.data.outcome-evidence`, and the pinned
increment's explicit requirement that failure after confirmed GC records that
completion even if the final count is unavailable.

**Path:** `native/maintenance.rs:42-53` returns `FailureEffect::Partial` after a
successful `gc` when final counting fails. `maintenance/repo_gc.rs:255` propagates
that error via `?`; `run_sweep_locked` at lines 222-231 increments `repos_packed`
only for `Ok(true)` and logs every error as “repo skipped after error”. If this is
the sweep's only eligible repository, `run_repo_gc` subsequently logs “nothing to
pack” although GC completed. The detailed warning retains SCM's confirmation, but
the derived completion count and summary contradict it.

**Counterexample:** first count returns 9; `gc` succeeds; final count exits nonzero.
The executed SCM failure test demonstrates `Partial`, the completed-GC detail and
exactly `count → gc → count` attempts. Inspection of the API's sole result consumer
then proves that result contributes zero to `repos_packed`. A DB-backed reproduction
of the final public sweep count was not executed by this reviewer (main owns API/DB
execution).

**Proposed repair/check:** preserve known GC completion in the sweep result/count
while retaining the failed-inspection warning and unknown final loose count. Do not
infer completion for `Unknown` or `Refused`. Add a public sweep regression using
the three-stage failure above, expecting one confirmed repack, plus nonzero-GC and
initial-read failure controls expecting zero. Keep per-repo isolation and no replay.
This is a completion-accounting defect, not evidence of incorrect packing.

### F2 — P2: migrate the affected CLI partial-error rendering regression

**Path:** `native/mod.rs:52` correctly generalizes `FailureEffect::Partial` display
text from a remote-ref statement to “Git confirmed an effect”. The unchanged
`g5d-cli/src/process.rs:307-333` test still constructs a partial error whose detail
does not name a confirmed ref, then requires “confirmed the requested remote ref”
at line 311. That expectation is no longer supplied by Display.

**Executed evidence:** the current CLI executable's exact
`process::tests::native_git_failures_keep_target_and_effect_in_text_and_json` test
fails at line 330 (`message.contains(expected)`); receipt `cli-error.log`.
The current SCM tests for protocol-confirmed push outcomes and a real push followed
by a deadline both pass, so this is a stale affected consumer test rather than a
demonstrated loss of production push evidence.

**Proposed repair/check:** update the fixture/expectations to the generic partial
effect contract and separately assert that operation-specific detail survives both
text and JSON. Keep the real push confirmation tests; do not restore push-specific
wording for GC errors. Rerun this exact CLI test.

## Commitment coverage and evidence

| Commitment | Actual path | Counterexample / positive control | Result or gap |
|---|---|---|---|
| `git_execution`, `g5.architecture.commands`, `g5.design.minimal-api`, `g5.design.information-hiding` | API `gc_repo_if_needed` → `NativeGit::collect_garbage`; private `Invocation` | API raw process/backend construction vs typed complete GC call | Source inspection preserves ownership and closed arguments/environment; exact-source guards pass. No SQL/scheduler policy moved into SCM. |
| `git_native_runner`, `g5.design.concerns`, `g5.design.abstraction` | `NativeGit::{api,command}`, `Invocation::{api_context,supervise}` | API inherited controls/secret variables vs fixed PATH; CLI context remains inherited/runtime-config-specific | Executed API environment child test passes and sentinel global file stays unchanged. CLI context branches preserve prior code. Bare name resolves once to absolute binary. |
| `git_native_runner`, `g5.design.cleanup-proof` | sweep cancellation → native cancellation → awaited active operation → supervisor kill/wait → SQL unlock | Active busy GC child cancelled/deadlined; descendant effect after cancellation | Executed GC deadline/cancellation and existing descendant-cancellation tests pass. Inspection shows cleanup await before unlock. DB path not executed here; ordering-test limitation E1 below. |
| `git_locations` | `RepoStorage::local_path` → SCM layout check | Missing directory, nonrepository directory, bare repo, checkout | Executed GC test passes all four. Both real storage owners supply canonical repository roots; adapter owns bare/worktree detection. No new layout kind or root reconstruction. |
| `git_ref_updates` and preserved GC semantics | actual native `gc`, existing per-repository/system config | Exact threshold admitted vs higher threshold skipped; refs/HEAD/index/config/dirty checkout unchanged | Executed real-Git test demonstrates packing and all unchanged-state controls. This does not establish retained-source/reader serialization or new pruning safety. |
| `git_outcomes`, `g5.data.outcome-evidence`, `g5.architecture.recovery` | count parser → GC → final inspection → API and CLI consumers | Malformed/missing count; nonzero read; nonzero GC; successful GC then failed inspection; accepted push then failed completion | SCM executed controls pass, including exact attempts and partial confirmation. API accounting violates the completed-effect rule (F1). CLI rendering test fails (F2). |
| `git_retry` | linear GC stages; existing scheduled sweep | Error attempts are never replayed; valid next stage occurs only after successful prior stage | Executed attempts assertions pass; inspection finds no retry state, new queue/worker or automatic operation replay. Existing sweep scheduling remains at API owner. |
| `git_guard`, `g5.quality.review.guards`, `g5.quality.review.cases`, `g5.design.enforcement` | `typed_git_violations`, migrated-owner inventory, new maintenance module inventory | Raw native alias/qualified call; missing production owner; git2 alias vs test fixture and renamed private helper | 13 exact-source repository tests plus 1 independently added calibration test pass (14 total). Added calibration checks both API owner names and transitive process-import aliases. Guard remains structural, without type/macro expansion proof. |
| `g5.architecture.dependencies`, `g5.architecture.compatibility` | API core → SCM native; new private maintenance module; `Context` enum replaces optional CLI config | Upward API dependency or libgit2 reference in GC vs native-only execution | No new manifest dependency or upward import by inspection; no raw handle/process capability exported. Native-only Cargo build not rerun by this reviewer; main owns refreshed build evidence. |
| `g5.scope.capacity-recovery.gc` | unchanged API enumeration, sequential loop, advisory lock and threshold config | Multiple sweeps/erroring repo vs retained-source/reader concurrency | Scheduling and per-repo error isolation preserved by inspection. Existing DB tests inspected, not run here. Future reader/retention serialization remains explicitly unimplemented and is outside this increment. |

## Executed checks and limits

- Cached SCM `api_gc` selection: 6 passed, including one environment child entry
  that is also actually invoked by the parent environment test (`scm-api-gc.log`).
- Cached SCM push outcome, real-push deadline and descendant cancellation tests:
  3 passed (`push-outcomes.log`, `push-deadline.log`, `cancellation.log`).
- Exact current guard source, compiled directly with cached rlibs into `/tmp`, plus
  reviewer calibration: 14 passed (`guard.rs`, `guard`, `guard.log`). The original
  13 tests are unchanged in that investigative copy.
- Cached CLI exact affected rendering test: 1 failed, as F2 (`cli-error.log`).
- Scoped `git diff --check`: passed.

**E1 — ordering evidence limit:** the new DB test checks that the lock is held
before cancellation and that both child exit and unlock have happened after the
sweep returns. It would still pass if the implementation unlocked immediately on
cancellation and only then awaited child cleanup before returning. Thus its assertions
do not discriminate the advertised **before unlock** guarantee. The current code
orders them correctly by inspection. Strengthen the test with a concurrent lock
contender that observes child liveness at acquisition after cancellation, or another
deterministic boundary observation; main should preserve this distinction even if the
current DB test passes.

No API/DB execution, full CLI suite, native-only rebuild, Linux execution, NFS-hang
simulation, production deployment, retained-source pruning or sibling-object
confidentiality testing was performed here. The latter redesign remains parked by
the user's instruction. No broader Git/provider migration completion is claimed.

## Proposed ontology follow-up

No governing meaning amendment is needed. After coherent repairs and checks, update
the existing `git_execution`, `git_native_runner`, `git_outcomes`, `git_locations`,
`git_guard` and `git_retry` metadata to include the bounded API-GC execution evidence,
retaining wider migration and reader/retention limits. Keep F1/F2 and E1 receipts and
dispositions in the main review artifact; do not convert an unexecuted public outcome
or ordering guarantee into a runtime pass.
