# Git abstraction — initial implementation review

## Scope and governing contract

The user's request is to commit the reviewed design and start implementation, with
clear errors and caller-decided retries instead of new tracking storage. The design
handoff is preserved at `3f7d2874`; the pinned document is
[ontology-plan-git-abstraction.md](ontology-plan-git-abstraction.md), including its
ontology diff, governing cases and initial implementation slice.

This review covers the initial typed operations for resolution-input pins,
projection ref discovery and semantic buffer diffs; the three migrated API callers;
and the extended authorized-reader guard. The remaining native, provider and
backend migration is explicitly unfinished, not claimed by this slice.

Independent discovery uses three fresh parallel full-scope reviewers without the
author's solution history. Their initial reports are retained in
[review 1](reviews/git-abstraction/independent-1.md),
[review 2](reviews/git-abstraction/independent-2.md) and
[review 3](reviews/git-abstraction/independent-3.md).
The main agent owns consolidation, repairs and final verification. Source remains
unchanged during independent discovery; temporary probe/build artifacts are isolated.

## Initial self-review evidence

- `cargo test --offline --locked -p g5-api-core --lib git_conflict_resolution::tests`:
  passed the native related/unrelated rebase case, including conflict completion,
  preserved authorship and provider branch immutability.
- `cargo test --offline --locked -p g5-api-core --lib source_control_change_graph_service::tests`:
  23 passed, including full authored-block context and bounded output.
- The exact `authoritative_workspace_reader_guard.rs` source was compiled with
  `rustc --test`, its cached workspace dependencies and the correct manifest directory:
  all four tests passed. This avoids building unrelated API executable dependencies.
- Initial `g5-scm` library suite: 156 passed, seven failed. Five existing socket-based
  credential/proxy tests were denied local bind by the sandbox. They need a rerun with
  local socket access, not production changes.
- Self finding S1: the new default/full-context contrast assertion mistakes a hunk-header
  label for an included context line. Check actual context lines, and compare
  the default output with the original helper.
- Self finding S2: a malformed reflog does not force a commit failure in a bare repo
  with reflog writes disabled. The uncertain-outcome test needs an actual commit-time
  filesystem failure, with reflog writes enabled, through the real public operation.

## Consolidated findings and disposition

All three independent reports are saved. Main verified their caller and serializer
traces against source before repair. Consolidated findings:

1. **R1 — preserve existing provider retry (P1; all reviewers):**
   `remote_integrations/resolution.rs` reuses the run ID after SQL rollback and owns
   the row-lock/no-committed-task decision. The create-only API is insufficient.
   Add explicit expected-pair replacement under those existing fences, retaining
   create-or-confirm for workspace resolution; check both expected values before
   mutation. Preserve the existing rollback/advanced-head integration test.
2. **R2 — public error evidence (P1; all reviewers):** generic GitError hides useful
   guidance. Keep generic sanitization, but map this operation to a safe typed
   public failure with effect category, repository/ref identity and corrective
   action. Verify the real AppError serialization; never publish backend diagnostics.
3. **R3 — test-only syntax (P2; all reviewers):** exempt cfg(test)/test items from
   production checks and prevent their functions from satisfying required owner
   checks. Add positive and negative fixture pairs.
4. **R4 — function-value bypass (P2; reviewer 1):** inspect maintenance-reader member
   references, not only call expressions; test binding a reader as a function value.
5. **S1/S2 — new test fixtures:** correct assertions that mistake a hunk-header
   label for included context, and force a real commit-time failure using an enabled
   reflog blocked by a nonempty directory. An isolated public-operation probe
   confirmed that failure yields `Uncertain`; default diff bytes match the original
   raw helper exactly.

These findings are recorded before repairs. No new schema or retry persistence is
necessary. The design's fresh-run-only assumption is corrected before dependent
replacement work; the existing `git_ref_updates` contract already requires checked
replacement and `git_retry` preserves this recovery owner.

### Later discovery during repair recheck

Reviewer 1 confirmed the replacement and guard repairs, then traced public failures
into the UI: the human-visible consumer renders only `error.message`. The new message
still did not distinguish refusal from possible writes, and the mapper omitted the
failure stage. Before repair, record R2b: make the safe message effect-specific and
include a typed stage (validation, locks, preconditions, staging or commit). Keep raw
backend diagnostics private and test the serialized message as well as structured data.
The existing rollback/advanced-head API regression has now passed, closing R1's
end-to-end verification gap.

## Repair verification and commitment coverage

R1 is repaired by explicit expected-pair replacement under the existing provider
row/workspace fences. The existing API/PostgreSQL regression passed both failed
enqueue and failed commit cases, partial preparation, advanced heads and repeated
start after task commitment. R2 is repaired with a deliberately public safe error
variant; generic GitError diagnostics remain hidden. R2b adds effect-specific human
messages and typed failure stages. R3/R4 are repaired by excluding test-only syntax
from both access and operation-presence checks and checking reader member references,
including function values. S1/S2 now exercise the intended behavior and pass.

| Commitment | Actual path and discriminating evidence | Result / remaining gap |
|---|---|---|
| `git_execution`, `git_owner`, abstraction/minimal-api/dependencies | Three migrated consumers use typed `g5-scm` operations; backend handles stay private to their implementations. Existing native-rebase and change-graph behavior tests pass. | This slice implemented; raw handles, other consumers, native-only features and provider consolidation remain. |
| `git_ref_updates`, existing recovery ownership | `resolution.rs` locks/checks both refs before writes; tests cover absent, identical, conflicting, partial, symbolic, concurrent and stale expected pairs. Provider row lock distinguishes abandoned preparation from committed task pins. | Shared operation tests and the real rollback/advanced-head API regression pass. Other ref operations remain migration work. |
| `git_outcomes`, authoritative evidence and secret redaction | Actual commit-time reflog failure yields uncertainty; API error mapping retains stage/effect/target/action and sanitizes backend diagnostics; human messages distinguish no writes from possible writes. | Resolution producer and public response verified; broader partial protocol acceptance, cancellations and provider lost replies remain unassessed. |
| `git_retry` | No schema, persistence, retry loops, worker or automatic replay added. Existing explicit provider retry and committed-task no-op retained. | Source inspection and real API retry test pass. Native/provider migration remains. |
| `projection_owner`, `content_reader`, `git_locations` | Maintenance retains RepoStorage and blocking-read ownership; shared readers peel heads/exact refs and distinguish missing refs from unavailable data. Buffer diffs consume already-selected bytes and preserve output budgets. | Shared-reader tests, 23 change-graph tests and all eight projection-convergence integration tests pass. Exhaustive crash/rebuild coverage is not claimed. |
| `git_guard`, structural guard calibration | Existing content-reader checks retained; selected migrated owners reject backend access; maintenance reads reject calls/function values outside their owner. Fixtures cover missing production target, test-only namesake, valid test fixture and harmless helper rename. | Four exact-source guard tests pass. Whole-platform guards and stale push-order check remain unimplemented. |

Verification commands and results:

- `cargo test --offline --locked -p g5-scm --lib --quiet`: all 165 passed on the final code after allowing local socket fixtures.
- `cargo test --offline --locked -p g5-api-core --lib git_conflict_resolution::tests --quiet`:
  both passed on the final code, including human-visible effect/stage messages and private-diagnostic exclusion.
- `cargo test --offline --locked -p g5-api-core --lib source_control_change_graph_service::tests`:
  all 23 passed; no later diff-path changes.
- `cargo test --offline --locked -p g5-api --test remote_repo_tracking_integration resolution_retry_recovers_after_enqueue_or_commit_rollback_and_remote_advance -- --exact`:
  passed using local test databases and loopback fixtures.
- `cargo test --offline --locked -p g5-api --test projection_convergence --quiet`:
  all eight passed using isolated local test databases.
- The exact-source reader guard passed all four tests after repair.
- Ontology IDs/footer entries, local links, embedded ontology diff and diff whitespace
  were checked. Rustfmt passed for the changed Rust files.

Review used three independent full-scope contexts, one main-agent consolidation/repair
and a bounded independent recheck, retaining the initial reports. No measured token or
time comparison is available. The initial abstraction slice is the delivery scope;
remaining native/provider/backend migrations are still explicitly open in the design.

All consolidated implementation findings and the later message/stage finding are
repaired and verified. No scoped test failure remains. The full design is not complete:
native execution, provider/transport consolidation, remaining direct backend consumers
and complete production guards remain explicit implementation work. The design commit
precedes this implementation; the implementation changes remain uncommitted for review.

## Native runner increment — discovery scope

Continue the existing review for the native runner feature boundary, CLI clone/fetch/push
migration, credential injection, cancellation/output lifetime and source guard. The
previous resolution/ref/diff findings and evidence remain valid unless this increment
changes a dependency. The frozen incremental patch and hashes are retained in
`/tmp/g5-native-review-snapshot/`; comparison inputs are the prior initial-slice files
in `/tmp/g5-native-baseline/` and HEAD for previously unchanged files.

Self-checks before independent discovery: the native-only build and six initial native
tests pass, including a local HTTP redirect fixture. CLI compilation passes. One test
initially used a deadline too short to establish its child fixture; its bounded deadline
now permits startup while still proving descendant-held pipes cannot evade cleanup.
Two additional outcome/redaction tests and CLI regression tests are pending. No new
retry storage or replay has been added. Three fresh full-scope reviewers will save raw
reports before main-agent consolidation; implementation stays frozen during discovery.

### Native self-review findings before repair

- NS1: the added public-outcome test fails because `NativeGitError::Display` does not
  render its populated target. This is a code defect, not an assertion to weaken.
- NS2: the real managed-push lease regression reaches the expected Conflict but its
  assertion requires the old exact recovery sentence. Preserve actionable pull/rebase
  guidance while asserting refusal, expected head and the unchanged remote ref instead
  of asserting an unproven explanation for every remote rejection.
- NS3: private Git subprocess probes show inherited `GIT_DIR` overrides an explicit
  `-C` path, and `GIT_CONFIG_PARAMETERS` overrides environment-injected config. Define
  and enforce CLI invocation control-variable isolation; retain runtime PATH/loaders
  and its isolated global config. A stale parent command must not select another repo
  or override this operation's auth policy.
- NS4: the runtime's authorized `insteadOf` relay mapping is inherited through
  `GIT_CONFIG_GLOBAL`. Unconditionally refusing all rewrites can break unauthenticated
  CLI use of that relay. Inspect the actual guest caller and retain this legitimate
  context without sending a token to arbitrary rewritten destinations.

All six structural guards, all ten existing workspace-manifest tests and the
native-only normal dependency tree (no git2/libgit2-sys) pass. The combined library
run passed 171/173: NS1 and an existing HTTPS fixture's short accept timeout failed.
The latter must be rerun with bounded test concurrency after repairs; its first-slice
runtime evidence is retained, not used to dismiss this observed failure.

### Native consolidated findings — recorded before repair

The three initial reports are retained in `reviews/git-abstraction/native-independent-{1,2,3}.md`.
Main checked their source traces and distinguishes cross-port redirect following
(review 1; header stripped) from same-origin path credential forwarding (reviews 2/3).
Both violate the explicit redirect fence; the latter proves credential exposure.

1. **N1, P1 — environment authority (all):** isolate repository-selection and
   per-command configuration variables; preserve declared runtime PATH/loaders and
   isolated configuration. Verify a public push's actual remote OID under conflicting
   GIT_DIR and credentialed HTTP under conflicting GIT_CONFIG_PARAMETERS.
2. **N2, P1 — lost remote status (review 1):** `! [remote failure]` is not a
   rejection receipt. Preserve Unknown and the porcelain explanation, including a
   real accepted push with the per-ref response removed. Keep genuine stale-lease
   refusal as a control; retain confirmed remote completion if a later stage fails.
3. **N3, P1 — guest relay (review 3 / NS4):** preserve the runtime-owned member
   mapping and its token isolation through a declared context. Do not permit arbitrary
   rewrites or inject a token into the relay's Git child. Retain direct-CLI refusal.
4. **N4, P2 — terminal cancellation (review 2):** connect CLI signal ownership to
   runner cancellation and await its cleanup before process exit, including synchronous
   push. Retain future-drop/deadline cleanup. No retry mechanism is involved.
5. **N5, P2 — public target evidence (all / NS1):** include sanitized target in
   Display and G5Error conversion and verify the CLI output path.
6. **N6, P2/P3 — guard calibration (all):** inspect actual process imports/qualified
   paths instead of every similarly named identifier or harmless string. Preserve
   alias, missing-target and test-only checks.
7. **NS2 — recovery guidance:** keep actionable pull/rebase instructions and check
   ref preservation; do not explain every remote refusal as an observed branch move.

These are implementation defects under the existing contracts, not reasons to weaken
those contracts. The native increment remains unready until repairs and feasible
public-path checks pass. No initial finding is erased by later design clarification.

### Scope steering after native discovery

The user raised sibling-workspace Git read isolation, then directed us to stop the
extension if Git was not a practical vehicle. Source inspection confirmed repository-
level read admission followed by full-store upload-pack; Git's documented protocol
limits mean branch filters alone cannot repair it. A separate repository/object-store
redesign would be required. The unimplemented security ontology amendment was withdrawn,
the accepted-MVP exposure restored, and the original abstraction scope resumed. No
security code or destructive store migration was performed. This remains a known
limitation and must not be presented as fixed by this work.

### Native repair verification

N1–N6 and NS2 are repaired under the existing commitments. Invocation-owned environment
controls replace inherited repository/config authority. Guest member mapping comes only
from the runtime-owned global include graph, while actual remote destinations still
have to match; platform credentials never enter the relay Git process. Porcelain
`remote failure` preserves uncertainty and its explanation. Display retains sanitized
targets. The CLI owns signals and waits for runner cleanup even when synchronous push
returns its cancellation error before the signal listener exits. The source guard
follows process imports and paths instead of unrelated identifiers or strings.

| Commitment | Actual path and discriminating evidence | Result / remaining gap |
|---|---|---|
| `git_execution`, `git_native_runner`, stable/minimal APIs | `g5-scm::native` has complete clone/fetch/push/member-remote operations; CLI callers supply owned values. Native-only dependency tree has no git2/libgit2-sys. | Implemented for this increment; host, dispatcher, API protocols and other raw backend consumers remain open. |
| `git_ref_updates`, unchanged workspace admission | `push.rs` preserves all-member fast-forward admission, manifest old-head leases and per-member outcomes. Shared real-repository tests check exact CAS, absent-branch creation and no implicit tags. Existing managed-push test checks the authoritative remote OID after refusal. | Shared/CLI tests pass; other protocol/ref paths are not claimed migrated. |
| `git_native_runner`, calling resource/lifecycle owners | Future drop and deadline tests prove child reaping and no delayed descendant writes, including descendant-held pipes. CLI subprocess tests signal the parent process group during blocking push and require cleaned-up Git plus exit 130/143. | Pass on Unix/macOS; non-Unix descendant-tree cleanup is not runtime-verified. |
| `git_credentials`, runtime token isolation | A public push under conflicting GIT_DIR/GIT_WORK_TREE updates the intended repository's remote OID. Malicious inherited GIT_CONFIG_PARAMETERS and local URL redirect settings cannot forward the supplied HTTP credential. Runtime-pattern includeIf relay receives no Authorization header; unexpected pushurl is refused. | Public local-transport regressions pass. No live deployed VM/provider claim. |
| `git_outcomes`, authoritative evidence, redaction | A protocol proxy removes per-ref status after an actual bare remote accepts the push: result stays Unknown and the remote OID proves the write. Definite rejection and confirmed-write/later-failure controls remain distinct. CLI text/JSON retain target/effect/evidence/action; raw and truncated tokens are scrubbed. | Producer and CLI consumer checks pass; other Git/provider outcomes remain open. |
| `git_retry`, minimal state | Cancellation tracking is in-memory child ownership only; no schema, journal, retry queue, worker or automatic replay. Existing caller retry policy stays in place. | Source inspection; explicit retry/lease regressions pass. |
| `git_guard`, structural calibration | Guard fixtures distinguish actual std/tokio process imports and aliases from harmless `process`, unrelated `Command`, header text and test-only code; removed production targets still fail. | Six exact-source tests pass. Platform-wide guard closure remains migration work. |
| `git_locations`, authorized content/projection owners | Native operations receive caller-owned checkout paths; runtime mapping consumes the owner's supplied config instead of reconstructing layout. The initial API reader changes are unaffected. | Source inspection plus retained initial-slice evidence; typed location consolidation remains open. |

Final executed checks for this increment:

- `cargo test --offline --locked -p g5-scm --no-default-features --features native --lib native::tests -- --test-threads=2`: 13 passed, including two subprocess helper entries.
- `cargo test --offline --locked -p g5-scm --features native --lib -- --test-threads=2`: 178 passed. The previously failing HTTPS accept fixture passed with bounded concurrency; it was not weakened.
- `cargo test --offline --locked -p g5d-cli --lib native_git::tests -- --test-threads=2`: two passed, including the helper and the parent test covering both signals.
- `cargo test --offline --locked -p g5d-cli --lib -- push::tests workspace_manifest::tests process::tests --test-threads=2`: 24 passed.
- Exact-source architecture guard compiled with cached workspace dependencies and the actual API manifest directory: six passed.
- Native-only normal dependency tree excludes libgit2; full host/guest dependency closure is still pending.

The three initial native reviews and pre-repair snapshot remain retained; main owns the
coherent repair and evidence above. The full Git/provider design remains incomplete.

### Later native repair discoveries — saved before repair

The bounded independent recheck in `native-independent-3.md` found two remaining P2
violations. R3-5: fresh workspace-context clone resolves includeIf mappings from the
ambient cwd, so cloning the current incarnation's manifest from the workspace root
cannot find its member relay. Resolve canonical-member routes from the runtime owner's
configuration independently of destination existence; never fall back to token-bearing
repository Git. R3-6: interrupted output collection drops already-received porcelain
confirmation. Retain bounded stream evidence through cleanup and classify it before
returning the failure, proving the remote OID and confirmed/Partial result with a real
push whose descendant holds the pipe open. The earlier passing tests did not cover
these two cases. They require no new contract or retry persistence.

### Later repair checks

R3-5 is repaired by reading the member files explicitly named in the trusted runtime
configuration and resolving the canonical member URL independently of ambient gitdir.
The runner applies that mapping per invocation, so fresh clone and the subsequent
pinned fetch retain the canonical persisted origin without copying the relay capability
into repository config. A real local HTTP clone verifies HEAD, origin, token absence,
no persisted relay URL, and immediate fetch from the new checkout.

R3-6 is repaired by retaining bounded stream buffers outside the cancellable completion
future. The private protocol path interprets complete porcelain records after cleanup;
it preserves both the deadline kind and confirmed/Partial effect. A real push followed
by a descendant-held pipe verifies the authoritative remote OID, deadline cleanup,
confirmed-ref message and absence of the descendant's delayed write. Incomplete records
are never promoted to confirmation. The lost-ack control remains Unknown.

Final reruns after these repairs: native-only suite 15 passed; combined shared suite
180 passed; affected CLI push/manifest/error/signal tests 26 passed. The six guards and
native-only dependency closure remain valid (no subsequent changes to their inputs).
Rustfmt passes for all 20 changed Rust files. The second bounded independent recheck
closed R3-5/R3-6 with no remaining concrete defect in those guarantees. The embedded
ontology diff matches the comparison base exactly; all changed commitment/footer pairs,
local document links and diff whitespace checks pass. Initial findings, snapshots and
raw reports remain retained. Live deployed VM/provider execution and the wider host/API,
backend and provider migrations remain explicitly unverified or unimplemented. This
increment is complete; the full design is not. This review records the initial typed-operation
and native CLI implementation checkpoint.


## Native lease consolidation increment

The user requested commit and continuation. The preceding implementation is committed
as `156ca53e`; this increment replaces the old `GitRepo` native subprocess lease path
with the shared runner, preserving branch/OID sources, provider origin restrictions,
GitHub platform-token SSH identity normalization, SSH keys and compensation deletion.
The existing design's native-lease section pins scope; host/API protocol and provider
HTTP migrations remain outside this increment. The new typed delete operation requires
an explicit expected head and leaves publication/compensation policy with its caller.

Initial self-review: the first broad suite passed 176/177. Its authoritative provider
publication regression caught the commit-OID variant being treated as a branch; the
private adapter now carries `PushSource` explicitly for both branch and commit callers.
No authority rule or assertion was weakened. The old credential-helper/process-drain
implementation and three mechanism-specific tests are removed; new origin-refusal and
SSH credential-lifetime tests plus existing real HTTPS/native lifecycle regressions
cover their obligations at the surviving implementation. The native-only dependency
contract remains required even though the libgit2 feature includes native execution
for `GitRepo`'s existing lease methods.

Seven exact-source architecture tests pass, including the new prohibition on native
process construction in the shared repository adapter. A frozen incremental patch and
source hashes are saved under `/tmp/g5-lease-review-snapshot/`. Three fresh full-scope
independent reviewers will preserve their findings before main consolidation/repair.
Production remains frozen during their discovery.

### Consolidated lease discovery, before repairs

All three fresh full-scope reports are saved as
[lease review 1](reviews/git-abstraction/lease-independent-1.md),
[lease review 2](reviews/git-abstraction/lease-independent-2.md), and
[lease review 3](reviews/git-abstraction/lease-independent-3.md). Production stayed
frozen until all reports were saved. Main consolidation retains three causes:

