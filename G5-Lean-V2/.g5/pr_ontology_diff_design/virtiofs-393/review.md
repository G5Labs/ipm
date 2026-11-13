# Proposal review — 2026-10-13

Raw experiment artifacts and the duplicated chat-intent request were removed from this branch at the user's request. Historical results and qualification limits below remain as summaries.

## Repo-seed negotiation follow-up — 2026-10-16

Full-history QA identified a dirty-resume regression: a detached seed without
named refs did not advertise its existing history during Git fetch. The first
negotiation-tip fix passed file-transport tests but failed real HTTP/VM recovery;
the protocol-v0 HTTP helper did not forward that option. Bootstrap now creates a
fresh temporary ref for its verified HEAD during fetch and deletes it before
returning, including ordinary fetch failures. Aborted boots remain disposable.
The seed format, authoritative receipt checks and generic path are unchanged.

The native regression now exercises API-style stateless HTTP plus file protocols
0/2, seeded and empty members, an older seed, exact receipt HEAD, modifications,
deletion and binary content. It rejects the previous negotiation-tip fix over
HTTP and passes with the temporary ref; cleanup is checked after success and a
missing remote ref. The full local SCM run had one existing fixture startup timeout;
that test passed alone and all 233 tests passed with bounded test concurrency.
Scoped Clippy also passes. VM qualification
and current-head formal CI remain in progress; earlier results do not qualify this
amendment. The follow-up report and [PR #516](https://github.com/G5Labs/G5-Lean/pull/516)
record those results separately. Raw evidence remains outside the PR.

Reviewed main: `26605ef842a38d27221e1f9961a6b564be5784b6`. Local checkout: `7dafa9d3e73d7015e8263875b757178dbf762961`. This assessment covers research, experimental evidence and the proposed design, not an implemented runtime change.

## Independent inspection

The `code_audit` subagent independently inspected relevant latest-main files through the GitHub connector, challenged the storage proposal and reviewed the final design, candidate ontology patch and investigation report. Final result: no remaining blocking design contradictions identified. The `upstream_research` subagent independently checked pinned libkrun sources and kernel filesystem documentation. The `local_experiments` subagent executed and self-checked the isolated HVF samples; the root agent inspected the harness/evidence and independently reproduced the Cargo symlink-clean behavior.

## Findings resolved in the same design

| Finding | Resolution |
|---|---|
| Local branch's retained-root and Stop contracts are stale | Design cites immutable current-main contracts; proposed ontology patch targets that main file and is not applied to unrelated local ontology |
| Global `CARGO_TARGET_DIR` can change warmed sccache identities | Explicit Cargo output flags in a selected build profile, with real image/fleet hit verification |
| Symlink target loses its link on Cargo clean | Actual local probe retained; automatic symlink relocation rejected |
| Bind-mount target may break whole-directory deletion | Fresh guest `rm -rf` produced `Resource busy`; automatic target bind rejected; target is an ordinary subdirectory under scratch |
| Compile-to-scratch can leave consumers running old `target/` binaries | Build/test/clippy/clean and selected output consumers share one profile path; explicit caller/project settings stay authoritative |
| Current 64 GiB scratch can be smaller than reported target alone | Combined Docker/build capacity and inode measurements, host overrides/cap and full-slot load tests required before enabling |
| Scratch-size changes on a busy host violate published sizing | Drained/replacement host rollout, current sizing publication/custody contracts included |
| Same-disk no-fsync evidence overstates certainty about physical storage | Report distinguishes omitted explicit fsync from possible storage/writeback costs and whole-file time from individual request latency |

## Executed evidence and limits

- Local experiment: 60 successful samples, 420 phase timings, three repetitions per condition. Direct libkrun scratch guests, not product E2E.
- Cargo symlink cleanup: Cargo 1.96.1 returned success, removed the target link and left scratch data.
- Linux bind cleanup: contents deleted but mountpoint deletion failed; this is not a Cargo test.
- Document validation checks local links, candidate block/metadata pairing, exact patch application to the inspected main base, and agreement between retained samples and summarized measurements.

The initial review above preceded QA access. The subsequent authorized QA measurements are recorded below. Product integration, cache-policy changes and worker-pool validation remain unexecuted; this task produces a researched proposal, not release qualification.

G5 Local state was inspected; its CLI is not available on PATH. No helper approval, baseline advance, implementation review receipt, Git staging/commit, external comment or deployment was recorded. The candidate ontology diff is an ordinary reviewable artifact against the pinned main snapshot.

## Cache-system follow-up

The code_audit subagent independently traced the actual latest-main cache routes, guest launcher, producer and retained October 10/12 QA observations. Root inspected the pinned sccache hit/output extraction code. The investigation now separates implemented OCI/sccache facades from deferred registry/frontend layers, remote read-only backend from a nonexistent private write-through L1, and guest-direct Cargo from Docker builds already on block scratch. Stage 0 now explicitly keeps the current cache enabled and matched during storage comparisons; incremental compilation is controlled separately. These are measurement refinements, not additional runtime commitments; the candidate ontology patch is unchanged. Existing QA timings are attributed historical evidence, not new measurements or proof of filesystem causality.


## Authorized QA follow-up and revised priority

Root ran 30 small-file samples and three Cargo A/B pairs in the user-named `preview-test` guest. The local_experiments agent independently reviewed the executed micro harness, validated all 180 phase timings and computed ranges. Root confirmed normal Python assertions, successful bounded completion, source hashes, and exact agreement of remote/local canonical micro and Cargo rows. The upstream_research agent challenged causal interpretations: the comparison includes the whole guest-visible filesystem paths; warm stat already hits cache, reopened-read behavior is consistent with but does not trace close-to-open invalidation, and the concurrency curve alone does not prove one bottleneck.

Cache-enabled fresh g5-contracts test-build medians were 52.85 s on virtio-fs and 48.35 s on ext4, an 8.5% reduction. All six fresh builds had identical 39 hits/25 misses; all twelve builds/no-ops succeeded with validated artifact existence and unchanged Cargo.lock. Online registry lookup remained in both arms because QA registry permissions prevented index persistence. Thus the target move does not meet the proposed 20% workflow threshold in this package pilot; source and toolchain reads remain on virtio-fs. It is retained as a candidate pending output-heavy workflow evidence.

The code_audit agent found and reproduced a separate latest-main OCI directory-mode merge defect using byte-verified exact function bodies. Existing directories ignored explicit upper-layer mode changes; new-directory/file controls passed. This fits QA's registry ancestors remaining root:root 0755 despite image intent, but is not a proven deployed-image RCA. The user subsequently clarified that performance is the objective; this defect is kept separate rather than prioritized ahead of performance. Any future fix still requires image/layer/template verification, explicit-directory provenance, inherited-parent handling, privilege/whiteout/symlink preservation, and appropriate cache invalidation. No runtime chmod or product code change was made.

The revised design and candidate ontology metadata distinguish this pilot from release acceptance. The permission correction restores existing image intent; it does not add a new shared cache policy.

## Performance priority correction

Following the user's explicit performance requirement, upstream_research independently
reviewed the proposed three-layout diagnostic and input-cache direction. Its key
conditions are now in the plan: identical source content and resource/cache policy,
actual cache-eligibility verification across changed paths, separately measured
snapshot setup cost, interleaved repetitions and representative real workflows.
The source-snapshot experiment does not validate a production replacement for durable
`/work`; existing registry/toolchain reads remain outside that relocation. The
candidate ontology patch needs no new commitment for this diagnostic. No new
performance run or runtime implementation occurred in this amendment.

At the user's explicit request, the separate OCI defect was published as
[#501](https://github.com/G5Labs/G5-Lean/issues/501), including reproduction,
causal limits and acceptance criteria. It is not a claimed performance fix.


## Same-guest layout continuation, 2026-10-13

The authorized continuation completed A2 (normal layout), C1 (tmp only on ext4), B1 (source plus output on ext4), R1 (registry only on ext4), and return baseline A3. All ten formal Cargo commands succeeded. Independent raw review reconciled tool/environment identity, 528-artifact graphs, 607 hits/167 misses, no-op counters, actual exports, registry manifests and daemon namespace proofs. A1 is an excluded pilot because its registry shared the work export; it does not enter comparisons.

Fresh times were 464.732 / 461.811 / 407.362 / 433.386 / 462.447 seconds. Return-baseline drift was only 0.49%, but treatment repeat variance remains unmeasured. Source-plus-output placement saved about 12%; it does not establish target-only benefit. Registry copying consumed nearly all its first-run saving. No candidate met the proposed 20% real-workflow threshold.

The reviewed next proposal is a rootfs-only, opt-in Auto/Always retention prototype after complete writer/reader/alias audit. Keep work export, TTLs, writeback and workers fixed; the per-export selector is new, unimplemented capability. Neither placement results nor the earlier whole-runtime host/VM ratio establish its efficacy. The prior small-file speed gate was replaced with relevant operation diagnostics and the real-workflow gate, because repeated-read retention need not accelerate unique creates. Output scratch remains conditional, and a worker pool is separately justified by queue traces.

Post-run source verification exactly matched the preparation digest for all 2,496 tracked paths. The first cleanup validation stopped before deletion because uid 1000 could not traverse Docker's private parent; a private read-only verification bind resolved that without changing permissions. Guest scratch, host copies/uploads, dedicated processes and mounts were cleaned; QA was restored to Stopped. No product code, applied ontology or deployment was changed.


## Direct issue #393 validation and revised performance priority

The private QA prototype now measured the policy change rather than inferring it from placement. Canonical downloaded archive SHA-256 is `ce9db782884f7ad8a06ba3e9208b5fd7a2547d238b9aec8ed4bed08c4e68ad7c`; all 577 members and indexed hashes passed extraction verification. Root and independent workflow review checked all six configurations against source/tool/profile/controlled-edit hashes, 528-artifact graphs and every numeric cache counter delta. All 24 Cargo stages succeeded; the original A1 postcheck failure and bounded untimed completion remain explicit. The R2 SSH transport disconnect did not interrupt the remote successful run or leave owned helpers.

Two A/R screening pairs measured 31.68%/32.05% lower fresh-check elapsed time for root-only Always, with +0.69% baseline and +0.14% candidate repeat drift. A2 and R2 both report 0.008s average cache-hit reads, yet retain the application-level difference. This is evidence for the tested retention policy, not all bundled issue knobs or a complete time decomposition. Work-only retention and target-only ext4 had smaller single-observation fresh benefits; output placement remains a complementary conditional design.

The candidate ontology and embedded diff are synchronized and apply cleanly to the worker ontology that is unchanged between reviewed main `26605ef` and the `28a6ed` recheck. Independent review clarified that full host/alias/lifecycle coherence gates production integration/canary, not the already authorized isolated experiment. No product code or applied ontology was changed; no baseline/approval was advanced. Five alternating repetitions, broader workflows, full coherence and later TTL/writeback/worker-pool evaluation remain open.


## Complete-runtime and full-block follow-up — final evidence, 2026-10-13

The user's follow-ups broadened diagnosis beyond virtio-fs and then explicitly requested moving the complete workload onto block storage. Static same-ELF CPU trials found only about 2–3% guest overhead, while process creation and initial memory backing had specific larger costs. Whole balloon unbind improved selected memory/rustc microdiagnostics but reduced the real Cargo pair only 0.669%, so it is not the selected performance lane.

Matched whole-workload chroot A/B/B/A fresh checks completed at A 485.574546/493.381370 seconds versus B 184.573034/191.262010 seconds. Median elapsed fell 61.6086%, saving 301.5604 seconds; no-op medians fell 86.1923%. A distinct private launcher with only NONE→FULL changed completed at 189.979019 seconds, with FLUSH actually negotiated. Its single result is not pooled into the two B observations and does not establish crash durability.

Root verified the downloaded archive (`8c42af25b672fd9bb4258934718d3f871dc87bbd67c5db69bca0bf2338d84299`), and independent review rechecked all 246 regular members/245 indexed files, all five mounts/exports/provenance records, source/tool/profile/command/528-artifact/586-unit equivalence and every numeric cache counter. All ten Cargo phases succeeded. The failed smoke and precise unused-directory preflight correction are retained. Separate resource review confirmed much lower system CPU and native dependency unit time; it also retained host reclaim, small B2/A2 swap-out, higher sampled block VMM RSS, missing guest metrics and clock/window limitations. No result is presented as a pure syscall cost or production block-root boot qualification.

The primary proposal and candidate ontology were revised in the same plan: a disposable complete block workload root preserving current fresh materialization, guest Git capture/push/complete receipt and exit-proven cleanup. Rootfs Always remains a separate compatibility alternative. The authoring review applied the unapplied candidate patch to exact main 28a6ed in a temporary copy and verified the embedded diff, stable IDs, metadata and anchors. Actual ontology/product code remain unchanged. Full Start latency, runtime bundle/capability integration, broader workflows, lifecycle failure/recovery and full-slot capacity remain future implementation/qualification, not claimed executed coverage.

All raw evidence was downloaded under the user's explicit consent; no source/rootfs/images/private environment were exported. Cleanup verified no live owned process/mount before deleting the owned base and exact uploads. Installed libkrun and firmware hashes remained unchanged. QA UI confirmed `preview-test` Stopped.


## Implementation conformance review — 2026-10-14 UTC

This is the implementing agent's review, not an independent review. The current code uses the existing owned OCI clone, published scratch budget, block device, host custody/exit proof and complete Git receipt. No guest-image reuse, block seed cache, second disk, database sizing field or recovery authority was introduced. The review checked actual consumers: relative/self-contained Git metadata; guest-routed Files/watchers; independent host-enforced read-only runtime; Docker's exact-mount query; normal cancellation process groups and helper parent-death behavior; Ready projection against the current row's wall-clock lease; and first-Ready/renewal ordering. The Docker mount compatibility fix preserves prepopulated data on the same ext4 device. The two new E2E corrections follow the existing relative-root API and terminal heartbeat contract without weakening filesystem or neighbor-isolation assertions.

Current Linux, development-image lifecycle, Docker, failure and build-output evidence is recorded in the same [plan](ontology-plan.md#current-linux-qualification--2026-10-14-utc). Runtime code is `f001fcbb`; test fixes are `bfffe6e7`. Performance and browser continuation remain in progress. The loaded two-slot development-image start approached the configured readiness bound; fleet cache coverage, full-slot concurrent build capacity and sparse host-filesystem exhaustion remain unqualified. Original frontend/browser failures and retry-passed tests are retained. These limits prevent claiming completed release qualification; the legacy default remains unchanged. No G5 Local approval or baseline advance was recorded.

The resumed browser run found a real bootstrap compatibility defect: `umask 077` escaped secret staging and reached workload processes. Project Doc Save reported `PUSH_FAILED` with a loose Git object unreadable to the ordinary publisher. Commit `2fd8cfce` captures and restores the incoming mask before image hooks/dispatch. The same actual KVM probe fails against `f001fcbb` with observed `0077`, and passes two fresh boots against the corrected binary, including ordinary-user reads of root-created files and the complete read-only-runtime attack checks. Linux release build and focused preparation tests passed; the rebuilt two-VM lifecycle, Docker, browser Save/Fork and provider recovery journeys are running. Their results, rather than the earlier candidate's passes, will qualify the correction.

A bounded sparse outer-filesystem exhaustion experiment also passed on `f001fcbb`: a private 6 GiB host filesystem backed a 2 GiB guest disk; reserving all but about 16 MiB of available outer space caused the guest write to report an I/O error. Cleanup followed process-group exit proof. No shared host filesystem was filled, and the guest-written image was never mounted by host tools. This tests error/cleanup behavior, not full-slot production capacity or successful unsaved-data recovery after host ENOSPC.

The corrected candidate has now passed all 24 core browser journeys and all 21 provider continuity journeys without retries/skips, plus the rebuilt two-VM development-image lifecycle, Docker and five failure probes. The actual old/new KVM creation-mask comparison and the previously failing Project Doc Save establish the permission correction. The 265-file integration archive is verified locally. Original frontend failures, watchdog interruptions and retry-passed cases remain recorded; no full green CI or production rollout is claimed. Final performance and concurrent build qualification are still pending.

The final runtime comparison passed ten interleaved arms (five per layout) and all 30 Cargo phases. A strict local reanalysis of the verified raw archive reproduced the summary byte for byte: source, tools, environment, commands, each 497-artifact graph and numeric compiler/cache-service counters match. Fresh check median fell 57.46% (326.363 → 138.821 s); population/boot plus three checks fell 42.90% (340.198 → 194.249 s). The latter excludes API materialization, initial clone and final teardown. Two concurrent block VMs also completed all three phases; this is the tested two-slot configuration, not production fleet capacity. Per-VM image population costs approximately 49 s and initially allocates 9.2 GB. Template-cache coverage is proven; fleet-cache coverage remains unexecuted.

The final recovery repeat passed five real API/guest journeys, including abrupt host loss and receipt restoration with stale credential/Preview rejection. Its initial host-loss attempt failed on the test runner's omitted signing-key variable; the retry used the existing fixture secret volume internally. This fixture failure is retained, and the repository qualification runner now checks the signer before starting a full suite. Both filesystem layouts reproduced the frontend's default V8-heap exhaustion; the explicit 4 GiB heap control then passed the production build, compiled preview and all 4,672 frontend tests without skips. The original CI alignment failure remains preserved; the later pass does not establish its cause.

Final review outcome: the opt-in implementation conforms to the chosen disposable-storage/Git-receipt contract at the recorded scope. The 356-file final archive passed member-by-member verification and follow-up analysis; two concurrent VMs match the formal source/tools/artifacts/cache counts, five recovery cases pass, and the explicit-heap frontend suite passes all 4,672 tests. The original failed CI receipt remains unchanged. Fleet-cache integration, production slot/capacity sizing, cold image-pull timing and other images remain rollout qualifications. The owned QA VM and boot disk were deleted after evidence verification. No independent-review claim, human approval record or baseline advance was added.

## Pristine-template startup conformance review — 2026-10-14

The implementing agent reviewed the startup amendment against fresh materialization, complete receipt recovery, template cache trust, runtime isolation and published slot capacity. The configured resolved OCI identity is passed from the same extraction to each owned root; a hit clones only a matching, complete, non-writable pristine artifact. Misses retain full population. Prewarm holds the cross-process publication lock, reclaims abandoned or obsolete cache before admission, validates/syncs before publication and keeps one bounded variant. Helper parent-death behavior and inherited locks prevent a killed builder from racing replacement cleanup. Git restoration remains on the existing host path; bootstrap imports its archive before hooks/readiness and removes the handoff. No guest disk is retained or mounted by the host.

Review and qualification fixed three compatibility/capacity findings: enforce full-slot admission even without a configured image; reclaim both abandoned staging and ineligible old artifacts before replacement admission; omit optional POSIX ACL tar operations unsupported by the pinned guest kernel. Final candidate `4a8e2f60` passes Linux preparation/OCI/host/CLI checks, real template publication/private clone/crash tests, six direct startup observations, four matched serial Cargo runs and two concurrent builds followed by simultaneous ordinary-user ENOSPC and recovery. All raw checkpoint hashes and three local reanalyses agree. This is root-agent review, not independent review. Ten actual KVM/host failure and isolation probes now pass after correcting the retained test-container identity/cgroup/capacity fixtures. Final startup-candidate product qualification passes all 13 stages: two-VM Files/PTY/private-write/runtime-isolation and receipt Stop/Start checks, Docker overlay2, API-restart/failure recovery, oversized Stop, terminal reconnect/ownership transfer, 24 core browser journeys, 21 provider-continuity journeys, and abrupt host-loss receipt restoration with stale access fencing. Browser suites use one worker, zero retries and zero skipped/flaky/unexpected results. Provider CLIs are deterministic CI doubles; API, host, KVM, Git, PTY, browser and Preview are real. The 173-file final evidence archive is hash-verified locally, all three direct analyses reproduce byte-for-byte, and the actual host/delivered runtime hashes match the candidate. The separate N2 product fixture measured public API Resume-to-running medians of 54.004 seconds full population and 5.665 seconds template, two ABBA observations per path with the same two-member Git HEAD vector and preparation-path assertions. Its 250-ms polling timer is not comparable to the C3 direct timer. The previous runtime's results remain historical, separate evidence.

Final startup review retains the generic-template decision: the tested startup benefit is 83.4%, while the small matched fresh-build cohort changes by +1.05%. No additional recovery authority or repository cache was introduced. The real API fixture also confirmed that an image-policy mismatch takes full-population fallback; the corrected pinned-image comparison is retained separately. Runtime E2E CI passed; the Rust all-features fixture correction changes tests only and passes local compile verification. Other images, larger slot counts, fleet cache, cold OCI pulls and production rollout remain unqualified. This is the implementing agent's review, not an independent review.

The 173-file final startup archive and actual runtime hashes are verified locally. The two owned test instances, both boot disks and transfer snapshot are removed with Not Found verification. Rust CI rerun remains blocked by the current account's missing `cloudbuild.workerpools.use` permission; no IAM or pool configuration was changed and no green status was synthesized.


## Cloud Hypervisor implementation review — 2026-10-15

This is the implementing agent's review of the confirmed replacement in PR #516.
Earlier sections qualify earlier runtimes only. The inventory begins with the final
plan's changed ontology (`microvm_backend`, `build_scratch`, `workload_repo_seed`,
`deployment.workload_templates`) and the unchanged materialization, receipt, runtime
bundle, capacity and custody commitments. Raw results remain outside ontology.

| Commitment | Actual path | Counterexample/check | Current assessment |
|---|---|---|---|
| Independent trusted bootstrap/runtime; complete private workload | `g5d-isolation/isolation/{cloud_hypervisor,workload_root}.rs`, runtime/init builders | Image loader/NSS execution refused; read-only runtime; two private guests; native self-reexec | Real KVM gates and restricted service boundary pass |
| Exact member/receipt restore, fresh disposable roots | Shared `g5-scm/native/materialization.rs`, guest bootstrap, existing host materialization | Two-member seed recovery; live/stopped Files, browser Save, Git push, Fork, Stop/Start | Real platform flows and atomic whole-service-loss receipt restoration/fencing pass |
| Scoped optional seeds, no first-Start dependency | Host `source_preparation` and API host source grant | Membership/environment/read-authority refusal and revocation; exact restore | API/KVM and background preparation pass; interleaved generic/seeded source and API startup measured, release-profile API startup/recovery pass |
| Supervised passt/VMM; exit proof before slot/root release | VM cgroup owner, host custody and reaper | Forced VMM exit with a live neighbor; zero capabilities, seccomp, AppArmor, empty-root passt; two-VM network negative control | Sandbox/neighbor checks and whole-service loss/restart pass |
| Companion delivery and bounded capacity | OCI workload template/artifact owner, image build pipeline | Registry-fixture roundtrip; incompatible/corrupt inputs and capacity checks | Fixture and concurrent two-slot capacity/recovery checks pass; published-registry delivery and current-head CI remain open |
| Restricted self-hosted supervisor | `install-worker-service.py`, trusted worker OCI staging | Real systemd parsing with space/percent paths, negative quoted-scalar and nonexecutable controls; actual restricted service | Three parser tests, actual two-VM launch, private writes and whole-service recovery pass |
| Removal of obsolete runtime mechanisms; retained local process mode | Runtime, CLI classification, ownership handoff and CI configuration | Linux root ownership checks, CLI errors, architecture contract, CI step dependencies | Focused Linux/portable checks and actual cgroup/API recovery pass after custody repair |

Initial findings retained: native guest execution through the ELF loader broke
self-reexec; direct execution with a pinned guest interpreter fixes real chat/CLI
and module reap checks. Self-hosted units quoted scalar paths and complete bind
pairs incorrectly, preventing startup; separate scalar/list parsing fixes actual
registration and discriminating Linux parser tests. Offline systemd verification
also ignores RootDirectory when resolving ExecStart; the verification copy resolves
the trusted-root executable while the installed service keeps its chroot path.
CI exposed the retired KVM-step/TSI-alias expectations, an invalid Boolean apt-helper
input and a collapsible-if lint. Repairs preserve lane/API/runtime isolation and
focused Linux checks pass, as do local Clippy and self-hosted bundle contracts.
A fixture correction alone is not product-runtime evidence.

A bounded pass from the current governing text retained these open requirements:
first-Start cancellation of optional preparation, exact grant refresh before seed
publication/attachment, full-slot/staging headroom, guest flush behavior, custody
after sidecar/host loss, actual companion delivery and matched final-code performance.
Existing scoped component evidence is reused; pending final integrations remain gaps.

The remaining `isolation_backends` legacy VM tests still invoked `krunvm` and
asserted direct host visibility of guest writes. They are removed, together with
their CI exclusion; the 13 process/selection tests pass. The explicit KVM gate,
owned-root GC and API private-disk/recovery scenarios own the replacement behavior.

Later service integration findings: a restrictive installer umask made the canonical
rootfs parent untraversable by VMM/passt. The installer now gives only their group
search permission, without directory listing or extra disk access; AppArmor remains
unchanged. After that repair the guest booted, exposing a real localhost integration
mismatch: the host mapped the API origin to its guest door, but the dispatcher still
compared unmapped member URLs. The repair shares the existing transport mapping and
normalizes only the in-memory guest layout before the unchanged exact-route check.
Persisted authority and local process URLs remain unchanged. Real two-VM API/PTY
qualification now passes layout, private writes, Files and Stop/Start under this
restricted service. Both live passt processes have zero effective/permitted
capabilities, enforced seccomp/AppArmor and empty roots; the supervisor excludes
SYS_ADMIN. Whole-service recovery remains pending: its first run reached Preview
but the isolated API lacked its Preview origin setting; no product guard was relaxed.
The shared mapping passes seven config checks, including route/origin rejection
and unchanged saved metadata. Thirty bare-cache and eighteen module lifecycle
checks pass; portable Clippy passes for foundation, dispatcher, isolation and host.
The seed export parameters are grouped without changing the cache lock, current
read-token, cancellation or size-limit semantics.

The isolated host-loss injection reached actual supervisor/VMM death and empty
host inventory, but systemd 255 returned EINVAL while iterating the dying service
processes. The E2E harness therefore failed before its recovery assertions. The
repair uses the exact verified service cgroup's atomic kernel kill operation,
then restarts that same service. It retains localhost, host identity and singleton
occupancy guards; no product custody assertion or security boundary is relaxed.

Final standalone qualification now passes both the two-VM API/PTY scenario and
whole-service-loss recovery: old manifest/flush/Git authority and Preview are
fenced, both committed members restore in a new VM, unflushed content is absent,
and a fresh Preview runs. Five consecutive seeded Starts pass with the small
two-member fixture. These timings are not the G5-Lean workload distribution.
Linux Clippy passes for the runtime crates and E2E target; the current runtime
gate and systemd parsing checks pass. Companion publication remains unexecuted:
the standalone host's existing registry permission is download-only; IAM was not
changed. Controlled G5-Lean Cargo comparison and current-head CI remain pending.

A controlled Cargo run exposed a shutdown race after the guest reported successful
completion: passt's clean one-off exit preceded the loaded 7-GiB VMM's final exit
by more than the fixed 250-ms grace. The supervisor now uses the existing bounded
five-second graceful-reap budget for clean passt exit, while abnormal sidecar
exit fails immediately. It still requires actual VMM exit and the guest result
before success. The failed cohort is retained. Linux Clippy, the strict KVM gate,
two-VM API/PTY and whole-service receipt recovery pass again; matched Cargo and
passt-kill qualification are still running for this repair.

Latest-main reconciliation (2026-10-15): main replaced the old runtime/worker-host/
deployment pages with Host, Host Control Plane and Platform Operations. The confirmed
Cloud Hypervisor, private-disk, seed, lease, runtime-identity and delivery commitments
move to those current owners with their stable IDs and scoped evidence. The user's
explicit later backend replacement supersedes the named libkrun mechanism in Ben's
isolation statement, preserving isolation and no fallback. New organization-scoped UI
and reserved `fork` workspace-name behavior are compatible and retained. Runtime code
has no merge conflict. Historical libkrun/rsync/model results remain scoped to their
original implementations; they are not evidence for the new backend.

Custody reconciliation found that the old unconfirmed-kill retry applied only to local
process mode. A failed microVM boot could therefore release its slot while retaining
only a host checkout hold; a cgroup wait failure also permanently abandoned its waiter.
This conflicts with the already approved replacement's exit-proof contract. Reuse the
existing live reservation/retry policy for every backend, and retain the VM process
scope owner until its descendants are proven gone. New storage or recovery authority
is unnecessary. Fault regression and actual API/cgroup recovery must pass after repair.

The custody repair keeps both pre-publication and published process owners alive across
bounded wait failures, and applies the existing reservation retry to every backend.
The direct run path follows the same rule. Portable host tests pass (91, one fixture
ignored), including cancelled boot, Stop, delayed proof, blocked replacement and later
release. The real materialization lifecycle test also checks that another workspace's
cleanup cannot reclaim the held root. A macOS-only assertion incorrectly assumed a
non-root skipped chown changes the inherited parent group; it now checks UID there
and retains the complete UID/GID assertion for actual privileged handoff. Portable
Clippy passes; Linux feature-enabled tests and actual cgroup/API recovery remain queued.


### Standalone qualification update — 2026-10-15

The custody repair passes Linux host tests (96, one fixture-only ignored), both
cgroup checks including the real delegated-kernel test, and feature-enabled Clippy.
The current development binary passes the strict KVM/runtime/network gate, restricted
service two-VM API/PTY/private-write/Stop-Start journey and whole-service-loss receipt
restoration/fencing. Killing passt reaps all owned processes and removes the disk,
cgroup and slot reservation in 0.989 seconds; replacement Start restores both members.
Release-profile qualification is running separately; development timings are not
presented as release-runtime measurements.

Matched offline `g5-api` checks ran H–V–V–H on the standalone N2 host with 2-CPU
affinity and 7-GiB workload memory. All twelve fresh/no-op/comment-edit phases pass;
source, tools, artifacts and numeric cache work match. Fresh medians are 204.494
seconds on host and 246.705 seconds in Cloud Hypervisor (20.6% more VM time).
Each fresh phase has 402 compiler-cache hits and 322 misses. These four observations
use the preceding candidate's unchanged VM launch/data path, with local baked cache
and no fleet cache; they do not establish percentiles or a direct improvement against
the differently configured historical libkrun cohorts. The earlier successful-Cargo/
failed-shutdown cohort is retained separately from the passing repaired run.

A 1,421-file backend/build source fixture has direct launch-to-verified-source medians
of 3.032 seconds generic and 2.465 seconds seeded (three observations per arm,
G–S–S–G–G–S). A new branch/commit after publication is fetched and restored exactly,
with clean status and unchanged seed identity. Preparing its generic/seed artifacts
took 105.710 seconds once, outside Start. The small two-member actual API comparison
has development-runtime medians of 4.243 and 3.513 seconds; ranges are broad and it
is not a customer-workload distribution. Generic Start passes while optional seed
publication is blocked. Source readiness excludes dispatcher/API readiness; the
uploaded source excludes frontend and Git history.

Two concurrent 64-GiB guests each write 54,785,118,208 bytes to ordinary-user ENOSPC,
then delete and recover durable writes. Host free space remains 256,447,184,896 bytes
at full guest pressure. A separate 512-MiB guest exhausts inodes at 31,789 files with
431,177,728 bytes still free, then recovers. This covers the tested two-slot budgets,
not larger images or fleet capacity.

An exploratory whole-device API-pause UDP test receives no paused-period data; it
cannot distinguish intentional device-pause drops from receive starvation. A separate
unpaused control receives 1,093 packets. Three vCPU-only stop/resume bursts each
receive 153 packets after resume, without later peer data, with production VMM
arguments and confinement. This supports bounded post-stall receive progress; without
internal ring instrumentation it does not prove all notification races absent.
Both experiments remain separate, including the failed first run.

Startup logs show two OCI manifest resolutions even with prepared immutable artifacts;
one registry fetch contributes about 2.5 seconds to a slow sample. Avoiding that repeat
is a follow-up requiring identity, freshness and authorization-preserving reuse. No
unreviewed cache bypass or early Ready projection is introduced. Actual companion
publication/first boot remains unexecuted under the host's download-only registry
permission; no IAM changes are made. Current-head CI and release qualification are
still running. This is the implementing agent's review, not independent validation.


Current-head Cloud Build reached the strict guest gate and failed before boot:
the privileged nested test container hides securityfs, so AppArmor policy loading
cannot find its kernel interface. The CI entrypoints mount that existing interface when absent, retaining mandatory
parser success and enforced-sandbox assertions. The extracted-image fixture compiles
policy during package setup and leaves actual loading to that strict gate. Production
installation and Docker image-build policy behavior are unchanged.
This is test/installer integration, not an AppArmor allowance or production privilege
expansion; nested-container qualification must confirm the correction.


The release-profile binary built from `218929ef5` passes the strict KVM/runtime/network
gate, restricted-service two-VM integration and atomic whole-service-loss receipt
recovery. Its small two-member API Start medians are 3.200 seconds generic and 2.906
seconds seeded (three per arm; ranges 3.172–3.212 and 2.905–3.027 seconds respectively).
The API test server remains a development build. Forced passt loss then reaps the VM,
disk and cgroup and releases its slot in 1.100 seconds; replacement restores both
members. This last scenario initially raced the deliberately restarted host's old
Ready projection and ambiguous closing control stream. Waiting for a heartbeat newer
than the test's restart fixes that fixture precondition; the original failure and a
cleanup-script assertion are retained. Product readiness/custody assertions are not
relaxed. The release binary and runtime-disk hashes are captured with the raw logs.

The E2E pipeline separately refuses its worker container's cgroup precondition. The
failed build reports the private pool's worker release as `legacy`; a source-free
manual diagnostic is denied `cloudbuild.workerpools.use`. The existing CI entrypoint
now prints actual membership/filesystem diagnostics on refusal, preserving the private
unified cgroup requirement. No IAM or shared-pool configuration is changed.

## Claude Fable 5.1 review convergence — 2026-10-15

Four user-authorized, read-only Claude Code review rounds inspected PR #516 and
iterative corrections. The final round reports no open P0/P1/P2 code findings;
merge readiness remains unestablished. The original full review covered runtime,
source authorization, lifecycle, delivery and consumers; the second added the
previously uncovered harnesses, block E2E scenario and QA deployment configuration.
Later rounds focused on corrections and their interacting owners.

Corrections clear VMM/passt environments, refuse bootstrap replay after reboot,
preserve per-workspace image selection, admit per-Start storage without counting
occupied slots twice, and pin cold preparation outside the dispatcher-ready timer.
Capacity refusal preserves the warm cache. Partition I/O limits use the whole disk;
exact seed-generation reuse still refreshes read authority; legacy loopback aliases
use the host door. Bootstrap archive creation is bounded. Daemon cache locks remain
close-on-exec, while dedicated builders retain writer-lifetime inheritance. The
bootstrap inode finding was rejected because its filesystem contains one archive,
not one inode per workspace file. The unused harness capacity argument was removed.

Local evidence for the correction set: isolation 124 passed/1 Linux-only ignored;
feature-unified host tests 91 passed/1 ignored serially; guest bootstrap marker test
passed; macOS feature check, Clippy and no-default-feature check passed. Worker script
checks passed (72, with 3 platform skips), as did 22 Cloud Build script checks and
changed shell/Python syntax checks. The new actual-child lock regression fails when
the old inheritance behavior is restored and passes with the correction. One parallel
host run had an EPERM in the unchanged process-group stop test; isolated and serial
reruns pass, with the original failure retained. A nonblocking seed-hint test now
accounts for a parallel fork's temporary pre-exec lock copy. The complete PR patch
contains no Chinese characters.

Added Linux gates exercise alternate-image preparation/pin consumption, reboot
refusal, child environment hygiene and loopback routing. They have not run on this
correction set. Neither have Linux compilation/Clippy, partition-backed cgroup I/O,
cold-helper exit custody, registry companion publication or fresh managed-host boot.
The existing PR-head Rust and E2E builds fail at legacy Landlock and hybrid-cgroup
worker prerequisites; Terraform passes. No shared-worker/IAM change, deployment,
merge, human approval record or baseline advance is implied by code convergence.

The bounded one-variant cache still serializes image changes and pays cold preparation
when alternating environments. A cold Start can exceed its initial command/lease;
current authority is still required for Ready, and its completed pristine template
can serve a later Start. Stop during preparation retains custody until the existing
boot cancellation path observes it. These limits require Linux/product qualification.
Detailed reviewer reports and raw validation logs remain local output artifacts.


## Main reconciliation and CI repair — 2026-10-15

Merged main `5fb27ecb4` while preserving its host-runtime placement checks and
renewable stop claims alongside source preparation and exit-proven VM custody.
Resolved both conflicts: protocol tests retain legacy network decoding and the new
runtime-identity coverage; runtime validation uses the shared identity reader and
keeps mismatch diagnostics. The libkrun implementation, firmware directory and
build script were already deleted in this PR; removed the obsolete link-stub memory
and its index/setup references as well. Legacy wire/database fields and the old
artifact-registry resource remain for rolling compatibility and rollback.

Rust CI `25586682-2590-409c-b2d3-182ff5b59f09` exposed a Linux build without the
Cloud Hypervisor feature calling a feature-gated environment helper. Moved the
shared command constructor and its actual-child environment test to the ungated
isolation owner; VMM and passt still receive empty environments.

Local validation after the merge: protocol tests 15 passed; isolation tests 124
passed with one Linux filesystem test ignored; `g5d --no-default-features` and
`g5-api --all-targets` checks passed; full formatting check passed; worker-script
tests 72 with three platform skips and CI-structure tests 22 passed. The serial
host suite had 90 passes, one ignored fixture and one macOS process-group `EPERM`
failure; that exact detached-session test passed on isolated rerun. This is recorded
as a local test limitation, not a fully passing suite.

The strict Linux VM gate still fails because the shared legacy CI worker lacks
required Landlock support; the E2E worker previously refused its non-v2 cgroup
namespace. No protection or test is disabled. The user authorized upgrading the
shared QA pool to Cloud Build worker release `2026.09`, but the API returned 403;
`testIamPermissions` confirms the signed-in account can get, but cannot update,
worker pools. No pool settings changed. The QA runbook documents the narrow
worker-release update and rollback; actual Linux/KVM and delivery qualification
remain open until a capable worker is available and the final candidate passes.

## Retired runtime removal audit — 2026-10-15

Audited tracked source, dependency metadata, build scripts, configuration and current
setup guidance for libkrun, its firmware and associated launcher names. Removed unused
host-version DTO/frontend fields and SQL projections, the uncalled `krunvm` E2E probe,
an obsolete system package and setup/build instructions. Cargo resolves no retired
runtime package; no executable old-backend dependency or tracked runtime directory
remains. Old JSON inventory payloads remain readable without re-emitting retired
fields. Current setup documents Linux/KVM or acknowledged local process mode.

Intentional references remain in immutable applied migration columns, old-payload
compatibility tests, negative linkage guards, historical evidence/fixtures and the
rollback artifact registry. Its Terraform resource and IAM remain managed to avoid
destroying rollback artifacts. Legacy network decoding is compatibility only.

Validation: 30 worker-host contract tests, 16 command-tool tests, the self-contained
binary guard fixtures, command inventory and migration checksums pass. API all-target
and E2E test compilation pass; frontend build typechecking passes after installing
the locked dependencies in this worktree. The first API check exhausted local disk;
after removing only this task's generated incremental cache, its retry passed with
incremental compilation disabled. The initial shared frontend dependency tree lacked
packages; the isolated locked install resolved that setup failure. Changed shell
syntax, diff whitespace and added-line Chinese-character checks pass. These are local
checks, not a rerun of Linux/KVM, live SQL or platform E2E qualification; the shared
CI worker prerequisite and delivery qualifications remain open.

## Isolated CI worker experiment and harness repair — 2026-10-15

The disposable `2026.09` worker provides Linux 6.12, Landlock ABI 6, usable KVM
and cgroup v2. Existing-main compilation/container smoke passes on that release
and again after reverting the same disposable pool to `legacy`. The shared pool
remains unchanged; the initial experiment pool and temporary IAM grants were removed.

Actual candidate runs exposed harness assumptions. The first correction reused
Docker's existing cgroup mount; full E2E then exposed that a private namespace hides
ancestor memory limits and correctly fails RAM admission. CI now exposes the complete
hierarchy read-only with the host namespace, validates its exact Docker cgroup against
the container identity, and separately bind-mounts only that subtree writable before
creating the supervisor subgroup. The host-wide hierarchy is never made writable.
On guest reboot, passt's one-off exit can make the supervisor terminate the VMM
before the persistent bootstrap marker reports its refusal. The gate now accepts
either explicit refusal, requires one workload entry and an actual kernel reboot,
rejects success/timeouts, and checks that no process retains the owned disk/network
paths before cleanup. The independent bootstrap-marker regression remains required;
no product isolation, reboot policy or disk recovery guarantee changes.

The first experiment's diagnostic boot, alternate-image, exit-status, passt sandbox
and two-VM network/private-write checks passed, but its complete gate failed at the
old reboot assertion. Full Rust and E2E suites have not yet passed on the corrected
candidate. A fresh disposable pool is used for that qualification; these preparatory
findings are not merge approval or a shared-pool rollout.

The complete candidate Rust run also exposed stale architecture inventories and a
loopback rewrite applied to native dispatcher fixtures. Address mapping now applies
only when the API actually uses the guest host door; route/origin rejection remains
covered alongside a native-loopback regression. Optional repository-seed lookup
errors fall back to ordinary authorized materialization, whose actual launcher still
verifies the image. Architecture inventories reflect the reviewed platform-process,
provider-proxy and host source-preparation owners, and the API sizing fixture includes
the generic-template reserve. These fixes require a new full-suite receipt.

The second full Rust run passes all 3,643 library tests and reaches 225/226
process integration cases; the remaining Codex fixture incorrectly appends the
upstream API prefix to the local proxy URL. Its corrected assertion preserves
numeric loopback-port and placeholder-key checks; the complete four-case native
auth regression passes locally with fixture sockets permitted. All four full E2E
host lanes become Ready, but browser question publication and chat cases expose
additional failures still under investigation. Template publication through the
actual deployment script succeeds under the isolated registry namespace. None of
these partial results establishes full CI or end-to-end delivery completion.

The third complete Rust run passes every test stage; its final gate reports only
`RUST_FMT`, corrected in the architecture-inventory fixture. Full E2E exposed a
real Docker-host DNS regression: the embedded resolver DNATs port 53 to a loopback
high port before the passt UID's output filter, so matching the translated port
rejects DNS. An isolated real-VM A/B kept the binary and VM fixed: direct-IP HTTP
returned 200 in both arms; internal/public DNS failed before the rule correction,
then both resolved and internal-hostname HTTP returned 200 afterward. The policy
now authorizes the connection's original DNS port. The two-VM network gate adds
UDP/TCP embedded-resolver DNAT and direct-high-port refusal, while retaining host,
metadata and cross-VM denial. Git transport diagnostics retain the connect/DNS
cause while removing the request URL.

The full worker-image boot gate reached the real host's runtime assertions but
failed because its validation container lacked Python. Its prerequisite install
now includes Python; the extracted-image Cloud Hypervisor gate already passed.
These are bounded repairs to existing connectivity and delivery contracts. New
full-suite, image publication and independent-host receipts remain required.

Fable 5.1's bounded follow-up confirms the original-tuple DNS rule and image-gate
prerequisite fix, with no P0/P1 findings. Its two test-precision findings are
addressed by adding a real loopback high-port listener plus guest refusal, and
making the synthetic DNS server ignore truncated/malformed questions. The gate
sends malformed UDP/TCP inputs before its valid DNS assertions. These follow-up
changes affect test coverage only; production source remains `b165b8672`.

Fable's follow-up closes both P2 test findings; no P0/P1/P2 remain in the bounded
CI repair. The stricter real-VM gate passes at `115c58c89`, including malformed
DNS probes, loopback high-port refusal, two-VM isolation and exit-proven cleanup.
Main then advanced to `cf56dcbd5`. Its issue-domain/search-budget and exact review
byte/display commitments were reconciled before resolving dependent files. The
merge retains those features and the useful native-Git timing note; four modified
libkrun-only tests and the obsolete linker-stub memory entry remain deleted under
the confirmed replacement. New final-source qualification is required because
main also changes API and guest CLI behavior; older run receipts stay versioned.

The merged-source full Rust run passes all 229 process-integration and 1,575 API
cases, but three host library tests fail their short wall-clock assumptions under
concurrent load. Their fixture now signals the actual reap attempt before custody
assertions; proven-exit readiness uses virtual time and still asserts no elapsed
readiness wait, the exact exit error, one launch and one reap. Control-session
reads/joins rely on the existing nextest hang guard and retain all protocol,
journal, reconnect, authority and drain assertions. These are test-only changes;
the image and platform E2E runs continue against the identical production source.
The three corrected lifecycle tests pass five concurrent local repetitions. Fable's
follow-up closes the logic findings after replacing the journal-drain busy loop
with a 1 ms poll. Its remaining fixture-hygiene observation was transient: after
the concurrent tests finished, the worktree was clean and their temporary fixture
directories were absent. The existing nextest guard remains the outer failure
bound. Timer and inline retry ownership were checked in the code.

The final full Rust run at `5ea34ba98` passes all 43 stages: 3,657 library,
229 process-integration and 1,575 API cases, plus the 13- and 127-case isolation
lanes. The complete E2E run at production-equivalent `10a6bd740` passes all
69 stages, including 223 ordinary browser cases and 34 provider-flow cases;
14 existing conditional browser skips remain, with no failed browser attempts.
The later commits affect test code and this review only. These manual isolated
builds do not publish GitHub-required check contexts or deploy shared QA.
Provider CLIs use deterministic CI fixtures; VM, API, Git and browser components
are real.

The worker image at `10a6bd740` passes all 16 build stages, including the real
worker boot assertions, extracted Cloud Hypervisor network/isolation gate, KMS
signature and SBOM verification, and isolated GCE publication. A fresh unscheduled
GCE host uses that exact image and the actual QA host service account to import
the deployment-published ext4 companion; execution tracing confirms no reformat.
Host registration, background repository-seed publication and selection, two-VM
private storage, Files/PTY interchange, Stop/Start receipts, guest Docker overlay
and bind mounts, and whole-supervisor-loss recovery with stale Preview fencing
pass against a private API/database stack. Docker was installed on this host only
for the test API/database containers. The host-registration fixture needed the
actual environment digest instead of the ordinary container-CI sentinel, and its
seed wait needed to allow the production background I/O limits; neither correction
changes the product's startup or resource policy.

Five sequential Stop/Start cycles on that prepared host restore edits in both
small fixture repositories and prove distinct VM/kernel identities. Median Stop
is 3.51 s; Start to Running is 2.19 s; Start to a successful terminal command is
2.51 s. Complete restart has median 6.00 s and range 5.89–6.18 s. Each guest has
2 vCPUs and 7 GiB configured RAM on a c3-standard-8 host. These measurements are
not a full G5-Lean checkout or shared-QA browser/network benchmark. Template reuse
takes 0.15 s; host registration reaches Ready in 2 s. Background seed publication
takes about 308 s with the 13 GB environment and configured I/O limits, without
gating readiness. Traced cold preparation takes 419 s and still pulls/extracts OCI
before companion import; it is not an uninstrumented startup performance result.
The fresh host also rejects a companion that expands beyond its admitted capacity
and successfully prepares a pristine disk from verified OCI. A missing companion
likewise takes that fallback; neither case silently accepts the incompatible
artifact. The isolated pool, host, disk, images, artifact packages and experiment
pointers are deleted, and temporary IAM grants are removed. Read-only verification
confirms their absence and an unchanged shared QA pool configuration. Raw evidence
and CI logs are retained locally; required GitHub checks still need a compatible
shared CI execution path before merge.

### Complete G5 source restart qualification — 2026-10-15

A second independent c3-standard-8 host uses the same verified `10a6bd740`
runtime and development environment. Its code repository contains the exact
committed G5-Lean tree: 2,937 files, 80,128,026 bytes, tree
`8ff32267b3543e33ff23600586c1232a08af667f`. Only the committed snapshot was
authorized for transfer, so it has a synthetic baseline without historical Git
commits, ignored build outputs or workspace-installed dependencies. Every timed
guest has 2 vCPUs and 7 GiB configured RAM; the API/database is local and isolated.

All 15 restart cycles pass, with 18 distinct initial/restarted boot identities.
Whole-tree content verification runs outside the timer. Each Stop publishes a
complete receipt; each dirty restart restores three modified existing source
files and one docs file. Host logs prove the selected generic/seeded path.

| Condition, five cycles each | Stop median | Start to terminal median | Complete restart median | Complete range |
| --- | --- | --- | --- | --- |
| No repository seed, clean | 3.86 s | 5.15 s | 9.01 s | 8.54–10.43 s |
| Seed ready, clean | 3.80 s | 3.47 s | 7.31 s | 7.24–8.09 s |
| Seed ready, edited code/docs | 3.85 s | 5.38 s | 9.19 s | 8.84–9.89 s |

These ordered operational cases use normal caches and the production background
preparation limits; they are not randomized/interleaved trials. The earlier
6.00 s small-fixture measurement is not a full-G5 timing or a same-host control.
The extra dirty-versus-clean time occurs during Start and warrants profiling of
the receipt restoration path before attributing it to a particular substep.

Existing API phase logs locate about 2.12 s of seeded Stop in admission drain,
1.50 s in capture/receipt verification, and 0.13 s in VMM stop/exit proof. Current
terminal shutdown sends SIGTERM with a two-second grace. A separate probe of
owned idle Bash 5.3.9 PTYs inside the guest confirms that all three SIGTERM trials
remain alive until escalation, while three SIGHUP trials exit in 8.5–8.7 ms.
This is a mechanism probe, not a product patch or a measured restart improvement.
A terminal-hangup optimization would need to retain session exit proof, bounded
escalation and Git-save ordering, with running-job/trap/fork/reconnect coverage.

Raw timing, content proofs, lifecycle phases and the probe are saved in the local
`output/g5-repo-restart-20261015` report. Test-only API compilation used a local
cache after the host identity could not read the shared CI cache; IAM was not
changed. No product code, shared QA deployment or CI configuration changed.
The independent host and disk, restored experiment image and restored archive
are deleted, with absence verified after the evidence export.


## PR #608 read-only review — 2026-10-16

Scope: [PR #608](https://github.com/G5Labs/G5-Lean/pull/608), head `e98169136797ba15b14f3b537eef828aabb96d39`, base `31ac2da0104d1516e96a4ef6ee41e254c7fa7ec8`. The user asked whether the PR makes sense. This is a read-only follow-up review with the prior implementation history and PR author's reported qualification gap available, not a blind independent evaluation. No PR files, comments, deployment or approval records were changed.

Governance inventory: the versioned plan's `Ontology changes and governance`, confirmed October 14 design, and current `hosts.g5.md` blocks `g5.architecture.build_scratch`, `g5.architecture.microvm_backend`, `g5.runtime.boot.backend.process-local` and `g5.host.runtime-bundle`. The actual PR ontology diff adds the namespace-root outcome/entry mechanism and a qualification note under `build_scratch`. There is no PR-specific saved design amendment; the PR/issue supply the bounded bug report and proposed fix. The historical October 13 local sections above do not describe this reviewed head.

### Initial findings

1. **[P2] Guard the new Linux-only calls in macOS default-feature builds.** `backend/g5d/src/cli/vm_init.rs:199` calls `libc::setns` and `libc::CLONE_NEWNS` without `#[cfg(target_os = "linux")]`. `cli/mod.rs` includes this module based only on the `cloud-hypervisor` feature, which is enabled by default. Thus default `cargo check -p g5d` and the first command of `just check` cannot compile on macOS. The earlier `cfg!(target_os = "linux")` runtime assertion in `run()` does not remove code from compilation. A local probe containing the exact base `enter_workload()` compiles; the exact candidate function fails with E0425 for both names, using the repository's libc version 0.2.189. This is a function-level compilation reproduction, not a complete workspace build. Explicit process-mode setup disables default features and is not affected by this specific defect. Proposed repair: compile the namespace-entry body only on Linux and return unsupported on other targets, preserving both default and feature-disabled build checks.

2. **[P2] Connect the regression to an executable qualification lane.** `scripts/run-ci-script-tests.py:18` adds the new oracle to `OTHER_LANES`, which unconditionally skips it. A search of the PR tree finds no actual CI/qualification caller. The Python oracle independently reproduces the mount sequence; it neither invokes `init.c` nor `vm_init.rs`, so reverting production changes would not make the new-order oracle fail. Existing `test-cloud-hypervisor-ci.sh` boots minimal real guests, but does not exercise Docker exec. `test-cloud-hypervisor-guest.sh` only tests `docker run`, the operation that already worked in the bug report. Proposed repair: wire the oracle into a real privileged lane, and extend the actual candidate-guest test to start a persistent container, run `docker exec`, wait for a Docker/Compose health check, and exercise the affected preview profile. Run it against the candidate initramfs and g5d before accepting the fix as verified. This finding is a regression-coverage gap, not evidence that the proposed Linux fix fails.

### Mechanism assessment and checks

The root-cause explanation is technically sound. In Linux v6.12, `mntns_install()` resolves `/` from the namespace root using `LOOKUP_DOWN`, then resets the joining process's root and working directory. Chroot alone does not change that namespace topology. Moving the guest disk onto `/`, stacking the deliberately selected workload view there, and using setns in forked children address the distinction. The changes preserve the bootstrap parent's trusted root and keep recovery before hooks/workload entry by inspection. Source: [Linux namespace implementation](https://github.com/torvalds/linux/blob/v6.12/fs/namespace.c#L5427-L5467).

| Commitment | Actual path and check | Result / limit |
| --- | --- | --- |
| Namespace joins resolve to the workload/container | `init.c:64,103`, `vm_init.rs:190-214`; compared with Linux `mntns_install` and the oracle's pivot/join path | Mechanism consistent by inspection; actual candidate Docker exec remains unexecuted in this review |
| Recovery precedes customer execution | `vm_init::run`: claim, input validation, import/restore, network-file publication, hook and workload spawn | Ordering retained; no new execution of customer code during recovery found |
| Trusted bootstrap and independent read-only runtime | `init.c`: separate bootstrap root, runtime bind, nonrecursive `/run` in the workload view | Mount intent preserved by inspection; real candidate runtime/credential isolation still needs qualification |
| Local development remains supported | Feature defaults, `cli/mod.rs`, `Justfile:875-878`, process build helper; before/after extracted-function compilation | Confirmed default-feature macOS compilation defect; process-mode feature-disabled build path avoids it |
| Docker/preview behavior and failure propagation | Existing strict KVM and guest scripts, complete new oracle, CI registration/caller search | Existing coverage misses exec/health checks; new oracle is skipped and not coupled to production code |
| Minimal implementation and ownership | Existing init and bootstrap functions; no new storage, service, fallback or lifecycle owner | Scoped correction rather than a new architecture; no ownership change required |

Executed locally: Rust formatting check for the changed Rust file; Python AST parsing; Git diff whitespace check; exact extracted-function base/candidate compilation comparison. The base passes and candidate fails on macOS as described. Probe files are in `/private/tmp/g5-pr608-review-20261016`. No Linux static-init compilation, privileged oracle execution, complete Rust workspace build or real KVM boot was performed here: the local machine is macOS and its Docker daemon is unavailable. Author-reported results remain author evidence, not rerun results.

The bounded ontology-to-implementation pass additionally checked one-shot bootstrap, fresh recovery ordering, retained runtime read-only mounts, child-only root entry and unchanged cleanup ownership. No additional implementation defect was established. Full host lifecycle, network and Git recovery requalification is outside this small read-only review; their prior evidence does not qualify the changed init automatically.

### Proposed documentation adjustment and conclusion

The ontology addition captures the needed outcome, but could avoid requiring one permanent syscall strategy: “After trusted recovery, workload processes must enter a filesystem view whose root matches the mount namespace root. Docker exec, health checks and namespace joins must reach the intended container filesystem while preserving the trusted bootstrap and read-only runtime boundaries.” Record the specific MS_MOVE/bind/setns strategy in implementation metadata. This is an optional wording improvement, not a blocker.

Conclusion: the fix direction is justified, and no Linux mount-sequence failure was established by inspection. Address the confirmed macOS compilation regression and close the real-guest exec/health-check qualification gap before treating it as merge-ready. At the latest status read during this review, the Rust and E2E checks were still in progress. Proposed fixes and ontology wording were not applied.


### Authorized repair follow-up

The user requested fixing PR #608 after this review. Preserve the existing mount-root design, guard Linux-only namespace entry on other build targets, and add real Docker exec/health-check coverage to the candidate-guest CI path. Initial findings above are retained; verification and any remaining gaps will be recorded after the changes.

Implemented: `enter_workload` compiles its namespace syscalls only on Linux and explicitly rejects other platforms, with a macOS unit test. The strict candidate-guest gate now runs the original mount oracle and legacy negative control, then actual Docker exec, Compose exec and a Compose health check with a locally imported static-shell image. Docker tools and Compose are copied from the trusted CI runner into an opt-in 1-GiB fixture; ordinary fixture consumers keep their original 512-MiB layout. CI-builder and worker-image qualification supply the packaged Compose dependency. The full-environment guest acceptance invokes the same Docker checks after its production daemon hook. No workload network, host confinement, production capacity or recovery policy changes.

Executed on macOS after the repair: `cargo check --locked --offline -p g5d --tests`; `cargo test --locked --offline -p g5d --lib vm_init` (2 passed); `cargo check --locked --offline -p g5d --no-default-features --features test-helpers`; and the repository's process-mode lint path, `cargo clippy --locked --offline -p g5d --no-default-features --features test-helpers -- -D warnings`, all pass. The exact repaired-function compilation/runtime probe passes too. Shell syntax, Python AST, Cloud Build YAML parsing and diff-whitespace checks pass. An additional strict default-feature macOS clippy invocation stops on 15 unchanged Linux-only dead-code declarations in `g5d-isolation`; no unrelated suppressions were added. The default-feature compilation itself passes.

Self-review checked fixture-size propagation into alternate-image/reboot/network/runtime cases, preserved non-Docker fixture callers, offline container import, bounded health waits, private test resources and test-only daemon settings. The real Docker checks consume the actual candidate mount state, so the synthetic mount replay is no longer the only regression evidence. Linux/KVM execution and the actual full-stack preview journey remain pending at publication; CI wiring is not a claim that either has passed. No deployment or merge was performed.

Review-comment follow-up: the mount oracle now rejects non-x86_64 platforms before its architecture-specific syscall or any mount operation. The suggested additional `chroot("/image")` after `setns` was not adopted: Linux `mntns_install` explicitly resets the process root, as verified against the source above; `/image` belongs to the old bootstrap view and is not a required path in the joined workload root. Actual candidate-guest checks remain the authority for runtime qualification.

First CI execution (`5e0651aac`, Rust build `c7edd9e2-ad3d-4fd5-a018-d3666326f520`): Linux compilation, Clippy and release artifacts passed. The actual candidate guest booted, its runtime remained read-only, and the mount oracle plus legacy negative control passed. Docker then stopped before readiness with `invalid userland-proxy-path`: the minimal fixture omitted `docker-proxy`, a packaged executable helper resolved by dockerd at startup rather than by `ldd`. Add that helper to the existing executable/library closure and avoid Compose cleanup diagnostics before its configuration exists. Keep the daemon's proxy behavior and the real exec/health assertions unchanged. Docker execution and complete CI remain unqualified until the corrected fixture reruns successfully.
