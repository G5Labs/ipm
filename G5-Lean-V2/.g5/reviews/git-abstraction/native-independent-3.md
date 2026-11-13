# Native Git increment — independent review 3

Read-only independent discovery of the full native increment. No implementation history,
author explanation, peer findings or peer review artifacts were read. No production or
ontology files were edited. No Cargo commands were run alongside the main build.

Snapshot: `/tmp/g5-native-review-snapshot/increment.patch`, SHA-256
`6cd1c7b023e60c79fd094fa951f36d2ad41317f183d23ca238f4ed41752fa3cd`.
The three new native source files were compared byte-for-byte with that snapshot and
matched during discovery. The saved design's ontology diff additions/removals also
matched the actual ontology diff against `origin/main`.

Inputs included the original saved request, the complete pinned design including its
first governance/diff section, current Git/governance text and metadata, implicated
runtime, durability, security, architecture and guard-calibration blocks, the complete
increment, and CLI/runtime consumers. The earlier resolution/diff implementation was
considered only through its feature-gating and guard interactions.

## Initial findings (saved before sharing)

### R3-1 — P1: the strict destination check rejects the runtime's legitimate member relay

- Changed location: `backend/g5-scm/src/native/mod.rs:391–421`, reached from
  `fetch_checkout` at 201 and `push_branch` at 242. The check compares Git's expanded
  URL with the canonical manifest URL and rejects any difference.
- Actual consumer: `g5d-dispatch/src/git.rs:180–214::configure_native_remotes` writes
  a per-member `includeIf gitdir:` configuration that intentionally rewrites that
  canonical URL to the incarnation's loopback `GitTransport` lease. Its
  `process_environment` exports this `GIT_CONFIG_GLOBAL` at 239–240, and
  `g5d-dispatch/src/server.rs:253–274` supplies it to owner terminals. The CLI launcher
  preserves it. `push.rs::push_managed_candidate` still supplies the manifest URL.
- Consequence: a normal `g5d push` from a configured workspace terminal/agent is
  rejected before it reaches the trusted relay; refresh/fetch of an existing member
  is likewise rejected. The broad rewrite-negative test covers a forbidden rewrite
  but omits this supported runtime-owned variation. Deferring the dispatcher runner
  migration does not avoid the regression because its environment already feeds the
  newly migrated CLI.
- Governing: `g5.architecture.git_native_runner`, `git_credentials`,
  `g5.runtime.workspace-cli`, `workspace-cli.context`; the design explicitly preserves
  the guest runtime credential boundary and requires inspection of reused context.
- Discriminating evidence: isolated real-Git probe reproduced the runtime's canonical
  `includeIf gitdir:<canonical-worktree>/.git` and `url.<lease>.insteadOf` settings.
  The exact new `git -C <member> -c safe.directory=<member> remote get-url --push --all origin`
  returns `http://127.0.0.1:7101/member-lease/<repo>`, while `expected_url` remains the
  configured HTTPS repository URL, so the new predicate returns false. No network,
  production state or test resource was used in this probe.
- Proposed repair: have trusted runtime composition supply a typed/validated relay
  destination or appropriate guest execution context, retaining canonical identity
  separately. Do not broadly allow arbitrary URL rewrites or discard the relay's
  credential isolation. Add a real `GitRuntime`/CLI or equivalent owner-created relay
  integration test alongside the unauthorized-rewrite refusal case.

### R3-2 — P1: inherited Git control variables override destination and credential fences

- Changed location: `backend/g5-scm/src/native/process.rs:56–70,105–145` inherits
  all process variables except tracing. Authentication appends protected settings via
  `GIT_CONFIG_COUNT`, but leaves `GIT_CONFIG_PARAMETERS` intact. Native Git processes
  the latter after the former, so inherited configuration can override the runner's
  `http.<url>.followRedirects=false` and credential-helper settings.
- Runtime evidence: an isolated real-Git `config --get-urlmatch` probe with the exact
  injected settings plus `GIT_CONFIG_PARAMETERS="'http.<url>.followRedirects'='true'"`
  returned `true`. An isolated loopback HTTP probe using the exact fetch argument
  and config construction then redirected `/repo/info/refs` to `/stolen/info/refs`
  on the same origin. Both handlers received the synthetic platform Authorization
  header. Script: `/tmp/g5-native-review3-same-origin.py`. A cross-port variant
  (`/tmp/g5-native-review3-redirect.py`) followed the redirect but the installed
  libcurl stripped Authorization there; no cross-origin credential leak is claimed.
  These probes used synthetic credentials only. The sandbox initially disallowed
  listening sockets; the bounded loopback-only runs subsequently received escalation.
