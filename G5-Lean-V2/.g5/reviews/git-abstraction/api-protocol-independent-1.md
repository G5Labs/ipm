# API protocol increment — independent review 1

Initial discovery saved before reading peer reports, implementation narration, or
consolidation feedback. This is an independent, read-only, full-scope review of the
uncommitted increment on `design/git-abstraction`, base `ab5bf96e`. No production,
test, design, or ontology source was edited; no subagents were created.

## Review inputs and scope

- Original request: `.g5/chat_intent/design-git-abstraction/request.md`.
- Pinned design: `.g5/ontology-plan-git-abstraction.md`, beginning with `Ontology
  changes and governance`, its governing inventory, and the new `API smart-HTTP
  protocol execution` section.
- Review workflow and design guide: `.agents/skills/ontology-review/SKILL.md` and
  `.agents/skills/ontology-authoring/references/design_review.md`.
- Current governing bodies and paired metadata: Git/governance execution, ref
  updates, runner, outcomes, retry, credentials, locations and guards; durability
  network admission/CAS/ref scope and receipt admission; runtime drain; module
  boundaries; system design principles; structural guard calibration; and PR
  follow-push continuation.
- Entire tracked diff, both new protocol source files, and their material
  consumers: route authorization/admission and confirmed-ref continuation,
  native invocation/cancellation/observations, test guards and crate features.
- Frozen snapshot: `/tmp/g5-api-protocol-review/snapshot.tar`, SHA-256
  `d3bf8d04289255c6c270aa00a1b59b5199ff84b47d9c4ed6fd5b6c394829455a`.
  All 13 archived files were compared byte-for-byte with the reviewed working
  files and matched. The accompanying `tracked.patch` and `files.txt` define the
  incremental scope.

The original request's retry restriction and parked confidentiality extension were
supplied as review constraints. No known defect or peer finding was supplied.
The increment changes no ontology body; the design's new protocol section applies
the existing governing meanings. Broader unimplemented Git/provider migration is
outside this increment and is not treated as a new defect.

## Initial findings

No actionable production-code defect was established in this review. The selected
native behavioral checks and current structural guard checks passed. This is not a
claim that every design acceptance case was executed: the API admission/disconnect
and downstream publication assertions remain source-inspected here, with runtime
evidence to be supplied by the main integration run.

No contract amendment, new retry state, automatic replay, or confidentiality
guarantee is warranted by the reviewed changes. The extra task owns the existing
push and its continuation; it does not create a retry mechanism.

## Commitment ledger

| Commitment | Actual path and discriminating case | Initial result or limit |
|---|---|---|
| `g5.architecture.git_execution`, `git_owner`, `g5.design.minimal-api`, `information-hiding` | `routes/git.rs:444`, `:477`, `:605` delegate advertisement/upload/receive to `native/protocol.rs`; route process construction and direct backend dependency are removed. Public inputs carry the admitted ref vector; no public command/environment setter or backend handle is added. | No finding by inspection. All 15 current reader/backend/SCM surface guard tests pass. |
| `g5.architecture.git_ref_updates`, `g5.git.network.cas` | `native/protocol.rs:116` compares actual command vector with the declared vector before receive execution; exact old OIDs reach native Git unchanged. The atomic capability reaches Git unchanged. Test submits an undeclared command, then the same mixed valid/stale commands with atomic and per-ref modes. | Native runtime test passes: undeclared and atomic-refused effects stay absent; the allowed sibling is present only for per-ref acceptance. |
| `g5.git.network`, `g5.git.network.ref-scope`, `g5.git.receipt.write-admission` | `routes/git.rs:523` retains header authentication, read/write/ref authority, admission, symbolic-ref refusal, no mapped-branch create, and immutable replay checks before SCM. `acquire_push_admission` rechecks workspace/current-incarnation state under retained SQL authority. | No source regression found. Exact current receive guard and its missing-guard/missing-effect controls pass. User/incarnation, stop/transfer and receipt matrix was not rerun against a DB in this review. |
| `g5.architecture.git_native_runner`, `g5.runtime.drain`, `g5.design.cleanup-proof` | `process.rs:402` gives upload streams an owned supervisor and cancellation sender; `:528` concurrently delivers input and drains both outputs; `:555` bounds completion and kills/reaps on interruption. `NativeGitStream::poll_read` waits for process completion before clean EOF. | Native stream exit, deadline and drop checks pass. Existing descendant-held-pipe deadline, cancellation/descendant-effect, and confirmed-write deadline regressions pass. No orphan was observed in those cases. Windows and detached descendants that deliberately leave supervision were not exercised. |
| Push admission survives HTTP cancellation | `routes/git.rs:505` detaches the owner task rather than the native mutation alone; `receive_pack_owned` keeps admission through native completion and observations, then commits admission before independent queue/reviewer work. `api_git_push_policy.rs:927` aborts the router awaiter and uses a competing workspace lock before releasing native receive. | Source supports the intended order. The DB-backed contender, actual ref, queue and event assertions were inspected but not independently executed; main integration evidence is still needed. |
| `g5.architecture.git_outcomes`, `git_ref_updates`, `g5.governance.pr.follow-push` | `protocol.rs:150` uses captured output even after interruption; `ReceivePackResult` carries statuses and failure together. `routes/git.rs:622` consumes confirmed status without a fallible ref reread, preserves siblings, publishes existing accepted-ref signals after admission release, then returns a partial/unknown failure. | Fake receive runtime checks preserve acceptance through late exit and broken stdin. Existing real-push deadline test preserves confirmed effect. API queue/events/reviewer behavior through disconnect and post-acceptance failure is source-inspected only in this review. No rollback claim found. |
| Binary payload/diagnostic separation | `process.rs:208` removes only the diagnostic stdout cap for buffered protocols; stderr remains capped. Upload uses a 64 KiB duplex buffer, not a truncated capture. Protocol output is excluded from diagnostic fallback. | Runtime tests preserve a 256 KiB advertisement, fetch pack larger than 512 KiB, worktree contents, and receive confirmation with 256 KiB noisy stderr. Exact fetched-object reconstruction and simultaneous large stdin/stdout upload stress were not independently exercised. |
| `g5.architecture.git_locations`, `git_native_runner`, `g5.architecture.compatibility` | `protocol.rs::protocol_command` accepts storage-owner paths, anchors repository identity, clears inherited API environment, preserves API PATH/TLS inputs and worktree `updateInstead`, and retains non-fast-forward admission policy. Route protocol framing/content types remain unchanged. | Bare and ordinary worktree native runtime cases pass, including checked-out worktree content after receive. No new root reconstruction or host/guest layout conflation found. Linked-worktree HTTP service and non-macOS runtime were not tested here. |
| `g5.architecture.git_credentials` | API auth remains in route policy; server protocols use no inbound bearer token as a Git credential. Context clears inherited controls, runner diagnostics use existing redaction, and raw protocol output is not error text. | No credential injection or secret-bearing argument regression found by inspection. No live TLS/custom-CA check was run. |
| `g5.architecture.git_retry` | Increment adds a request-lifetime task and stream supervision, no schema, persistent tracking, retry loop, or replay worker. Existing immutable identical-OID replay policy remains in its API owner. | Preserved by source inspection. Stream cancellation tests assert one attempt. |
| `g5.architecture.git_guard`, `g5.quality.review.guards`, `g5.design.enforcement` | Guard inventory includes all four route owners and all three protocol operations, rejects process aliases/qualified construction, rejects missing targets, and accepts helper renames/test fixtures. Route guard requires the real publication effect and authority before receive. | All 15 current structural Git guards pass. Both changed route-order tests pass using exact extracted test/helper source, including negative and harmless-change controls. Static checks are not runtime authorization proof. |
| `g5.architecture.dependencies`, `g5.design.abstraction`, `g5.design.minimal-mechanism` | `g5-api-routes/Cargo.toml` and lockfile remove direct `git2`; native protocol operations reuse the shared runner and errors without importing API/SQL ownership. | No unjustified layer or reversed dependency found. Current offline Cargo metadata is checked by the passing manifest guard. No new full Cargo build was performed. |

