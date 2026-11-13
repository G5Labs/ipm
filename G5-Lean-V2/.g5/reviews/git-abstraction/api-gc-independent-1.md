# API GC execution — independent review 1

Initial findings saved before reading peers' reports or the main review narrative.
Discovery was read-only; only this report and isolated `/tmp/g5-api-gc-review-1/`
investigative files were written. No delegation or production/test/ontology edits.

## Scope and authority

Reviewed the frozen increment from `99f7961d` on `design/git-abstraction`, including
the untracked `backend/g5-scm/src/native/maintenance.rs`. All nine entries in
`/tmp/g5-api-gc-review-snapshot/sha256.txt` matched before and after investigation.
The main review file was hashed but not read. The supplied known-gap hints were
the cached SCM executable and direct guard compilation technique, not findings.

Read the plan's `Ontology changes and governance` first, its actual recorded
ontology diff, original request/clarifications, relevant design body, and `API
garbage-collection execution`. This increment adds the GC implementation section
but no ontology body change. Governing Git blocks and paired metadata read:
`git_owner`, `git_execution`, `git_native_runner`, `git_outcomes`, `git_retry`,
`git_locations`, `git_guard` (all `g5.architecture.*`). Also read the relevant
minimal API, abstraction, information-hiding, enforcement, recovery, outcome
evidence, secret-redaction, guard-calibration and later GC-scheduling contracts.
Applied the ontology-review skill and its design/architecture review guide.

The scope preserves existing repository GC/retention semantics and caller-owned
scheduling. No new retry persistence/replay or sibling-object confidentiality
work is authorized by this increment. Those deferred policies are not findings.

## Findings

### F1 — P2: update the affected CLI partial-effect fixture

`backend/g5-scm/src/native/mod.rs:52` changes `FailureEffect::Partial` display text
to the necessary generic “confirmed an effect” message. Production push adds its
specific confirmed-ref detail at lines 492–497. The unchanged consumer test
`backend/g5d-cli/src/process.rs:307`,
`native_git_failures_keep_target_and_effect_in_text_and_json`, still constructs a
Partial error with only `detail: "remote did not report status"` and asserts both
text and JSON contain `"confirmed the requested remote ref"` (line 311).
That assertion cannot pass with the changed shared formatter.

Executed a standalone probe linked to the refreshed current SCM rlib, using the
same error fields and expected substring. It rendered:

```text
Git push failed for https://api.example.invalid/git/member refs/heads/main: remote did not report status (Git confirmed an effect, but completion failed; inspect current state before retrying)
```

The existing consumer assertion failed. This is a test regression, not evidence
that real push lost its confirmed-ref detail: the real native partial-push tests
passed. Fix the CLI fixture to use a realistic operation-specific detail and test
the generic Partial contract, retaining a separate assertion that actual push
errors preserve confirmed-ref evidence. Run the exact CLI test afterward. The
full CLI test itself was not compiled/executed in this review.

Governing: `git_outcomes`, `g5.quality.review` (affected consumers), ordinary test
suite correctness.

### F2 — P2: the GC process guard rejects harmless process inspection

The new native-owner selection at
`backend/g5-api/tests/authoritative_workspace_reader_guard.rs:173` applies the
existing `native_process_path` check to GC. Traversal collects every nested scoped
path (lines 201–203), so `std::process::id()` contributes the parent path
`std::process`, which the predicate rejects (lines 82–87). The guard consequently
rejects PID collection for logging even though it starts no process and retains
the typed GC call.

Executed two extra tests against an otherwise exact copy of the guard:

```rust
// Forbidden control: rejected, as required.
use std::process::*;
fn collect() { let mut child = Command::new(configured_binary); }

// Valid control: wrongly rejected.
fn collect() {
    let pid = std::process::id();
    runner.collect_garbage(path, threshold);
}
```

The forbidden-control assertion passed; the valid-control assertion failed.
An earlier valid `use std::process::Stdio` fixture was also rejected. The supplied
helper-rename and test-only fixtures pass, but do not calibrate this broader
false positive. Fix path inspection to distinguish a complete benign path from
its intermediate module components while retaining rejection of Command imports,
aliases, wildcard process imports and actual process construction. Add PID or
other harmless process-value access as a positive control.

Governing: `git_guard`, `g5.quality.review.guards`; this is a reused-checker defect
exposed by applying it to the newly migrated owner, not an observed runtime bypass.

### F3 — P2: preserve confirmed GC in the sweep's aggregate outcome

`maintenance.rs:42–53` correctly returns a Partial error after successful `git gc`
when the final count fails. Its API consumer nevertheless treats every error as
“repo skipped after error” and leaves `repos_packed` unchanged at
`backend/g5-api-core/src/maintenance/repo_gc.rs:222–231`. For a one-repo sweep this
returns `repos_packed: 0`, then `run_repo_gc` logs “nothing to pack” at lines 97–101,
even though the warning detail confirms GC completed. `SweepStats::repos_packed`
is documented as the number of repositories repacked.

Evidence is inspection plus the executed SCM completed-GC/failed-final-count test;
the composed DB-backed sweep case was not executed here. The old implementation
had the same aggregate limitation, but it is a directly affected consumer of this
increment's new explicit completion evidence and conflicts with the promised
completion reporting. Main review should retain or explicitly disposition this
scope judgment.

Keep the final inspection error, but carry known GC completion to the sweep's
aggregate count/logging using the typed evidence. Do not infer completion for
Unknown failures. Add a public sweep test with count → successful GC → failed
final count, asserting both completed packing and the inspection failure are
retained; pair it with a failed/unknown GC control that does not increment the
completed count.

