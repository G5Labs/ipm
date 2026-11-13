# Host/native increment — independent discovery 3

Initial findings saved 2026-10-02 before sharing or reading other reviewers' findings. This is an independent, full-scope, read-only discovery against `HEAD d3869bca` plus the uncommitted inventory in `/tmp/g5-host-native-review/{tracked.patch,files.txt,snapshot.tar}`. No production, ontology, design or repository test files were edited. Probe files and mutable fixtures are under `/tmp/g5-host-review-3`.

Inputs: original request, the complete pinned design including `Ontology changes and governance` and `Host cache, checkout and runtime configuration`, current governing Git/runtime/durability/boundary/principle blocks and their metadata, actual changed files and material consumers. No author or peer findings were supplied. The ontology body has no additional uncommitted changes in this increment; its existing migration/evidence limitations remain accurate pending consolidation. Wider guest/provider migration and parked object confidentiality are separate work.

## Initial actionable findings

### R3-1 — P2: Equivalent cache URLs fail on warm refresh

**Code regression.** `backend/g5-scm/src/native/materialization.rs:45` calls the exact-string `check_remote` using the caller's `expected_url`, while `backend/g5d-foundation/src/workspace/repo_cache.rs:110` deliberately gives URLs with and without a trailing slash the same cache identity. A cache cloned from `/absolute/origin.git` therefore cannot be reused by `ensure_warm("/absolute/origin.git/")`; it returns Destination/Refused although both requests select the same source and cache. The previous refresh executed the stored origin and accepted this case. This can fail workspace startup after an otherwise harmless URL spelling change.

**Evidence:** the isolated compiled probe invokes the actual public `RepoCache::ensure_warm` and source SCM implementation. First call succeeds; the trailing-slash call returns `GitFetch` with “Git destination differs from the requested repository.” `/tmp/g5-host-review-3/probe.log` retains the result. Existing key canonicalization tests pass but do not exercise reuse through the new destination check.

**Governing obligation:** preserve cache identity and URL admission in the pinned host increment; `g5.architecture.git_locations`, `g5.architecture.compatibility`, and the design's preservation of existing materialization behavior.

**Suggested fix/regression:** reconcile destination admission with the cache owner's existing canonical identity while continuing to refuse a genuinely different origin or URL rewrite. Exercise cold-then-warm public cache operations in both trailing-slash orders, and a malicious changed-origin control. Account for existing caches rather than only normalizing newly created origins.

### R3-2 — P2: Host wrappers reintroduce redacted URL credentials

**Migrated-path conformance defect, inherited wrapper behavior.** `repo_cache.rs:774` and `:756` populate `WorkspaceError::{GitClone,GitFetch}.repo_url` with the raw input after SCM has sanitized its own target. Those variants display the raw URL. An admitted HTTPS input with userinfo is refused by SCM, but formatting the public host error still exposes its password. The SCM half of the same error correctly shows `[REDACTED:https_basic_auth]`.

**Evidence:** the public cache probe with a synthetic `https://user:review-secret@example.invalid/repo.git` prints the fake credential in the outer error prefix and a redacted inner error. No network request is needed because SCM refuses before execution. This is not a claim that the old wrapper was safe; it is a concrete failure of the credential/error guarantee on the operation being migrated.

**Governing obligation:** `g5.architecture.git_credentials`, `g5.architecture.git_outcomes`, `g5.security.credentials.redaction`; acceptance requires config/URLs/errors to contain no secrets.

**Suggested fix/regression:** preserve the typed sanitized native error or sanitize every user-visible URL field before constructing the host wrapper. Verify both Display and Debug/error-chain surfaces on malformed credential-bearing URLs and actual failed authenticated requests, including encoded credentials. Preserve the existing bounded 403 classifier without adding retry state or replay.

### R3-3 — P2: Newly added sibling host files evade the native-spawn guard

**Guard defect.** `backend/g5-api/tests/authoritative_workspace_reader_guard.rs:308` recognizes native ownership only for named files. The newly scanned `g5d-host`, `g5d-foundation`, `g5d-runtime-core` and `g5d-worker` trees get blanket `git2` checks, but a new sibling file containing `tokio::process::Command::new("git")` passes because it is neither `native_owner` nor `mixed_tools`. Moving the same spawn into the listed `workspace_vm.rs` is rejected. The source scan consequently does not stop new native bypasses in the migrated host boundary.