## Checks actually executed

Mutable resources were isolated in test-created temporary directories and
`/tmp/g5-api-protocol-review-1`; shared production source, DB state, environment
files and frozen snapshots were untouched.

1. Existing current SCM test executable
   `target/debug/deps/g5_scm-554aebe82f3c130a`:
   all four `native::protocol_tests` passed (1.90 s). This binary was built after
   the latest reviewed native source modification and includes the new tests;
   it was reused to avoid a competing Cargo build.
2. Same executable: `deadline_includes_pipes_held_by_descendants_after_leader_exit`
   and `deadline_retains_confirmed_ref_after_real_push` passed (6.83 s combined);
   `cancellation_reaps_child_and_stops_descendant_effects` passed (0.77 s).
3. Current complete `authoritative_workspace_reader_guard.rs` compiled directly
   with `rustc --test` against cached dependencies into the isolated directory:
   all 15 tests passed (3.02 s), including offline manifest metadata checking.
4. Both changed route guard tests and their exact helper functions were copied
   verbatim to an isolated harness and compiled with `rustc --test`:
   `smart_http_receive_checks_authority_and_admission_before_scm` and
   `remote_tracking_publication_handlers_require_workspace_write_scope` passed.
   This does not stand in for the entire route-auth suite. Direct compilation of
   that entire file against manually selected cached dependencies failed because
   the chosen Serde/Serde JSON artifacts had incompatible `serde_core` identities;
   this was a local harness dependency-selection problem, not evidence of a
   repository build defect.
5. Snapshot/current-source equality check passed for every archived file.

No DB-backed API suite, full-workspace build, live provider, deployed server,
cross-platform run or exhaustive malformed-protocol/failure matrix was executed
by this reviewer. In particular, the central real HTTP disconnect/admission and
confirmed publication outcomes need the main task's independent API run evidence
before being marked verified.

## Scoped metadata reconciliation proposed for consolidation

After the main integration results are known, update the existing metadata rather
than changing governing intent. `git_owner`, `git_execution`, `git_native_runner`,
`git_outcomes`, `git_ref_updates`, `git_retry`, `git_locations` and `git_guard`
still describe route protocol execution and the route-auth guard as pending (and
`git_locations` names the removed `routes/git.rs::repo_git_dir`). Record the new
SCM owner and actual evidence/limits there; preserve unrelated migration gaps.
Update dependency/enforcement evidence for the removed API-routes backend
dependency as appropriate. Keep the pinned design's ontology diff synchronized
with any such metadata edits. This is ordinary post-review reconciliation, not an
initial production-code finding or permission to broaden the design.

Review effort: one independent reviewer, no delegation. Tool/runtime durations
above are observed check durations; no comparative review-benefit measurement is
claimed.
