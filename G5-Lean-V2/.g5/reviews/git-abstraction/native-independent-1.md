# Native Git increment — independent review 1

Read-only discovery against `/tmp/g5-native-review-snapshot/increment.patch`, SHA-256
`6cd1c7b023e60c79fd094fa951f36d2ad41317f183d23ca238f4ed41752fa3cd`.
The reviewer did not implement the increment, read peer reviews or author history,
or modify production/ontology files. Initial findings were saved before sharing.

Inputs: the original request, the pinned `.g5/ontology-plan-git-abstraction.md`
including its first ontology diff and Native execution migration section,
current governing blocks and footer entries, the entire native increment, and
the affected CLI push/materialization and runtime-environment consumers.
The saved ontology diff describes the design and initial slice; native migration
metadata still needs the main agent's post-review assessment. No new design
artifact or changed policy is proposed.

## Initial findings

### R1 — P1: inherited Git environment can select a different repository

**Location:** `backend/g5-scm/src/native/process.rs:56–70`, especially environment
inheritance and reliance on `-C`; affected operation
`backend/g5-scm/src/native/mod.rs:223–274`.

`Invocation::new` inherits `GIT_DIR`, `GIT_WORK_TREE` and other repository-selection
variables, only removing trace variables. `-C <request.repository>` does not
override `GIT_DIR`. Consequently, both `check_remote` and the actual push can use
an entirely different checkout while the typed request claims the requested
repository was pushed. The manifest/libgit2 owner continues reading the intended
checkout, so it can report/publicize a different head than native Git actually sent.

**Runtime counterexample:** two isolated checkouts shared an expected remote URL.
The intended checkout's `main` was `aae02913f888e7d608480e7c1f45f9f7b09e7d03`;
the other checkout's `main` was `f0c3f15f6d1f8a7226840524d3a75a9b927576b9`.
Calling the copied public `NativeGit::push_branch` with the intended repository
and inherited `GIT_DIR=<other>/.git` returned `OK Updated` and set the remote to
the *other* head. This is an actual ref effect, not a mock assertion.
Evidence: `/tmp/g5-native-review1-harness/ambient_repo_probe.py` and
`/var/folders/yf/bc92js5n0bjg59_gc7ztxqjr0000gn/T/g5-native-review1-ambient-cf2al0sf/result.json`.

The same missing environment policy leaves `GIT_CONFIG_PARAMETERS` above the
runner's appended `GIT_CONFIG_COUNT` settings. A loopback probe with an inherited
URL-specific `http.<expected-url>.followRedirects=true` made the actual runner
follow a redirect that the ordinary environment refused. In this tested
cross-port case, modern Git/curl removed Authorization, so this probe does **not**
establish cross-origin token disclosure. It does establish that the runner's
explicit redirect fence is overridable. Evidence:
`/tmp/g5-native-review1-harness/redirect_probe.py` and
`/var/folders/yf/bc92js5n0bjg59_gc7ztxqjr0000gn/T/g5-native-review1-redirect-th9f98ht/probe-True.json`.

**Governing:** `g5.architecture.git_ref_updates`, `git_native_runner`,
`git_credentials`, `git_locations`; the design explicitly requires a declared
CLI environment policy while preserving runtime PATH/loaders and isolated config.

**Proposed repair/check:** define and enforce that policy centrally. Preserve
runtime-owned PATH/loaders and legitimate isolated config selectors, but reject
or remove repository/operation overrides and the higher-priority parameter
channel. A regression should set a conflicting `GIT_DIR`, execute the actual
public push, and inspect both local repositories and the authoritative remote.
Also verify the redirect fence with inherited `GIT_CONFIG_PARAMETERS`.

### R2 — P1: a missing remote status is falsely reported as no ref effect

**Location:** `backend/g5-scm/src/native/mod.rs:276–290`; consumers
`backend/g5d-cli/src/push.rs:170–172,785–792`.

Every porcelain `!` record becomes `Rejected/Refused`. Native Git also uses `!`
for `[remote failure] (remote failed to report status)`, where the server may
already have accepted the ref. This must remain Unknown; the current result
explicitly says “this invocation made no Git changes” and managed pushes turn it
into a definite conflict/refusal.