- A separate real-Git probe confirmed the same undeclared inherited-control problem
  for repository selection: with `GIT_DIR=<B>/.git`, a command containing `-C <A>`
  added the remote to B's config and left A untouched. The runner's new
  `set_remote_url` and its repository-scoped checks do not clear this variable, so
  their repository argument is not authoritative. `GIT_WORK_TREE`/namespace and
  related overrides likewise need an explicit context policy rather than inheritance.
- Governing: `g5.architecture.git_credentials`, `git_native_runner`, `git_ref_updates`,
  `git_locations`; design context rules explicitly require allowed inherited `GIT_*`
  variables, destination-bound injection and redirects that cannot forward secrets.
- Proposed repair: define the CLI context's allowed Git environment and remove or
  safely normalize ambient command/config/repository overrides before preflight and
  execution, while retaining runtime-owned configuration and PATH/loader inputs.
  Ensure the operation's safety settings have final precedence. Add public-path
  tests for inherited `GIT_CONFIG_PARAMETERS`, a same-origin cross-repository
  redirect, and `GIT_DIR` selecting another checkout. Test the valid relay context
  from R3-1 so sanitization does not break it.

### R3-3 — P2: conversion to the user-visible error discards the target

- Changed location: `backend/g5-scm/src/native/mod.rs:43–75`. `at_target` populates
  the URL/ref/repository, but `Display` prints only operation/detail/effect. Conversion
  to `G5Error::GitError` uses that display string and discards the structured target.
- Consequence: a deadline, spawn, destination or unknown-completion failure emitted
  by CLI materialization or a multi-member push does not say which repository/ref
  needs inspection. `with_published_members` preserves previous members but does not
  restore the failing member's identity for a generic native error. This defeats the
  requested clear, actionable manual-retry result.
- Governing: `g5.architecture.git_outcomes` and `git_retry` require operation and target,
  known effects and an inspect-before-retry instruction.
- Discriminating evidence: direct source inspection of the complete conversion path.
  The increment's own `native/tests.rs:333` asserts that the displayed error contains
  `refs/heads/main`; the first synthetic case has detail `RPC failed; HTTP 409`, so
  the current formatter cannot satisfy that assertion. This test was not run here
  to avoid concurrent Cargo work; its contradiction is visible in the source.
- Proposed repair: retain the sanitized target in display and the public error
  conversion (and preserve typed failure/effect fields where consumers require them).
  Test the actual `G5Error`/CLI envelope after conversion, including a second-member
  failure, redaction and an unknown result, rather than only inspecting a private field.

### R3-4 — P3: the new source guard mistakes ordinary names for execution ownership

- Changed location: `backend/g5-api/tests/authoritative_workspace_reader_guard.rs:84–92`.
  Any identifier named `process`, `Command` or `base64`, and any string containing
  `extraHeader`, fails the guard, regardless of import resolution or operation.
- Counterexamples by inspection: renaming a local result to `process`, adding an
  unrelated domain enum `Command`, or explaining an `extraHeader` error in a string
  fails the test while creating no Git bypass. The current harmless fixture changes
  only a runner local name and cannot detect these false positives.
- Governing: `g5.architecture.git_guard`, `g5.quality.review.guards` require ownership
  checks that accept harmless refactors; spelling checks are only supplemental.
- Proposed repair: narrow the syntax check to relevant process/base64 import paths
  and process-construction/credential-assembly usages, preserve alias detection,
  and add the harmless name/string cases. Static checks still need not prove arbitrary
  dynamic Rust code safe. This is a guard-quality issue, not an observed product failure.

## Commitment and coverage ledger