**Evidence:** an isolated executable compiled from the exact guard helper functions returns `[]` for `g5d-host/new_git.rs`, `g5d-foundation/workspace/new_git.rs`, `g5d-runtime-core/new_git.rs`, and `g5d-worker/new_git.rs`. It rejects the same source under `g5d-host/workspace_vm.rs`, and allows the legitimate `cp` restore control. See `/tmp/g5-host-review-3/guard-probe.log`. The unmodified 18-test source-contract suite passes, demonstrating the missing calibration.

**Governing obligation:** `g5.architecture.git_guard`, `g5.design.enforcement`, and the design requirement that migration exceptions name existing operations and not admit new bypasses.

**Suggested fix/regression:** apply native-construction ownership checks across the relevant migrated crates, with explicit operation-level exceptions for legitimate non-Git process owners and remaining migration work. Add new-file fixtures for literal and configured Git programs, alongside a harmless legitimate non-Git operation.

## Coverage and verification ledger

| Commitment | Actual path and discriminating evidence | Result / limit |
|---|---|---|
| `git_execution`, `git_native_runner`, `dependencies`, minimal API and information hiding | New native materialization/restore/config operations; host/runtime consumers and manifests; `cargo tree -p g5d-host -p g5d-worker -p g5d-runtime-core -p g5d-foundation --edges normal,build --offline` contains no git2/libgit entries | Inspected; native dependency closure confirmed for selected package graph. No full host build was rerun. |
| `git_locations`, compatibility | Cache-key canonicalization, owner URL admission, stored origin checking; actual public cache probe | R3-1. Workspace routing code is moved without behavior changes; old default-feature exports remain available. |
| `git_ref_updates`, local materialization | Full-branch cache clone/refresh/prune and exact checkout test, invalid non-full OID refusal | Existing native test passes. Scope is local heads/tracking/HEAD, not mapped-branch authority. |
| `git_native_runner`, cache custody | In-memory owner task retains both locks, cancellation, wait_idle and 480s budget below 540s stale age; custom public-cache blocked fake transport | Actual child is started, cache lock exists, caller is aborted, and lock disappears only after direct child no longer exists. Original real-network cancellation test could not bind loopback in this sandbox. Cleanup under uninterruptible OS I/O and process loss is not proved. |
| `git_credentials`, `git_outcomes`, security redaction | Invocation-only URL-scoped credential test and public refusal probe | Native credential argv/environment test passes; R3-2 at host wrapper. Real host HTTP delivery/redirect/TLS and encrypted-key SSH handshake were not exercised here. |
| `git_retry` and existing recovery ownership | `refresh_under_lock` still owns eight-attempt bounded 403 visibility recovery; SCM executes once; lock supervision adds no retry persistence | Preserved by source inspection. No new tables, queues, worker replay or tracking found. Actual 403 recovery against API not exercised. |
| `g5.git.restore`, cleanup/proof, `g5.runtime.materialization` | Host validates receipt vector and dirty equality, chooses detached mode for sequencers, replaces retained root before SCM; SCM checks receipt leaves/commit types, fetches local namespaces, restores exact branch/HEAD and binary patch | Two existing host restore tests pass: receipt/dirty/sequencer/null restore and internal/provider tracking refs. Missing/wrong receipt, attached foreign HEAD, binary payload beyond diagnostic limit, failure after confirmed steps and hostile retained-root behavior inspected but not independently fault-injected. No new merge of fetched tip found. |
| `git_outcomes` | Native materialization tracks config completion; restore retains completed fetch/cleanup/branch stages, optional parent error stays informational | Source-inspected. Partial/unknown runtime fault injection remains an evidence gap; existing happy-path tests cannot establish it. |
| Runtime exact-root trust and compatibility | Runtime selects paths/cuts and re-exports SCM serialization/exclusive-create/replace/ensure | Mechanically compared old implementation body to new: identical. All four runtime trust-file tests pass, including planted symlink replacement. |
| `git_guard`, enforcement | Exact copied 18-test suite plus sibling-file negative probe and legitimate cp control | Existing suite passes; R3-3. Guards remain structural, not dynamic type/macro proof. |

## Check receipts and boundaries

