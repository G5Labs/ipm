# API Git proof observations — independent discovery 1

Read-only independent review of the implementation increment relative to `f95fac40`.
The frozen input is `/tmp/g5-api-observations-review/snapshot.tar` plus its inventory and
tracked patch; the untracked `backend/g5-scm/src/native/observations.rs` is included.
No author explanation, main/peer findings, or earlier implementation review findings
were consulted. No production or ontology files were edited, no delegation was used,
and investigative repositories are isolated in `/tmp/g5-observation-review-1`.

## Initial findings

### R1 — P2: Corrupt storage is reported as an ordinary absent proof

**Paths:** `backend/g5-scm/src/native/observations.rs:30` and `:103`, consumed by
`backend/g5-api-routes/src/routes/workspace_flush.rs:151` and `:186`.

The ref reader treats every nontruncated exit 1 as `None`, and object-kind inspection
accepts every successful `<oid> missing` record as `None`, without considering Git's
storage-error diagnostics. Real Git produces exactly those results for damaged storage:

- Replacing an existing loose ref's contents with `not-an-oid\n` makes
  `rev-parse --verify --quiet refs/heads/main` exit 1 with
  `warning: ignoring broken ref refs/heads/main`.
- Replacing an existing loose object's compressed bytes with `broken` makes
  `cat-file --batch-check=%(objecttype)` exit 0 and print `<oid> missing\n`, while stderr
  reports an inflate failure and inability to unpack the object header.

The new methods consequently return absence. Flush completion turns these repository
failures into `FLUSH_BRANCH_MISMATCH` / `FLUSH_NOT_COMMIT` (409) rather than the specified
`FLUSH_UNAVAILABLE` (503), losing operational failure evidence. Receipt advancement is
still refused; this finding is error classification/recovery correctness, not proof of
an invalid receipt advancing. The new batch-check design explicitly claims to distinguish
missing objects from repository failures; successful status plus the missing record is
insufficient to do that. The baseline also conflated some failures; the selected migration
contract requires this distinction and the new operation does not implement it.

**Governing:** the plan's API Git proof observations contract; `g5.architecture.git_outcomes`,
`g5.architecture.git_native_runner`, and `g5.git.receipt`.

**Repair:** distinguish genuine missing refs/objects from diagnosed corruption or I/O
failure before returning an ordinary absence. Preserve harmless Git warnings if using
stderr classification; this environment's Apple Git emits a harmless `confstr()` warning
when the API clears its environment, so rejecting all nonempty stderr would regress valid
reads. Keep diagnostics scrubbed.

**Regression expectations:** real corrupt loose-ref and loose-object fixtures must return
an operational error through the native API and `FLUSH_UNAVAILABLE` through authenticated
flush completion, with `complete_at` / selected receipt unchanged. Genuine missing refs
and objects and valid reads with harmless warnings must retain their ordinary results.

**Evidence:** direct execution of the exact native commands with the API environment,
recorded in `/tmp/g5-observation-review-1/git-probes.json`; source tracing through the
unchanged response/receipt transition. The Rust API/DB path was not executed by this reviewer.

### R2 — P2: Repository discovery can silently read an enclosing repository

**Paths:** observation calls to `self.command(Some(repository), ...)` at
`backend/g5-scm/src/native/observations.rs:25`, `:55`, `:91`, `:123`, using
`backend/g5-scm/src/native/process.rs:157`.

Migrated reads select a repository using `git -C <owner-supplied-path>` without bounding
upward discovery. If the supplied storage directory exists but has lost its Git metadata
and is inside another checkout, Git reads that enclosing checkout. A direct reproduction
with `repo/nested` (an ordinary directory under a valid repository) makes the new ref read
return the parent's main OID and object-kind read return the parent's commit type. The
baseline explicit `GIT_DIR=repo/nested` invocation instead fails with "not a git repository".
`RepoStorage::local_path` constructs the supplied path without verifying repository
presence, so it does not rule out this case. The typed operation is supposed to observe
that repository, not a discovered ancestor selected after its metadata disappears.