1. **Encrypted SSH on Linux:** all reviewers independently found the executable
   askpass file retained an open writable descriptor, producing `ETXTBSY`. Treat
   this as P1 for the supported Linux deployment. Retain closed temporary paths
   through cleanup; require successful passphrase delivery in the regression.
2. **Relative local remotes:** review 3 reproduced a public GitRepo regression for
   `../remote.git`. Preserve Git's local path semantics without broadening token
   origins; test actual leased publication, stale refusal and exact deletion.
3. **Normalized destination rewrites:** review 2 reproduced publication into an
   unintended repository after converting a GitHub SSH identity to HTTPS. Validate
   the normalized push destination, including both `insteadOf` and `pushInsteadOf`,
   before credentials or mutation; retain successful identity normalization.

The Linux fixture evidence in the raw reviews checks the OS mechanism, not a full
Rust/OpenSSH handshake. Main's pre-repair checks pass 179 default SCM tests, 19
native-only tests, 26 CLI tests, seven architecture guards, and three real
PostgreSQL/Redis remote-publication API integration cases. Those passing suites do
not invalidate the discriminating counterexamples. Native-only dependencies omit
libgit2. Existing equal-OID publication-proof and outer API-timeout cancellation
limits remain separate inherited gaps. No new retry state or security-isolation
extension is authorized by this repair.

### Lease repairs and verification

The coherent repair closes the writable file descriptors while retaining `TempPath`
cleanup ownership; the askpass fixture only reports readiness after receiving the
expected synthetic passphrase. The actual Rust native tests pass in an offline Linux
container. Restoring only the old writable-descriptor ownership in an isolated copy
makes the strengthened test fail, while the repaired source passes. This closes the
Linux execution defect; a complete encrypted-key OpenSSH handshake remains unexecuted.

The native validator accepts relative local destinations for unauthenticated/SSH-key
contexts without broadening token origins. The public GitRepo regression checks create,
update, stale push refusal, stale compensation refusal and exact deletion against
`../remote.git`, and verifies the persisted remote remains relative.

