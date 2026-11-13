# API GC independent review 3

Independent read-only discovery on `design/git-abstraction`, 2026-10-02. Scope is
the increment after `99f7961d`, including the new
`backend/g5-scm/src/native/maintenance.rs`, its API owner, shared native runner,
guards, tests and affected CLI error consumers. No production/test/ontology edits
or delegation were performed. Investigative files are isolated under
`/tmp/g5-api-gc-review-3`.

## Inputs and snapshot

Read the repository AGENTS instructions, ontology-review skill and design-review
guide; started at the saved plan's `Ontology changes and governance`, then read
the request, design, API GC execution increment and applicable governing bodies
and metadata. No peer report or main-agent review narrative was read before this
report. The handoff supplied the cached SCM artifact identity/location and the
fact that main owns API/DB execution; it supplied no defect hints.

`shasum -a 256 -c /tmp/g5-api-gc-review-snapshot/sha256.txt` matched all nine listed
files. There is no ontology-body/footer diff against `99f7961d`; the saved plan
adds the GC increment and its governing operations link. The broader ontology
diff in the plan is the prior approved migration, rather than a new meaning
change in this increment.

Before executing cached tests, refreshed these identities:

- SCM executable `target/debug/deps/g5_scm-08d6ca130345ed39`, SHA-256
  `e02c3c33ee531f93b38ecff5629f5e8ca4672fa7e1fa389e87397d44b65d4a2f`.
  Its depfile includes `native/maintenance.rs`; all fourteen declared SCM source
  inputs are older than the executable; fingerprint features are
  `default,libgit2,native`. It lists/runs the six new API GC tests.
- Affected CLI executable `target/debug/deps/g5d_cli-4b8cae290564ff67`, SHA-256
  `847d10f39f76fc942e1e411de34c2d41ac5a8258d594321953117c0a3595050d`,
  newer than both the changed SCM error source and the CLI test source.

These are reused build artifacts, not fresh whole-crate builds. The architecture
guard was compiled freshly from the exact current source using cached rlibs,
with `CARGO_MANIFEST_DIR` set to the actual API crate.

## Initial findings

### F1 — P2: update the affected CLI partial-effect regression fixture

`backend/g5-scm/src/native/mod.rs:52` changes the shared Partial display message
from a remote-ref-specific assertion to `Git confirmed an effect`. This is the
right generalization for successful GC followed by failed inspection, and real
push failures retain their confirmed-ref detail at lines 492–497. However,
`backend/g5d-cli/src/process.rs:311` still constructs a Partial error without
that detail and expects the former shared message.

Executed the exact affected test from the refreshed CLI artifact:

```
target/debug/deps/g5d_cli-4b8cae290564ff67 --exact process::tests::native_git_failures_keep_target_and_effect_in_text_and_json --nocapture
```

It fails at `process.rs:330`, `message.contains(expected)`. This is a concrete
affected-consumer test regression, not evidence that actual push results lose
their confirmation: the SCM confirmed-ref tests pass. Fix the fixture to reflect
the new general effect contract and supply/assert the known push completion in
the specific error detail. Preserve text/JSON target, failure stage, effect and
retry guidance assertions. Recheck the exact CLI test plus real confirmed-push
and GC partial-completion cases. Governing: `git_outcomes`, compatibility and
the design's requirement to preserve affected consumers.

### F2 — P2: the new real guard pins private helper names despite its passing calibration fixture

The added API inventory at
`backend/g5-api/tests/authoritative_workspace_reader_guard.rs:41` requires the
private helper `gc_repo_if_needed`; the added SCM inventory at line 917 requires
private `loose_object_count`. The check at lines 949–951 permits renaming an
unrelated helper in a synthetic source, but does not test either private name
that the actual invocation pins.

Executed a fresh copied-source guard probe with only global token renames in
the actual two source texts (production files untouched):

- `gc_repo_if_needed` → `collect_one_repository`, including all call sites;
- `loose_object_count` → `inspect_loose_objects`, including all call sites.

The unchanged real required-operation lists reject these harmless refactors:

```
maintenance/repo_gc.rs: missing guarded operation gc_repo_if_needed
g5-scm/native/maintenance.rs: missing guarded operation loose_object_count
```

The probe's assertion that both refactors are allowed fails. Evidence is
`/tmp/g5-api-gc-review-3/guard-counterexample.rs` and
`guard-counterexample.log`. Fix by anchoring missing-target protection to the
stable public owner operations (`run_repo_gc`, `do_repo_gc_sweep`,
`collect_garbage`) while continuing to scan every production function for
forbidden native construction. Calibrate against actual private helper renames
and retain the disappearing-public-target negative control. Governing:
`git_guard`, `g5.quality.review.guards`, and the saved increment's explicit
helper-rename acceptance case.

## Commitment and path ledger

| Commitment | Actual path and counterexample/check | Result or gap |
|---|---|---|
| `git_execution`, `g5.architecture.commands` | `run_sweep_locked` → `gc_repo_if_needed` → `NativeGit::collect_garbage`; compare old count/gc/count implementation and scan API production native construction. | Inspection: API retains SQL, storage selection, threshold input, sequential scheduling, logging and error isolation. SCM owns count/GC/count mechanics. Exact-source guard passes and rejects aliased/qualified process construction. No raw-argument or environment escape hatch was added. |
| `git_native_runner` | `NativeGit::api` → `command` → `Invocation::api_context`/`supervise`; successful collection versus hung fake GC with deadline or cancellation. | Executed: all six GC tests pass, including active child reaping for timeout and explicit cancellation. Shared cancellation/descendant, descendant-held-pipe deadline, SSH-file cleanup and confirmed-push deadline tests pass. API cancellation explicitly awaits the active operation before returning to unlock. DB lock ordering remains main's execution responsibility. |
| API/CLI context compatibility | Compare old API `env_clear` + fixed PATH with new context; inspect unchanged CLI context construction and every replaced `runtime_config` condition. | Executed environment probe passes with hostile HOME/PATH/GIT_DIR/GIT_CONFIG_PARAMETERS/GIT_CONFIG_GLOBAL and ambient secrets. Fixed API PATH/bare-name resolution and zero/empty setup refusal pass. CLI inherited-control regression passes. API adds explicit locale/prompt suppression and narrows `safe.directory=*` to the supplied path; LocalRepoStorage supplies canonical absolute paths. No global/system writes. Repository/system config preservation is inspection evidence; no isolated system-config runtime test was executed. |
| `git_locations` | Storage `local_path` → native bare/worktree materialization predicate and Git `-C`. | Executed: checkout, empty bare repository, missing path and non-repository skips pass. Ordinary API storage returns canonical roots. No host/guest/dev-mirror layout policy was moved. Linked-worktree layout was inspected but not separately executed. |
| `git_ref_updates`, preserved working state | GC's existing command versus before/after refs, HEAD, raw index, dirty file and repository config. | Executed real-Git packing test passes, with unchanged refs/HEAD/index/checkout/config and a real packfile. No stronger pruning/retention or cross-reader serialization guarantee is claimed. |
| Supplied threshold and existing GC semantics | Below-threshold count must create no pack; exact threshold must collect; missing materialization must skip. | Executed positive and negative controls pass. SCM accepts the owner's threshold without inventing policy. Only existing `git gc` is issued. |
| `git_outcomes` | `loose_object_count`, GC result, final inspection; malformed/missing count, nonzero read, GC nonzero, completed GC followed by failed read. | Executed: refusal/unknown/partial categories, target and operation evidence pass. Post-GC failure explicitly names completed GC. Shared actual push confirmation still passes. F1 identifies an affected CLI fixture regression. |
| `git_retry`, recovery ownership | Inspect entire increment and attempt log in malformed/GC-error/final-read-error test. | Executed attempt counts are respectively one read, one read + one GC, or read + GC + read. No automatic replay, sleep/retry, tracking columns/tables/queue/worker was added. Existing periodic sweep remains the maintenance owner's schedule. |
| `g5.scope.capacity-recovery.gc` and API owner isolation | Sweep session advisory lock, `for repo_id`, per-repo warning branch, shutdown cancellation handoff. | Inspection supports existing one-at-a-time scheduling/error isolation and cooperative cleanup-before-unlock. Existing and new DB integration tests were read; not executed by this reviewer. The future retained-source/active-reader policy remains explicitly unimplemented, not accidentally claimed complete. |
| `git_guard`, cases/calibration | Fresh exact guard suite, alias/raw qualified construction, missing owner, fixture exemption and helper probes. | 13 exact-source tests pass, including positive/negative cases; actual private helper renames fail in F2. Guard is structural, not a full Rust type/macro proof. |
| Minimal API, abstraction, information hiding, dependencies | New `ObjectMaintenance` and one public `collect_garbage` operation; private maintenance/count/runner mechanics; inspect SCM manifest and API imports. | Inspection: useful complete typed operation, no SQL/orchestration dependency introduced, no new public backend handles or generic command/environment maps. Native-only build was not freshly executed in this review. |
| Authorized content reader / projection owner | Existing exact reader guards and unchanged content/projection ownership. | All exact guard tests pass; this GC operation does not add a content-reading API or bypass the authorized reader. Full reader behavior is outside this increment. |