This changes proof identity and can turn unavailable storage into apparently valid proof
if the enclosing repository contains the relevant refs/objects. No cross-repository attack
is claimed for the normal flat production layout; the demonstrated defect is wrong-repo
fallback for an accepted owner-supplied path.

**Governing:** `g5.architecture.git_locations`, `g5.architecture.git_outcomes`, the plan's
repository-failures rule, and `g5.git.receipt`.

**Repair:** resolve and validate the exact supplied bare/worktree/linked-worktree root
inside SCM, preventing ancestor fallback while retaining linked-worktree support. Do not
move layout assembly back to routes.

**Regression expectations:** a nonrepository child directory and a checkout whose metadata
has disappeared inside another valid checkout must error even when its parent has every
requested ref and object; ordinary, bare, linked, and symlinked valid roots must still work.

**Evidence:** direct old/new command comparison in
`/tmp/g5-observation-review-1/location-regression.json`, plus object-read reproduction in
`git-probes.json`. Rust public-path/DB execution remains unverified.

### R3 — P2: Post-acceptance observation failure drops confirmed push follow-up evidence

**Paths:** `backend/g5-api-routes/src/routes/git.rs:686` (consumer), new error/deadline
behavior through `:1073`, and `GitError::Native` at `:86`.

After `receive-pack` has completed and returned its per-ref report, the route performs
`current_ref_tip(...).await?` before even consulting an `ok <ref>` report. The new reader
can fail on deadline, malformed output, or repository inspection failure. That early return
discards the already captured report and any earlier accepted siblings, rolls back the
admission transaction, and skips `enqueue_after_push`, branch events and reviewer notices.
Git refs have already moved. The response only describes a read invocation that "made no
Git changes", with no known accepted push evidence or Update PR recovery instruction.

The optional observation should not gate confirmed protocol evidence. This is an affected
consumer of the increment's newly explicit read failures, even though protocol execution
migration itself is subsequent scope. The existing unconditional `?` predated this increment;
the new operation introduces deadline/strict-output failure paths into it, and the current
plan explicitly requires accepted-push notifications to remain intact.

**Governing:** `g5.architecture.git_outcomes`, `g5.architecture.git_ref_updates`,
`g5.architecture.pr_flow`, and `g5.governance.pr.follow-push`.

**Repair:** preserve and process confirmed per-ref acceptance independently of optional
post-write observations. Collect any observation failure without dropping confirmed siblings,
release admission correctly, deliver the existing follow-ups for confirmed refs, and report
unknown/unverified refs with explicit accepted effects and manual inspection/Update PR guidance.
Do not add retry storage or automatic replay.

**Regression expectations:** use a configured Git wrapper that delegates real receive-pack,
then fails/times out only the subsequent ref observation. Exercise the real push route;
assert the accepted ref moved, existing publication queue/branch/reviewer effects are retained,
and the response preserves accepted/unknown evidence. Include mixed accepted/rejected refs
and the no-report-status fallback case; a preflight read failure must still make no push.

**Evidence:** source-confirmed early-return path and ownership/ordering trace. This failure
injection was not executed because this reviewer was assigned no concurrent DB/full-Cargo
work; it remains an integration evidence gap, not a claim that the runtime test passed.

## Check results and scope accounting

`git diff --check f95fac40` passed. Real Git probes ran under the API's cleared environment
and fixed PATH. All new tests and relevant existing consumers were source-inspected.
No cargo build, Rust test suite, API/database suite, or live provider/guest test was run by
this reviewer; the main reviewer owns shared build/database verification. Existing test
assertions are described as source evidence only, never as executed evidence.