Normalized GitHub SSH transport is checked again before mutation. `ls-remote --get-url`
checks ordinary rewrites; effective `pushInsteadOf` configuration is checked using its
longest-prefix rule. Refusal tests cover both SSH forms and both rewrite families, with
an alternate bare repository remaining unchanged. The authenticated TLS fixture keeps
a more-specific identity mapping valid despite a broader push rewrite, then verifies
real push/delete and the original persisted SSH identity. No persistent config changes
or extra public API were needed. An intermediate repair using an invocation-only remote
was rejected by the positive fixture: [Git requires a repository-local remote](https://github.com/git/git/blob/v2.50.1/builtin/remote.c#L1606-L1609) for
`remote get-url`. That approach was removed before final checks.

| Commitment | Actual path and discriminating evidence | Final assessment |
|---|---|---|
| `git_execution`, `git_native_runner`, minimal API | GitRepo lease adapter calls shared typed push/delete; duplicate subprocess/auth implementation removed; native-only dependency graph excludes git2/libgit2-sys. | Implemented for this increment. Host/dispatcher/API protocol and raw-handle closure remain later work. |
| `git_ref_updates`, network ref scope/CAS | Public branch/commit push, exact-head deletion and real remote OID tests, including relative destinations and redirected normalized identities. | Pass. Caller policy remains unchanged; already-equal absent-head publication is still not proof of first publication. |
| `git_credentials` | Real HTTPS authentication/redirect/origin checks; normalized GitHub SSH identity push/delete; positive Linux askpass plus lifetime/permission/removal assertions. | Pass for exercised mechanisms. Full SSH handshake and deployed provider credentials remain unverified. |
| `git_native_runner` cleanup | Native Linux/macOS cancellation, deadline, descendant-held pipes and credential cleanup; CLI SIGINT/SIGTERM subprocess tests. | Pass. Cancellation from the API owner's outer timeout through existing synchronous wrappers remains unverified. |
| `git_outcomes`, `git_retry` | Real lost acknowledgment remains Unknown; confirmation then deadline remains Partial; stale lease refusal preserves refs and guidance. | Pass. GitRepo keeps string-shaped legacy errors; no new tracking table, replay or retry owner. |
| `git_locations` | GitRepo owns bare/worktree selection; actual relative local push/delete regression. | Pass for migrated paths; broader typed-location work remains open. |
| `git_guard`, reader boundaries | Seven exact-source architecture checks, including forbidden process imports, disappeared targets and legitimate internal/backend fixtures. | Pass. Existing content-reader/projection boundaries retained; whole-platform guard closure remains open. |
| Publication owners and persisted consumers | Three PostgreSQL/Redis API tests assert actual external branch publication and durable sync state, including an idle follow-up cycle. | Pass. Provider HTTP consolidation and broader publication-proof gaps are unchanged. |

Checks before the tie correction: 181 default SCM tests pass; 20 native-only tests pass on Linux; 26 affected
CLI tests and seven freshly compiled architecture guards pass. The changed positive TLS
fixture also passes after adding the longest-prefix identity case. Three API publication
integration tests pass against the repaired source. Rust formatting and patch whitespace
checks pass. Prior unaffected resolution/reader/projection evidence remains applicable.

Linux setup used exact copied production/native-test sources from `g5-scm` and
`g5-contracts`, the existing lockfile/registry and local `g5/base-rust-warm:26.04`, with
network disabled. The temporary workspace excludes unrelated members and unused
libgit2-only dev dependencies (`openssl`, `g5-semantic`); it is native execution evidence,
not a claim of a full Linux workspace build. Raw initial findings are preserved; three
fresh parallel reviews were consolidated before the main repair, followed by one bounded
independent repair recheck. Broader Git/provider migration and the parked sibling-object
confidentiality extension are not completed by this increment.

### Repair recheck discovery, before the final correction

The bounded independent recheck closes askpass and relative-local compatibility, but
keeps normalized-destination finding 3 open. Review 2's real public-path counterexample
shows Git groups rewrite entries by the first occurrence of each URL base, so choosing
an equal-length match by flat config order can select identity in the validator while
Git selects a different destination. The initial repaired suites above did not cover
this tie. The final correction must refuse tied maximum-prefix candidates whenever
any candidate changes identity, while preserving a strictly longer identity mapping.
This is fail-closed validation of an ambiguous rewrite configuration, without trying
to duplicate Git's base-group ordering. Add the exact three-entry counterexample and
require the alternate repository to remain unchanged before closing finding 3.


### Final lease assessment

The final validator refuses all maximum-prefix ties with any nonidentity candidate,
while retaining successful strictly longer identity overrides. The new exact
three-entry regression refuses before mutation and leaves the alternate repository
empty. The full SCM suite now passes 182 tests and the Linux native-only suite passes
21 tests. The prior 26 CLI, seven guard and three API publication checks remain
applicable: this final correction only tightens normalized-token destination admission,
which is exercised by the shared public-path and authenticated TLS tests. No other
production path changed. All three consolidated causes are repaired; the final narrow
independent tie recheck is recorded in review 2. Remaining migration and verification
limits stay explicit in the scoped ontology metadata.

Final recheck also found a regression-fixture calibration issue: `git config --add`
regroups repeated URL sections, so the test's physical order did not reproduce the
flat-order counterexample even though the independent public driver did. Production
refusal is verified by that original driver. Correct the test by appending repeated
config sections directly and asserting their emitted order; the old flat selection
must then fail this test. This later test discovery is retained separately from the
closed production defect.

A final source check found the new SCP-form predicate also admitted `helper::value`,
which Git interprets as a remote-helper transport rather than SSH. That is outside the
supported transport set. Tighten the predicate before commit and require public native
operations to refuse these forms before repository/process access for both None and SSH
key authentication. Repository-relative paths with a colon after a slash remain local.

The corrected, physically interleaved tie fixture passes on macOS and Linux; restoring
only the flat-order selection in an isolated Linux copy makes it fail with an unexpected
`Updated` outcome. The remote-helper guard also passes through the public native API.
Final source checks pass **183 SCM tests and 22 Linux native-only tests**, alongside the
retained 26 CLI, seven architecture and three API integration checks. All saved findings
in this increment are closed with the verification limits above; no new retry storage
or automatic recovery was added.

## API core backend closure increment

The user asked to continue after `d8f6e729`. This increment migrates API core's remaining
production libgit2 mechanics: exact commit timestamps, selected-branch readiness, local
repository preparation, blob hashes, parsed zero-OID checks and workspace ref syntax.
The existing design's API-core section pins behavior and retained owners. API core's
`git2` dependency is now dev-only; an offline production library check passes. All 186
SCM tests pass, including new repository-state and preparation-side-effect cases.

Source inspection found `GitRepo::commit_committed_at` already implemented the needed
immutable timestamp read; both callers reuse it. Existing initial-commit preparation
was inspected and remains parentless/unattached. Native execution, provider HTTP and
workspace-ops public-backend closure remain outside this increment. No schema, retry
state, credential flow or authorization owner changes.

The source will remain frozen for three fresh independent full-scope reviews. API
integration checks run against that snapshot; raw findings will be saved before main
consolidation and any repairs. Metadata is reconciled after the reviewed result.

### Consolidated API core discovery, before repairs

All three fresh full-scope reviews are saved as
[API core review 1](reviews/git-abstraction/api-core-independent-1.md),
[API core review 2](reviews/git-abstraction/api-core-independent-2.md), and
[API core review 3](reviews/git-abstraction/api-core-independent-3.md). Production and
ontology remained frozen until all three reports were saved. Each inspected the full
increment and affected consumers; none found a production defect. Main consolidation
agrees after comparing the actual reused operations and ownership paths, not from the
number of agreeing reports. Raw reports and the pre-repair snapshot are retained.

Main's API build did find a test compilation error: the new organization preparation
assertion compares `Reference::symbolic_target()`'s `Result<Option<&str>, Error>` with
`Option<&str>`. Unwrap the successful lookup in that assertion; this changes no production
behavior. The other four selected API integration cases already pass: Home's custom
unborn repository, immutable captured commit timestamps, membership/version comparison,
and the complete review suggestion/artifact flow. The new organization preparation test
and three existing bootstrap cases still need execution after this test-only repair.

Current evidence also includes 186 shared SCM tests, three affected API-core unit tests,
a production API-core library check and nine exact-source architecture guards. Reviewers
independently reran focused shared tests and freshly compiled the current guard. The
remaining reconciliation is metadata for API-core closure, retained reader/registry
limits and guards; no governing meaning amendment is required. No review effort or
benefit timing was measured. Broader workspace-ops/raw-handle, native and provider
closure remains open, and sibling-object confidentiality remains parked.

### API core final assessment

The test-only repair unwraps the symbolic-target lookup and preserves the intended HEAD
assertion. All four organization tests now pass, completing eight selected API integration
cases. No production repair was required. Scoped metadata now records API-core backend
closure and current guard/consumer evidence; inherited gaps remain explicit. The design's
first ontology diff was refreshed against its original comparison base, with no governing
meaning change.

| Commitment | Actual path and discriminating evidence | Result or remaining gap |
|---|---|---|
| `git_execution`, minimal API/dependencies | API core delegates to owned SCM operations; production library compiles without a direct `git2` dependency; backend dependency is dev-only. | Implemented for API core. Workspace-ops/raw-handle, native and provider closure remains open. |
| `content_reader`, `workspace_registry`, `git-authority` | Repository versions resolve a selected branch once, then use immutable commit metadata; API test distinguishes author/committer time, moves the branch, and rejects missing/blob targets. Membership comparison remains exact. | Both API cases pass. Existing reader freshness and pin/prune gaps remain. |
| `content_reader`, bootstrap state | Shared regression distinguishes custom unborn HEAD/no refs, existing tag/populated refs, unavailable branch and invalid targets; real Home API preserves clean unborn project metrics and relational attention. | Shared and API checks pass; no mutable-checkout fallback or new source-selection authority. |
| `git_ref_updates`, `git_locations`, registry/content authority | Shared preparation test preserves index/worktree/refs and rejects invalid names before creation; actual service returns a parentless README candidate without a branch. Three existing org/bootstrap cases exercise the checked publication owner. | All pass. Preparation is not publication; broader protocol/location closure remains open. |
| `git_execution`, value-policy ownership | Known blob hashes and OID parsing/null tests; two observation-cache tests; exact branch naming unit test; full review flow publishes and accepts content-addressed artifacts. | All pass. Artifact storage/limits, cache expiry/binding and workspace naming policy remain with their owners. |
| `git_guard`, reader/projection boundaries | Nine exact-source tests scan production source/dependency inventory and reject aliased backend, missing target and wrong reader-owner fixtures while retaining legitimate/test-only use. | Pass. Whole-platform/raw-interface/provider guards remain incomplete. |
| `git_retry`, `git_outcomes`, outcome evidence | Entire diff and initial preparation/landing split inspected; observation failures preserve caller behavior. | No new persistence, automatic replay or retry owner. Existing error categories retained; wider native/provider outcome migration remains open. |

Final checks: **186 SCM unit tests, three affected API-core unit tests, eight API
integration tests and nine exact-source architecture guards pass**. The production API
core library check, changed-file Rust formatting and patch whitespace checks pass.
Database tests used existing local PostgreSQL and Redis. Independent review used three
fresh full-scope contexts, one saved main consolidation and one test-only repair; initial
raw reports remain alongside this assessment. Broader abstraction completion and the
parked security redesign are not claimed.

## Workspace operations backend closure increment

Continuation after `7054bdfb` moves live unattached commit preparation into SCM, with
workspace-ops retaining its public consumer entry points. The shared exact-files operation
omits the unused worktree-root argument; only the compatibility wrapper retains it.
All validators remain with their complete operation. Clone/init/current-branch mechanics
move behind GitRepo, preserving the two existing clone transport variants. Public raw
constructor/lock/callback APIs close after migrating their callers; fixture code opens
repository paths through supported interfaces.

Full source inventory found only test callers for isolated-worktree writes and the
repository-local safe-directory setter, so those unused APIs and their five tests were
removed. Four new tests cover branch/clone behavior and exact-tree validation effects.
The live global safe-directory helper remains explicitly outside this backend increment.
No policy owner, retry state or provider behavior is added. Production library checks,
186 SCM and 48 workspace-ops tests pass; architecture guards pass ten tests, and four
SCM doctests include three forbidden-interface cases and one callable-interface control.

During self-check, removed production imports needed test-only replacements; the detached
HEAD fixture needed one fewer unwrap. An overbroad guard edit initially included workspace
operations in the API-only content-reader scan; its scope was restored while keeping the
backend scan across both owners. These were repaired before independent discovery.
An initial SCM run could not bind local test sockets in the sandbox; the authorized local
server run passes. No failures were converted to skips or weaker behavioral expectations.
Production remains frozen for three fresh full-scope independent reviews; API/database
and CLI consumer execution is owned by main. Initial findings are saved before repairs.

### Consolidated workspace-ops discovery, before repairs

Three independent full-scope reports are saved as
[review 1](reviews/git-abstraction/workspace-ops-independent-1.md),
[review 2](reviews/git-abstraction/workspace-ops-independent-2.md), and
[review 3](reviews/git-abstraction/workspace-ops-independent-3.md). The snapshot stayed
frozen until all reports were saved. Consolidation retains two reproduced causes:

1. **Incomplete initial-file batch validation.** This is inherited unchanged code, not
   a regression from relocation; the sole current production caller provides fixed
   `README.md`. Nonetheless the migrated public operation can silently lose requested
   content for ancestor collisions and can write blobs before rejecting `.git`, NUL or
   repeated-separator paths. Reuse complete canonical path/collision validation before
   object creation, retaining the initial operation's `.g5` restriction, accepted trimming,
   parentless semantics and no-ref behavior. Test both collision orders and invalid later
   entries against actual object/ref/index/checkout state, plus valid multi-file success.
2. **Public-interface guard gap.** Existing visibility is closed, but former-name
   compile-fail checks do not reject a newly named raw accessor or re-export. Add a
   signature/type/export guard with backend alias propagation, and calibrate against
   private helpers and owned public wrappers. Retain the real external privacy probes.

The passing pre-repair evidence is 186 SCM tests, 48 workspace tests, 23 intent-log and
23 CLI unit tests, six API integration cases, ten guards and four doctests. Native-only
SCM builds and excludes libgit2 from its normal dependency tree. These results do not
invalidate the reviewers' counterexamples. All eleven moved preparation/helper bodies
were identical to the base modulo the private lock access; that explains the inherited
finding, not its acceptance. Main owns one coherent repair; no governing meaning change,
retry persistence or broader security redesign is needed. Raw reports remain preserved.

### Workspace repair recheck, before guard correction

The bounded independent recheck closes initial-batch validation: original collision/NUL
probes now refuse before writes, and all 49 workspace tests pass. It leaves the guard
finding open for two adjacent calibration cases: private `use git2::*` can hide a public
`Repository` return, while a wholly private trait's associated backend type is incorrectly
reported as public. Correct wildcard name resolution and respect private trait scope;
retain their exact counterexamples and valid private-helper controls before closing it.

### Workspace repair evidence

The shared initial-files operation now validates the complete normalized batch with the
existing exact-tree path/collision validator before taking a repository lock or writing
objects. Initial trimming and `.g5` exclusion remain; exact preparation retains its own
durable `.g5` policy. The new public-path regression compares the complete object inventory,
HEAD, real index bytes and dirty checkout bytes for invalid later entries, both ancestor
collision orders and duplicates after trimming. A valid two-file parentless commit contains
both exact blobs, including a literal pathspec-like name, without moving refs or checkout.
All **49 workspace tests** pass, and the bounded reviewer independently reran the original
public API probes: collision/NUL now return validation errors with no candidate blob.

The surface guard now follows explicit backend aliases and re-exports across source
modules, checks public signatures/fields/associated types, and treats backend wildcard
imports conservatively. Private trait implementations and imported private-trait aliases
remain valid. Its calibrated positive cases include owned public wrappers and private
raw helpers; negative cases include new accessor names, callback parameters, public
fields/exports and a public wrapper's raw `Deref::Target`. This remains a structural guard,
not Rust type/macro expansion. Final bounded independent disposition follows below.

All **12 exact-source guards and four external API doctests** pass. After the validation
repair, the six selected API integrations pass again: unpublished initial preparation,
three organization/bootstrap cases, the review publication/acceptance flow and mixed
unborn-member materialization. Earlier valid evidence remains **186 SCM tests, 23
intent-log tests, 23 CLI consumer tests**, production API-core/workspace-ops library checks,
and a native-only SCM build with no normal libgit2 dependency. Changed Rust formatting
and patch whitespace checks pass. No live shallow/SSH clone matrix or full encrypted-key
SSH handshake was executed; those limits remain in metadata.

| Commitment | Actual path and discriminating evidence | Result or remaining gap |
|---|---|---|
| `git_execution`, minimal API, information hiding, dependencies | Workspace-ops delegates preparation/clone/init/current-branch; raw constructor/lock/callback assembly is private; production dependency is dev-only. Public-surface guard and external privacy/control doctests exercise closure. | Implemented for this increment; wider native/provider closure remains open. |
| `git_ref_updates`, content authority | Complete initial/exact preparation validates batches, writes only unattached objects through a private index, and preserves real HEAD/index/checkout. Regression observes actual object inventory and exact committed tree contents. | Initial validation defect repaired; no publication authority or failure-atomicity promise for arbitrary disk errors. |
| Reader/registry ownership, compatibility | API checked bootstrap, review and materialization consumers pass; intent-log/CLI tests preserve immutable inputs and branch/layout selection. Compatibility re-exports retain existing callers. | Scoped consumers pass; broad platform facade conformance is not claimed. |
| `git_locations`, `git_credentials` | Shared clone/init preserve selected/default branches, URL and depth options, distinct transport variants and callback policy; local clone/current-branch fixtures pass. | Source equivalence and local runtime evidence; remote shallow/SSH clone remains unverified. |
| `git_guard`, enforcement | Source/dependency/public-interface guards cover migrated owners and reject forbidden aliases, imports and raw surfaces while retaining valid private helpers. | Exact-source tests pass; structural limits and whole-platform migration remain explicit. |
| `git_retry`, `git_outcomes` | Shared validation errors precede invalid-batch effects; candidates remain separate from checked landing; diff adds no persistence, replay or worker. | No retry tracking or automatic replay added; broader native/provider outcome migration remains open. |

Three fresh full-scope reviews, saved consolidation and coherent main repairs were used.
Raw findings and the pre-repair snapshot remain preserved. Review effort was not timed.
The global safe-directory helper and sibling-object confidentiality remain outside this
increment; neither is asserted fixed by backend consolidation.

The final bounded independent recheck closes the surface finding for the reviewed
explicit signatures and private-helper calibration. Its fresh exact-source build passes
all 12 repository guards plus two reviewer counterexample/control checks, including
private-trait aliases across modules and public associated backend types. Both consolidated
findings are now closed within the documented verification limits.

## API garbage-collection execution increment

Continuation after `99f7961d` moves API maintenance's native GC mechanics behind
`NativeGit::collect_garbage`, with a named API context and owned result values. The
existing sweep retains SQL enumeration/advisory locking, threshold and scheduling policy,
per-repository error isolation and logs. Shutdown forwards cancellation and awaits cleanup
before unlocking. The API-selected binary resolves once per handle; the shared supervisor
owns all children/streams. No new retry, retention or reader-serialization policy is added.

Before independent discovery, 192 SCM tests, five existing API GC unit tests and thirteen
exact-source guards pass. Shared tests cover actual packing with unchanged refs, real
index/config/checkout, missing/bare storage, exact threshold, malformed/nonzero reads,
confirmed GC followed by inspection failure, no replay, fixed API environment and active
child cancellation/deadline cleanup. The production library check passes. API/database
lock/error/shutdown checks are running against this snapshot; native-only evidence is
being refreshed. Existing CLI native tests exercise the unchanged CLI context and confirm
that a failed completion after an accepted push still reports the confirmed remote ref.

Self-checks caught an overbroad backend guard extension admitting the API native owner but
also rejecting a still-unmigrated CLI value-helper import; its existing scope was retained.
An initial 300ms fake-process deadline could expire during setup under parallel load,
before the intended hanging GC stage. The deadline fixture now allows one second per
invocation while still requiring the hanging stage and child reaping. The full shared
suite passes. Neither failure was hidden or converted to a skip.

Production and ontology now freeze for three fresh, full-scope independent reviews. Raw
reports will be saved before consolidation or repairs; no review benefit/timing claim.

### Consolidated API GC discovery, before repairs

Raw full-scope reports are saved as [review 1](reviews/git-abstraction/api-gc-independent-1.md),
[review 2](reviews/git-abstraction/api-gc-independent-2.md) and
[review 3](reviews/git-abstraction/api-gc-independent-3.md). The nine-file snapshot
remained unchanged throughout discovery. All three used fresh contexts; reviewer 2
inadvertently displayed the main increment's 28-line verification narrative before
saving findings, so that review is qualified rather than fully unexposed. It read no
peer findings. Reviewers 1 and 3 saved findings before any main/peer narrative. No review
benefit or cost was measured. Consolidation retains these causes and obligations:

1. **CLI fixture mismatch:** Partial display now correctly describes a general completed
   effect, but the existing CLI text/JSON fixture still expects the old remote-ref phrase
   without supplying push-specific detail. The exact consumer test fails. Update its
   realistic operation details and generic effect expectation, preserving actual push
   confirmation and target/stage/retry assertions.
2. **Guard calibration:** the new actual inventories pin two private helper names, so
   real helper-only renames fail despite the synthetic control. Also, a nested
   `std::process` path causes harmless PID/Stdio access to be rejected. Anchor missing
   targets to public owner operations, check complete Command paths and resolved aliases,
   retain wildcard-construction refusal, and test the actual inventories against helper
   renames plus legitimate process values.
3. **Lost completed GC in API accounting:** SCM records successful GC followed by a failed
   final count as Partial; API drops that evidence from `repos_packed` and can report
   "nothing to pack". The old API had this aggregate limitation, but the new explicit
   outcome must reach its current consumer. Count confirmed GC while retaining its failed
   inspection warning; never count Refused/Unknown as confirmed. Add a public sweep test
   with real packing followed by failed inspection, initial-read refusal and an unknown
   GC completion control, including exact attempt counts.
4. **Ordering evidence limit:** the DB shutdown test checks lock/child endpoints, not the
   instant of lock acquisition after cancellation. Source correctly awaits cleanup before
   unlock. Add a concurrent lock contender that checks child liveness at acquisition;
   retain source ordering as part of the evidence rather than claiming universal runtime
   scheduling proof.

Main additionally found by inspection that API bare-name lookup selects the first regular
file even when it lacks executable permission, unlike the existing PATH execution
semantics. Preserve executable-candidate selection and test rejection of a non-executable
first candidate with a valid later candidate; retain the fixed API PATH. This is a bounded
resolver compatibility repair, not a new executable-selection policy.

Main's frozen-snapshot checks pass 192 SCM tests, five API-core GC tests, three DB-backed
GC integrations, two CLI signal/cleanup tests and thirteen exact-source guards. The
native-only build excludes libgit2. The first Linux parallel run passed 25 native tests
and failed one existing fixture at spawn with `ETXTBSY`; all 26 pass unchanged with serial
test scheduling. No production retry was added to mask that fixture failure. The initial
macOS short-deadline fixture lacked early-task diagnostics, so its precise setup failure
was not established; improve that diagnostic while retaining the hanging-stage/cleanup
assertions. Source copies, raw findings and initial receipts remain preserved. Main owns
one coherent repair under unchanged governing meaning.

### API GC final repair assessment

The API consumer now recognizes this operation's documented Partial guarantee: collection
completed and only final inspection is unavailable. It logs that inspection failure and
increments the completed count. The new public sweep regression uses actual Git packing
behind an executable wrapper: confirmed GC plus failed count contributes one; a refused
first count contributes zero; a GC whose response is failed remains Unknown and contributes
zero even though the fixture observes physical packing. Captured logs retain completion,
failed inspection and inspect-before-retry guidance. Exact stage traces prove no replay.
All four GC integrations pass, including the existing advisory-lock and per-repository
error-isolation cases.

The CLI fixture now supplies real push confirmation in its detail and checks the generic
Partial category in both text and JSON. The actual push-confirmation/deadline regressions
still pass. Guard inventories retain public owner operations and scan all implementation
functions without pinning the new private helpers. Positive controls use actual source
helper renames, PID and Stdio access; configured/aliased/wildcard/`{self, ...}` Command
construction and missing public targets remain rejected. All fourteen exact-source guards
pass. API bare-name lookup shares executable-file selection with CLI, and its refusal/valid
later-candidate regression passes.

The shutdown integration now starts a second lock acquirer, proves through PostgreSQL's
`pg_locks` that it is blocked on the active sweep, then cancels GC. The contender observes
the child gone when it acquires the released lock. This improves E1's runtime boundary
observation; source inspection also establishes the cleanup await before unlock. It is
not a proof of every scheduler interleaving, role=all process teardown, blocked SQL/NFS
operations or deployment behavior. The strengthened test passes. After the bounded
independent recheck, only that test's explicit PostgreSQL waiting precondition and a public
method comment documenting Partial/Unknown guarantees changed; the test was rerun.

Reviewer 3's bounded recheck independently passed the exact CLI error test, all fourteen
guards and seven SCM GC/lookup checks, and closed its initial findings. It inspected the
completion-accounting repair and attributed DB runtime evidence to main. The initial
reports remain unchanged with appended dispositions. During repair self-check, an added
`{self, Stdio}` negative fixture exposed the parser's separate `self` node kind; import
resolution now handles it. A macOS wrapper fixture used `cat` outside the fixed API PATH,
misclassifying its test modes; POSIX shell built-ins now set and validate those modes.
Expected completion counts were retained, not weakened.

| Commitment | Actual path and discriminating check | Final disposition or limit |
|---|---|---|
| `git_execution`, minimal API, information hiding, dependency direction | API GC delegates one typed operation; shared count/GC/count and private context/runner own mechanics. Source guard rejects native construction and public backend exposure. Native-only compilation/dependency inventory excludes libgit2. | Implemented for GC; other native protocols/runtime/provider consumers remain. |
| `git_native_runner`, context and credentials | API context clears ambient controls/secrets and uses its fixed PATH; executable lookup skips non-executable candidates. CLI contexts and real native push behavior retain their original policy. | Runtime controls and valid variants pass. Repository/system configuration preservation is inspected; no isolated system-config fixture or deployed environment claim. |
| `git_native_runner`, cleanup, API scheduling | Active GC deadline/cancellation reaps the child; source awaits that result before SQL unlock. Public DB test observes active lock, blocked contender, then reaped child at contender acquisition. | Runtime and source evidence pass with E1/deployment/SQL/NFS limits above. Existing periodic scheduling, one-repo-at-a-time policy and advisory lock stay in API. |
| `git_locations`, `git_ref_updates` | Owner-selected paths; missing/bare/checkout cases and actual packing preserve refs/HEAD/index/config/dirty content. Exact threshold collects; above threshold skips without pack creation. | Pass for tested layouts. Linked-worktree behavior is inspection-only; no new pruning/retained-source/reader-serialization policy is claimed. |
| `git_outcomes`, authoritative completion, compatibility | SCM stage errors distinguish Refused/Unknown/Partial; API completion totals and captured warning preserve Partial; Unknown physical effects remain unconfirmed. CLI text/JSON retains target, stage and effect. | Native, API/DB and exact affected CLI controls pass; wider provider/protocol outcome migration remains. |
| `git_retry`, recovery ownership | Stage attempt traces and full diff; existing periodic sweep remains at its owner. | No new tracking tables/state/queues/workers or automatic operation replay. |
| `git_guard`, calibrated enforcement | Exact real-source private helper renames and benign process-value access pass; Command aliases/wildcards and deleted public targets fail. | Fourteen guards pass; structural type/macro limits and broader migration remain explicit. |
| Authorized reader and later GC scheduling | No content API/source-selection change; existing reader guards still pass; current GC command/retention semantics retained. | No reader bypass added. Future active-reader/pin/pruning serialization and parked sibling-object confidentiality remain unimplemented. |

Final evidence: **193 SCM tests, five API-core GC tests, four GC API/DB integrations,
the affected CLI text/JSON test, fourteen exact-source guards and 27 Linux native tests
pass**. Earlier CLI signal/cleanup controls remain valid, with unchanged signal ownership.
Linux used exact copied SCM/contracts sources and the existing offline local warm image,
a minimal two-crate workspace and no network; this is native-only evidence, not a full Linux
workspace/deployment build. The executable lookup repair was included in the final Linux
run. Source-formatting, whitespace, metadata pairing and the saved ontology-diff check are
part of final reconciliation. No unresolved production finding remains in this increment;
broader Git/provider migration is still incomplete.

## API Git proof observations increment

Pinned design: `ontology-plan-git-abstraction.md#api-git-proof-observations`;
implementation baseline `f95fac40`. Three fresh full-scope reviewers ran in parallel
under ontology-review, with identical inputs and no implementation history, author
narrative or peer findings. Production, tests and ontology were frozen during discovery.
The pre-repair source inventory/archive is `/tmp/g5-api-observations-review`; durable
raw reports are [review 1](reviews/git-abstraction/api-observations-independent-1.md),
[review 2](reviews/git-abstraction/api-observations-independent-2.md) and
[review 3](reviews/git-abstraction/api-observations-independent-3.md). Each saved its
report before sharing. Main owns this consolidation and the following coherent repair.

### Consolidated API observation discovery, before repairs

| Finding | Evidence and distinct obligation | Disposition / required repair |
|---|---|---|
| O1: selected repository can fall back to an ancestor | All reviewers and main reproduced Git `-C` discovery in a non-repository child. Existing explicit Git-directory selection did not do this. Violates storage identity. | Anchor every observation to the supplied bare/worktree/linked-worktree Git directory inside SCM. Add all-operation negative controls and retain the valid layouts. |
| O2: corrupt storage can appear absent | All reviewers/main reproduced broken refs; reviewer 1 additionally reproduced damaged compressed objects returning a successful batch `missing` record with error diagnostics. Ordinary missing refs/objects must remain ordinary absence, while harmless Apple Git environment warnings remain compatible. | Interpret storage-error diagnostics in the native evidence path, reject malformed negative responses, and test native plus public flush outcomes without receipt advancement. |
| O3: revision expansion substitutes a different ref | Reviewer 3 reproduced shadow `refs/heads/refs/...` branches satisfying the old `rev-parse` spelling for absent immutable leaves. This inherited weakness remains a conformance defect in the newly declared exact-ref operation. | Use exact ref lookup, with native and API shadow-ref controls. Preserve existing symbolic-ref inspection/refusal policy. |
| O4: late read failure discards accepted push continuation | All three independently source-traced the unconditional post-receive read before captured `ok <ref>` evidence. New deadlines/malformed-output failures extend an inherited early-return path. Read-only Refused evidence cannot stand for the enclosing write. | Consume confirmed protocol acceptance independently, retain successful siblings through needed observation failures, and return partial/unknown write evidence with inspection guidance. Preserve admission release before queue/events/reviewer work. Exercise real receive-pack with injected late failures. |
| O5: SHA-256 abbreviations pass full-OID validation | Reviewer 3 reproduced a 40-character prefix resolving to a SHA-256 commit. Existing syntax-only route validation also had this gap. | Check OID length against the selected repository object format for object and ancestry operations; test both allowed formats and forbidden prefixes. |
| O6: new symbolic-ref API regression does not compile | Main API build found `git2::Reference::symbolic_target()` returns a Result around the Option. | Correct the test assertion, then execute the push policy suite. This does not constitute production evidence until passing. |

Pre-repair evidence: 197 SCM unit/regression tests passed on macOS with local socket
permissions; 31 native-only tests passed on Linux in the existing network-disabled image
using isolated SCM/contracts sources (unused libgit2-only dev dependencies omitted).
Fifteen exact-source architecture guards, seven flush API tests against local PostgreSQL/
Redis, and nine Git route unit tests passed. An initial sandbox-only native run had three
local-listener permission failures; the permitted full SCM run resolved them. Reviewer 3
independently ran four focused existing native binaries; other reviewer checks were real
Git probes and source inspection. Passing tests did not cover O1–O5 and do not override
those counterexamples. The combined push/flush build failed at O6; only the separately
run flush suite supplies API runtime evidence so far.

No governing meaning amendment is needed: these repairs implement the approved exact
identity, verified receipt, accepted-ref signal, typed outcome and no-new-retry rules.
Protocol process/stream migration remains separate. Full provider continuation and
receipt-service alternate-caller/pinning/pruning coverage are not implied by this increment.

### Observation repair recheck, before final error-mapping correction

Reviewer 3's disclosed source recheck confirmed O1–O5's native/route repairs and
preserved its original report. It identified one remaining adapter issue: repository-
relative OID validation now returns `InvalidInput`, but `require_commit` treated it
as temporary `FLUSH_UNAVAILABLE`. A syntactically 64-hex SHA-1 receipt is a permanent
invalid proof, not an execution failure. Map only object-input invalidity at that
proof boundary to the existing `FLUSH_NOT_COMMIT` 409, with a wrong-width API control
and subsequent valid completion. Keep setup/ref/storage/deadline failures unavailable.

During repair verification, the first exact-lookup attempt with `show-ref --quiet`
was rejected by the corrupt-ref regression because Git suppresses that diagnostic;
the final implementation uses exact filtered ref enumeration plus dangling-symbolic
inspection. Test-only setup corrections replaced read-only compressed-object fixtures
before corruption, supplied the isolated storage's worktree directory, and added the
remote-integration binding required to discriminate real queue publication. Expectations
were retained; the missing binding was fixed instead of accepting a zero queue count.

### API observation final repair assessment

O1–O6 and the repair-recheck mapping finding are resolved in the scoped implementation.
SCM anchors Git directories explicitly, uses exact ref enumeration (including a
separate dangling-symbolic check), observes diagnosed storage failures, and verifies
OID width against the repository format. The flush adapter preserves proof conflict
codes. Receive-pack processes `ok <ref>` evidence without a later read and retains
confirmed siblings through fallback failures; unconfirmed writes return Unknown and
confirmed effects followed by failed verification return Partial. These are in-memory
outcomes and existing queue/event continuation, not new retry state or replay.

| Commitment | Actual path → counterexample/check | Final result / boundary |
|---|---|---|
| `git_execution`, minimal API, information hiding | Four owned native observation methods; private stdin delivery stays in the process supervisor. Flush and push policy supply selected storage and inputs. | Source and guarded API compilation pass; no command/env/child escape hatch or new upward dependency. Wider native/provider migration remains open. |
| `git_locations` | Exact `--git-dir` relative to selected `-C`; real bare, checkout and linked-worktree controls; invalid child of a valid repo; API storage moved into its ancestor with matching proof. | Native and public API controls pass. Invalid selected storage is unavailable and cannot borrow parent proof. |
| `git_outcomes`, `g5.git.receipt` | Missing/noncommit/wrong-width identities, exact leaves versus shadow branches, reversed ancestry, corrupt loose refs/objects and metadata loss through public flush completion. | Seven flush tests pass after final mapping correction; failures preserve complete_at/selection and restored valid proof completes. Missing values remain proof conflicts; diagnosed storage/runner failures are unavailable with sanitized detail. Other service entry points and pin/prune races remain outside this evidence. |
| `git_native_runner`, lifecycle | Shared concurrent stdin/stdout/stderr supervision; deadline, explicit cancellation and dropped awaiter during object checks; large bidirectional pipe control. | Final four observation tests pass on macOS; all 32 native tests pass on Linux with native-only dependencies. Full 197-test SCM run before observation-only repairs covered shared runner/lease/credential regressions. Linux uses isolated source/minimal workspace, not a full Linux API build. Non-Unix descendant cleanup remains unverified. |
| Full Git identities | Exact ref names and selected repository's SHA-1/SHA-256 width; full valid IDs versus 40-character SHA-256 prefixes; no revision-DWIM proof. | Both formats and rejected prefixes pass native tests. Full SHA-256 API push/flush lifecycle is not claimed. |
| `git_ref_updates`, network CAS/ref scope, receipt write-admission | Existing owner/incarnation policy tests plus symbolic-ref refusal with a configured integration; leased rewrites, stale server CAS, missing branch, immutable receipt scope. | Eight push tests pass (the environment-gated child entry is a no-op in the outer run; its parent explicitly executes three fault scenarios). No new ref-writing authority. Exhaustive principal/concurrency combinations remain unverified. |
| `git_outcomes`, accepted-ref signals and PR continuation | Real receive-pack, executable wrapper failing only later ref enumeration; confirmed result, confirmed branch with rejected flush sibling, and omitted report-status unknown result. Observe actual refs, existing queue and branch-event receiver. | All three fault scenarios pass: confirmed effects keep queue/events; rejected leaf remains absent; unknown result emits neither confirmed signal nor replay. Errors retain Partial/Unknown and inspection/Update PR guidance. Existing automatic PR update test also passes. Reviewer-copy notifications on injected failure are source-inspected, not separately exercised. |
| `git_retry` | API error/continuation paths plus wrapper's failed-read attempt count. | No schema, retry column/table, new queue/worker or automatic replay; failure attempted once. Existing provider queue and flush lifecycle remain their owners. |
| `git_guard`, enforcement/calibration | Flush owner and SCM observations required by name; helper renames, alias/direct process bypasses and test fixtures; existing reader/backend guards retained. | Fifteen source-guard tests pass against the repaired sources. Protocol-owner process construction remains a declared gap, not a covered owner. |

Additional evidence: nine Git route unit tests passed before the localized post-receive
repair; the full public push matrix/fault tests exercise the repaired path. `rustfmt`
and final diff/metadata checks accompany the commit. The independent raw reports and
pre-repair findings remain above; one reviewer provided a bounded source recheck, and
main owns the final error-mapping repair and runtime assessment. No measured token/time
comparison was attempted.

## API protocol execution — initial independent consolidation

Three fresh full-scope reviewers inspected the frozen increment from `ab5bf96e` with
identical task inputs and no implementation-session or peer findings. Their preserved
reports are [1](reviews/git-abstraction/api-protocol-independent-1.md),
[2](reviews/git-abstraction/api-protocol-independent-2.md), and
[3](reviews/git-abstraction/api-protocol-independent-3.md). Main owns consolidation and
one coherent repair. Snapshot: `/tmp/g5-api-protocol-review/{tracked.patch,snapshot.tar,files.txt}`.
All reviewers confirmed the same thirteen-file snapshot. This section records initial
findings before any repair; reviewer agreement is not the correctness criterion.

| Finding | Evidence and disposition | Required repair / discriminating check |
|---|---|---|
| P1: accepted effects lost on continuation failure | Reviewer 3 and main trace `?` returns from admission release, scoped publication DB acquisition, enqueue and commit after native receive success. This inherited behavior is now reused by the typed outcome owner, so it remains a conformance defect: an ordinary Internal response discards confirmed refs and can suppress siblings' signals. Reviewer 1's no-finding outcome does not refute the concrete path. | Retain confirmed refs and Partial/Unknown evidence through release/publication failure. Attempt independently admitted continuations and confirmed signals without replay; report the failed stage and explicit Update PR recovery. Inject a DB continuation failure after real accepted receive, then inspect actual refs, signals, queue and response. |
| P2: ordering guard counts comments and rejects formatting | Reviewers 2 and 3 independently ran the exact helper: commented guards/effect pass and a multiline real service call fails. Reviewer 2 rates P2 and reviewer 3 P3; the demonstrated guard obligation is accepted irrespective of rating. | Match actual Rust calls and required arguments, ignore comments/string decoys, distinguish `require_push` from `require_push_ref_updates`, accept equivalent whitespace/helper renames, and reject missing effect/guard and wrong order. |
| P3: TLS fixture pre-handshake timeout | Main full SCM run: 201/202 pass; the remaining existing libgit2 local proxy fixture twice fails at `peek().unwrap()` on an idle CONNECT before any TLS bytes. This is fixture evidence, not a production transport regression. The helper already permits empty pre-TLS closures and requires actual expected requests afterwards. | Treat an idle pre-TLS connection as incomplete fixture traffic, while keeping nonempty TLS handshake strict and existing request assertions. Recheck the failed case and strict non-TLS control. |

Pre-repair main evidence: 8 public push-policy tests pass, including parent-invoked five
fault scenarios (confirmed, rejected sibling, unknown report, post-acceptance process exit,
and aborted router awaiter retaining SQL admission through native completion); 7 public
flush tests pass; 15 architecture and 11 route-auth checks pass. All 36 native-only tests
pass on Linux in isolated SCM/contracts source without network access. macOS full SCM has
the TLS fixture failure above; native tests all passed under local-socket permissions.
Earlier sandbox-only runs could not start local HTTP listeners and one deadline fixture
missed its effect; the permitted full run resolves those failures. Direct ad-hoc compilation
of route-auth initially selected incompatible cached Serde identities; Cargo compilation
and all eleven tests pass. Reviewer check boundaries and durations remain in raw reports.

No governing meaning amendment is needed. Repairs enforce the approved effect evidence,
existing continuation ownership, no-new-retry and calibrated-guard contracts. Broader
host/guest/CLI/provider migration and unexecuted deployment/alternate-caller boundaries
remain open; this increment does not claim overall completion.


### API protocol repair and final assessment

Main repaired P1–P3 without changing governing meaning. Reviewer 3's disclosed bounded
recheck finds the continuation defect repaired by source inspection. Reviewer 2's first
recheck found an additional harmless-format counterexample (a final argument comma);
main then changed comparison to named argument expressions and added actual-call controls.
The preserved report distinguishes initial discovery and both repair assessments.

| Commitment | Actual path → discriminating evidence | Result / remaining boundary |
|---|---|---|
| `git_execution`, dependency direction, information hiding | API advertisement/upload/receive delegate to native SCM; owned service/ref/status values; API-routes drops git2 dependency; current source/public-surface/manifest guards. | No route process builder or public raw process/env/backend escape hatch; fifteen architecture checks pass. Host/guest/other CLI/provider closure remains open. |
| `git_ref_updates`, CAS/ref scope, receipt admission | SCM checks actual versus admitted command vector and validates complete refs/OID widths; native Git retains exact CAS and atomic/per-ref behavior; API retains authorization, immutable replay and receipt admission. | Native bare/worktree scope-mismatch, stale atomic set and per-ref partial acceptance pass. Eight API push-policy and seven flush tests pass. Full principal/concurrent receipt race matrix remains unverified. |
| `git_native_runner`, `g5.runtime.drain` | Concurrent input/output with bounded diagnostic stderr and binary protocol output; upload reader owns cancellation and waits for exit before successful EOF; independent receive owner retains SQL admission. | Four protocol tests plus shared descendant/cancellation checks pass on macOS; all 36 native-only tests pass on Linux. Public router awaiter cancellation proves a competing workspace lock waits, then actual refs/queue/events complete. Live server TCP disconnect and non-Unix process cleanup remain untested. |
| `git_outcomes`, accepted-ref signals and PR continuation | Captured `ok` survives process exit/broken stdin; route records continuation failures without dropping known effects or independently accepted work. Failed admission release skips independent SQL, retaining in-memory hints and uncertain/partial recovery details. | Public fault parent executes six cases: confirmed, rejected sibling, unknown report, post-acceptance process failure, cancellation, injected publication queue failure. Actual ref/queue/event assertions pass; publication failure returns Partial with confirmed branch and Update PR guidance. Admission-commit failure, combined process+publication failure, multiple accepted branch siblings and reviewer-copy notifications are source-inspected boundaries, not separate runtime evidence. Existing automatic PR-follow-push test passes. |
| `git_locations`, compatibility, credential isolation | Configured API binary, fixed API environment/TLS CA inputs, explicit storage-selected Git directory, native non-FF/updateInstead policy. | Native real bare/worktree push and >512 KiB binary fetch, >64 KiB advertisement and noisy diagnostic controls pass. Linked-worktree serving and deployed custom-CA contexts are untested; prior observation-specific linked-worktree evidence remains valid. |
| `git_retry` | One admitted task; no new schema/tracking/queue/replay; existing independent publication queue remains with its owner. | Fault wrapper confirms one receive per request, including unknown and partial failures. Explicit recovery guidance preserves human/agent retry decisions. |
| `git_guard`, guard calibration | Git route/protocol owner inventories, no direct backend/process bypass, real publication/native effect required after actual auth/admission calls; syntax inspection excludes comments/string decoys and compares named argument expressions. | Fifteen architecture and eleven route-auth tests pass, including missing/commented guards/effects, distinct method names, actual helper rename, multiline/trailing-comma controls. These are structural checks, not complete Rust type/macro or control-flow proof. |
| Shared transport regression | Full macOS SCM suite followed by corrected incomplete-CONNECT fixture and strict malformed TLS control. | Initial full run 201/202 passed; all four TLS/native transport tests pass after fixture-only repair. The fixture still rejects nonempty non-TLS input and requires expected authenticated/redirect observations; no production network policy changed. |

Verification reports distinguish full suites from focused reruns: public fault cases were
rerun after final calibration; shared runner source was unchanged by review repairs, so
Linux/native evidence is retained. Documentation and metadata now identify SCM protocol
ownership and remove the obsolete route Git-directory helper/stale-ordering gap. The
comparison-base ontology diff is refreshed with those scoped assessments. Review used
three parallel independent full-scope contexts, main consolidation/repair and two bounded
reviewer rechecks; no comparative cost/benefit measurement was attempted.

## CLI mechanics and semantic identity — initial independent consolidation

Three fresh full-scope reviewers assessed the frozen increment from `8a856ad3` with
identical inputs and no implementation history or peer reports. Raw reports are
[1](reviews/git-abstraction/cli-boundary-independent-1.md),
[2](reviews/git-abstraction/cli-boundary-independent-2.md), and
[3](reviews/git-abstraction/cli-boundary-independent-3.md). Snapshot:
`/tmp/g5-cli-boundary-review/{tracked.patch,snapshot.tar,files.txt}`. Main records this
consolidation before repairs; every distinct obligation remains below.

| Finding | Initial evidence / disposition | Repair and discriminating check |
|---|---|---|
| C1: new CLI consumers lack process signal cleanup | All reviewers and main trace Doctor/Spec/Job to the unwrapped process entry. Reviewers 1/2 demonstrate detached SCM children surviving parent-group signals. Actual complete command entry was not exercised by their probes. | Include each new command family in the existing cancellation owner; exercise real CLI dispatch selection and SIGINT/SIGTERM, asserting child cleanup before exit. |
| C2: default-branch observation narrows transport | All reviewers demonstrate unauthenticated `git://` rejected before Add's unchanged clone; an explicit branch bypasses the new refusal. | Admit the native unauthenticated transport while retaining credential/destination restrictions; exercise a local Git daemon and the Add path without branch override. |
| C3: relative mirror destination resolves against process cwd | Reviewer 2 reproduces old source-relative push succeeding and new canonicalization failing; a process-relative decoy can instead receive the push. Minority finding accepted on concrete location evidence. | Resolve against source repository and keep push/copy destination consistent in owner planning; test with a distinct cwd and a decoy repository. |
| C4: test-only cfg misclassification | Reviewer 2 and main run the actual expanded guard: it fails on `cfg(all(test, unix))` fixture imports. | Recognize cfg predicates implying test, preserving production `any(test, unix)` checks and required-target validation. |
| C5: configured program bypass | All reviewers execute the checker with variables/constants/configured paths; reviewer 3 also demonstrates raw-string spelling. New checks accept these. | Guard actual process-construction ownership and declare narrow non-Git tool cases; calibrate aliases, configured/constant/raw strings, harmless tools and missing targets. |
| C6: mirror rewrite fixture path alias | All reviewers/main reproduce macOS `/var` or `/tmp` versus canonical `/private/...` mismatch; canonical TMPDIR passes. This is a fixture defect, not demonstrated redirect leakage. | Configure rewrite against the actual canonical destination while retaining the symlink-root case and unchanged-ref assertion. |
| C7: new remote-configuration test compile error | Main focused SCM build finds `Remote::url` returns Result in this version; the new assertion expects Option. | Correct the test assertion to inspect successful URL and run it, retaining pushurl/user/ref/discovery checks. |

Initial main evidence: 244 CLI tests and 57 semantic engine tests pass; native-only suite
37/38 passes with C6; architecture guard 15/16 passes with C4; focused libgit2 configuration
test does not compile due C7. Reviewers' source/runtime limits and isolated probe paths
remain in their reports. No governing meaning change is needed: the repairs enforce
existing native compatibility, location, cleanup and guard commitments. No retry state
or replay is added. Host/guest/provider migration remains separate unfinished work.

### CLI repair and final incremental assessment

Main repaired C1–C7. Reviewer 3's preserved later probe additionally confirms C3's wrong
repository effect, not only failure; the source-relative repair covers both. Reviewer 2's
bounded recheck closes C3/C4 and the original C5 examples, then identifies alias-chain and
raw-string calibration cases. Main repaired those; the final bounded recheck closes them
with the actual checker. Initial reports and later assessments remain intact.

| Commitment | Actual path → discriminating evidence | Result / boundary |
|---|---|---|
| `git_execution`, backend isolation, minimal API | CLI remote configuration/discovery and native probes/mirror delegate to owned SCM methods; CLI git2 is dev-only; semantic author probe removed and constructor accepts email | CLI compilation and 246 tests pass. Configuration regression checks create/update while preserving pushurl, identity and refs; nested checkout/bare discovery passes. Host/guest/provider closure remains open. |
| `git_native_runner`, CLI signal ownership | Actual parsed command execution selects the existing interrupt owner for Doctor/Spec/Job/Push; callback injection starts real SCM version or push operations with a child and delayed descendant | Eight SIGINT/SIGTERM cases prove 130/143 exit after child cleanup and no delayed descendant effect. This exercises the production parser/lifetime selector and runner, not a complete real Docker job or every Doctor/Spec workflow. |
| Compatibility and credentials | Real local Git daemon with non-main symbolic HEAD; actual Add without branch override, then fresh and existing workspace materialization; shared destination/credential controls retained | Correct topic branch/content and unchanged isolated user/system Git configs pass. Existing native file/HTTP/SSH-helper/relay regressions pass. Full SSH authentication and deployed guest CLI execution remain unexecuted. |
| `git_locations`, `git_ref_updates`, mirror outcome | One fast-forward branch push to the owner-selected local mirror; source-relative resolution with separate process cwd and a decoy; owner planning uses the same absolute path for push/copy | Actual intended ref advances, decoy stays empty, other branches/tags stay absent, source config unchanged and divergence is Refused without moving destination. Mirror rewrite is refused with the canonical match prefix under normal macOS tempfile aliases. Docker copy execution is not separately exercised. |
| Conservative spec observations | Actual CLI spec tests and native checkout/history fixture | Unborn, new/tracked nested and literal wildcard names, ignored files, detached HEAD, corrupt refs and invalid outside locations pass; errors retain conservative fallback. Unusual path encodings remain untested. |
| Semantic provenance and dependency direction | Explicit local email input, existing actor/executor validation, first-line trimming and per-instance capture | All 57 filesystem semantic-engine tests pass, including distinct engines, changed input and typed unknown. No production caller of the deleted implicit author probe exists in this checkout. |
| `git_outcomes`, `git_retry` | Shared typed read errors and mirror confirmation/refusal/uncertainty; CLI warning policy retains detail | No new persistence, replay, retry loop or worker. Existing workflow owners remain in place. Shared native outcome/credential/cancellation tests pass. |
| `git_guard`, fixture/calibration requirements | Full CLI/semantic backend scan, dev-only manifests, required consumer/query methods, mixed-tool process construction ownership, recursive aliases and test-only cfg classification | All 17 exact-source checks pass. Configured/constants/aliases/raw Git examples reject; ordinary/raw Docker and helper renames pass; `all(test,unix)` excludes fixtures while `any(test,unix)` remains checked. Missing targets fail. Structural analysis does not claim arbitrary macro/type/dataflow proof. |
| Native platform regression | Final current source, no-default native feature | All 39 native tests pass on macOS and isolated Linux. Linux uses SCM/contracts-only source and a local image with networking disabled, not the full product build. Non-Unix descendant behavior remains unverified. |

Intermediate repairs/checks are not hidden: the first repaired macOS run had three
short-deadline fixture timeouts during concurrent compilation, and the first expanded CLI
signal fixture missed its startup deadline. A subsequent complete native run without
compilation and the final complete CLI run pass; no production timeout was enlarged.
The new remote-configuration libgit2 test passes after C7. Semantic code was unchanged
by review repairs, so its 57-test evidence is retained. Main owns the final conformance
assessment; three initial independent reviews and two bounded reviewer rechecks were used,
without a measured cost/benefit comparison. Scoped metadata and the pinned ontology diff
are updated; this is completion of the CLI increment, not the overall design.

## Host cache, restore and configuration — initial independent consolidation

Three fresh full-scope reviewers assessed the frozen increment from `d3869bca`
without implementation history or peer findings. Raw reports are
[1](reviews/git-abstraction/host-native-independent-1.md),
[2](reviews/git-abstraction/host-native-independent-2.md), and
[3](reviews/git-abstraction/host-native-independent-3.md). Main saves this consolidation
before repairs. Snapshot: `/tmp/g5-host-native-review/{tracked.patch,snapshot.tar,files.txt}`;
tracked patch SHA-256 `c46656d32b8b8b831fef6a5a05bf146450383af8f6d94fcdda43b415fd6fe626`.

| Finding | Initial evidence / disposition | Repair and discriminating check |
|---|---|---|
| H1 P2: equivalent cache URL spellings refused | All reviewers reproduce public `RepoCache::ensure_warm` failure when a trailing-slash variant reuses the same cache key. | Preserve existing cache identity while binding authentication to the actual admitted origin; both cold/warm spelling orders and a different-origin/rewrite refusal control. |
| H2 P2: host wrappers reintroduce credentials | All reviewers reproduce a sanitized SCM refusal wrapped with the raw credential-bearing URL in public Display/Debug. | Preserve typed sanitized errors, remove raw URL reporting on admission refusal, retain the existing narrowly scoped read-403 recovery. Check encoded userinfo/query and selected-token canaries. |
| H3 P2: sibling-file native bypass | Reviewer 3 executes the checker: new files in each migrated host/runtime crate accept literal/configured Git constructors. Minority finding accepted on concrete evidence. | Cover migrated crate boundaries with narrow existing non-Git operation exceptions; calibrate new files, aliases/configured programs and legitimate tool controls. |
| H4: repository anchoring | Main self-review: Host uses `-C` alone and can discover a parent when a requested child is damaged/missing. | Explicit Git-directory anchoring; damaged child must leave parent refs/config untouched. |
| H5: restore OID format checked after effects | Main self-review: full-width syntax accepts SHA-256 IDs for a SHA-1 repository until after fetch. | Check repository-specific width before fetch; mismatched input refuses with no tracking changes. |
| H6: native environment and path fixture | Main notes false/no/off NOSYSTEM values were misread; reviewer 3 flags inherited XDG/proxy/SSH settings as an unverified compatibility boundary. macOS Git rejects the non-UTF-8 filesystem fixture independently of this change. | Preserve explicit supported config selection, inspect actual deployment expectations; retain real Linux path coverage and test argument bytes on Unix without assuming macOS filesystem support. |

Initial main evidence: foundation workspace 35, host workspace 14, runtime configuration
4, Linux native 42 and exact-source guards 18 pass. macOS native 41 pass and the one
non-UTF-8 filesystem fixture fails with the same `Illegal byte sequence` as direct Git.
The host normal dependency graph contains no git2/libgit2-sys. Reviewer public restore
probes additionally exercise attached foreign/detached HEAD, older mapped versus newer
tracking OIDs, large binary unstaged content, sequencer cleanup, optional-parent failure
and authoritative missing receipt despite cached objects. Individual reports preserve
their sandbox/build limits. Full VM boot, deployment transport/configuration combinations
and non-Unix descendant cleanup remain unexecuted.

No governing meaning change is required: these repairs enforce `git_locations`,
`git_credentials`, `git_outcomes`, `git_native_runner`, `git_guard`, compatibility and
existing restore proof. No new retry state, queue or replay is introduced. Guest/provider
migration remains unfinished work after this increment.

### Host repair and incremental assessment

Main repaired H1–H6 under the existing contracts. The cache compares its historical
trimmed/single-trailing-slash identity with the actual expanded origin, validates that
transport, and scopes authentication to it. Changed origins and rewrites still refuse.
Host wrappers preserve `NativeGitError`; obsolete raw Git error variants are removed.
Rejected URL admission omits the raw input, and native diagnostics scrub arbitrary URL
userinfo/query material as well as selected credentials. Host invocations explicitly
anchor Git directories; restore checks all supplied OID widths before any fetch.

The host context explicitly preserves selected home/XDG/system/global config, SSH
agent/tools, HTTP proxy, CA and loader settings. Proxy/tool values are omitted from
Debug. Native test coverage separates Linux non-UTF-8 filesystem support from Unix
argument-byte preservation; macOS's backend restriction is not represented as success.
The expanded source scan also exposed a later guard defect: recursive module aliases
could expand indefinitely. Resolution now tracks alias roots, with a discriminating
recursive-prefix fixture. Existing generic process supervision and feature-gated host
fixture constructors have narrow operation exceptions; new sibling files remain guarded.

| Commitment | Actual path → discriminating evidence | Result / boundary |
|---|---|---|
| `git_execution`, native dependency direction and compatibility | Foundation cache/checkout, host restore and runtime trust-file functions delegate to SCM; workspace path routing is available without the Git feature. | Host normal graph excludes git2/libgit2-sys. Foundation 39 and host restore 14 tests pass; the unchanged runtime trust-file suite's 4 tests and path-routing probes remain valid. Guest/provider closure is unfinished. |
| `git_locations`, `git_ref_updates` | Public cold/warm cache calls in both URL spelling orders; changed origin/rewrite controls; bare branch refresh/prune, exact checkout and damaged-child control. | Equivalent cache identity succeeds; different destinations refuse; older exact commits stay selected and damaged child leaves parent config/refs unchanged. SHA-1 restore supplied SHA-256 branch/receipt OIDs refuses with no FETCH_HEAD/tracking refs. |
| `git_native_runner`, custody and host context | Public dropped `ensure_warm` after real blocked TLS transport starts; both cache locks and process socket observed; explicit context fake transport; shared native cleanup matrix. | Both locks outlive child cleanup. Selected config/proxy/SSH inputs reach the invocation; arbitrary ambient Git controls remain excluded. 44 native tests pass on macOS and 45 on isolated Linux. Full deployed proxy/SSH/CA authentication combinations and non-Unix cleanup remain unexecuted. |
| `git_credentials`, `git_outcomes` | Public cache refusal rendered via Display, Debug and G5Error; raw/encoded userinfo, query and selected-token canaries; actual credentialed HTTP denial. | Canaries are absent. Errors retain operation/kind/effect and inspect-before-retry guidance. Restore source inspection retains confirmed stage evidence; injected failure at every stage is not runtime-proved. |
| `git_retry` | Actual HTTP fixture returns read 403 then 404; public cache caller and removed stub observed. | Exactly two requests, final typed non-403 error, no incomplete cache. Existing host recovery stays above one-attempt SCM; no new tracking tables, persistence, queues or replay. |
| Restore, cleanup/proof and `runtime.materialization` | Host restore suite plus independent real-Git attached/detached/binary, missing-receipt and hostile retained-control probes. | Exact mapped/head/dirty distinctions, clean index, cleared sequencers, optional-parent failure and missing remote receipt despite cached objects pass. Retained roots are replaced before host Git. Complete VM boot, warm/cold race matrix, eviction and KVM performance remain unexecuted. |
| `git_guard`, enforcement calibration | Full host/runtime crate constructor scan, new sibling literal/configured/alias fixtures, narrow existing cp/cloud-CLI/port-scan/supervisor/test-helper exceptions and required operations. | 21 exact-source tests pass. Test helper gating and bounded recursive aliases are checked. Guard remains structural, without arbitrary macro/type/dataflow proof. |

Initial final-source verification had nine short-deadline native fixture failures while
concurrent checks ran; an isolated rerun and the subsequent complete macOS native suite
pass without increasing production or fixture deadlines. The Linux suite passes with
networking disabled in the isolated SCM/contracts workspace. Reviewer 3 independently closes H1–H3 with actual public cache/error probes and
22 checks (the 21 repository guards plus an isolated extra alias fixture). Main owns
the coherent repair and final assessment; three independent discoveries and that
disclosed bounded recheck are recorded without an invented cost/benefit comparison. This closes the host
increment only; implementation continues through guest and provider owners.

## Guest member operations and flush — initial independent consolidation

Three fresh full-scope reviewers examined the frozen increment against `1fd78123`
without implementation history or peer findings. Raw reports are
[1](reviews/git-abstraction/guest-native-independent-1.md),
[2](reviews/git-abstraction/guest-native-independent-2.md), and
[3](reviews/git-abstraction/guest-native-independent-3.md). Main saves this consolidation
before repairs. Snapshot: `/tmp/g5-guest-native-review/{tracked.patch,snapshot.tar,files.txt}`.
Each finding is accepted on its evidence, including the distinct minority findings.

| Finding | Initial evidence | Required repair and discriminating check |
|---|---|---|
| G1 P1: unit-test compilation | Reviewers 2/3 confirm deleted `set_mode` remains called by a Git credential fixture. | Test-local permissions; build/run current dispatcher unit tests. Cached binaries do not close this finding. |
| G2 P2: missing HEAD panics Update | Reviewer 3 reproduces Git preconditions: symbolic mapped branch and tracking exist but HEAD commit does not; adapter assumes an OID and panics before module completion. | Association refuses unavailable HEAD; remove unchecked adapter expectations. Real Update RPC must terminate and release admission for a subsequent operation. |
| G3 P2: branch interpreted as option | Reviewer 3's public SCM `switch_back("-f")` discards dirty bytes and returns success. Current dispatcher blocks that name, but the exposed SCM boundary does not. | SCM validates short branch semantics and disambiguates checkout. Public operation refuses unsafe names preserving HEAD/index/worktree; ordinary existing/new local branches remain supported. |
| G4 P2: configured secret in flush errors | Reviewer 1's real dispatcher filter emits the fixture workspace JWT; the new error log/RPC contains it. SCM does not know this owner-held credential. | Owner scrubs known values before logging/returning SCM errors, retaining operation/effect detail. Actual flush RPC/log canary tests include truncated output. No credential is passed to Git. |

Initial verification: main's macOS native suite 63 and isolated Linux native suite 64
pass (two unrelated tests filtered); reviewer 1's complete native-only suite has 65
passes. Reviewer 3 records timing-sensitive upload-stream failures under concurrent
suite execution and a passing isolated rerun; do not erase those observations. Main's
pre-helper-removal 103 dispatcher unit passes do not establish current compilation.
Current-source ownership guards: 22 pass. Real dispatcher changed-member, selected
checkpoint, conflict/parent update, multiple repository and dirty/foreign-head flush
replay paths pass. Full root Linux dispatcher fork test proves module UID 1000 can
write objects after successful and failed capture. Main's isolated public checkpoint
dropped-awaiter probe proves child/index custody cleanup, unchanged HEAD and a
subsequent successful native add. Logs are recorded with the final assessment below.

Uncovered boundaries remain: full bundled VM boot, non-Unix descendants, every
ref/index compensation interruption, the complete partial-leaf/lost-reply guest matrix,
and adversarial native-user concurrency. No contract amendment is needed for G1–G4;
repairs enforce existing outcomes, credentials, runtime custody and ref-scope obligations.
No retry state or replay is added. Provider/backend closure is remaining implementation,
not part of the guest-completion claim.

### Guest repair and incremental assessment

Main repaired G1–G4 under the existing contracts. Test fixture permissions are local;
association rejects unavailable HEAD and Update no longer assumes JSON OIDs. SCM
rejects option/HEAD shorthand and disambiguates checkout while preserving existing
and newly materialized local branches. Dispatcher errors scrub the owner's configured
secrets, including incomplete capture prefixes, before logging or serialization.
Native stderr keeps the shared 64 KiB diagnostic bound even when inventories allow
larger stdout. This also permits the real RPC truncation counterexample to terminate
with useful bounded detail. No token is supplied to the native process for redaction.

| Commitment | Actual path → discriminating evidence | Result / boundary |
|---|---|---|
| `git_execution`, `git_native_runner`, `git_locations`, dependency direction and minimal API | SCM member operations/configuration; dispatcher runtime context and policy composition; explicit Git-directory selection; normal dependency inspection and guards | Guest direct Git assembly is removed; no public argv/env-map/backend escape hatch. Dispatcher normal graph excludes libgit2. Current dispatcher 103 unit tests and 22 source guards pass. Provider/backend remainder is not claimed complete. |
| `git_ref_updates`, CAS/ref-scope and compatibility | Real member push, stale/unreconciled refusal, per-member parent update, selected checkpoints and SCM switch-back | Existing/new local branches preserve dirty content; unsafe option/shorthand names refuse without changing HEAD/index/worktree. Existing dispatcher routing/update/checkpoint suites pass. Full native-user concurrent ref/index matrix remains unexecuted. |
| `git_outcomes`, `git_retry`, runtime admission | Real missing-HEAD Update RPC followed by a new successful commit and admission close; typed completed-stage/effect errors; existing fixed-OID flush retry | G2 closes at the real consumer: terminal refusal and later write succeed. No new persistence, tracking, worker, queue or automatic replay. Existing flush state/recovery remains its prior owner. |
| `git_native_runner`, runtime drain/custody | Public checkpoint held at commit-tree, dropped awaiter, `wait_idle`, lock/scratch inspection and later native add; real noninterrupting fork; root Linux dispatcher fork | Dropped operation preserves HEAD and cleans index custody before idle; root Linux module UID 1000 can write objects after successful and failed capture. Full VM and every inter-stage cancellation remain unexecuted. |
| selected checkpoint/isolation/retry | SCM private selected/whole index algorithm and real dispatcher expected-version/indexed-attributes fixtures | Raw version mismatch refuses; unrelated staging and literal names survive; whole/no-op behavior passes. Crash exactly between ref CAS and index installation is not separately injected. |
| durability working-state, flush/content/identity/receipt/limits/exclusions | Typed snapshot/pack/ref-pair operations under existing dispatcher vector/preflight/receipt owner | Real dirty/foreign-HEAD shutdown flush and fixed-identity replay pass; code/docs noninterrupting fork passes. Root no-follow ownership tests execute on Linux. Entire partial-leaf, lost-reply, size/cancellation and full VM matrices remain unexecuted. |
| credentials, reserved environment and redaction | Exact in-memory relay credential selection, isolated native environment/config, real failing-filter flush RPC/log tests | G4 closes for full token and actual 64 KiB truncation inside the token: RPC and logs contain neither whole credential nor retained prefix. Operation/effect detail remains. Existing credential/helper-free environment tests pass. |
| `git_guard`, calibrated enforcement | Full dispatcher dependency/constructor scan, narrow generic non-Git tool exceptions, field-attribute parser limitation and required operations | 22 guards pass, including bypass in the same function as the tolerated pattern attribute and harmless equivalent. Structural checks do not claim arbitrary Rust macro/type/dataflow proof. |

Final check receipts: `/tmp/g5-guest-dispatch-repaired.log` (103),
`/tmp/g5-guest-missing-head.log` (real RPC), `/tmp/g5-guest-redaction-repaired.log`
(real RPC/log full and truncated canaries), and `/tmp/g5-guest-guard-repaired.log` (22).
The native suite passed 66 macOS / 67 Linux cases; its new switch fixture alone failed
because it omitted origin's fetch mapping required by Git tracking. Supplying the real
origin configuration fixes the fixture without a production change, and that case
passes on both platforms (`/tmp/g5-guest-switch{,-linux}-repaired.log`). Thus all 67
macOS / 68 Linux cases have passing final applicable evidence. The complete-run logs
retain the initial fixture failures rather than representing an unexecuted second full
run as green. The dropped-checkpoint test is included in both suites. Earlier root
real-dispatcher evidence is `/tmp/g5-guest-dispatch-linux.log`; other unchanged member
and flush regression logs are `/tmp/g5-guest-e2e-{changes,more,multi-final,flush}.log`.

Three independent discoveries and main's coherent repair/conformance assessment were
used; no measured cost/benefit claim is made. Scoped footers and the actual pinned
ontology diff are refreshed. This closes the guest increment; work continues on provider
protocol ownership and final backend closure.

## Backend dependency closure — initial independent consolidation

Three fresh full-scope reviewers examined the frozen 18-file increment against
`59c81a76`, using the original request, pinned design/governance and current consumers
without author history or peer findings. Initial reports were saved before sharing:
[1](reviews/git-abstraction/backend-closure-independent-1.md),
[2](reviews/git-abstraction/backend-closure-independent-2.md),
[3](reviews/git-abstraction/backend-closure-independent-3.md).
Snapshot: `/tmp/g5-backend-closure-review/{files.txt,tracked.patch,snapshot.tar}`.
This consolidation is saved before repairs.

| Finding | Initial evidence | Repair / required check |
|---|---|---|
| B1 P2: wrong PATH candidate | All three reviewers reproduce an owner-controlled first Git at mode 0610 or 0641 being selected despite the API identity lacking execute permission, while old `which` chooses the usable second entry. | Central lookup checks effective execution access, including platform access controls; public lookup and API override/search/fallback checks preserve valid selection without a subprocess. |
| B2 P3: harmless identifier rejected | Reviewer 2's exact checker rejects a local parameter named `git2`/`libgit2_sys`; actual backend paths also reject. | Recognize dependency/import/path references, retaining manifest alias checks and a harmless exact-name local control. |
| B3: native guard coverage still incomplete | Reviewer 2 demonstrates literal native Git in a newly added backend crate escapes existing named native scopes. This is a retained design gap, not a newly introduced live bypass. | Extend final native ownership enforcement to new backend crates, calibrating actual legitimate non-Git constructors as specific owner operations. Record any unresolved coverage rather than conflating dependency closure with complete execution enforcement. |

Initial main checks: all 232 SCM tests pass (`/tmp/g5-backend-scm-tests.log`), all API
and g5d test targets compile with g5d's no-libkrun setting
(`/tmp/g5-backend-all-consumers.log`), and 22 ownership guards pass. Independent
reviewers preserve their own focused checks/probe limitations. The explicit backend
error mappings produce the same owned error values at the same operation sites; no
ref effects, retry state or recovery owner changed. Only SCM retains a production
direct libgit2 dependency. The CLI intentionally still transitively uses SCM's backend;
native host/dispatcher and all-feature contracts graphs do not link it. Provider
migration remains unfinished. No governing meaning amendment is required for B1–B3.

### Backend repair and incremental assessment

Main repaired B1–B3. Effective-identity executable access replaces permission-bit
heuristics, preserving the configured API override and fallback. Backend-reference
checks recognize Rust import/path contexts, allowing harmless local names. Native
constructor checks now traverse every backend Cargo package; legitimate agent tool,
VM/network helper, user execution and fixture constructors have specific exceptions.
Reviewer 2 independently closed B1/B2 and confirmed the native coverage repair, then
found a later P3 calibration defect in the new fixture gate: text matching admitted
commented/missing declarations. Main replaced it with the existing AST approach and
added commented, missing, unguarded and harmless-comment controls. All 24 guards pass.

A later API-core unit-test build exposed a fixture still using the removed backend
conversion. It now constructs the same owned error; its real remote-auth classification
test passes. The earlier API/g5d aggregator test-target build did not compile this
API-core unit fixture; that evidence boundary is retained rather than hidden.

| Commitment | Actual path → counterexample/check | Result / boundary |
|---|---|---|
| `git_execution`, dependency direction, information hiding | Contracts feature/conversion removal; private SCM mapper; all-package manifest scan; normal/build trees for all-feature contracts, host and dispatcher | Only SCM retains a production direct backend dependency. Native graphs exclude libgit2. CLI remains intentionally transitively linked through SCM. No backend escape is added. |
| `git_native_runner`, compatibility | Public SCM lookup with unusable owner-controlled 0610/0641 candidates; API selection with explicit override, absent PATH and fallback; actual selected child runs | Public lookup and API startup helper tests pass on nonroot macOS. No complete server startup or Windows access-control check is claimed. |
| `git_outcomes`, ref/read compatibility | Private mapper gives identical owned errors at prior implicit-conversion sites; original 232 SCM cases and current remote integration auth classifier | SCM suite passes; changed fixture and API-core tests compile, focused classifier passes. Conversion does not alter ref effects or operation order. |
| `git_guard`, enforcement calibration | 24 current-source guards; new-crate literal/configured/alias spawns, backend wildcard/alias imports, exact-name local controls; AST fixture declaration checks | All pass. Specific non-Git constructor exceptions are inspected. Structural checking does not establish arbitrary Rust type/macro/dataflow proof. Provider enforcement remains migration work. |
| `git_retry`, minimal change | Complete dependency/mapping/lookup/guard diff | No new persistence, retry state, replay, queue or owner-policy change. Parked branch confidentiality is not reopened. |

Receipts: `/tmp/g5-backend-lookup-repaired.log`,
`/tmp/g5-backend-api-lookup-repaired.log`,
`/tmp/g5-backend-auth-classification-repaired.log`, and
`/tmp/g5-backend-guard-repaired.log`. Initial 232-SCM and API/g5d test-target receipts
remain valid for unchanged operation logic. Three independent discoveries, one bounded
independent repair check and main's coherent repairs were used; no cost/benefit metric
is inferred. Scoped metadata and the actual pinned ontology diff are refreshed. This
closes the backend increment; provider protocol ownership remains in progress.

## Provider App and issue boundary — initial independent consolidation

Three fresh reviewers inspected the complete frozen 18-file increment against
`93561a76`, with the original request and governing design but without author history
or peer findings. Their reports were saved before sharing:
[1](reviews/git-abstraction/provider-app-independent-1.md),
[2](reviews/git-abstraction/provider-app-independent-2.md),
[3](reviews/git-abstraction/provider-app-independent-3.md).
Snapshot: `/tmp/g5-provider-app-review/{files.txt,tracked.patch,snapshot.tar}`.
This consolidation precedes production repairs.

| Finding | Independent evidence | Required repair/check |
|---|---|---|
| P1 P2: rate limit becomes access denial | All three public-client probes show repository access 403 with rate-limit evidence becomes `AccessDenied`; the credential owner consequently recommends permission repair. | Preserve rate evidence before ordinary 401/403/404 access classification; test header/body rate reports and ordinary denied access. |
| P2 P2: known 401 loses reconnect signal | Reviewer 1 serves 401 headers followed by a truncated body; the adapter returns generic failure despite retained status evidence. | Apply endpoint-specific status classification to transport failures as well as complete responses; preserve reconnect for observed 401 and generic failure for absent status. |
| P3 P2: returned issue URLs cross instances | Reviewer 1's configured-instance detail response accepts a foreign canonical issue URL and foreign repository URL. | Validate returned web/API URLs against independently selected endpoints, retaining valid prefixes, fragments and renamed repositories; check detail/search/comment/timeline outputs. |
| P4 P2: redaction damages retry evidence | Reviewer 3's token beginning `0` changes Retry-After `60` to `6[REDACTED]`; request IDs can also be damaged. | Complete provider response fields use full-value secret redaction before diagnostic truncation. Test unchanged benign hints and echoed-secret suppression; native truncated-capture policy remains separate. |

All reviewers verified the snapshot and 26 exact-source guards; provider fixtures
pass, and reviewer 1 compiled exact-source counterexample probes. Cached API-core
checks predate final transport edits and are corroboration only. Main's current
API-core test-target check passes; owner runtime tests must be rebuilt after repair.
No governing meaning change is needed: these repairs enforce existing evidence,
selected-host and reconnect commitments. Account/login/PAT, PR lifecycle/read/checks,
and platform-report protocol migration remain explicit later work. No retry storage
or automatic replay is introduced; branch confidentiality stays parked.

### App/issue repairs and incremental assessment

Main repaired P1–P4 together. One endpoint-specific classification handles complete
responses and observed statuses on body failure; rate limits retain evidence before
ordinary access decisions. Provider diagnostics redact complete secret values before
truncation, without applying native incomplete-capture rules to intact headers.
Issue detail/search/comment/timeline boundaries check selected API/web ownership;
renamed repositories, separate API/web bases, configured prefixes and issue fragments
remain supported. Invalid required detail/comment links fail clearly; optional timeline
and search entries are omitted. No request follows returned links.

| Commitment | Actual path → discriminating check | Assessment / remaining limit |
|---|---|---|
| `scm_provider_client`, credential scopes and information hiding | App and issue scoped clients; selected grants; unsupported/write-capability refusal; private transport and public compile-fail examples | Implemented for this increment. JWT/cache/binding/actor/domain policy stays above SCM. Candidate/account, live login, PR and report migration remains. |
| `github_transport`, `git_outcomes`, current-user PR authorization | Public access operation with ordinary denied/missing/revoked/matching/changed repositories; header/body 403 rate reports; truncated 401; complete benign Retry-After/request ID and echoed token | P1/P2/P4 closed. Provider fixtures retain exact evidence and reconnect classification. Rebuilt API App/binding owner suite passes 22 tests; full public PR/SQL/RLS flow remains unexecuted. |
| `github_transport`, issue presentation | Public detail and search with separate API/web prefix, renamed repository, valid fragment and foreign origin/port/prefix; foreign comment link | P3 closed at typed operation boundary. Existing issue parsing/filtering/timeline fixtures pass. Full issue-route/database publication and live Enterprise behavior are not claimed. |
| `git_retry`, recovery ownership | One-request expectations on ambiguous and accepted-but-unreadable writes; existing API owner adapters | No tracking, retry storage, replay, queue or worker. Existing outbox/comment recovery is unchanged. |
| `git_guard`, dependencies/minimal API | 26 exact-source guards, seven doctests and provider-only graph without native/libgit2 | Partial migrated-file ratchet verified; complete provider enforcement remains later work. Source guards are not arbitrary Rust type/dataflow proof. |

Final provider suite: 28 passed (`/tmp/g5-provider-app-repaired.log`); current API owner
suite: 22 passed (`/tmp/g5-provider-app-owner-repaired.log`); source guards: 26 passed
(`/tmp/g5-provider-app-guard-repaired.log`). Initial regression preparation had test-only
compilation mistakes and an old fixture pairing public-GitHub links with a loopback web
base; the fixture now declares its actual independent web/API bases. No production
behavior was relaxed to pass it. Earlier seven doctests and providers-only dependency
inspection remain applicable to unchanged public visibility/dependency structure.
Scoped metadata and actual pinned ontology diff are updated. Three independent
reviews plus main's coherent repair/conformance assessment close this increment.

## Final provider migration — initial independent consolidation

Three fresh full-scope reviewers inspected the frozen 30-file delta against `2433688c`
without implementation history or peer findings. Each saved discovery before sharing:
[1](reviews/git-abstraction/provider-final-independent-1.md),
[2](reviews/git-abstraction/provider-final-independent-2.md),
[3](reviews/git-abstraction/provider-final-independent-3.md).
Snapshot: `/tmp/g5-provider-final-review/snapshot.tar`, SHA-256
`fd795bc7cef26fd21401dc667d3e5b4e9cb9ca05a2edbcfd8119b7cd7ea873b6`.
This consolidation is saved before production repairs.

| Finding | Evidence and disposition | Coherent repair/check |
|---|---|---|
| F1 P1: configured check secret crosses diagnostic cap | Reviewer 1 reproduced a configured secret prefix leaking from a public checks operation. Accepted. | Supply the full owner redaction set to transport before message/header capping; cover primary and annotation failures. |
| F2 P2: semantic merge refusal bypasses error boundary | Reviewer 2 reproduced an HTTP-200 `merged=false` response returning the selected token, exposed by the owner. Accepted. | Convert negative protocol outcome into a redacted typed refusal with response evidence; verify owner projection and successful merge control. |
| F3 P2: report issue identity insufficient | Reviewer 2 reproduced a bound report client accepting another repository/resource/number on the same host. Accepted. | Bind returned issue links to configured repository, resource and number, retaining prefixes and accepted-write effect; cover create and recovery lookup. |
| F4 P2: owner truncates outcome guidance | Reviewer 3 reproduced a long unknown create diagnostic losing uncertainty and inspect-before-retry guidance through the existing 500-character formatter. Accepted. | Put bounded outcome/action/evidence ahead of expendable diagnostic text; verify actual persisted/public owner result with one create attempt. |
| F5 P2: whole-SCM HTTP guard exemption | Reviewers 1 and 3 independently calibrated a raw private SCM client bypass accepted by every relevant guard. Accepted. Reviewer 3 also identifies missing provider-dispatch enforcement. | Limit construction/delivery to shared transport operations, permit typed request assembly, and calibrate sibling/adapter bypasses plus provider-dispatch composition boundaries. |
| F6 P2: configured Enterprise repository identity owner | Reviewer 3 shows the configured selector accepts an instance URL that the reused attached/credential helper rejects as non-public-GitHub. Accepted as an unclosed acceptance obligation. | Resolve repository identity through selected instance endpoints in actual credential/read owners; test attached configured instance and foreign-origin refusal. |
| F7 verification: five inconsistent lifecycle fixtures | All reviewers inspect fresh 45-pass/5-fail results with generated repository identities but hard-coded `acme/api` links. Accepted as fixtures, not permission to weaken validation. | Correct successful fixture URLs/assertions and rerun all 50 owner scenarios. |

Fresh frozen receipts: 32 provider tests, 28 guards, 51 core auth units, 46 auth routes,
13 PR/check reads and nine report recovery tests pass; API test targets compile and
seven doctests pass. The five lifecycle failures remain visible until repaired.
Earlier native evidence is retained for unchanged paths. These fixes enforce existing
commitments; no new retry state, replay, owner, or confidentiality scope is added.

### Bounded independent repair check and later discovery

Reviewer 3 rechecked F4/F6 read-only after main's repairs and saved
[the repair assessment](reviews/git-abstraction/provider-final-repair-check-3.md).
The maximal bounded-format probe closes F4's presentation counterexample; the actual
owner test remains the required execution receipt. RC3-1 finds a remaining F6 path:
the new GitHub instance value retained host/port but discarded the configured `/git`
prefix, so personal import/Git token admission accepted sibling and normalized-dot
paths even though App/PR identity selection refused them. Main accepts the finding
before repair: retain the configured prefix in that instance value, reject alternate
and dot/escaped path spellings before credential admission, and verify actual personal
resolver/import owners as well as the App PR path. No new authority is introduced.

Initial owner verification used standalone binaries without the isolated test-service
environment; those runs were interrupted and restarted with the existing local test
PostgreSQL/Redis ports. Receipts retain the interrupted setup separately. Repaired
report recovery passes nine cases. PR/check read tests pass 13 and fail the new
configured-instance test only at its mistaken `available` expectation; the existing
protocol returns `ready`, which is the governed readiness value. Correct that test,
then exercise its remaining foreign-origin denial assertions. Production output is
not changed to satisfy the test.

### Final repairs and conformance assessment

Main repaired the consolidated findings in one implementation sequence. Checks pass
all configured secret values into the private transport before diagnostic/header caps.
Negative merge protocol responses become typed, redacted refusals retaining observed
HTTP/request evidence; the existing merge owner stores and exposes that evidence on
its PR read surface. Report create and marker lookup require the selected repository,
`issues` resource and returned number under the configured web origin/prefix, allowing
repository-name case differences. Confirmed malformed create results remain Partial.
Provider presentation puts outcome/action before bounded hints and diagnostic text,
so the existing PR status/event limit cannot discard uncertainty.

SCM's blanket HTTP exemption is removed. Only named shared-transport operations may
construct/deliver raw HTTP; typed adapters may assemble methods and owned requests.
Whole-backend provider-name checks distinguish protocol selection from named
composition/credential policy, persisted-record validation and public presentation.
New-crate, private SCM sibling and in-adapter bypass controls fail as intended, while
shared transport/private helper controls pass. These are structural checks, not a
Rust type/macro or arbitrary dynamic mediation proof.

Repository instance composition now consistently uses the existing App API/web
configuration, independently of OAuth login and platform-report destinations. A
GitHub instance carries its selected HTTPS origin, port and prefix into personal and
App credential resolution and native/libgit2 injection. The App/PR identity parser
checks attached clone identity under the same web prefix. RC3-1 is closed by main's
repair and reviewer 3's independent 58-assertion public-policy/grant probe, including
sibling/lookalike, raw and escaped dot paths, wrong port and public-origin controls.
The real API import and both personal resolver paths pass the same discriminating
admission cases; actual attached-instance PR/check reads pass and foreign-origin
attachments make no provider request.

Lifecycle fixtures now return the repository actually selected by each scenario.
An initially over-broad fixture repair also changed an App fixture that legitimately
uses `acme/api`; main restored that fixture rather than relaxing production validation.
New evidence tests initially queried `failed` instead of the existing `provider_failed`
event and used a default incoming PR list instead of the explicit source list. Those
test selectors are corrected. A long uncertain create now demonstrably preserves
inspection guidance in `last_error`, the event and the API response, with one create
attempt. The semantic merge-refusal regression verifies the persisted and public PR
error and unchanged target; the merge endpoint keeps its existing aggregate error.

| Commitment | Actual path → discriminating evidence | Final assessment / boundary |
|---|---|---|
| `scm_provider_client`, `scm_credential_scopes`, minimal API and information hiding | Candidate/account/App/repository/connection/login/report grants; API owners delegate wire mechanics; capability-denial and privacy tests; complete source inventory | Migration complete. SQL, identity linking, binding/cache/signing, actor choice, publication and outbox policy remain above SCM. No raw HTTP escape. |
| `github_transport`, `git_credentials`, selected instance | App API/web composition → policy/grant → attached identity and credentials → SCM request; client prefix/URL probes, actual App PR/check reads, import and both personal resolver tests | F6/RC3-1 closed. Independent login/report destinations and exact origin/port/prefix retained. Live SaaS/Enterprise and encrypted SSH handshake remain unexecuted. |
| `git_outcomes`, credentials redaction, PR check details | Shared transport redacts before caps; semantic merge refusal; provider Display → existing 500-character owner status/event → public reads; client and real API regressions | F1/F2/F4 closed. Selected and configured canaries absent; uncertainty, inspection guidance, request IDs and retry hints preserved. No replay. |
| `report_owner`, report identity, recovery | Report grant → create/marker link validation and commit-pinned screenshots → existing outbox; invalid identity controls and nine recovery scenarios | F3 closed. Known accepted write evidence survives invalid response. Existing durable recovery owns subsequent action; no new state/worker. Live installation permission issuance remains unexecuted. |
| `pr_flow`, current-user creation, title and follow-push | Credential owner and existing create/update/refresh/merge state transitions; 51 lifecycle scenarios with real API/DB/Git and mock providers | F7 closed. Identity, actor selection, included members, exact provider pins, manual title recovery, authority and completed-provider evidence preserved. |
| PR read/check authority and revision | Authorized user/current-incarnation routes → attached repository/least privilege → bounded SCM observations; 14 route scenarios | Current incarnation, changed head/base/rerun, pagination, partial annotations, configured secrets, redirects and configured instance/refusal pass; no CI or publication mutation. |
| Login/SCM authority and identity owner | AuthService login/email linking versus candidate/account/connection protocol, PAT/import and personal/App resolver owners; 47 API auth scenarios plus retained 50 core auth and 22 App/binding unit cases | Separation preserved; no invented repository for account operations or login-to-repository credential substitution. External browser/revocation matrix remains unexecuted. |
| `git_retry`, recovery/minimal change | Complete source/schema diff, one-request provider fixtures, uncertain create and existing report recovery | No new migrations, tracking tables/columns, queues, journals, workers or failed-operation replay. Existing independently governed recovery remains unchanged. |
| `git_guard`, dependencies, compatibility and enforcement | 31 exact-source guards; seven privacy doctests; providers-only normal/build graph; API test targets | F5 closed at the specified structural boundary. Only SCM has a production direct libgit2 edge; providers-only has no native/libgit2/SQL dependency. Existing authorized-reader/native/remote guards retained. |
| Earlier native/ref/location/admission/flush/host owners | Prior committed owner evidence plus final combined SCM native/libgit2/provider suite | No final-provider change to ref/flush/host algorithms. Current 268-case SCM suite passes serially. Earlier full VM, non-Unix, every interruption/CAS compensation and partial-leaf/lost-reply matrix limits remain; branch/object confidentiality stays parked. |

Final receipts:

- `/tmp/g5-provider-final-scm-serial.log`: **268 passed** against current source,
  including all 36 provider cases and native/libgit2 cases. Two preceding concurrent
  runs hit native deadline/stream-fixture assertions (267/1 and 265/3); an isolated
  stream check and the entire serial run pass without native production/test changes.
  Those initial failures remain in `scm-all.log` and `scm-current.log`; serial evidence
  does not establish stress/concurrency timing guarantees.
- `/tmp/g5-provider-final-current-auth.log`: **47 passed**;
  `current-pr-reads.log`: **14 passed**; `current-reports.log`: **9 passed**.
- `/tmp/g5-provider-final-current-pr-lifecycle.log`: **50 passed**, with one new test
  failing only its source/target list selector. After that test-only fix,
  `/tmp/g5-provider-final-merge-public-repaired.log`: **1 passed**. The other 50 cases'
  implementation and fixtures are unchanged. This establishes all **51 lifecycle
  scenarios**, not a claim that the earlier combined run was wholly green.
- `/tmp/g5-provider-final-core-auth.log`: **50 passed** and
  `/tmp/g5-provider-final-core-app.log`: **22 passed**, retained for their unchanged
  OAuth/App unit paths; subsequent prefix admission is covered by current API/SCM tests.
- `/tmp/g5-provider-final-current-guard.log`: **31 passed**;
  `current-doctests.log`: **7 passed**; `current-compile.log`: API test targets compile.
  Earlier provider-only dependency-tree receipt remains valid for the unchanged
  feature/dependency structure.

Main's self-review, three fresh full-scope independent discoveries and the bounded
independent repair recheck complete this design's implementation review. Scoped
ontology assessments are updated; raw independent reports and failed setup/test
receipts are preserved. No unexecuted live-provider/VM outcome is promoted to a
verified claim. There is no remaining planned implementation or unresolved finding
in the approved migration.


## Abstraction audit — initial consolidation

The user requests a fresh audit of one-path consolidation, future duplicate-path
prevention and meaningful tests, specifically simultaneous in-platform merge/fork.
Frozen implementation: `b310368d`. Three fresh full-scope independent contexts saved
[review 1](reviews/git-abstraction/abstraction-audit-1.md),
[review 2](reviews/git-abstraction/abstraction-audit-2.md) and
[review 3](reviews/git-abstraction/abstraction-audit-3.md) before sharing. Main's source
trace and isolated guard probes are self-review. No production changes occurred during
discovery. The findings below are saved before the coherent repair.

| Finding | Consolidated assessment | Required repair / discriminator |
|---|---|---|
| AA1: unfenced fork source capture | All three reviewers and main establish the same inherited, material interleaving: fork reads A-old, platform merge lands A-new/B-new, fork reads B-new. Later repository locks and child proofs preserve the mixed manifest. No runtime reproduction yet. | Capture source membership/receipt/head vector under the existing workspace publication fence, release it before slow provider/live work. Real public merge plus checked fork tests must prove complete before/after cuts and replay, including two repositories and overlapping orders. |
| AA2: retained unchecked writer alternatives | Reviewers 1/2 and main identify public unconditional local updates, unleased force push and unused legacy merge/mirror workflows. Required local CLI checkout behavior is valid, but does not justify exposing unchecked authority alternatives. | Remove unused workflow/writer exports; route live local changes through expected-state updates; restrict fixture setters. Guard new API policy bypasses without confusing legitimate preparation/local checkout with server authority. |
| AA3: SCM native scan omission | All reviewers reproduce a new private SCM sibling starting Git while guards pass. Current runner itself is consolidated. | Traverse every SCM source; permit native construction only in the actual runner operation; calibrate new siblings/helpers and legitimate fixtures. |
| AA4: content-reader syntax bypass | All reviewers reproduce ordinary UFCS/whitespace/receiver renaming escaping the literal matcher. | Syntax-aware content-operation ownership checks, scoped legitimate owners and new-file/alias/associated-call controls. |
| AA5: public native process escape | Reviewer 3 calibrates public Command return types passing the surface guard. Accepted despite other reports not listing it. | Extend forbidden exported backend types to native command/process handles, aliases and re-exports with positive owned-wrapper controls. |

Current baseline receipts: 31 source guards, 268 SCM cases (serial), 48 workspace API
cases and 10 branch-landing API cases pass. The initial sandboxed SCM invocation
could not bind local fixture sockets; its 241/27 receipt is retained separately and
is superseded for runnable fixture evidence by the permitted local-network run.
These tests are substantive but lack the composed merge/fork case. CI selects the
guard integration binary; documentation tests alone are not hard-gating in the
existing CI lane. No new retry persistence, replay worker, external-provider or
branch-confidentiality requirement is introduced by these repairs.


### Abstraction audit — runtime reproduction before repair

The deterministic public `/clone` versus public `/merge` regression now reproduces
AA1 against the unchanged admission behavior: fork paused after its first source
member; the real platform merge completed; fork then completed with a mixed persisted
`repo_manifest` and child refs. The test fails at the authoritative manifest assertion,
not a model assertion (`/tmp/g5-audit-concurrency-before.log`, 0 passed / 1 failed).
The fixture uses local code and docs repositories, real HTTP/SQL/Git owners, and no
external ref writer. Main then added source-fence capture before source membership,
receipt selection and head reads; provider/live work and canonical name locks stay
after the capture transaction ends.

### Abstraction audit — bounded repair discoveries

The [independent repair check](reviews/git-abstraction/abstraction-repair-check-1.md)
accepts successful source-capture fencing and unused API retirement, but identifies
RC1: Push rereads the destination after capturing its proposed head, allowing a
concurrent advance to be blessed and overwritten. Main accepts this before repair:
capture canonical/alias expectations first and pass them into alignment; test that
an intervening commit survives refusal.

The reverse-order API test initially required the publication advisory waiter, but
actual public authorization first waits at `authz.lock_user_scope_authority`'s shared
workspace row lock behind merge's update lock. The repaired test observes either
precise settling lock and still requires no frozen fork before merge commits. Both
success-order tests now pass (`/tmp/g5-audit-concurrency-repaired-2.log`).

The same recheck exposes a material failure-time boundary: definite SQL COMMIT
refusal releases the transaction fence before per-ref compensation. A new fork can
read a mixed vector during those rollback writes. Main accepts this as AA1's later
compensation finding before repair: reopen the existing scoped transaction and take
the same workspace fence for compensation; if that protection cannot be obtained,
leave accepted refs intact and report the unresolved result. Add a deterministic
real landing/SQL-refusal plus public-fork test. No new tracking state or retry loop.

During that compensation repair, main found its local COMMIT-refusal classifier
accepted every server database error, while ScopedDb's existing rollback callback
policy excludes FATAL/PANIC, connection-class SQLSTATEs and unknown completion
(40003). Reuse the stricter database owner's classification for both paths; unknown
outcomes must keep their refs. This closes another duplicate policy instead of adding
a third classifier. Existing callback behavior is preserved.

A final source inventory found four more unused legacy ref APIs (remote-tracking
`fast_forward_ref`, HEAD reparenting and two local-branch deletion variants). No
production or test caller remained anywhere under backend; they are retired too.
API guard calibration also rejects direct Git commit calls while admitting ordinary
zero-argument SQL transaction commit. The rule is syntactic and does not claim full
Rust receiver typing.

## Abstraction audit — final assessment

The implementation is generally consolidated around one owner per meaning after
these repairs. Native execution has one private runner, backend object/ref mechanics
live in SCM, API content selection goes through the authorized reader, and workspace
publication/fork remain distinct checked policy owners. Native/libgit2 backends,
local checkout versus server authority, receipt cleanup and differently scoped
provider/login/report grants are legitimate variants, not interchangeable duplicate
workflows. Unused effectful merge/pull/mirror paths, unchecked writers, four additional
dead ref helpers and their sole structured-merge injection glue are removed. The
canonical semantic resolver and unattached exact-head preparation remain.

| Commitment / finding | Actual owner and discriminator | Result / boundary |
|---|---|---|
| `git_execution`, `git_ref_updates`, minimal API / AA2 and RC1 | SCM checked ref methods; CLI captures canonical/alias expectations before proposed HEAD; caller inventory and actual CLI preservation/rebase tests | Retired alternatives have no caller; live ref changes use expected state. Stale alignment refuses without deleting the destination. Semantic resolution and workspace composition remain tested. |
| `fork_owner`, `fork.source`, `fork.replay`, `git_ref_updates` / AA1 | `admit_operation` holds the source publication fence before membership/receipt/head reads; real public fork/merge requests pause between two source reads or after merge refs before commit | Original code failed the frozen-manifest assertion. Repaired branch/durable-fallback captures preserve the complete before/after vector, child code/docs membership, Git refs, proof refs and completed replay. |
| `git_ref_updates`, `git_outcomes`, recovery ownership / later compensation finding | Branch landing reopens its existing scope and fences definite-refusal restoration; real deferred SQL COMMIT failure pauses after one of two refs is restored while public fork starts | Fork waits and freezes the complete restored vector. Failure to obtain/settle compensation protection reports uncertainty/incomplete refs. A fork winning the fence before compensation may legitimately pin the complete currently landed vector. Partial cross-repository I/O failure remains an explicit outcome, not a claim of Git transaction atomicity. |
| `git_outcomes`, one authority per fact | `ScopedDbError::commit_was_refused` now supplies both scoped rollback callbacks and branch compensation; existing definite-refusal integration plus shared-rule inspection | Duplicate loose classification removed; unknown SQL completion and FATAL/PANIC do not justify compensation. Full network-loss/server-severity injection matrix is unexecuted. |
| `git_guard`, enforcement / AA3 | Whole-backend syntax scan now includes SCM; only native runner construction is admitted; new SCM sibling/helper probes fail and typed helper controls pass | Blanket SCM native exemption closed. |
| `content_reader`, `git_guard`, enforcement / AA4 | Syntax-aware API operation ownership; UFCS, import alias, receiver rename and whitespace probes; named canonical private helpers and ordinary SQL commit pass | Ordinary reader and direct ref/commit bypasses fail. Current API packages and newly added API packages are traversed. |
| Information hiding, `git_guard` / AA5 | Public backend surface scanner includes std/tokio Command, Child and related handles; alias/re-export controls | Native escape surfaces are rejected along with libgit2/HTTP backend types. |
| `git_retry`, minimal change | Complete source/schema delta | No new production tracking tables, columns, queues, retry journals or replay workers. Existing fork replay remains its existing feature owner. |

The 32 architecture checks are selected by CI's required API integration lane
(`infra/ci/cloudbuild-ci-rust.yaml`, integration nextest filter and propagated status).
The changed checks are calibrated against bypasses and valid variations. They are
structural safeguards, not a whole-language Rust type/dataflow/macro proof or a
semantic-duplication detector. An agent can still invent a differently expressed
algorithm inside an allowed owner; review must catch that. Seven prior privacy
doctests remain evidence for unchanged visibility, but the doctest CI lane itself
uses a non-gating status and is not the claimed enforcement mechanism.

Current check receipts:

- `/tmp/g5-audit-concurrency-final.log`: **3 passed**, including both successful
  public merge/fork orders and the real failed-landing compensation/public-fork
  overlap. The two successful-order tests include three scenarios because the
  pre-merge test runs both branch and receipt-absent durable sources. No provider
  mock substitutes for these local repository transitions.
- `/tmp/g5-audit-guard-final-2.log`: **32 passed**, current source and calibration.
- `/tmp/g5-audit-landing-final.log`: **10 passed**, real stale-cut, dead/hung session,
  unborn membership and partial-write recovery scenarios.
- `/tmp/g5-audit-compensation-existing.log`: **1 passed**, existing refused-COMMIT
  regression preserves already-landed refs not written by the attempt.
- `/tmp/g5-audit-cli-push-final.log`: **7 passed**; CLI manifest **11 passed**,
  workspace-ops **38 passed**, canonical semantic resolver **41 passed** in their
  respective `/tmp/g5-audit-{cli-manifest,workspace-ops,semantic}.log` receipts.
  Common compatibility facade compiles (its now-empty unit target runs zero cases).
- `/tmp/g5-audit-final-api-build.log`: final workspace, landing and authoritative-ref
  integration targets compile. `/tmp/g5-audit-final-scm-build.log`: final SCM tests
  compile after all obsolete methods were removed. Retired workflow tests are not
  counted as current implementation coverage.

Initial failures remain preserved: the original unfenced source capture failed at
the persisted mixed vector; the initial reverse-order test expected the wrong lock;
a CLI test incorrectly expected a same-branch safe checkout to hard-reset the worktree
and was narrowed to its actual ref/refusal contract without changing production
checkout behavior. Intermediate compile failures were corrected before runtime
checks. The TLS fixture's combined-versus-isolated discrepancy is recorded separately
below, not erased by its isolated success.

Known inherited limits remain: receipt selection/pinning versus pruning, recorded
provider-connection recovery, full live-VM/session/retained-receipt interleavings and
the exhaustive interruption/partial-effect matrix are not closed by these tests.
Branch/object confidentiality remains outside the approved abstraction migration.
Three independent full-scope discoveries were consolidated before main's repair;
reviewer 1 then performed bounded read-only repair checks. Main owns final source,
checks and ontology reconciliation. No reliable token/time cost measurement was
collected for the review comparison.

The final complete workspace run (`/tmp/g5-audit-workspaces-final.log`) passes
**51 / 51**, including all three added concurrency regressions. The final landing
suite and existing compensation regression also pass as recorded above. Ontology
validation confirms **673 paired blocks in 32 files**, and `git diff --check` passes.

The shared ScopedDb target also passes **16 unit cases**
(`/tmp/g5-audit-authz-final.log`), including its clear-before-COMMIT versus unknown
transport-outcome controls. The stricter server-error classification is reused
unchanged; no synthetic severity matrix is claimed.

### Remaining macOS SCM test-order failure

The current SCM implementation has 254 tests after retiring 14 tests solely for the
removed effectful merge workflow. Two combined serial runs pass **253 and fail 1**
(`/tmp/g5-audit-repaired-scm.log`, `/tmp/g5-audit-scm-final.log`). The failing
`https_challenges_and_redirects_do_not_cross_scm_origins` passes alone
(`/tmp/g5-audit-scm-tls-recheck.log`). Reviewer 2 independently narrowed the failure
to the native authenticated lease-push test followed by this libgit2 TLS test; see
[the preserved investigation](reviews/git-abstraction/abstraction-tls-check-2.md).

Main retained the discarded connection error in the fixture's assertion. Temporary
stage diagnostics show the proxy receives CONNECT, then times out waiting for TLS
bytes. macOS SecureTransport returns handshake error `-9806` before a complete HTTP
request. Increasing the fixture wait from two to ten seconds reproduces the same
failure (`/tmp/g5-audit-scm-timeout-pair.log`); that ineffective timeout change and
noisy temporary diagnostics were reverted. The original credential-origin and
endpoint-reachability assertions remain intact, with the useful error/destination
diagnostic retained. No production transport behavior was changed.

This is an **unresolved same-process transport-order failure**, not a proved
credential leak or a harmless flake. Root cause is not established; next work needs
to trace SecureTransport's pre-ClientHello state after native transport execution,
then retain the narrowed pair as a regression. CI uses nextest process isolation,
but nextest is not installed on this host and CI was not executed here. Individual
success does not close this ordering defect. Thus implementation repairs for AA1–AA5
and RC1 are verified, while the complete combined SCM suite is not wholly green.

## PR 294 main integration — initial findings

Current main `8a3ec325` adds native-memory capture and consolidates ontology owners.
Self-review reconciles these contracts before resolving code: Git current-head
authority and repository-scoped credentials remain compatible with SCM ownership;
retired generic principle blocks stay retired, with scoped enforcement evidence
kept under `g5.architecture.commands`. Both branches' metadata evidence and limits
remain, including the memory-specific detached-writer gap.

The substantive code conflict is ignored native memory absent from HEAD: main's
`g5.memory.repository` and `g5.git.exclusions` require capture refusal with paths and
repair/retry guidance. Move its Git check into SCM's existing temporary-index capture,
return a typed refusal for the dispatcher to project, and preserve real index/HEAD.
Keep both branches' dispatcher tests and revalidate memory, flush and missing-HEAD
admission. No new persistence or automatic retry.

Later integration discovery: the unchanged all-backend guard fails on three new
`g5d-agent-turn/src/memory.rs` Git constructors from main (31/32 checks pass,
`/tmp/g5-pr294-guard-before.log`). Route startup root/HEAD/ignore observations
through typed SCM capture eligibility, preserving HEAD-tracked ignored content,
force-staged refusal and repository-binding checks. This is an actual new-path
regression caught by the existing guard, not only a synthetic calibration.

### PR 294 main integration — bounded self-review and checks

- `git_execution`, `git_guard`, `memory.repository`: startup root/HEAD/ignore reads
  now use SCM's native-only dependency and typed capture-eligibility query. The
  original guard detects all three incoming constructors; after repair all **32**
  checks pass (`/tmp/g5-pr294-guard-after.log`). No allowlist expansion.
- `memory.repository`, `git.exclusions`, working-state capture: SCM checks the
  owner-supplied required roots using the HEAD-seeded scratch index, returning
  owned ignored paths. The dispatcher preserves `FLUSH_MEMORY_IGNORED`, repair/retry
  guidance and terminal-capture handling. **7** binding tests and **2** dispatcher
  integration tests pass (`/tmp/g5-pr294-memory-final.log`,
  `/tmp/g5-pr294-shared-memory.log`), including ignored force-staged additions,
  committed ignored memory, private-state exclusion and repaired retry. The opt-in
  pinned-Claude test remains ignored; no new live-model evidence is claimed.
- Restore and admission: the host dirty-receipt/warm-reset test passes with main's
  added memory assertions (`/tmp/g5-pr294-host-restore.log`), and the abstraction's
  missing-HEAD admission test passes (`/tmp/g5-pr294-missing-head.log`). Shared
  checkout query regression passes (`/tmp/g5-pr294-checkout-query.log`).
- Current-head authority and repository credentials: preserve main's clarified
  owners; revise stale recorded-personal-connection gap wording to the current
  repository-scoped requester contract. Interrupted credential-transition and
  receipt pin/prune races remain unverified.

An initial memory test run was 6/7: SCM's error lost the caller's existing
"committed native memory" context for a non-repository. Restoring that context
with the bounded SCM diagnostic makes all seven pass without changing assertions.
Ontology scanning finds 675 body blocks with matching metadata and one inherited
main-only orphan footer (`g5.design.commit-notify`); that unrelated consolidation
issue is unchanged. The initial published head's CI is still pending; only checks
for the subsequent merged remote head can establish final CI success.

## PR 294 external review follow-up — initial findings

Self-review of CodeRabbit review `5399213006` against `436b186d`; external findings
were supplied before inspection. The checkout is clean and current-head Rust/E2E
CI is pending. No previous passing receipt closes these newly identified cases.

| Finding | Verified path and consequence | Repair and discriminating check |
| --- | --- | --- |
| CR1 / 4171869294 | Repository App credentials unwrap a separately configured origin after minting; mismatched trust can panic. | Deny before minting; mismatched-origin credential-owner regression confirms no token request. |
| CR2 / 4171869297 | AppState hardcodes github.com without an App despite an independently configured SCM API endpoint. | Resolve configurable SCM web/API endpoints consistently; personal Enterprise PR/read and origin checks. |
| CR3 / 4171869298 | Invalid platform-report endpoints silently disable delivery. | Log the endpoint-validation failure without secrets. |
| CR4 / 4171869306 | Git and flush HTTP adapters expose native target paths/stderr; push continuations nest raw errors. | Separate public operation/effect/ref evidence from server diagnostics; HTTP failure assertions reject paths and stderr while preserving accepted refs. |
| CR5 / 4171869307 | The upload-pack timeout includes bounded-buffer backpressure from the HTTP reader. | Exclude downstream write waits from the execution budget; delayed-reader completion plus stalled-process/deadline/drop cleanup controls. |
| CR6 / 4171869308 | Trailing-secret-prefix cleanup accepts one-character matches and damages ordinary diagnostics. | Retain bounded-secret protection with a meaningful minimum prefix; short-suffix and long-prefix controls. |
| CR7 / 4171869312 | Every ProviderError becomes 503, including local refusal and provider rejection. | Preserve kind/status/effect in appropriate client errors; mapping matrix includes malformed provider responses. |
| CR8 / 4171869315 | Two E2E filesystem helpers call the old FsQueryEngine constructor. | Supply explicit fixture provenance and compile/run affected filesystem scenarios. |
| CR9 / 4171869318 | Restore supplies a stripped host environment while cache warming supplies host proxy/CA/SSH/config. | Reuse the host environment owner; restore regression exercises required host configuration. |

Governing inventory: `git_native_runner` (deadline/cleanup and host policy),
`git_outcomes`/`git_retry` (safe actionable evidence without replay),
`git_credentials`/`scm_credential_scopes` (origin-bound selection), `github_transport`
(configured provider endpoints), `git_guard` (one execution path), `g5.git.restore`
and `g5.architecture.semantic_surfaces` (restore/identity compatibility). Existing
TLS ordering and deployed-runtime limits remain open. These are repairs within the
approved behavior; no retry storage or provider expansion is introduced.

### Later discoveries during the external-review follow-up

- CR1 correction: tracing `ScmOriginPolicy::effective_provider` shows it already
  requires `provider_for_url` on the same immutable policy and URL. The reported
  panic is not reachable through this path; the initial table overstated it.
  Retain that typed provider through credential construction instead of converting
  it to a string and looking it up again. Mismatched-origin no-mint behavior is
  checked, without claiming a newly fixed panic.
- CI at `436b186d` fails in cargo-deny's feature graph (build
  `b69810d6-1c7b-4ded-a394-3031a7d6aa0d`), before dependency policy checks. The same
  panic reproduces locally with cargo-deny 0.20.2. The branch moved g5d's SCM and
  workspace-ops dependencies to dev-only but left their feature forwarding in
  its normal `test-helpers` feature. Enable those helpers directly on the dev
  dependencies; preserve dependency audit policy and native-only production edges.