Governing: plan API GC execution section, `git_outcomes`, and
`g5.data.outcome-evidence`.

## Obligation ledger

| Commitment | Actual path | Counterexample/check | Result or gap |
|---|---|---|---|
| `git_execution`, `git_owner` | API `gc_repo_if_needed` → `NativeGit::collect_garbage` → private `Invocation` | Inspect all changed production code, callers and public surface; execute source guard | Preserved: no API subprocess construction, raw arguments, backend handles or new policy authority. |
| `git_locations` | `RepoStorage::local_path` remains in API; bare/worktree detection moves to SCM | Runtime real checkout, bare repo, missing path; inspect local/filestore path implementations | Preserved on tested layouts. Local storage returns its canonical owner-supplied root; no cross-layout reconstruction added. Linked worktree is supported by `.git` existence inspection but was not separately executed. |
| API threshold and existing GC behavior | `collect_garbage` initial count, `< threshold`, plain `gc`, final count | Runtime below-threshold no pack versus exact-threshold real packing | Passed; repository refs, HEAD, config bytes, index bytes and dirty file preserved. No new GC flags/pruning/serialization policy. |
| `git_native_runner`: API environment | `NativeGit::api`, `resolve_api_binary`, `Invocation::api_context` | Runtime fixed PATH and absent inherited HOME/GIT controls/secrets; input validation; inspect CLI Context refactor | Passed for API controls and executable-name selection. Context selectors remain private; existing CLI setup behavior is unchanged by inspection and relevant passing native regressions. |
| `git_native_runner`: lifetime | sweep cancellation → shared cancellation → supervisor kill/reap | Runtime GC deadline/explicit cancellation; shared descendant-held-pipe and dropped-caller tests; inspect sweep awaiting operation | Native checks passed. The composed DB lock/cancellation test is inspection-only here; main owns that execution. No process lifetime or lock release claim is made from native tests alone. |
| `git_outcomes`, `g5.data.outcome-evidence` | count/GC/final count error mapping, target attachment, API stats/logs, CLI conversion | Runtime malformed count, nonzero read, GC failure, final inspection failure; inspect consumers; standalone CLI assertion probe | Native refusal/Unknown/Partial distinction passed; F1 and F3 remain. |
| `git_retry`, recovery owner | one GC attempt per admitted repo; existing API sweep loop | Runtime fake executable attempt traces for refusal/Unknown/Partial; inspect diff | Passed: no immediate replay, new worker, table, queue or other retry storage. Existing periodic scheduling remains its owner. |
| `git_guard`, guard calibration | extended `typed_git_violations`, required owner function inventory | Exact guard suite; forbidden configured/aliased/wildcard process controls; valid test fixtures/helper renames/PID access | Existing 13 tests pass; wildcard refusal passes; F2 identifies a failed positive control. Guards still do not perform Rust macro/type expansion. |
| minimal sufficient API, abstraction, information hiding, dependency direction | private `maintenance`/`process`; public typed `ObjectMaintenance`; private Context | Inspect public exports, manifests, reused runner policies and affected call sites | Small complete operation hides backend mechanics; scheduling stays above it; no manifest/dependency or persistence additions. F3 needs a sufficient typed representation/consumer for partial completion. |
| `g5.scope.capacity-recovery.gc` | unchanged plain GC retention and existing advisory lock | Compare old/new command and scheduling | Preserved scope: active-reader/pruning serialization is later policy and remains unverified/unimplemented here. |

## Executed evidence and limits

- Snapshot checksums all matched twice. Cached SCM test executable identity:
  `target/debug/deps/g5_scm-08d6ca130345ed39`, SHA-256
  `e02c3c33ee531f93b38ecff5629f5e8ca4672fa7e1fa389e87397d44b65d4a2f`.
  Fingerprint identifies default/libgit2/native; executable mtime is newer than
  changed SCM sources and its test inventory includes the new GC cases. This was
  a refreshed cached-artifact check, not a clean rebuild.
- Executed `native::tests::`: **23 passed, 3 failed**, 166 filtered. All six new
  API-GC tests passed. The three failures occur at localhost listener binding with
  `Operation not permitted`: HTTP credential scoping and two runtime member-relay
  tests. Those network-dependent guarantees are unverified in this sandbox.
- Compiled the exact current guard source directly with cached tree-sitter,
  serde_json and walkdir rlibs, setting its true `CARGO_MANIFEST_DIR`; **13/13
  passed**. Isolated extra forbidden/valid controls yielded **1 passed, 1 failed**
  as detailed in F2.
- F1 probe linked current `libg5_scm-750bbfac5cb00943.rlib`, whose dependency list
  includes `native/maintenance.rs` and whose mtime is newer than changed source.
  Its display assertion failed as reported. The initial direct link needed the
  existing Homebrew OpenSSL native search path; adding it allowed execution.
- No DB/API runtime checks were performed to avoid sharing mutable test resources
  with the main reviewer. Inspected existing enumeration/threshold/error-isolation
  tests and the new shutdown/lock test. Main must supply the public-path DB evidence.
- No production deployment, Linux/NFS execution, full CLI suite, full SCM suite,
  native-only rebuild, or arbitrary whole-platform guard guarantee is claimed.

## Proposed metadata disposition

After consolidated repair/rechecks, update scoped `git_execution`,
`git_native_runner`, `git_locations`, `git_outcomes`, `git_retry` and `git_guard`
metadata to include this GC path and bounded evidence. Preserve broader migration
and API outer-timeout/host/guest gaps. No governing ontology meaning change is
needed; the three repairs fit the approved design. Preserve the separate later
GC scheduling limitation and parked sibling-object confidentiality limitation.
