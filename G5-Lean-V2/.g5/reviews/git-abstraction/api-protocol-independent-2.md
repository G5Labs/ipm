# API smart-HTTP protocol increment — independent review 2

Initial discovery report, 2026-10-02. This is an independent, full-scope, read-only review of the uncommitted increment on `design/git-abstraction`, base `ab5bf96e`. No production, test, design or ontology source was edited. No peer report or implementation-session narration was read before saving this report. The parent supplied the scope, frozen snapshot and exact compiled-test paths/build provenance only. No further agents were delegated.

The source read matches all 13 files in `/tmp/g5-api-protocol-review/snapshot.tar` byte-for-byte; the supplied `tracked.patch` and `files.txt` were the inventory. Coverage includes the complete tracked diff, new `native/protocol.rs` and `native/protocol_tests.rs`, API auth/admission/replay/publication consumers, the shared runner and its existing native consumers, dependency removal, and both changed structural guards.

Inputs: original `.g5/chat_intent/design-git-abstraction/request.md`; `.g5/ontology-plan-git-abstraction.md`, beginning with its Ontology changes and governance inventory and including API smart-HTTP protocol execution; ontology-review skill and design-review guide; affected Git/governance, durability, runtime, boundary, design-principle, guard-calibration and redaction bodies and metadata. The user’s no-new-retry-state/no-automatic-replay constraint and parked object-confidentiality extension are preserved as scope limits.

## Initial finding

### R2-1 — P2: Match executable calls rather than raw substrings in the new push-order guard

**Location:** `backend/g5-api/tests/route_auth_static.rs:835-836`, consumed at lines 818-828 and 842-873.

`required_git_guards_precede_effect` tests `source.contains(effect)` and then delegates to raw substring positions. Both callers pass an unfiltered function body. Consequently a commented-out authorization call still counts as an authorization check, and a commented-out publication call satisfies the new missing-effect requirement. The exact argument-text match also rejects an ordinary multiline rendering of the same valid call. The calibration fixture at lines 869-873 inserts an unrelated `renamed_helper` word, so it never exercises a real harmless call-format change.

**Consequence:** the newly added checks can go green when the guarded effect or authorization is removed by commenting it out, and valid formatting-only changes can fail. This is a defect in the promised guard, not evidence of an authorization bypass in the current production route.

**Governing commitments:** `g5.architecture.git_guard`, `g5.quality.review.guards`; the pinned protocol increment specifically requires the actual workspace-publication service call after scope/owner authorization, including a missing-effect control and harmless-refactor acceptance.

**Discriminating evidence:** compiled the exact three helper functions (`required_git_guards_precede_effect`, `each_token_precedes_all`, `earliest_token_index`) extracted from the frozen source in an isolated Rust probe at `/tmp/g5-api-protocol-review-2/guard_probe.rs`. Its output was:

```text
commented-out guards accepted: true
commented-out effect accepted: true
valid multiline call accepted: false
```

**Recommended repair:** use the existing Rust syntax approach available to the structural guard suite, or another small token/call parser that excludes comments and string literals and recognizes call boundaries independent of whitespace. Check the ordered real call expressions with the necessary scope/owner arguments, avoiding substring overlap such as `require_push` matching `require_push_ref_updates`.

**Regression:** apply the checker to the actual function body with each real guard call commented out individually, with the real service/native effect commented out, and with the effect call reformatted across multiple lines. The first two variants must fail; the formatting variant and actual private-helper rename must pass. Keep the existing removed-function and wrong-order controls.

No additional production-code defect was established in this discovery pass.

## Commitment and evidence ledger