The old-head E2E failure is also confirmed: build
`194e6e7e-3017-48a3-88c7-e339443abe11` stops in `e2e-rust-prime` with E0061 at
both stale `FsQueryEngine::new` callers (CR8). The corrected target now compiles
and all 80 filesystem scenarios pass locally.

### External-review repairs and verification

| Commitment / finding | Actual repair and check | Result / remaining limit |
| --- | --- | --- |
| Credential origin / CR1 | Keep the validated provider value through App credential construction; retain origin and coverage checks. Add mismatched-origin/no-request control alongside narrow-token owner cases. | Reported panic was unreachable, as explained above. Revised credential fixture compiles; final DB rerun deferred to CI. |
| Configured instance / CR2 | `AuthConfig` exposes `G5_SCM_GITHUB_WEB_BASE_URL`; AppState resolves API/web once with App precedence for trust, account and PR owners. Extend attached-identity PR/check test to personal Enterprise credentials. | Config default/prefix test passes. API run was 12/14; new personal detail fixture needed a title and refusal expectations needed 409. Corrected suite compiles; final rerun awaits CI. |
| Visible configuration failure / CR3 | Platform-report endpoint validation logs its already sanitized typed error before disabling delivery. | Source inspection; no log-capture test claimed. |
| Safe outcomes / CR4 | Native errors expose one public operation/effect formatter; route completion separately appends confirmed refs and fixed Update PR guidance. Raw target/stderr stay in server logging. | HTTP tests pass for every effect, confirmed-ref continuation and flush proof failure (`review-http` log; 3 selected tests, including one existing preview test). |
| Native deadline / CR5 | Cumulative execution budget excludes bounded response writes; Git reads/exit still count, and cancellation retains supervisor cleanup. | 66 native cases pass, including delayed-reader complete/deadline/drop modes and existing descendant controls. |
| Redaction / CR6 | Full secrets remain scrubbed; trailing partial-secret matching requires at least eight bytes. | Native suite preserves ordinary short suffixes, full short secrets and meaningful bounded prefixes. |
| Provider outcomes / CR7 | Invalid input → 422; unsupported/rejected → 409; transport/deadline/rate-limit/initialization/malformed response → 503. Preserve status/effect text. | Mapping matrix passes for all kinds and all effects; changed API rejection expectations are pending CI rerun. |
| Semantic callers / CR8 | E2E helpers provide explicit fixture author identity. | Full E2E target compiles; all 80 filesystem grounding scenarios pass. |
| Host policy / CR9 | Restore reuses the cache owner's selected environment. A child-process fixture requires host configuration at the actual runner, then executes real Git restore. | Both ordinary dirty/warm restore and required-config restore pass. Live proxy/CA/SSH handshakes are not newly claimed. |
| Dependency boundary / CI1 | Move dev-only helper activation onto g5d dev dependencies, removing stale normal-feature forwarding. | `cargo deny check` passes advisories, bans, licenses and sources without policy changes; all 32 abstraction guards pass. |