| Commitment | Actual path / discriminating cases | Result or gap |
|---|---|---|
| `git_execution`, `git_native_runner`, `g5.design.abstraction`, `minimal-api`, `g5.architecture.commands/dependencies` | Private `Invocation`; public clone/fetch/push/set-remote operations; native/libgit2 gates; CLI call sites | No public argument/environment escape hatch added; dependencies remain downward. Native-only source imports inspected. Actual feature builds left to main. Runtime context violation R3-1/R3-2. |
| `git_ref_updates`, network CAS/ref-scope | Full branch refs; explicit lease including absent destination; `--no-follow-tags`, no recursive submodules; CLI preflight fast-forward and member list | Existing managed CAS/fast-forward/member-partial flow preserved by inspection. New tests assert stale lease refusal, absent create, unchanged replay and no tag push. This reviewer did not execute them. Ambient repository selection violates declared target (R3-2). |
| `git_credentials`, security publication/platform-token/redaction | Token-only CLI channel, URL validation, `GIT_CONFIG_*` injection, trace removal, redacted output and bounded capture | No newly written credential argv/config URL found; unsupported provider/SSH grants refused. Redirect/environment violation R3-2. New destination checks reject legitimate relay R3-1. Provider tokens remain outside this increment. |
| `git_outcomes`, `git_retry`, recovery | Native error kind/effect; porcelain requested-ref status; sync adapter; existing member result wrapper | No new persistence, replay or retry worker. Refused/partial/unknown branches exist; target lost at consumer R3-3. Timeout after a confirmed porcelain record is not covered: `supervise` drops collected output on timeout and reports Unknown. Recheck this boundary before claiming preservation of all known effect evidence. |
| Native deadline/cancellation ownership | Detached supervisor owns child; oneshot drop; Unix process group; bounded drains and timeout | Source supports child/pipe cleanup on future drop, including pipes held after leader exit; dedicated tests exist. No independent execution here. Cleanup wait itself is unbounded; behavior under unkillable kernel I/O and Windows descendant handling remains unverified rather than asserted correct. |
| Runtime bundle/workspace CLI/materialization, locations | PATH resolver, inherited loader/config, owner-supplied paths, guest global config consumer | Resolver canonicalizes once per handle. R3-1/R3-2 affect actual guest/external CLI contexts. Host retained-control isolation and distinct host/API contexts are untouched remaining migration work, not new claims from this increment. |
| Working-state/flush/checkpoint/exclusions | Explicit push source; no index/HEAD operations introduced; materialization still pins through existing owner | No new checkpoint/flush implementation; unchanged contracts remain governing. Existing `configure_safe_directory_global` calls in materialization remain a known broader migration gap, not a new repair finding attributed to this patch. |
| Reader/projection boundaries and `git_guard`, guard/cases | Existing syntax reader checks retained; narrowed CLI paths added; required-function and test-only fixtures | Prior reader/projection protections retained by inspection; new CLI guard overbroad R3-4. Whole-platform raw handle/spawn/provider closure remains unimplemented and is accurately scoped as migration work. |
| Provider scopes/shared HTTP/PR authorship/checks/push refresh | No provider or API receive-pack migration in increment; smart-HTTP protocol still same entry | No new provider fallback, authority or retry state found. Actual VM relay push and API confirmed-ref refresh end-to-end are unexecuted; R3-1 currently blocks the normal configured guest path. |

## Verification limits and proposed documentation disposition

- Inspection covers the full native increment, feature gates, migrated callers and
  material runtime interactions, not a specialty slice. Real-Git probes establish the
  specific URL/config and credential observations above; they do not substitute for
  executing the Rust public APIs or the deployed guest flow.
- No Cargo test/build or shared database/fixture mutation was performed. The main owner
  should run the native suite, focused CLI/materialization cases and feature closure
  checks after consolidation; R3-3 already exposes a source-level failing assertion.
- Preserve the current governing ontology. No intent amendment is required to repair
  these findings. The design's native context paragraph should explain the verified
  canonical-identity versus trusted-relay input once implemented, and metadata should
  describe actual CLI progress and retain unexecuted host/guest/provider coverage.
- Rough discovery effort: approximately 20 minutes, including source/governance reads
  and five isolated Git semantic probes (one corrected for macOS `/tmp` canonicalization).
  No repair effort was spent by this reviewer; no measured benefit comparison is claimed.

## Later bounded repair recheck