**Runtime counterexample:** an isolated receive-pack proxy passed the real Git
protocol and real ref mutation, then omitted only the per-ref `ok` status packet
from the reply. Real Git emitted:

```text
!\trefs/heads/main:refs/heads/main\t[remote failure] (remote failed to report status)
```

The copied public runner returned `kind: Rejected, effect: Refused` with the
no-changes message. The remote actually advanced to the requested
`1d4ebf38e06dbb5c022de6fb15e02d3cabb42a7d`. Evidence:
`/tmp/g5-native-review1-harness/lost_status_probe.py` and
`/var/folders/yf/bc92js5n0bjg59_gc7ztxqjr0000gn/T/g5-native-review1-status-2yx8phqz/result.json`.
The proxy did not fabricate a refusal: it removed evidence of acceptance, which
is the exact lost-status case the outcome contract must preserve.

**Governing:** `g5.architecture.git_outcomes`, `git_ref_updates`, `git_retry`;
`g5.architecture.recovery`.

**Proposed repair/check:** parse the porcelain status category/reason as well as
the flag. Only known pre-update refusals establish Refused. Remote failure or
missing status remains Unknown and reaches callers with inspection-before-retry
guidance. Preserve the stdout status evidence even when stderr has a generic
“failed to push” summary. Add a real receive-pack/lost-status regression that
asserts the remote OID alongside the returned effect; retain genuine stale-lease
and rejected-before-write cases as discriminating controls.

### R3 — P2: formatting discards the error's authorized target

**Location:** `backend/g5-scm/src/native/mod.rs:43–55,70–74`.

`at_target` records a sanitized repository/ref/lease target, but `Display` prints
only operation and detail. Conversion to `G5Error::GitError` uses this formatter,
permanently losing the target before the CLI presents the error. Spawn, deadline,
destination and many remote errors therefore give no repository/ref for manual
inspection, especially significant for multi-repository materialization/push.

**Check:** compiling the unchanged copied native sources into an isolated test
harness reproduced the existing test failure at `native/tests.rs:333`:
`push_outcomes_preserve_confirmed_refs_and_separate_stale_proxy_from_auth_failure`
fails `error.to_string().contains("refs/heads/main")` on the first case.
The same defect is visible in the public lost-status probe: Debug contains the
target and Display does not contain the ref. No production source was changed.

**Governing:** `g5.architecture.git_outcomes` and the user's clear-error/manual-retry
constraint.

**Proposed repair/check:** include the already-sanitized target in the public
message (or retain typed target/effect in the existing error envelope). Verify
both Display and the actual `G5Error` conversion for missing binary/destination,
timeout, lease refusal, partial completion and unknown outcome, including
redaction. Do not weaken the existing assertion.

### R4 — P2: the new native guard rejects harmless identifier changes

**Location:** `backend/g5-api/tests/authoritative_workspace_reader_guard.rs:84–92`.

The guard identifies a native bypass by any identifier named `process`, `Command`
or `base64`, regardless of syntax/owner. It therefore treats a local variable
rename to `process` as unauthorized Git execution. The calibration only shows
one favorable NativeGit fixture; it does not test a harmless use of these names.

**Check:** an isolated executable built from the exact `typed_git_violations`
function reported a bypass for `fn push() { let process = 1; }`, while permitting
the same fixture with another variable name. It also rejected
`fn push() { let Command = 1; }`. Evidence: `/tmp/g5-native-review1-harness/guard.rs`
and `/tmp/g5-native-review1-harness/guard`.

**Governing:** `g5.architecture.git_guard`, `g5.quality.review.guards`.

**Proposed repair/check:** constrain checks to imports/qualified paths and actual
command construction/credential assembly, tracking aliases where needed. Keep
the current forbidden aliases and missing-operation fixture, then add harmless
identifier and unrelated-command fixtures. The guard should not claim runtime
mediation.

## Coverage and evidence ledger