## Executed evidence and limits

- Six GC runtime tests: **6 passed** (five substantive tests plus the child
  fixture; the hostile-environment parent executes that child with controls).
- Exact-source architecture guard: **13 passed**, compiled independently;
  receipt `/tmp/g5-api-gc-review-3/guard-results.log`.
- Entire native test module: **23 passed, 3 blocked/failed**. All three failures
  occurred at local TCP listener binding with `Operation not permitted`, before
  their HTTP/relay assertions. Receipt
  `/tmp/g5-api-gc-review-3/native-suite.log`. This does not establish those three
  network guarantees or a code defect in them.
- Exact CLI partial-effect test: **1 failed**, F1.
- Additional actual-source helper-refactor calibration probe: **1 failed**, F2.

No API/DB test or full repository build was run here; main owns those resources.
No Linux run was executed in this independent pass. Inspection of the production
composition confirms indexer shutdown joins maintenance handles. `role=all`
already leaves handles detached when the HTTP server returns; end-to-end process
shutdown in that existing mode is not established by the new direct-sweep test.
That pre-existing composition behavior is a verification boundary, not presented
as a newly introduced defect. Cancellation while SQL enumeration/pool acquisition
or synchronous storage probes block is likewise outside the child-invocation
deadline evidence.

## Proposed follow-up

Repair F1 and F2 coherently and rerun their discriminating checks, GC outcomes and
the affected native/CLI regressions. Incorporate main's API/DB receipts before
claiming lock/error-isolation completion. Update only the relevant existing
ontology metadata (`git_execution`, `git_native_runner`, `git_outcomes`,
`git_retry`, `git_locations`, `git_guard`, and any assessed principle footers)
with scoped evidence and retained gaps; no governing meaning amendment is needed.
Do not broaden this increment into new retry state or the parked sibling-object
confidentiality redesign.

## Post-initial-report execution follow-up

After saving the initial findings, reran only the three loopback-blocked native
tests with `require_escalated` to permit their temporary local listeners. All
three passed (HTTP credential/redirect scope and the two workspace-runtime relay
variants), in 0.99 seconds. Receipt:
`/tmp/g5-api-gc-review-3/native-loopback-rerun.log`. Combined with the initial
native-module run, all 26 native tests now have passing execution evidence on
this macOS host; the initial sandbox failures remain recorded above. This does
not remove the API/DB, Linux or production-process shutdown verification limits.
No peer report or main narrative was read for this follow-up.

## Bounded repair recheck

At main's request, performed one read-only recheck of F1/F2 and the specified
adjacent repairs. The initial findings above remain unchanged. Main supplied
the adjacent issues and its execution totals for this pass; this section is a
repair recheck, not additional independent discovery. No production, test or
ontology edits were made, and no further delegation occurred.