| Commitment | Actual path | Counterexample/check | Result or gap |
|---|---|---|---|
| `git_execution`, `git_native_runner` | Native observations + private invocation supervisor | Public API exposes complete typed observations; stdin/stdout/stderr concurrency and cancellation path inspected | Boundary is appropriate; no raw command/environment escape hatch added. New tests discriminate pipe deadlock, caller-drop, deadline and one-attempt behavior; runtime execution not performed here. |
| `git_locations` | Owner-selected RepoStorage path → `-C` discovery | Bare/checkout/linked positive fixtures; nested invalid directory | Positive cases covered by test source; R2 is directly reproduced. |
| `git_outcomes` | Output parsing and API response adapters | True absence versus corrupt refs/objects; malformed output and truncation | R1 reproduced; most malformed success output is rejected. R3 loses enclosing push outcome evidence. |
| `git_retry` | Observation methods and adapters | Search/inspection for loops, writes and persistence | No new retries, queues, tables, persistence or operation replay. Existing lifecycle recovery remains above SCM. |
| `git_ref_updates`, `git_owner`, `network.cas`, `network.ref-scope` | Receive-pack admission → symbolic/current-ref observations → native CAS | Symbolic refs forbidden; absent mapped branch denied; exact old OID retained | Existing authority/admission and CAS logic preserved by inspection. New symbolic test asserts unchanged target and no queue; full principal/race suite not executed. Post-acceptance interaction is R3. |
| `receipt`, `receipt.write-admission` | `verify_manifest` → `WorkspaceFlushService::complete`; immutable receive admission | Wrong object type, missing object, leaf mismatch, reversed ancestry, metadata loss; successful repaired proof | New API test uses authenticated owner transition and checks durable selected receipt, rather than only helper results. Classification misses R1; negative proof/runtime suite unexecuted here. |
| `pr_flow`, `pr.follow-push` | `receive_pack` acceptance → enqueue/events/reviewer notification | Successful native push and optional post-read failure | Existing happy-path test is retained. R3 failure boundary lacks a regression. |
| `content_reader`, `projection_owner` | All call sites of new native operations | Search for uses outside Git policy/flush owners | No new application content-reading or projection bypass found. |
| `git_guard`, guard calibration | `authoritative_workspace_reader_guard.rs` | Forbidden process aliases, missing required functions, private helper renames and test fixtures | Guard extension is scoped to migrated flush/observation owners, with negative and valid controls. Partial protocol migration is accurately excluded. No source-guard execution claimed. |
| `minimal-api`, `abstraction`, `information-hiding`, `dependencies`, `compatibility` | NativeObjectKind + four typed methods; private stdin field; route wrappers | Examine introduced mechanism and removed direct process construction | Minimal coherent migration; no speculative state/abstraction or reversed dependency found. Native feature boundary unchanged. `current_ref_tip` becomes private after its only external consumer migrates. |
| `credentials.redaction` | `NativeGitError` → process scrub → API diagnostics | Read operations carry no injected credential; process detail and target use scrub | No new credential channel found; configured-secret/diagnostic end-to-end redaction not executed here. |

## Ontology and design disposition

The increment changes only the saved design, adding its API proof-observations section;
it changes no current I/D meaning. The first governance section records the branch-wide
ontology diff, while this increment's obligations are described later in the same plan.
The actual current owning bodies and paired metadata were read, including Git execution,
runner, outcomes, retry, location, guards, policy/ref effects, receipt/admission, network
CAS/ref-scope, affected architecture principles, content authority/reading, secret redaction
and PR-follow-push. Broader provider/host/protocol migrations and recorded pre-existing gaps
are not treated as completion requirements for this increment.

After consolidation and repair, update scoped `git_execution`, `git_native_runner`,
`git_outcomes`, `git_retry`, `git_locations`, `git_guard` and receipt metadata with actual
observation coverage and remaining limits. Their current entries still describe API Git/flush
execution as migration work or old unassessed receipt checks. Preserve broader migration gaps;
no governing meaning change is needed to fix the findings. If R3 is deliberately deferred,
record the precise accepted-push follow-up gap in the relevant outcome/PR metadata rather than
claiming that the observation increment preserves all accepted-push behavior.