| Commitment | Actual path and discriminating case | Result / limit |
|---|---|---|
| `git_execution`, `commands`, `design.abstraction`, `design.minimal-api`, `architecture.dependencies` | Public clone/fetch/set-remote/push requests; private `Invocation`/`Output`; CLI typed calls; Cargo features | Inspection: complete operations hide argv/environment/backend handles; downward dependencies preserved. No new persistence or generic command escape hatch. Full platform migration remains explicitly out of this increment. |
| `git_ref_updates`, `git_owner`, `git.network.cas`, `git.network.ref-scope` | Push single explicit destination; `--no-follow-tags`; explicit lease; accepted/no-op/refusal/unknown parsing; fetch configured tracking refs | Existing clone/fetch/push/lease test passes in isolated harness; R1 and R2 violate typed repository/effect meaning. Fetch retains the stated configured-refspec behavior. Server admission remains authoritative and unchanged. |
| `git_native_runner`, `runtime.workspace-cli`, `runtime.workspace-cli.context`, `runtime.bundle` | PATH binary resolution; inherited launcher PATH/loaders; child supervision; timeout includes drains; sync wrapper | Existing cancellation, descendant-pipe deadline and bounded-output tests pass in isolated harness. R1: allowed inherited Git environment is not defined/enforced. Full native-only Cargo/guest-runtime build not independently run. |
| `runtime.drain`, `runtime.materialization` | Changed CLI operations versus unchanged host/dispatcher owners | Inspection: no admission/drain owner changes or host retained-cache migration. Those contexts remain named future migration work; no claim that CLI context proves their guarantees. |
| `git_outcomes`, `git_retry`, `architecture.recovery` | Typed errors, push porcelain, CLI conversion and per-member completed list | No retry table/queue/worker/replay introduced. R2 loses uncertainty and R3 loses recovery target. Existing outer retry policy was inspected and is unchanged, rather than moved into the abstraction. |
| `git_credentials`, `security.credentials.platform-token`, `security.credentials.publication`, `security.credentials.redaction` | Only supported Token/None inputs; environment-only URL header; disabled helpers/prompts; redirect fence; error redaction | Normal HTTP redirect test and fake-child argv/redaction test pass. No provider token is newly selected or exposed. R1 shows inherited parameters override fence. Cross-origin token disclosure was **not** observed in the probe. |
| `git_locations` | Caller supplies checkout path and expected URL; native remote checks | No distinct storage layouts conflated; R1 breaks supplied repository identity through ambient override. Remote mismatch, multiple push URLs and configured rewrite refusal test passes. |
| `git_guard`, `quality.review.guards`, `quality.review.cases`, `design.enforcement` | Existing reader guards extended to two native CLI owners; alias/header and missing-operation fixtures | Required-operation presence and test exclusion inspected; actual guard function confirms R4. Public API/provider/global spawn enforcement remains documented migration work. |
| `git.exclusions` and existing checkpoint/flush/ref-publication owners | Implicated consumers and changed files | No new files or credentials persisted into checkouts by the native operation. Selected-file checkpoint/flush capture not changed. Existing global safe-directory helper and other backend accesses remain migration work; this review does not close them. |
| Provider clients/scopes/transport, authorized content reader, projection owner | Patch and implicated callers | No provider HTTP, reader authority/source selection or projection change in this increment. Initial resolution/diff slice was not re-reviewed beyond feature-gate interaction. |

The copied-source harness at `/tmp/g5-native-review1-harness` uses the unchanged
`native/{mod,process,tests}.rs` and a minimal equivalent `GitAuth` enum for Token,
None and unsupported-mode dispatch, linked to existing built dependencies with
`rustc`. It avoids Cargo and shared test resources while the main agent builds.
It is evidence for these native source paths, not a full crate/dependency build.
Seven of eight existing native tests passed (six non-HTTP plus the separately run
loopback test); the target-formatting test failed as recorded above. Additional
real-Git probes established R1/R2; the guard function was executed independently.
The loopback tests required and received sandbox escalation; only fake credentials
and isolated local HTTP listeners were used.

## Remaining verification and proposed metadata work

Main-agent coherent repair is required for R1–R4. Re-run full Cargo checks and CLI
integration regressions after repair, including native-only dependency closure;
this reviewer did not compete with the main build. The CLI end-to-end managed
multi-member and live smart-HTTP provider/guest execution were not run here.
Update `git_execution`, `git_ref_updates`, `git_native_runner`, `git_outcomes`,
`git_credentials`, `git_retry` and `git_guard` footer assessments after repairs,
retaining the remaining platform/provider migration gaps. No governing meaning
needs to be relaxed to accept these defects.