Receipts: `/tmp/g5-pr294-review-{native-unrestricted,provider-errors,guard,http,config,
restore-final,e2e-final,deny-final,api-rebuild,fmt-final}.log`. Formatting and
`git diff --check` pass. Initial sandbox socket failures were rerun with local-server
permissions. The E2E build exhausted local disk; deleting old generated test executables
and pre-October-1 incremental caches recovered space, and the E2E rerun passed.
Docker then remained unavailable for the final corrected API rerun; no passing API
receipt is claimed for those last fixture edits. The user explicitly requested pushing
and using CI, reserving local development for another project. Local testing and Docker
recovery attempts are stopped; subsequent verification runs in CI.

The bounded ontology return pass covers native deadlines/cleanup, safe known/unknown
outcomes, caller-decided retry, credential origins, configured transport, restore policy
and semantic identity. No new retry state, replay, migration, provider capability or
credential-origin relaxation was added. Native HTTP outcome projection and the amended
fetch budget are recorded in the same design; existing VM, live-provider and macOS
same-process TLS limits remain unchanged. The next remote head must pass its own CI;
old-head results are not completion evidence.


## Main simplification integration — initial reconciliation

Self-review of the append-only integration of main `fd2ad7ca` into `a2fb3d49`.
The user authorized bringing in main under AGENTS.md after automatic approval
review rejected history rewriting. Local runtime remains reserved for another
project; only source/format checks run locally, and combined runtime evidence
must come from the next CI head.