- SCM native materialization: **2 passed, 1 failed**. The failing non-UTF-8 destination test uses Apple Git 2.50.1 and returns `Illegal byte sequence`. A separate direct `git clone --bare` probe fails identically with inherited environment and with the new cleared environment. This is an environment/backend capability limitation here, not evidence of a newly introduced environment regression. No success claim for non-UTF-8 paths on this host.
- Foundation workspace tests: **34 passed, 1 blocked** by sandbox `PermissionDenied` binding `127.0.0.1`. The failed named test is `dropped_ensure_warm_releases_cross_process_lock_after_cleanup`; the isolated fake-transport cancellation probe described above passes without a network bind.
- Host restore tests: **2 passed**. Runtime task Git config: **4 passed**. Source guards: **18 passed** before the independently added negative sibling-file probe.
- Test binaries reused from the current checkout: `g5_scm-554aebe82f3c130a`, `g5d_foundation-bc85e7f51a9220eb`, `g5d_host-5d1327d0755d238c`, `g5d_runtime_core-8225599b3724de07`. A native no-run Cargo invocation reported the SCM artifact fresh. All test temporary resources used `/tmp/g5-host-review-3`; standalone probes compiled the actual current source against cached dependencies. No shared fixture or production file was mutated.
- Environmental compatibility beyond inspected configured PATH/HOME/system/global-config/loaders remains unverified. In particular, the explicit host environment does not carry XDG config location, HTTP proxy or custom SSH-command variables that the previous inherited process environment could consume; supported deployment expectations should be checked before claiming complete host-context equivalence.
- No full VM boot, Linux process cleanup, KVM performance comparison, real API credential rotation, or production network transport was run. These are evidence limits, not asserted code defects.

Implementation status and verification status are separate: this review finds three actionable boundary/compatibility defects; the inspected restore/config relocation is coherent, with the runtime limits above. No repairs were applied by this reviewer.

## Bounded repair recheck

Recheck requested after main saved its initial consolidation and applied the coherent repair. The original findings above are preserved. This pass covers R3-1/H1, R3-2/H2, R3-3/H3 and directly affected interactions; it is not another full-scope discovery. Only this report was edited. New probes live under `/tmp/g5-host-review-3/recheck` and compile the actual repaired source against cached dependencies.

| Initial finding | Recheck evidence | Disposition |
|---|---|---|
| R3-1 / H1 | Public `RepoCache::ensure_warm` succeeds for both cold base → warm trailing slash and cold trailing slash → warm base. For each order, changing origin to a different source returns typed Destination/Refused; restoring origin and adding an `insteadOf` rewrite to a different source is refused. The repair reads the actual expanded origin, admits only the cache owner's existing canonical spelling equivalence, validates it, checks further expansion, then uses that actual admitted URL for credential binding. | **Closed for the reproduced defect.** Actual public-path controls pass; credential binding to the selected expanded origin is source-inspected. |
| R3-2 / H2 | Public cache refusal probes cover plain userinfo, encoded userinfo, query token, and admission-rejected HTTP userinfo with a selected-token canary. Display, Debug, mapped `G5Error` Display and Debug expose none of the canaries. Host clone/fetch now preserve `WorkspaceError::GitExecution` directly; admission refusal no longer retains raw URL. Source inspection confirms the shared URL scrubber removes userinfo/query/fragment URL diagnostics, and 403 classification remains restricted to Exit from `clone bare cache` or `refresh bare cache`. | **Closed for the reproduced defect.** Typed effect/kind survives the wrapper and all public canary assertions pass. Real HTTP failure/redirect delivery remains covered by main's separate checks, not claimed by this probe. |
| R3-3 / H3 | The exact current source-contract suite plus one independent calibration test passes **22/22**. Additional calibration rejects literal Git, configured programs and chained module/type aliases in all four originally bypassed sibling paths; accepts the legitimate `cp` operation; and confirms a two-alias recursive module-prefix expansion terminates. Current suite additionally checks narrow generic-supervisor and test-fixture exceptions and keeps fixture exports feature-gated. | **Closed for the reproduced defect.** Whole host/runtime crate constructor scanning closes the named sibling-file gap. The guard remains a structural check with its documented type/macro limitations. |

Results are retained in `recheck/probe.log` and `recheck/guard.log`. The report does not substitute main's supplied foundation/native test totals for independently executed evidence.

The adjacent context repair explicitly carries XDG and selected system-config locations, supported SSH command/program selection, proxy values and lowercase/uppercase precedence through the host environment. Its transport Debug representation is redacted. This resolves the omission noted in the initial evidence-gap paragraph by source inspection; deployment combinations remain unexecuted here. Explicit host Git-directory anchoring and restore repository-format preflight are present before the affected operations, preserving refusal-before-fetch ordering; H4/H5's independent runtime disposition belongs to main's checks. No new tracking, retry persistence or automatic replay was added by the examined repairs.

No additional actionable defect was found in this bounded recheck. The three initial findings are closed with the evidence above; the broader runtime/deployment limitations in the initial report remain limits unless separately established by main.