Frozen repaired source hashes are saved in
`/tmp/g5-api-gc-review-3/repaired-source-sha256.txt`. Refreshed cached executable
identities before running them: SCM
`9a5944389d18e090160be7283afe97f2a1822b087ae17e51a56f510a8ee33400`
and CLI `96e7e91a612abb90d9ac5f5c8094efdbb5fc1b70a307cd0793a9097ccbe50b19`.
Both are newer than their affected source inputs. Compiled the repaired exact
guard source freshly again with cached dependencies.

| Obligation and repair | Recheck and disposition |
|---|---|
| **F1 / `git_outcomes`: partial effects remain meaningful in CLI text/JSON** | The fixture now asserts the general confirmed-effect message and supplies/asserts the specific confirmed remote-ref detail and failed stage. Executed the original exact CLI test: **1 passed**. Target, diagnostic detail, effect, error code and retry guidance assertions remain. **F1 closed.** |
| **F2 / `git_guard`: harmless private helper renames remain valid** | Required identities now anchor the API public `run_repo_gc`/`do_repo_gc_sweep` and SCM public `collect_garbage`. Executed the new actual-source rename test for both previously failing private helpers; it passes and still rejects removal of the public owner operations. The complete freshly compiled source guard suite reports **14 passed**. **F2 closed.** |
| **Adjacent guard calibration: permit process utility use, reject native construction** | `std::process::id()` and imported `std::process::{self, Stdio}` with `Stdio::null()` are accepted. Qualified/aliased Command creation, wildcard `Command`, and `{self, ...}` followed by `process::Command` are rejected by the executed calibration cases. Inspection confirms `self` imports resolve to the process module and process-module imports alone are no longer violations. **Repaired and checked within the structural guard's stated limits.** |
| **`git_outcomes`: API aggregate/log consumer preserves confirmed GC after failed inspection** | Inspected `gc_repo_if_needed`: only `FailureEffect::Partial` is mapped to `Ok(true)`, with a warning retaining the typed error; Refused and Unknown still return errors without incrementing `repos_packed`. `collect_garbage` currently produces Partial only after successful GC and failed final count, so this consumer is using the actual operation's semantics. Read the public-sweep DB regression: it performs real packing, injects failed final inspection, checks `repos_packed=1`, captures the warning/cause/retry guidance, and exercises Refused/Unknown controls with exact attempt logs and actual packfile checks. **Source/test design supports closure; DB execution was reported by main, not rerun here.** |
| **`git_native_runner`: cleanup precedes sweep unlock** | Source still cancels and awaits the active operation before leaving the locked sweep. The strengthened DB test holds a separate connection, starts a blocking advisory-lock contender, and checks child liveness when that connection acquires the lock, as well as before cancellation and after sweep return. **Main reports the DB test passed.** This is stronger observed ordering evidence, but the queued notification precedes submission of the database query, so it is not deterministic proof of every scheduler interleaving; source ordering remains part of the conclusion. |
| **API executable lookup retains a usable configured PATH variant** | Inspected the shared private `executable_on_path` used by API bare-name and CLI resolution: it skips non-files and candidates lacking any execute bit before selecting a candidate, then the caller canonicalizes once. Executed the new lookup test with a non-executable first match and executable second match, plus a path containing no executable candidate: **passed**. Existing API fixed-path/hostile-environment test also passes. **Adjacent defect repaired.** |

Executed in this recheck: **14 exact-source guards, 7 SCM GC/lookup tests, and
the 1 affected CLI test passed**. Logs:
`repaired-guard-results.log`, `repaired-native-results.log`, and
`repaired-cli-results.log` under the isolated review directory. The seven SCM
tests include the six original GC tests and the new lookup test; real packing,
threshold refusal, malformed output, known completion, no replay, environment
isolation and deadline/cancellation cases therefore retain execution evidence.

Main separately reports **193 SCM tests, all four GC DB integrations, and the
exact CLI/14 guard suites passing**. Those broader totals are supplied evidence;
this reviewer independently reran only the bounded checks above. The final
Linux native-only run was still ongoing at handoff, so no result for it is
claimed here. Earlier role=`all` process-shutdown, SQL/storage-blocking,
real-deployment and structural-analysis limits remain. No unresolved finding
remains in this assigned repair scope, and no governing ontology meaning change
is needed for these repairs.