| Commitment | Interaction / initial finding | Resolution and verification obligation |
| --- | --- | --- |
| `g5.git.mirror`, `g5.workspace.remote.base-parity` | Main removes the protected-ref importer and establishes ordinary Base pull-only tracking. | Keep the removal, migration 056 and all shared tracking/acceptance/advisory behavior. Remove conflicting obsolete SCM export/helper/tests. |
| `g5.git.landing.commit-point`, `git_outcomes` | This branch restores after COMMIT under a newly acquired fence; the gap still permits a reader to freeze heads later retracted. | Adopt main's pre-write constraint checks, under-fence partial-write restore and no post-COMMIT retraction. Preserve explicit errors and typed SCM ref writes. |
| `g5.fork.source.cut`, `g5.architecture.fork_owner` | Both branches add capture fencing; automatic merging may leave duplicate acquisition or readers. | Use main's single bounded committed-head helper and one vector for receipt currency; retain API merge/fork checks and main's reader inventory. |
| `git_guard`, `git_execution` | Main's new readers and ref restoration must use the abstraction, with no resurrected backend access. | Inspect combined callers and guards. Keep typed operations and dependency closure. |
| Public-path regression coverage | Both branches append tests; the old compensation test assumes a path main intentionally removes. | Keep compatible suites, replace the obsolete compensation case with pre-write refusal/capture coverage, and retain main's real partial-write/COMMIT-loss tests. CI must execute the combined suites. |