| Commitment | Actual path and discriminating case | Result / boundary |
| --- | --- | --- |
| `git_execution`, minimal API, information hiding, dependency direction, compatibility | Route calls typed `advertise_refs`, `upload_pack`, `receive_pack`; SCM retains command/env/process mechanics. `GitService`, declared updates and per-ref outcomes are owned values; no new raw backend/command escape hatch. API-routes loses direct git2 dependency and matching lock entry. | No finding by inspection. Native-only executable and architecture guards pass their exercised paths. Other platform migrations are outside this increment. |
| `git_ref_updates`, network CAS and ref scope | SCM parses actual request commands, compares the entire ordered update vector, validates full refs/OID widths, and passes native atomic/per-ref mode through. API retains authorization, branch/ref policy, locked admission, symbolic-ref refusal and immutable replay. | Real native tests reject vector mismatch before ref effects, reject stale atomic set, and preserve independently accepted sibling. No finding. Entire public auth/principal/race matrix was not rerun by this reviewer. |
| `git_native_runner`, stream/mutation lifetime | `Invocation::supervise` concurrently writes input/drains output and stderr; API streaming owns a bounded duplex and completion result; dropping the stream drops its cancellation sender. Detached `receive_pack_owned` retains SQL admission until native execution and cleanup complete. | Native tests pass stream exit/deadline/drop, concurrent I/O, inherited-context isolation, cancellation/descendant effects and descendant-held-pipe deadlines. API disconnect test source checks lock contention plus actual refs/queue/events; DB test was inspected, not executed here. No finding. |
| `git_outcomes` | `execute_captured` retains stdout on I/O/deadline/cancellation; receive report statuses survive later failure; API confirmed continuations precede returning partial/unknown error. Streaming EOF waits for supervisor completion. | Real native tests pass late failure after accepted report, broken stdin with captured confirmation, error-after-bytes on upload, and existing confirmed-push/deadline behavior. No finding. Protocol-specific captured acceptance followed by deadline and real HTTP response-body failure were not separately executed here. |
| `git_locations`, context/compatibility | Storage selection remains API-owned; SCM canonicalizes and sets explicit GIT_DIR/worktree, fixed API PATH, configured executable, TLS CA variables, non-FF and updateInstead receive settings. | Real native bare/worktree receive and large fetch tests pass; native context tests pass. No finding. Linked-worktree protocol, TLS-serving deployment and cross-platform protocol execution not separately exercised here. |
| `git_credentials`, security redaction | API credential stays in admission; protocols use `GitAuth::None`; inherited environment is cleared; server protocol stdout remains data while stderr/details are bounded and scrubbed. | Bounded diagnostic/large binary tests and native credential/error tests pass where they do not require loopback sockets. Three networking tests could not start their listeners under sandbox; no product failure inferred. |
| `git_retry` | New code adds no persistence, replay loop or retry service. The detached task owns one accepted request; it is not a retry worker. Existing continuation policy stays above SCM. | No finding by full diff inspection. Existing no-retry attempt-count and lost-status tests pass. |
| receipt and receipt write admission | Existing unfinished-receipt/current incarnation checks and immutable replay remain before shared execution; new command-vector verification does not create authority. | No changed-contract defect found by source inspection. Public receipt/flush integration suite not run here. Object confidentiality remains explicitly parked, not supplied by this change. |
| `git_guard`, `g5.quality.review.guards` | Process/backend guard expands to API Git and protocol operation inventory; dependency guard adds API-routes. New route-order guard targets the actual service/native calls. | Fifteen exact-source architecture checks and two targeted route-order checks pass, but the independent counterexample exposes R2-1. These checks do not prove full Rust macro expansion or runtime authority. |

The source contains no consequential contract amendment beyond the pinned protocol increment. The existing metadata still says API protocol consolidation and the stale push-order guard are pending and references removed `routes/git.rs::repo_git_dir`; after consolidation/repair, the main reviewer should update the affected paired metadata and the pinned design’s recorded ontology diff with verified status and remaining limits. This reviewer deliberately leaves those edits to the coherent repair owner.

## Executed checks

All tests reused executables the parent identified as compiled from the frozen source; this reviewer ran no Cargo build or shared database test.