This is a later read-only recheck after main consolidated N1–N6/NS2 and repaired the
increment. The initial report above is preserved. Inputs now include the consolidated
review and current native design. Main reported 13 native tests and SIGINT/SIGTERM CLI
subprocess checks passing; I inspected their assertions and the repaired production
paths, without rerunning Cargo while main runs broader regression checks. The withdrawn
branch/object security extension was excluded from this recheck.

### Repaired guarantees assessed

| Consolidated item | Recheck result |
|---|---|
| N1 / original R3-2 | Repaired for the demonstrated environment overrides: `Invocation::new` removes repository-selection/config-parameter channels before every preflight and operation. Public-path child tests exercise actual pushed remote OIDs under conflicting `GIT_DIR`; HTTP fixture supplies malicious inherited config and verifies no redirected request. Runtime PATH/loaders/global-config remain available. |
| N2 | `[remote failure]` now stays Unknown and its porcelain summary survives error conversion. The real receive-pack proxy removes the acknowledgement and checks the authoritative remote OID, so the supplied passing test discriminates accepted writes from definite rejection. Normal completed output with an accepted flag plus a later nonzero exit stays Partial. The timeout/stream-error boundary still discards already-received confirmation, detailed below. |
| N3 / original R3-1 | Existing-member fetch/push now resolves a unique loopback mapping from the runtime global config and checks the actual remote against it. The new test reproduces the owner's includeIf structure and confirms a relay request has no Authorization header; repository overrides are still rejected. Fresh-clone compatibility remains broken, detailed below. |
| N4 | Signal ownership is at the CLI composition layer, only around commands containing migrated operations. Shared cancellation reaches the private synchronous runtime and wait_idle follows active supervisor cleanup. The two subprocess tests inspect child reaping, deferred descendant effects and 130/143 exit codes. No new retry/persistence mechanism is introduced. |
| N5 / original R3-3 | Repaired: Display includes its sanitized target; `G5Error` conversion preserves it in text; CLI tests cover text and JSON consumers with effect guidance. |
| N6 / original R3-4 | Repaired for reported harmless changes: the guard now checks process import/qualified paths and follows aliases, with local `process`, domain `Command` and harmless header-string controls. Missing production target and test-only exclusions remain. |
| NS2 | Managed rejection retains expected-head evidence and conditional pull/rebase guidance without claiming an observed move for every refusal. Existing published-member decoration remains intact. |

### Later R3-5 — P2: workspace context makes fresh clones depend on the invoking directory

`g5d-cli/src/native_git.rs:15–19` selects workspace context for every native operation
whenever the dispatcher socket environment is present. However,
`g5-scm/src/native/mod.rs:187–190::clone_checkout` calls both relay discovery and URL
expansion with `repository = None`. `transport_destination` evaluates the runtime's
`includeIf gitdir:<member>/.git` rules using that process's ambient directory and refuses
when no matching route is visible. A command invoked from the normal workspace root has
no member Git directory, and a new/missing checkout has none yet either. Thus `g5d clone`,
`g5d add` and refresh materialization's new/missing-member path can fail before any clone,
despite being supported consumers of the migrated operation. Running from a different
member can also select the wrong member's conditional route set.

Evidence: an isolated real-Git probe reproduced the current fixture/owner configuration
and the exact `git config --global --includes --null --get-regexp '^url\..*\.insteadof$'`
discovery command. From the configured existing member it returned status 0 and the
route. From its workspace parent it returned status 1 and no route. Source inspection
then directly applies `routes.len() == 1` at mod.rs:481 and the refusal at 495–496.
The new relay test exercises only `fetch_checkout` on an already initialized member;
the ordinary clone test uses the direct context, so neither catches this gap.

This remains N3 owner-compatibility work under `git_native_runner`, `git_locations` and
the design's preserved CLI materialization behavior. Resolve a trusted operation/member
route independently of ambient Git discovery, including before the destination exists,
without admitting arbitrary user rewrites or putting the platform token into relay Git.
Add a workspace-context clone/materialization test invoked from a workspace root, and
cover missing checkout/new member as applicable to the runtime owner's supported flows.