The original review evidence above remains unchanged. Main's landing amendment
explains why the old compensation outcome is replaced; a green source check alone
will not close the combined concurrency guarantee.

### CI findings discovered during integration

Rust CI for `a2fb3d49` (build `ea9d4ad4-1cca-4887-86a5-a3699e5651f4`)
finished rebuilding and publishing its builder image in 30m25s, then failed:
- The API-core test initializer supplies `scm_github_web_base_url` to `AuthService`,
  although that configuration belongs to `AuthConfig` and the resolved origin policy.
- Clippy reports 31 large-error returns/closures caused by the 144-byte
  `ProviderError`, five redundant same-type conversions in checks, and one unused
  explicit lifetime in issue URL parsing.

Repair the initializer, use fixed-size owned strings for the optional error
evidence to reduce its inline size while preserving every value/redaction rule,
and remove the redundant syntax. Do not suppress lints or change dependency/cache
inputs. The existing provider evidence, error mapping and redaction suites must
run on the next CI head; no local runtime result is claimed.

### Integration resolution and bounded verification

- **Landing and source cut:** `branch_commit.rs` retains main's complete owner,
  including pre-write constraint checks, the existing `RefsApplied` rendezvous,
  under-fence partial restoration and no post-COMMIT restoration. Fork admission
  drops this branch's extra unbounded acquisition and delegates once to
  `read_committed_branch_heads`; receipt currency and frozen refs share its vector.
- **Test consolidation:** successful before/after merge captures and replay remain.
  The after-landing test reuses main's rendezvous; unused landing/restoration hooks
  are removed from the abstraction's read barrier. The old compensation test is
  replaced by a real deferred-constraint refusal with a concurrent public fork
  capturing the unchanged cut. Main's COMMIT-loss, partial-write, Base import,
  PR/VM/download and migration tests are retained. All combined runtime evidence
  is pending CI; no historical receipt is represented as a rerun.
- **Mirror and abstraction:** remove `ProtectedRefUpdate`, `fetch_protected_ref`
  and its obsolete test. The deleted schedulers stay deleted. Main's surviving
  readers, publication and convergence continue through existing typed SCM
  operations; no dependency/guard exception or policy relaxation was added.
- **CI repairs:** remove the nonexistent `AuthService` test field. Optional provider
  target/request/rate-limit evidence uses owned boxed strings; values, Display,
  error kinds/effects and selected-secret redaction remain intact. Remove the
  redundant conversions and issue-parser lifetime reported by Clippy. Existing
  provider tests cover the unchanged behavior and must run in CI.
- **Local evidence:** Rust formatting and the diff check against `origin/main` pass.
  Main's saved mirror-retirement design contains whitespace-only unified-diff
  context lines; those inherited lines are preserved. Source checks
  confirm constraint-before-write ordering, no restoration after failed COMMIT,
  one fenced fork read, no retired import references or conflict markers, 120
  paired ontology blocks across the four reconciled owner files, and the saved
  ontology diff matches `fd2ad7ca`. These are source checks, not the full 32-test
  abstraction guard suite. CI image dependency/hash inputs are unchanged.

The return pass retains Base's pull-only owner policy, co-landed imports and
advisories; main's migration 056 is inherited unchanged, not newly designed by
this abstraction. Receipt/pruning, live-provider/VM and same-process macOS TLS
limits remain open. The combined next-head CI results are still required.

## Current-head CI follow-up — initial findings

Self-review of Rust build `85a22e97-ac8d-4ce6-9e36-1cf1e674f83e` for
`7c1c33bb`. The builder cache hit; the previous compile failures are cleared.
Clippy requires `#[must_use]` on `ScopedDbError::commit_was_refused`. Runtime
results: 3,775/3,779 unit, 216/217 g5d integration and 1,170/1,204 API integration
tests passed (plus the reported skips). Failures remain; these counts do not
establish the combined concurrency guarantee.

- Dependency and managed-push source guards still expect the former production
  dependency/raw Git spelling. Trace the current physical owner and retain the
  lease, fast-forward and SQL-blindness requirements when repairing the guards.
- Issue fixtures advertise github.com URLs while selecting the HTTP mock as the
  web instance; repository credential fixtures likewise select the wrong origin.
  Repair fixture composition without relaxing production origin validation.
- Native cancellation uses a timer-driven descendant effect; CLI signal readiness
  can observe an empty PID file. Inspect cleanup and make the rendezvous establish
  the intended overlap without relaxing child/descendant cleanup assertions.
- The before-merge fork check times out observing a PostgreSQL waiter. Trace the
  merged landing/read fence and retain proof of a coherent two-repository cut.
- The idle HTTP bridge fixture accepts an empty connection before its handshake;
  inspect whether the port scanner's probe is being mistaken for the bridge.

The bounded return pass retains credential confinement, one physical Git owner,
exact leased publication, cancellation cleanup and fenced fork capture. Existing
guards and public-path assertions must still distinguish violations. Runtime
repairs will be verified in CI; no local runtime or service changes are authorized.

### Repairs awaiting CI verification

| Commitment / failure | Actual repair and discriminating check | Evidence / remaining gap |
| --- | --- | --- |
| One physical owner | The dependency guard checks g5d's dev-only dependency, the worker's foundation dependency and the actual workspace-ops consumers' production dependencies. Managed push checks the typed commit source, branch and expected-head lease, then the SCM owner's exact lease construction. | Lightweight manifest/source assertions pass; existing native lease and public managed-push tests must rerun in CI. |
| Provider identity | Issue/comment fixtures consistently use their selected mock web/API origins; the public route explicitly rejects an otherwise matching URL from github.com. Repository fixtures share a client and token manager configured for the HTTPS Git identity. A test-helper-only OAuth endpoint override keeps real refresh exchange on the mock. The PR lifecycle and worker receive that complete credential composition. | Production origin policy and grant validation are unchanged. Existing denial, refresh, no-fallback and issue publication assertions remain; CI must execute them. |
| Cancellation | Atomically publish complete PID files. Descendant writes wait for a test-controlled gate opened only after cancellation and leader reaping, eliminating the race where the old timer fired before cancellation. SIGINT/SIGTERM still require the correct exit, cleanup message, reaped child and no descendant effect. | Source/format checks; native and CLI cancellation cases remain unverified until CI. |
| Concurrent fork/merge | During the paused first-member read, an independent transaction must fail to acquire the actual workspace fence. Then observe the competing request waiting on settlement or its preceding SQL authority check. Preserve public fork/merge calls, complete old/new vectors, manifest, child refs and replay assertions. | This directly detects missing capture fencing even if SQL authority locks serialize the competing request first. Combined runtime proof remains pending CI. |
| Idle HTTP bridge | Both idle-bridge fixtures ignore empty probe connections and wait for a real HTTP request before acknowledging the handshake. Stop/fork, mux EOF and upstream closure assertions remain. | Source inspection; dispatcher tests must rerun in CI. |
| Clippy | Add `#[must_use]` to the commit-refusal predicate. | No error semantics or lint policy change. |

The bounded return pass changes no governing commitment, production retry state,
database schema, TLS assertion or credential trust rule. Only test composition,
test coordination, source guards and the Clippy annotation change. Formatting
and diff checks pass; no local runtime tests or development services were used.
E2E build `6287258a-c183-4ab3-b3f7-10cb204eed2c` passed for `7c1c33bb`
(including both Playwright shards, provider-agent Playwright, Rust E2E and
value-loop API profiles). That receipt predates these repairs; the next head
requires its own checks before this monitor can complete.

### Second CI follow-up — initial findings

Rust build `15b1bfa6-cdf9-4688-8883-f76e80821981` for `e6dc8fee` passes all
3,779 unit and 217 g5d integration tests, including the repaired cancellation,
dependency, bridge and managed-push cases. API integration passes 1,200/1,204.
After-merge and refused-landing fork captures pass; before-merge capture still
times out in the query-text-specific waiter despite its new independent fence
assertion passing. Preserve the real fence and saved-cut assertions, observe
actual blocking transactions in this isolated test database and report active
queries if the wait still fails.

Additional findings before repair:
- Search removes rejected/invalid records before calculating whether GitHub
  returned its complete page. Filtering a pull request falsely marks the page
  truncated. Calculate pagination from the original provider page and retain
  origin/domain filtering for returned issues.
- Invalid-query errors now include typed operation/effect/status context; the
  fixture's exact prefix expectation is stale. Require both that context and
  GitHub's useful reason rather than discarding either.
- Repository verification preserves HTTP 503 as a typed rejection, but the
  public error conversion maps every rejection to Conflict. Provider 5xx must
  surface as unavailable while retaining the status, effect and redaction.
- Clippy rejects eight arguments on the public-surface AST walker. Group its
  traversal context without changing guard coverage or adding a lint exemption.

These are bounded corrections to existing pagination, safe errors, architectural
guards and concurrency verification. No governing contract amendment is needed.

Second-pass repairs preserve the original findings above. Search now computes
completeness before local validation; its regression case discards the entire
page while asserting there is no invented next page. Public provider-error
conversion maps HTTP 5xx rejections to unavailable, with status/effect preservation
checked across read, partial and unknown outcomes. The real credential-refresh
test asserts 503 for upstream unavailability, 409 for the existing access refusals,
useful HTTP evidence and secret redaction. Query-error tests require both provider
context and GitHub's reason. The AST walker now groups only its traversal context;
all negative/positive guard assertions remain.

The concurrency waiter observes actual PostgreSQL blocking in the test's isolated
database rather than particular SQL text, retaining the independent fence probe,
public requests and saved-cut assertions. It prints bounded active-query evidence
on timeout, so any remaining coordination or implementation problem is actionable
in CI. The before-merge guarantee remains explicitly unverified until that test
passes. Local formatting, source inspection and diff checks pass; all runtime
verification, including these new regressions, remains assigned to the next CI head.

### Third CI follow-up — initial findings

Rust build `3b1cbd05-92b2-4fb2-8c3e-0a3cbb74948d` for `ada47a1c` passes
3,780 unit and 217 g5d integration tests, plus 1,203/1,204 API integration
tests. Pagination, provider errors and the structural guard repairs pass. The
remaining API failure is before-merge fork capture. Its new diagnostics show
idle client connections, not a PostgreSQL lock waiter: merge retains its admission
transaction while `read_committed_cut` acquires another connection, exhausting the
two-connection tenant pool while fork holds the other connection. Manifest and
source-download callers use the same nested acquisition pattern.