- `target/debug/deps/g5_scm-554aebe82f3c130a native::protocol_tests --test-threads=1`: **4 passed**. Covers actual bare/worktree receive, full large binary fetch, advertisement bytes, command-vector refusal, native CAS/atomic/partial acceptance, diagnostic bounding, late failure evidence and upload exit/deadline/drop.
- `/tmp/g5-protocol-guard --test-threads=1`: **15 passed**. Parent reports this executable was rustc-compiled from the frozen architecture guard source.
- `target/debug/deps/route_auth_static-1159657c244e6cdd smart_http_receive_checks_authority_and_admission_before_scm --exact --nocapture`: **1 passed**.
- Same route executable, `remote_tracking_publication_handlers_require_workspace_write_scope --exact --nocapture`: **1 passed**.
- Full native-only executable, `--test-threads=1`: **35 passed, 3 fixture setup failures** in 22.26 seconds. Each failure is `TcpListener::bind("127.0.0.1:0")` returning `PermissionDenied: Operation not permitted`, before the behavior assertion: `http_credentials_are_scoped_and_redirects_do_not_receive_them` (tests.rs:750), `runtime_member_relay_clones_without_existing_destination_or_ambient_member` (tests.rs:999), and `runtime_member_relay_preserves_canonical_identity_without_injecting_credentials` (tests.rs:809). These are unavailable network checks in this environment, not established regressions.
- Exact-source standalone guard probe: **counterexamples reproduced**, output above. Probe and binary only under `/tmp/g5-api-protocol-review-2`.
- `git diff --check`: **passed**.
- Frozen archive comparison: **13/13 identical**.

Unexecuted central integration evidence remains the public HTTP/API DB admission/disconnect/ref-publication flow; it must be supplied by the main reviewer’s authorized isolated API test run before claiming that runtime outcome. This pass provides source inspection, native runtime evidence and the guard counterexample only. Other verification boundaries appear in the ledger. Review effort was not separately metered.

## Post-repair recheck — 2026-10-02

Bounded independent read-only recheck requested after main consolidation. Reviewed only the repaired `backend/g5-api/tests/route_auth_static.rs` guard and its controls; no Cargo build or production/test-source edit was performed.

**R2-1 disposition: partially repaired; retain the harmless-formatting part until the optional trailing comma is handled.** The new checker traverses Rust call expressions, excludes comment/string decoys, compares exact method names, and binds the scope literal to the scoped DB call. Source inspection confirms the new controls mutate the actual publication and receive bodies, including private-helper renaming. Those changes address the original false-positive cases and multiline calls without a trailing comma.

The original valid multiline counterexample also included the optional final comma. `normalized(arguments)` includes punctuation, so the exact comparison at current lines 917-926 distinguishes `(&mut db, params.id,)` from `(&mut db, params.id)`. This is still a harmless formatting failure under `g5.quality.review.guards`; the new multiline fixtures omit that comma and do not discriminate this case.

Compiled the repaired helper verbatim into `/tmp/g5-api-protocol-review-2/repaired_guard_probe.rs`, linking already built tree-sitter libraries directly with rustc (no Cargo build). Result:

```text
commented guards: false
commented effect: false
multiline without trailing comma: true
multiline with trailing comma: false
scope only in unrelated literal: false
```

Recommended remaining repair: compare the normalized argument expressions/list elements rather than punctuation-bearing argument-list text, or otherwise ignore one optional trailing argument comma. Add the trailing-comma variant to both actual publication and receive call calibration fixtures. Existing comment, missing-call, exact-name, scope and wrong-order controls should remain.

This recheck does not reopen the production-path review or claim runtime authorization proof from the structural checker.

## Final guard repair recheck — initial compile result

The next repair compares normalized named argument expressions and adds trailing commas to both actual-call formatting fixtures. The first exact-source probe could not compile: inside `.iter().filter(|call| ...)`, `call` has type `&&Node`, and `arguments(*call, &wrapped)` passes `&Node` where `Node` is required (E0308). The necessary dereference is `arguments(**call, &wrapped)` or equivalent iterator adjustment. Reported to the main repair owner without editing source. Final behavioral disposition waits for the compile correction and probe execution.

## Final guard disposition — R2-1 closed

After the main owner corrected the filter dereference to `arguments(**call, &wrapped)`, recompiled the exact current helper and reran the same independent probe at `/tmp/g5-api-protocol-review-2/final_guard_probe.rs`. The build succeeded and produced:

```text
commented guards: false
commented effect: false
multiline without trailing comma: true
multiline with trailing comma: true
scope only in unrelated literal: false
```

The argument-expression comparison now closes the original harmless-formatting counterexample while retaining comment/effect and unrelated-scope rejection. Source inspection confirms that both the actual publication and receive multiline calibration controls include trailing commas. **R2-1 is repaired on the independently checked cases.** No further finding arose in this bounded recheck. Prior findings and the intermediate failed rechecks above remain preserved. This reviewer used direct rustc with existing tree-sitter artifacts only; the full route-auth Cargo suite is being run separately by the main reviewer and is not claimed as independent execution here.