Follow-up caller check: the public `g5d workspace refresh` command itself delegates to
`workspace_git::execute` and therefore is not evidence of this new failure. The concrete
shipped path is `g5d clone <current-workspace-UUID> <new-path> --token <incarnation-token>`
from the workspace root. `clone.rs:189–212` recognizes the UUID and does not request a
server clone; `resolve_auth_token` accepts the explicit token. It reads the current
manifest via the existing 401/403 fallback in `workspace_manifest.rs:617–640` to the
incarnation-scoped internal manifest endpoint (`routes/internal.rs:906–937`), then
reaches the new-checkout materializer at `clone.rs:249`. There is no guest/socket
rejection in this CLI path. This uses the current incarnation's workspace rather than
requiring another workspace's authority. Managed add may have additional API admission
constraints and is not needed to establish the finding. A deployed end-to-end clone
was not executed; this caller/admission trace plus the isolated discovery probe is
the available evidence.

### Later R3-6 — P2: a deadline can discard an already-confirmed requested ref

`native/process.rs::supervise` still owns output only inside the completion future and
`drain` returns its retained bytes only after EOF. Deadline, cancellation or stream-read
failure drops that future and returns a new error with the operation's fixed Unknown
effect. `native/mod.rs:311` propagates that error before parsing porcelain at 313–349.
Consequently, if native Git has already emitted the requested ref's accepted porcelain
record but a descendant keeps a pipe open until the deadline, the runner loses that
known completion and reports only unknown changes. This is the consequential evidence
gap noted in the initial ledger, now checked against N2's explicit requirement to retain
confirmed remote completion after a later-stage failure.

The new lost-ack test correctly proves Unknown when the confirmation never arrives;
it does not cover the inverse case. Existing descendant-pipe coverage checks cleanup
only, and the Partial test supplies a completed process rather than a deadline. Preserve
bounded collected output/effect evidence through interrupted collection and classify the
requested ref after cleanup. A discriminating test should perform a real push through a
wrapper that forwards the accepted porcelain record and then holds a pipe open: assert
the authoritative remote OID, deadline cleanup and retained confirmed/Partial outcome.
Until checked, do not claim all post-confirmation failures preserve known effects.

No production files, ontology or shared test resources were changed in this recheck.
The only additional execution was the bounded isolated includeIf discovery probe above.
The two remaining issues are within the original native abstraction scope; neither
requires the withdrawn branch/object isolation redesign or new retry storage.

## Later targeted recheck — R3-5 and R3-6 disposition

Main supplied a second frozen repair and reported all 15 native tests passing. This
short recheck inspected only the two remaining guarantees and their affected operation
paths/tests. No builds, production edits or additional scope were introduced.

- **R3-5 repaired.** `transport_destination` now collects the owner's conditional
  member-config paths from the declared runtime global config, reads their routes by
  canonical URL, deduplicates them and requires one valid loopback destination.
  Clone, fetch and push apply that selected route through invocation-only Git config,
  including their expansion/destination preflight. Route selection therefore no longer
  depends on the caller already being inside the member or the new destination matching
  the original `gitdir` condition. The new
  `runtime_member_relay_clones_without_existing_destination_or_ambient_member` test
  performs a real HTTP Git clone and its immediate pinned-materialization fetch; it
  checks the cloned HEAD, absent Authorization headers, canonical persisted origin and
  absence of the private lease in repository config. Those assertions discriminate the
  original failure and the important persistence/credential regressions. The fixture
  recreates the current owner's includeIf structure without importing that owner upward.
- **R3-6 repaired.** Capture buffers now outlive the interruptible completion future.
  The supervisor returns retained bytes only after child cleanup, alongside the original
  interruption category. Push uses this captured-output path, accepts only complete
  newline-terminated porcelain records, and reports the confirmed ref as Partial while
  preserving Deadline/Cancelled/Io as appropriate. Ordinary operations keep their
  existing interruption propagation. The new
  `deadline_retains_confirmed_ref_after_real_push` test executes a real push, verifies
  the authoritative remote OID, holds a descendant pipe past the deadline and asserts
  Deadline + Partial, public target/confirmation/deadline text and no later descendant
  effect. This closes the inverse case left open by the lost-ack test.

No remaining concrete defect was found in these two repaired guarantees. Runtime test
execution results are main-agent evidence, not a duplicate independent test run here.
Deployment-level VM/relay validation remains the previously disclosed broader evidence
limit; the bounded implementation and regression assertions support closing R3-5/R3-6.