The repair must consume the read-only admission transaction before opening the
short fenced capture, retain the same scoped capability and release the capture
before later phases. Keep parent application reauthorization, exact CAS, canonical
fence order and complete old/new fork assertions. Exercise manifest/download with
one tenant connection reserved so their actual public paths also detect nested
acquisition. Increasing pool capacity would conceal the defect.

Clippy additionally rejects the waiter's five-element diagnostic tuple as too
complex; name the diagnostic type without weakening its evidence or lint policy.
The bounded governing return pass retains `g5.fork.source.cut` and
`g5.git.landing.commit-point`; this is a resource-lifetime repair, not a contract
change. Runtime verification remains assigned to CI.

Third-pass repair: `read_committed_cut` now takes ownership of admission, rolls
it back, and opens/ends the fenced read sequentially. All three consumers and
public/internal manifest routes follow that ownership contract. Manifest metadata
and merge parent capture reopen only after the cut closes; merge's existing
application fence, ownership/binding rechecks and CAS remain unchanged. The
manifest/download regression reserves one of exactly two tenant connections and
requires both bounded busy refusal and successful retry through the public routes.
The before-merge test retains its independent fence probe, competing public merge,
complete persisted fork vector, child refs and replay assertions. Clippy's query
diagnostic uses a named tuple alias. Source inspection and formatting/diff checks
pass; no local runtime tests or development services were used. These fixes still
require latest-head CI evidence before the concurrency guarantee is closed.

### Fourth CI follow-up — initial findings

Rust build `e4df74fe-c601-419f-b360-fb53263528da` for `33fa4a7e` passes
all 3,780 unit, 217 g5d integration and 1,204 API integration tests (plus
the existing reported skips). The real before/after merge and refused-landing
fork tests pass, as does manifest/download with one of two tenant connections
reserved. This verifies the resource-lifetime repair and the exercised complete
fork cuts, persisted manifests, child refs and replay outcomes. It does not close
the separate live-VM, receipt-pruning or macOS TLS limitations.

The remaining Rust gate failure is Clippy's redundant `String::into()` in the
saved provider PR URL assertion at `api_workspace_pull_requests.rs:3016`.
Remove that conversion without changing the assertion. No production behavior or
governing contract changes are needed. E2E is still pending on this head.

Repair: removed only the redundant conversion and applied Rust formatting. The
persisted provider URL assertion is unchanged. Local formatting and diff checks
pass; the new head must rerun Clippy and complete its own CI checks. No local
runtime tests or development services were used.

### Fifth CI follow-up — initial findings

Rust build `d5db397e-c6ec-405f-a076-9372c0d8b66d` for `a9c19f44` again
passes all 3,780 unit, 217 g5d integration and 1,204 API integration tests.
Clippy proceeds past the repaired PR fixture and reports four SCM test issues:
the truncated-401 fixture ignores the socket read count, and three native protocol
fixtures clone an update solely to form a one-element borrowed slice. Consume
the complete request headers before sending the deliberately truncated response;
borrow the protocol update directly. Preserve the observed-401 classification,
protocol acceptance, late-failure and diagnostic-bound assertions. These are
test-only repairs within the existing design; E2E remains pending.

Repair: the truncated-401 fixture now accumulates headers through the HTTP header
terminator in its existing bounded buffer and asserts on premature EOF or an
oversized header. Protocol fixtures use `std::slice::from_ref` without changing
the request bytes or admitted updates. Local formatting and diff checks pass;
runtime verification stays in CI, and the new head must pass all applicable
gates. No production behavior, TLS assertion or local development service changed.

## Code review repairs

A high-effort code review of the branch against `origin/main` surfaced six issues;
all are repaired within existing intent, except the stalled-reader bound, which amends
the [Git runner contract](ontology/software_architecture/git_governance.g5.md).

| Finding | Repair | Evidence |
| --- | --- | --- |
| `change_branch` classified every `[remote rejected]` (hook, policy, `updateInstead`, unpack refusals) as `FailureKind::Rejected`, so `g5d push` reported a lease conflict and suggested pull/rebase. | `push_refusal` keeps `Rejected` for client `[rejected]` and receive-pack's own locked-update failures (`failed to update ref`, `cannot lock ref`, `atomic transaction failed`); other remote refusals are `Exit`/`Refused`. | `push_refusals_distinguish_ref_conflicts_from_remote_policy`; real pre-receive-hook refusal in `clone_fetch_and_push_preserve_scope_and_exact_leases`. |
| A connected client that stopped reading held upload-pack indefinitely: backpressure waits were excluded from the budget without bound. | One delivery blocked for the whole budget ends execution with a distinct diagnostic; cumulative shorter pauses remain excluded. | `upload_stream_backpressure_preserves_budget_and_cancellation` adds `stalled` mode and makes `complete`/`deadline` use several sub-budget pauses whose sum exceeds the budget. |
| API-context `safe.directory` used the caller's possibly symlinked path while Git compares the resolved cwd (old code trusted `*`). | Trust the canonical repository path; `-C` keeps the caller's path. | `repository_trust_names_the_resolved_directory`. |
| Receive-pack rebuilt the API runner (PATH lookup + canonicalize) per observation and re-validated the ref name inside `ref_tip`'s symbolic check. | One observer per request; `ref_tip` uses an internal already-validated symbolic read. | Existing push-policy, flush-receipt, VM and remote-tracking API suites pass. |
| Releasing manifest admission before the fenced capture also released the incarnation row locks. | The internal manifest route rechecks the incarnation after capture. | `internal_manifest_refuses_an_incarnation_replaced_during_capture` pauses the real read, replaces the VM epoch, and gets 403; it fails without the recheck. |
| Form requests appended a second `Accept` header. | One Accept value per request (plain JSON for form posts). | OAuth exchange assertion in `oversized_success_is_partial_and_oauth_rejection_is_refused`. |

Local verification on macOS with the dev Postgres stack: `g5-scm` (258 unit, 7
integration), `g5-api-routes`, `g5d-cli`, `g5d-dispatch`, and API
`api_workspace_manifest`, `api_git_push_policy`, `api_workspace_flush_receipts`,
`api_workspace_vms`, `remote_repo_tracking_integration`, `maintenance_integration`
pass; Clippy (`-D warnings`) and rustfmt are clean for the touched crates. The two
pre-existing 500 ms upload-stream tests flaked under full-suite load on the
unmodified head as well; both now use a 1.5 s budget. Still intermittent on the
unmodified head and unchanged here: the known macOS TLS order test
(`https_challenges_and_redirects_do_not_cross_scm_origins`),
`api_observation_deadline_and_cancellation_reap_child_without_retry`, and the g5d
PTY test `terminal_runs_g5d_pr_directly_with_workspace_context_and_preserves_errors`.

## Second abstraction audit — initial findings

[Request](chat_intent/design-git-abstraction/request.md#second-abstraction-audit) and
[design](ontology-plan-git-abstraction.md#second-abstraction-audit-repairs). Four
read-only discovery passes over `6b402c95` (native execution, libgit2 surface,
provider/HTTP, test coverage), spot-checked by the lead agent. Self-review of the
audit; the independent review of the repairs follows below.

| Finding | Governing | Kind |
| --- | --- | --- |
| Publication had two backends: fast-forward via libgit2 (no deadline, callback credentials, string errors) and lease via native; lifecycle re-resolved the source branch after its head check. | `git_native_runner`, `git_ref_updates` | Defect |
| API lease pushes built `NativeGit::cli`, using ambient PATH/environment instead of the API resolver. | `git_native_runner` | Defect |
| API ls-remote/clone/object download and g5d-cli clone/fetch/push for non-HTTP remotes ran through libgit2 without deadlines; several lifecycle ls-remote calls blocked the async executor. | `git_native_runner`, `git_outcomes` | Defect |
| Three libgit2 CAS paths (`apply_ref_commits`, `apply_branch_commit`, unused `apply_sync_watermark`); unused branch-name merge creation; unchecked public `delete_local_branch` used by g5d-cli pinning with ignored errors. | `git_ref_updates` | Defect |
| ~25 public `GitRepo` methods and most `g5-workspace-ops` wrappers had no production caller; an exact-base shim required a spurious worktree root. | `g5.design.minimal-api` | Cleanup |
| Ref-mutator guard scanned only `g5-api*` and named removed methods; pinned-read services were invisible to the reader guard; HTTP permits ignored the request destination; only `reqwest` was detected. | `git_guard` | Defect |
| Duplicated provider mechanics: installation-token issuance ×2, App JWT/cache ×2, issue transport/error parsing, 403 classification ×3, Link pagination, origin/repository validators; Adopt-Remote parsed identity without the configured Enterprise prefix; two SCM token read/encryption paths (only one refreshed GitLab); unused credential-store helper persisted a token URL. | `scm_provider_client`, `github_transport`, `git_credentials` | Defect/cleanup |
| Evidence: two wiremock no-replay tests discarded `expect(1)` via `reset()`; no hidden-sleep, dropped-connection PR create, issuance non-recursion or distinct-layout evidence; the atomic receive refusal could not observe a mutation of `main`; host token-rotation test lost in migration; a deadline fixture raced its own attempt marker. | `git_retry`, `git_outcomes`, `git_locations` | Missing evidence |

### Repairs and verification

Implemented in `fc119f17` (native network, ref paths, dead surface, guards, native
evidence) and `577cb36f` (merge of provider consolidation `0e15ddfe`, `c1c22afb`,
`8c40460f`).

| Commitment | Actual path | Check | Result |
| --- | --- | --- | --- |
| Network Git only through the runner | `native/remote.rs`; `GitRepo::{publish_commit,push_existing_commit,push_create_ref}` take `&NativeGit`; `git_runtime.rs`; g5d-cli `native_git.rs` | `libgit2_never_contacts_a_remote` (violating/test-exempt fixtures); `remote_observations_and_object_download_change_no_refs` | Pass (local) |
| One publication decision over exact commits | `publish_commit` | `publish_commit_fast_forwards_creates_and_leases_rewrites` (create, FF, rewrite, stale lease, stale FF refused, recovery, no-op) | Pass |
| Create-only never advances an existing branch | `push_create_ref` absent lease | `create_ref_publication_refuses_an_existing_destination_without_moving_it` | Pass |
| SCM origin binding survives the move | native `validate_url`/`authenticate` | `scm_publication_rejects_stale_origin_and_pushurl`, `foreign_challenge_cannot_extract_scm_token` (refused before contact, no Authorization), TLS lease/compensation transport tests | Pass |
| One CAS; merge analysis over verified heads | `apply_branch_commit` → `apply_branch_commits`; `analyze_head_merge_no_checkout` | Existing CAS/cut tests; watermark test ported to production fence shape | Pass |
| Checked CLI pinning | `GitRepo::checkout_branch_at` | g5d-cli suite (248) | Pass |
| Ref mutations confined in every crate; pinned reads named | guard | `ref_mutations_outside_scm_use_named_policy_owners`, `ref_mutation_guard_rejects_new_cli_writers_and_allows_named_owners`, calibration additions | Pass |
| Atomic receive refusal leaves refs unchanged | `receive_pack` | `receive_scope_cas_and_atomicity_are_enforced_before_or_by_git` now targets a remote-only `newer` object | Pass |
| Distinct layouts, no ancestor discovery | `explicit_git_dir`, observations | `distinct_location_layouts_resolve_their_own_git_directory` (API storage, host cache, member worktree, dev mirror, damaged dir inside an enclosing repo) | Pass |
| Live host credential | `RepoCache::git_auth` | `git_auth_reads_the_worker_token_live_after_rotation` | Pass |
| Provider no-replay/no-sleep, lost create reply, non-recursion, destination-checked permits | provider clients/guards | wiremock `verify()` before `reset()`; bounded-elapsed checks; raw-socket dropped POST (one request, Unknown); issuance guard; argument-level HTTP permits and hyper detection | Pass |

Local (macOS, no services): workspace Clippy `-D warnings` clean; `g5-scm`
all-features 234, guards 38, `g5-api-core` lib 1024, `g5d-cli` 248,
`g5d-foundation` 268, `g5-workspace-ops` 24, `g5d-runtime-core` 13, g5d
`crate_architecture`/`cli_commands`/`push_authority` pass. The libgit2 TLS
order test is removed with its transport. Under concurrent build load two native
timing tests failed once and pass in isolation and on rerun.

Unexecuted locally (CI): DB-backed API suites for publication, PR lifecycle,
remote tracking, fork preflight, external issues, App bindings and Adopt-Remote
identity; dispatcher member/flush integration; E2E.

### Independent review of the repairs — initial findings

Fresh-context, read-only reviewer over `b1047050..577cb36f` (inspection only).
Recorded before repair.

| # | Finding | Governing | Kind |
| --- | --- | --- | --- |
| 1 | `remote_branches` lists every head under the 64 KiB capture limit; repositories with roughly 800+ branches fail every tip read, including post-push confirmation (libgit2 had no cap). | `git_native_runner`, `git_outcomes` | Defect (high) |
| 2 | `fetch_branch_objects` can trigger Git's detached auto-maintenance in API storage, outside the runner's cleanup and the API GC owner. | `git_native_runner`; API GC owner | Defect |
| 3 | CLI SSH/local remotes now share HTTP's single-branch clone and configured-refspec fetch; libgit2 fetched all heads, so switching an unpinned non-HTTP checkout to an unfetched branch can fail. | `git_ref_updates` (materialization) | Design question |
| 4 | Workspace-switch blocker fetch refuses an origin that differs from the manifest URL, failing the switch instead of reporting a blocker. | N/A | Design question |
| 5 | libgit2 network guard misses method-call bypasses (`find_remote(..).fetch/push`, `Repository::clone`); ref-mutator list omits workspace/fork ref deletions and native push/delete; issuance guard is per-file. | `git_guard` | Defect / limit |
| 6 | `git_runtime::network` sets no overall deadline, so multi-invocation operations exceed the pinger (20 s) and fork-preflight (30 s) budgets formerly enforced by an outer timeout. | `git_native_runner` | Defect |
| 7 | Adopt-Remote rows previously stored with an Enterprise path prefix no longer match the canonical identity in the uniqueness pre-check. | `scm_provider_client` | Design question |
| 8 | CLI no longer sends the G5 token over HTTPS for `git@github.com:` remotes; SSH identity is used instead. | `git_credentials` | Behavior change (note) |
| 9 | Broken intra-doc link to a removed method in `repo.rs`. | N/A | Defect (trivial) |

### Independent-review repairs and verification

| # | Repair | Check | Result |
| --- | --- | --- | --- |
| 1 | Tip reads pass `refs/heads/<branch>` as the ls-remote pattern with exact matching; full listings use a bounded 8 MiB capture and refuse rather than truncate. | `remote_branch_reads_scale_past_the_diagnostic_capture_limit` (2,501 branches; suffix-only names do not match); fails with the old 64 KiB cap. | Pass |
| 2 | Object download disables `maintenance.auto`/`gc.auto` per invocation. | `object_download_never_runs_automatic_maintenance_in_storage` (synchronous gc config, reachable seed, no pack after fetch); fails without the setting. `GIT_TRACE` confirmed `git gc --auto` otherwise runs. | Pass |
| 4 | Switch blocker fetch maps a destination mismatch to a named blocker. | g5d-cli suite | Pass (inspection for the new branch) |
| 5 | SCM's libgit2 is built without HTTPS/SSH transports (`default-features = false`), so no libgit2 call can reach a provider; a manifest guard enforces it. Ref-mutator list adds workspace/flush/fork ref deletions and native push/delete/mirror writes with named owners. | `scm_libgit2_is_built_without_network_transports`, `ref_mutations_outside_scm_use_named_policy_owners` | Pass |
| 6 | `NativeGit::with_deadline`; `git_runtime::network` bounds every invocation of one operation by its caller's deadline. | Existing deadline tests; compile/clippy | Pass (inspection for budgets) |
| 7 | Adopt-Remote pre-check matches the canonical identity and the legacy prefix-bearing spelling. | DB path in CI | Pending CI |
| 9 | Stale doc references removed. | rustdoc via doctests | Pass |

Dispositions: #3 is accepted. Production CLI materialization runs in the guest over
the HTTP relay and already had single-branch clone and configured-refspec fetch;
non-HTTP development remotes now share that behavior instead of fetching every head.
Unpinned switches to an unfetched branch need an explicit fetch, as on HTTP. #8 is an
intended credential tightening: the G5 transport token is never sent to a provider
host for an SSH remote; SSH identity applies. The issuance-recursion guard remains
per-file; recursion through a helper in another module is a recorded structural limit.

Local verification after repair: workspace Clippy `-D warnings`; `g5-scm`
all-features 236 and doctests 4; guards 39; `g5-api-core` lib 1024; `g5d-cli` 248;
g5d `cli_commands`/`push_authority`/`crate_architecture`. DB-backed API suites, E2E
and the remote-tracking concurrency test await CI (`concurrent_member_cycles_converge_without_lease_expiry`
failed once on `b1047050`, whose code equals the previously green head).

| Governing ontology | Gap | Proposed action | Uncertainty / alternatives / blockers |
| --- | --- | --- | --- |
| `git_guard` | Issuance non-recursion is checked per file only. | Accept as structural limit; revisit if issuance gains cross-module helpers. | Dataflow proof is out of scope for source guards. |
| `git_ref_updates` (materialization) | Non-HTTP CLI checkouts no longer fetch all heads. | Accept (parity with production HTTP); add explicit branch fetch to switch if users need it. | Requires product decision on switch semantics. |
| `scm_provider_client` | Legacy prefix-bearing Adopt-Remote identities remain stored. | Dual-spelling pre-check now; optional backfill. | Requires production data query. |

### CI discoveries after the independent-review repairs

CI on `391e5415` (DB-backed suites) found three later discoveries, repaired in `1ce1481a`:

| Finding | Repair | Result |
| --- | --- | --- |
| Provider PR-lifecycle API tests modeled GitHub with a libgit2-only global `insteadOf` rule; the native API context correctly ignores global config and refuses rewritten destinations, so ls-remote reached github.com. | A `test-helpers`-only transport redirect in the native runner (`redirect_transport_for_tests`): canonical URL validation, destination checks and credential binding run unchanged; only the final transport argument is local. Compiled out of production builds. | Awaiting CI |
| Consolidated comment parsing required `updated_at`/`html_url`, so PR detail failed on partial records the previous view tolerated. | Fields are optional in the shared record; the issue mapping still refuses a comment without them; PR detail omits an unowned or absent URL. | Awaiting CI |
| The maintenance fixture created a commit without a Git identity on CI. | Explicit fixture identity. | Awaiting CI |

### Main integration: flush proof ownership

Main moved flush-receipt proof from the HTTP route into
`WorkspaceFlushService::prove` (proof before recording), which the second audit had
flagged as route-owned. Main's service read Git with a raw `tokio::process::Command`
(`safe.directory=*`, revision-expanding `rev-parse`, no deadline), contradicting
`g5.architecture.git_native_runner`; the native-observation guard rejects that
version. The merge keeps main's ownership and public error codes and runs the reads
through `NativeGit::api` observations (exact refs, explicit Git directory, 30 s
deadline). The guard now names the service's `prove` as the owner. The route-level
public-message test retired with the route proof; the service returns codes only.
Service proof DB tests await CI.

## Regression review against main

Three fresh read-only reviewers compared the branch with `origin/main` for
user-visible regressions (API/routes; native/host/guest/CLI; providers/credentials).
The user directed fixing the regressions and following same-origin redirects.

| Regression | Repair | Evidence |
| --- | --- | --- |
| Remote integration recorded revoked credentials as `fetch_failed` (classifier matched libgit2 wording only). | `prepare_pull_classified` carries the runner's typed `Unauthorized` refusal; the worker records `auth_required` from it. | `pull_failure_keeps_the_runners_credential_refusal`; native refusal typing test |
| App inventory failed above 1,000 repositories (10-page cap). | Cap raised to 1,000 pages (100,000 entries); paging follows `Link`. | Provider inventory tests |
| New wall-clock limits failed large transfers (CLI 120 s, host cache/restore 480 s, guest member 300 s). | CLI 1 h (cancellable), host cache/restore and member 30 min; cache lock stale age and maximum wait raised above the budget. | Compile-time lock invariant; host suites |
| GitHub clone URLs with trailing slashes lost identity. | Canonical parser trims trailing slashes (`…/r/`, `…/r.git/`); extra path segments stay refused. | Parser cases |
| Rematerializing a single-branch checkout could not switch branches. | `NativeGit::track_remote_branch` (idempotent) runs before fetch for the manifest branch. | `single_branch_checkouts_can_track_another_branch_idempotently` |
| PR-source push reported a remote failure before a head drift. | Local head check precedes the remote probe. | Inspection; DB path in CI |
| Non-Git CLI interruptions printed the Git effect warning. | Printed only when a native operation ran (`NativeCancellation::has_run`). | Cancellation test; CLI signal test |
| Provider REST refused GitHub's same-origin rename redirects. | Transport follows same-origin redirects (at most five); a redirected PR may name the moved repository on the same instance. Cross-origin redirects are returned and receive nothing. | `same_origin_redirects_reach_a_moved_repository_and_cross_origin_ones_do_not` |

Native Git still refuses credentialed redirects: Git keeps an injected header across
the redirects it follows and cannot confine it to one origin. A renamed repository's
stored clone URL must be updated for Git transport. The `github_transport` contract is
amended to state both. Intended changes recorded earlier (URL-rewrite refusal,
credentialed URLs, no global `safe.directory`, provider status mapping, expired-token
refusal, report destination configuration, App endpoint precedence) are unchanged.
