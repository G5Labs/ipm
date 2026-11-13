# Nested virtualization for Linux workspace VMs

Status: design confirmed by the owner on 2026-10-27 (question set `nested-virtualization`
in this workspace's Project Docs, all decisions as recommended) and implemented the same
day on PR #853; §6 lists the implementation and §7 the review and real-KVM evidence.

Request: the owner's handover, Project Doc `request_investigation.md`
(workspace `003f1792-2438-4c99-b200-53bbd2841ee0`, requested by Kaihui on 2026-10-27).
Goal: a workspace VM on a Linux worker host can run hardware-accelerated VMs itself.
`/dev/kvm` exists in the guest, the workload user and Docker containers given
`--device /dev/kvm` can use it, and a nested guest boots under KVM. Nested boot speed is
not a concern. Running platform-supervised inner microVMs from the in-guest `g5d`
(previews, dev stacks) is out of scope and must not be precluded.

## 1. Governing ontology and what changes

| Block | Role here | Change |
|---|---|---|
| [g5.host.isolation](../../ontology/software_architecture/hosts.g5.md#g5.host.isolation) | Workspace execution stays in the Cloud Hypervisor microVM; no fallback | unchanged |
| [g5.architecture.microvm_backend](../../ontology/software_architecture/hosts.g5.md#g5.architecture.microvm_backend) | Owns the launch (VMM, pinned kernel, trusted init, passt, Landlock) | unchanged; the new child below refines it |
| **new** [g5.architecture.microvm_backend.nested](../../ontology/software_architecture/hosts.g5.md#g5.architecture.microvm_backend.nested) | The host-level switch, what it exposes, what it leaves unchanged, rollout order, observability | added (D, agent, pending the owner's confirmation) |
| [g5.security.guest-authority](../../ontology/concepts/security.g5.md#g5.security.guest-authority) | The owner is guest root; the microVM boundary must protect host and hypervisor | unchanged (human-provided) |
| **new** [g5.security.nested-virtualization](../../ontology/concepts/security.g5.md#g5.security.nested-virtualization) | Why exposure is a per-deployment attack-surface decision, off by default | added (D, agent) |
| [g5.quality.delivery.ci.image-key](../../ontology/features/operations.g5.md#g5.quality.delivery.ci.image-key) | The CI builder is keyed on `infra/guest/kernel.config`; a config edit rebuilds the kernel | unchanged, relied on |
| [g5.runtime.boot.backend.process-local](../../ontology/software_architecture/hosts.g5.md#g5.runtime.boot.backend.process-local) | In-guest backend selection | unchanged; §3.4 shows why |

No amendment to the human-provided blocks is needed: nothing here relaxes the microVM
boundary, Docker, filesystem, network or recovery requirements.

## 2. Facts the design relies on (verified in this checkout and VM)

- The launch is `backend/g5d-isolation/src/isolation/cloud_hypervisor.rs` (`run`), which
  emits `--cpus boot=N`. Cloud Hypervisor v54 (`infra/guest/versions.env`) defaults
  `nested` to off since v53; the handover verified `--cpus nested=on` on the QA hosts
  (Ubuntu 6.8, `kvm_intel nested=Y`, Intel Xeon).
- The guest kernel is built from `infra/guest/kernel.config` by
  `scripts/build-cloud-hypervisor-runtime.sh` as an `allnoconfig` seed, with no module
  support and `# CONFIG_VIRTUALIZATION is not set`. The script fails the image build when
  a requested setting is missing from the emitted `.config`.
- Config-only check run in this VM against the pinned 6.12.112 source: adding
  `CONFIG_VIRTUALIZATION=y`, `CONFIG_KVM=y`, `CONFIG_KVM_INTEL=y`, `CONFIG_KVM_AMD=y`
  to the seed yields a `.config` with all four set, `CONFIG_KVM_X86=y` and the selected
  helpers (`KVM_COMMON`, `MMU_NOTIFIER`, `PREEMPT_NOTIFIERS`, …), with no missing
  setting. In 6.12 `KVM` depends only on `X86_LOCAL_APIC` (already on); it does not
  need `HIGH_RES_TIMERS`, which the seed leaves off. The L3 timer therefore runs at
  jiffies resolution; that affects speed, not boot.
- `GUEST_KERNEL_SHA256` pins the kernel *source tarball*, not the built image, so the
  handover's "re-pin" step does not exist: the config edit alone changes the built
  `vmlinux`, whose digest `vm-exec --report-version` and `g5d doctor` report.
  `build/kernel.config` is installed beside `vmlinux` (`/opt/g5-host/boot/kernel.config`).
- KVM's `vmx` initialization fails cleanly when the CPU shows no VMX, so with the
  switch off a KVM-enabled kernel creates no `/dev/kvm`. The "off" acceptance criterion
  needs no guest-side logic.
- devtmpfs creates `/dev/kvm` as root:root 0600; the guest has no udev. The trusted
  PID 1 (`infra/guest/init.c`) mounts devtmpfs before any image code and the same
  devtmpfs instance is what the workload sees. Guest root is the owner's
  ([g5.security.guest-authority](../../ontology/concepts/security.g5.md#g5.security.guest-authority)),
  and the customer image may lack a `kvm` group.
- Docker creates a `--device` node in the container with the host node's mode bits, so
  a world-usable node needs no group mapping for container users.
- Host configuration reaches `g5d host` through `/etc/g5/host.env`, which the control
  plane renders at bring-up (`backend/g5-api-core/src/cloud/startup_script.rs`) from
  API deployment environment; the cache door (`G5_WORKER_CACHE_DOOR_PORT`, QA first,
  "only hosts provisioned afterwards change") is the precedent. Self-hosted and local
  development hosts serialize an explicit key list
  (`infra/self-hosted/scripts/g5-workerhost.sh`, `scripts/worker-host-env.sh`).
- The host publishes free-form capability strings on hello; the API stores them in
  `worker_hosts.capabilities` without validation.
- The Ready event's `log_tail` carries the host `BootSummary`
  (`boot_to_ready_ms=… restore_fetch_kib=…`), durable in `worker_vm_events`; its parser
  ignores unknown fields.
- The in-guest `g5d` selects Cloud Hypervisor only when `cloud_hypervisor::available()`
  holds, which requires the VMM, `vmlinux` and initramfs files at the platform paths.
  The runtime disk ships none of them, so `/dev/kvm` alone cannot change selection.
- The worker-image README records that a third virtualization layer under the Packer
  qemu guest was unsupported on C3 build workers; the layer-2 gate therefore runs outside
  qemu. A nested boot inside a CI guest is exactly that depth.

## 3. Design

### 3.1 Policy switch (decision 1; recommended: deployment-level, default off)

One boolean per deployment, rendered into every API-provisioned host's environment and
read once by `g5d host`:

- API deployment env `G5_WORKER_NESTED_VIRTUALIZATION` (`1`/`true` = on, anything else or
  unset = off), set in `infra/helm/g5/values-qa.yaml` for QA first; staging and
  production leave it unset. Bring-up writes `G5D_NESTED_VIRTUALIZATION=on` into
  `/etc/g5/host.env` only when on, so existing hosts and older images behave as today.
  Like the cache door, rollback is removing the key and restarting the API; only hosts
  provisioned afterwards change, and host rotation carries the setting to the fleet.
- `g5d host` reads `G5D_NESTED_VIRTUALIZATION` (`on`/`off`, default off; any other value
  is a configuration error, as for `G5D_ISOLATION_MODE`). Self-hosted and local
  development hosts add the key to their serialized lists; the CI E2E worker hosts set
  it on so the real control-plane path boots nested guests.
- The value flows host config → `WorkspaceVmRuntimeConfig.nested_virtualization` →
  `MicrovmRunSpec.nested_virtualization` → `g5d vm-exec --nested-virtualization` →
  `--cpus boot=N,nested=on`. It is a host
  property, not part of `VmSpec`, so no spec version or control-plane change is needed.
- Fail fast: with the switch on, `g5d host` refuses to start unless it is a Linux
  microVM host and the platform kernel config beside the pinned kernel
  (`boot/kernel.config`) declares `CONFIG_KVM=y`. `vm-exec` enforces the same
  predicate, since the strict CI gate calls it directly. This prevents a host whose worker
  image predates the kernel change from registering and then booting guests that show
  VMX but no `/dev/kvm`.

Why not the alternatives:

- *Always on for Linux hosts.* Zero state, but it makes the attack-surface decision
  implicit and platform-wide and gives no QA-first stage. Upstream turned the default
  off for this reason; keeping an explicit switch records the decision per deployment.
- *Per-organization or per-workspace setting.* Needs a `VmSpec` field and version bump,
  a settings owner, UI and placement by host capability. Nothing in the request needs a
  per-tenant choice, and the deployment switch does not preclude it: the host already
  publishes a `nested-virtualization:v1` capability (§3.6) that placement could use
  later, and a `VmSpec` field could then narrow a host-enabled default per workspace.

Attack-surface trade-off, stated for the owner: with `nested=on` the host kernel's
nested-VMX (or nested-SVM) emulation becomes reachable from tenant guest root, which
today only reaches ordinary KVM exits. That is more host-hypervisor code exposed to a
tenant; it is why Cloud Hypervisor's default moved to off. The switch does not weaken
any other boundary: passt confinement, Landlock, the read-only runtime, seccomp and
cgroup delegation are untouched. The guest side exposes nothing new to anyone but the
owner, who is already root there.

### 3.2 Guest kernel (decision 2; recommended: include `CONFIG_KVM_AMD=y` now)

Append to `infra/guest/kernel.config`:

```text
CONFIG_VIRTUALIZATION=y
CONFIG_KVM=y
CONFIG_KVM_INTEL=y
CONFIG_KVM_AMD=y
```

Built in, because the kernel has no module support. `KVM_AMD` costs a few hundred
kilobytes of kernel text and nothing at runtime on Intel hosts; it keeps one platform
kernel for self-hosted or Azure canary hosts on AMD, where the guest would see `svm`.
Only the Intel path is qualified by the handover's experiment and by CI; the AMD path is
compiled, not verified. The alternative (Intel only, add AMD when an AMD host exists)
saves nothing operationally and would need a second kernel rebuild later.

No version or SHA pin changes. The CI image key already includes `kernel.config`, so
the warmed builder, the worker image (Packer `30-build-g5d.sh`) and the g5d-base image
rebuild the kernel on their next build.

### 3.3 Guest device permissions (decision 3; recommended: `0666 root:root`, set by trusted init)

`infra/guest/init.c` sets `/dev/kvm` to mode `0666` immediately after mounting devtmpfs,
ignoring `ENOENT` (switch off, or VMX absent) and failing the boot on any other error.
Equivalent to upstream systemd's default `kvm` rule, it needs no group in the customer
image, holds in the initramfs and workload views (same devtmpfs instance), and makes
`docker run --device /dev/kvm` work for root and non-root container users alike.

Alternative rejected: `0660` with the image's `kvm` group. The group is not guaranteed
to exist, the trusted init must not read image configuration before the runtime is
mounted, and the restriction protects nothing: the workload user has passwordless sudo.

### 3.4 Unchanged behavior, by construction

- Inner backend selection: `cloud_hypervisor::available()` still finds no platform
  artifacts on the runtime disk, so the in-guest `g5d` keeps the process backend. The
  existing `detect_backends` unit test and the artifact requirement cover this; the
  guest probe does not parse `g5d doctor`, whose report has no backend field.
- L2 boot time and memory: `nested=on` is a CPUID change; KVM built in adds about 1 MiB
  of kernel text. Measured by the existing boot-timing scenario, not gated.
- passt, Landlock, seccomp, cgroups, the read-only runtime and one-shot bootstrap: no
  code path touched. Cloud Hypervisor snapshot/restore is unused by G5, so the upstream
  nested-state caveat does not apply.
- `scripts/canary-tungsten-microvm.sh` keeps telling the canary agent not to run nested
  VMs: that canary checks the g5dev *supported* surface, and platform-supervised inner
  VMs remain unsupported in the guest.

### 3.5 CI gate (decision 4; recommended: hard-gate VMX and `/dev/kvm`, report-only nested boot)

`scripts/test-cloud-hypervisor-ci.sh` (strict lanes: `smoke-cloud-hypervisor` in
`cloudbuild-ci-rust.yaml`, the Packer `image-test-gate-cloud-hypervisor`, and the E2E
`setup-kvm-cloud-hypervisor` prerequisites) gains, in the first smoke boot, launched
with `--nested-virtualization`:

1. Hard: `vmx` or `svm` in `/proc/cpuinfo`; `/dev/kvm` is a character device with mode
   `0666`; a non-root user opens it read-write and `KVM_GET_API_VERSION` returns 12
   (Python in the fixture, dropping to an unprivileged uid).
2. Hard: `scripts/test-guest-docker-exec.sh`, under `G5_TEST_EXPECT_KVM=1`, runs a
   container with `--device /dev/kvm` that opens the node read-write.
3. Hard negative control: the existing switch-off boot (the exit-status check) asserts no
   `/dev/kvm` and no `vmx`/`svm` flag.
4. Report-only nested boot: the script stages the host's `cloud-hypervisor`, `vmlinux`
   and `initramfs.cpio` into the guest's `/work` (test inputs only) and boots them as an
   L3 guest with one vCPU, 256 MiB and no disk. Our trusted init then fails at the
   `/dev/vda` mount and powers off with `G5_GUEST_EXIT=125`, so that line on the L3
   serial is a complete boot proof with no extra artifact. The probe has a 90 s timeout,
   logs its outcome and wall time, and fails the gate only when
   `G5_NESTED_BOOT_STRICT=1`. Reason: this is the three-deep path the worker-image
   README records as unreliable on C3 build workers; a flaky hard gate would block
   unrelated PRs. Once the report shows stable passes, flipping the variable in the
   Cloud Build steps makes it strict; QA acceptance (§4.3) is the hard proof until then.

The full-environment acceptance `scripts/test-cloud-hypervisor-guest.sh` (QA
qualification, real g5dev image with its Docker daemon) runs the same probes 1 and 2.
The CI E2E worker hosts run with the switch on, and `host_boot_timings` asserts the
summary token (§3.6), covering the control-plane → host → `vm-exec` chain for real.

### 3.6 Observability

- `BootSummary` gains `nested_virtualization` (`on`/`off`), rendered in the Ready event's
  `log_tail` and so durable in `worker_vm_events`; the parser defaults to `off` for
  summaries from older hosts.
- `vm-exec` logs `nested=true|false` on its launch line; `g5d host` logs the resolved
  switch at startup.
- The host hello carries capability `nested-virtualization:v1` when on, so the fleet
  state is visible in `worker_hosts.capabilities` and available to future placement.

## 4. Verification plan

1. **In this workspace (no `/dev/kvm`).** Unit tests: the `--cpus` argument builder
   (`boot=2` vs `boot=2,nested=on`), `vm-exec` flag parsing, `build_vm_exec_command`
   emitting the flag from the run spec, `g5d host` env resolution (on/off/invalid, the
   kernel-config guard, Linux microVM requirement), the startup-script render (key
   present only when on), `BootSummary` round trip with and without the token. Build
   the kernel with the new config through `scripts/build-cloud-hypervisor-runtime.sh`
   (the config-only step already passes here; the full build needs the listed apt
   packages) and confirm `boot/kernel.config` has KVM built in. `detect_backends()`
   unit test continues to select the process backend without artifacts.
2. **CI.** The strict KVM lanes run the extended gate; the E2E lane boots real
   workspace VMs with the switch on.
3. **QA after deploy.** From this agent's own workspace VM on a newly provisioned QA
   host: `grep -ow -e vmx -e svm /proc/cpuinfo`, `ls -l /dev/kvm`, open as the workload
   user, `docker run --rm --device /dev/kvm … sh -c 'exec 3<>/dev/kvm'`, and the
   handover's reference probe booting a bzImage under the static v54
   `cloud-hypervisor`, recording wall time (not a gate). Record results in this document.

## 5. Decisions for the owner

| # | Decision | Recommendation |
|---|---|---|
| 1 | Policy switch shape | Deployment-level switch, default off, QA first (§3.1) |
| 2 | `CONFIG_KVM_AMD` now | Include (§3.2) |
| 3 | `/dev/kvm` in the guest | `0666 root:root` from trusted init (§3.3) |
| 4 | CI treatment of the nested boot | Hard VMX and `/dev/kvm` checks, report-only L3 boot with a strict flag (§3.5) |

The owner confirmed all four as recommended and authorized implementation through the
question set (`decision: true` on every question, 2026-10-27).

## 6. Implementation (2026-10-27)

Implemented as designed, in one PR on branch `cloud-hypervisor-nested-virtualization`:

- **Switch and plumbing.** `startup_script.rs` reads `G5_WORKER_NESTED_VIRTUALIZATION`
  (`worker_nested_virtualization`) and writes `G5D_NESTED_VIRTUALIZATION=on` only when on;
  `bringup.rs` passes it per bring-up; `values-qa.yaml` turns it on for QA. `g5d host`
  (`backend/g5d/src/cli/host.rs`) resolves `on`/`off`/unset, refuses other values, and
  `validate_nested_virtualization` requires Linux microVM isolation plus
  `CONFIG_KVM=y` in the platform `kernel.config` (via
  `cloud_hypervisor::platform_kernel_has_kvm`). The value reaches
  `WorkspaceVmRuntimeConfig.nested_virtualization` → `MicrovmRunSpec.nested_virtualization`
  → `vm-exec --nested-virtualization` → `cpus_argument` (`boot=N,nested=on`); `vm-exec`
  re-checks the kernel config because the CI gate calls it directly. Self-hosted
  (`g5-workerhost.sh`) and local development (`worker-host-env.sh`) pass the key through.
- **Kernel and init.** `infra/guest/kernel.config` adds the four KVM settings;
  `infra/guest/init.c` sets `/dev/kvm` to `0666` after mounting devtmpfs, ignoring
  `ENOENT`. Built here with `scripts/build-cloud-hypervisor-runtime.sh`: the emitted
  `boot/kernel.config` has `CONFIG_VIRTUALIZATION/KVM/KVM_INTEL/KVM_AMD=y`; `vmlinux` is
  22.7 MB stripped; the init compiles with `-Wall -Wextra -Werror`.
- **Observability.** `BootSummary.nested_virtualization` (`nested_virtualization=on|off`,
  absent reads as off); `vm-exec` and the host startup/ready logs carry the flag; the
  hello capability `nested-virtualization:v1` is published when on
  (`WorkspaceVmRuntime::nested_virtualization`, never in process mode).
- **CI gate.** `scripts/test-guest-nested-kvm.py` runs inside the guest:
  `--expect-present` (VMX/SVM flag, `/dev/kvm` char device `0666` root:root,
  unprivileged read-write open, `KVM_GET_API_VERSION` = 12), `--expect-absent` (negative
  control in the alternate-image boot) and `--boot DIR` (third-level boot of the staged
  platform VMM/kernel/initramfs as uid 65534, expecting the init's `G5_GUEST_EXIT=125`
  receipt; report-only unless `G5_NESTED_BOOT_STRICT=1`). `test-cloud-hypervisor-ci.sh`
  boots the first smoke guest with the flag, stages the artifacts, and
  `test-guest-docker-exec.sh` under `G5_TEST_EXPECT_KVM=1` opens the node from a
  `--user 65534 --device /dev/kvm` container. `test-cloud-hypervisor-guest.sh` runs the
  same probes (default on, `G5_TEST_NESTED_VIRTUALIZATION=0` for a host that keeps it
  off). The E2E lane starts every worker host with the switch on and `host_boot_timings`
  asserts the summary token against `G5_E2E_NESTED_VIRTUALIZATION=on`.

Local evidence in this VM (no `/dev/kvm`): `cargo check --tests` for the six touched
crates; `g5d-isolation` lib 102 passed (new `cpus_argument`, kernel-config and
`--nested-virtualization` argv tests); `g5-internal-api` boot-summary round trip;
`g5-api-core` `cloud::startup_script` 11 passed; `g5d` `cli::host::tests` 18 passed (two
of them need `G5D_ROOTFS_STATE_DIR` pointed at a writable directory here because
`/var/lib/g5d` is root-owned in this workspace); `g5d-host` lib 106 passed with five
failures that need reflink (`cp --reflink=always … Operation not supported`), reproduced
unchanged on the pre-implementation commit in a temporary worktree, and this guest has no
loop device for the XFS workaround; clippy `-D warnings` on all six crates;
`scripts/test_cloudbuild_waitfor_order.py` 30 passed; shell syntax and Python compile
checks. Real KVM evidence comes from the strict CI lanes and the QA acceptance in §4.3,
which has not run yet.

## 7. Review (2026-10-27, same session, not independent)

Checked the diff against §3, the two ontology blocks and the unchanged consumers:

- **Kernel config beside the kernel in every switch-on environment.** The CI builder and
  `g5d-base` install `/opt/g5-host` through `install-cloud-hypervisor-platform.sh`; the
  E2E host containers run `g5d-worker:ci`, built on that base, and the worker-image gate
  copies the whole `/opt/g5-host` out of the candidate image, so the guard's
  `kernel.config` is present wherever `--nested-virtualization` or the host switch runs.
  The CI image key and the Packer pipeline both include `infra/guest/kernel.config`, so
  those kernels are rebuilt for this change.
- **Hello capability.** The control plane stores `capabilities` as sent and only tests
  for the source-preparation string; the new string changes no admission or placement.
- **Boot summary consumers.** The qualification harness and the E2E scenario parse the
  summary; both accept the new token, and old summaries read as off.
- **Process mode.** The host refuses the switch in process mode, and the runtime never
  publishes the capability there.
- **Guest side.** `init.c` compiles under `-Werror`; the mode change precedes any image
  code and tolerates an absent node. The guest probe needs only the fixture's Python,
  `/dev/shm` and root inside the guest, all present in both guest fixtures.
- **Corrections made during review.** The design's claim that the guest probe checks
  `g5d doctor` was dropped: doctor has no backend field, and inner backend selection is
  covered by the `detect_backends` unit test plus the artifact requirement.
- **First real KVM run (PR #853, g5-ci-rust build `77881746`, 2026-10-27).** Under
  `g5d`'s exact launch (Landlock, seccomp, the `g5-vmm` identity) with
  `--nested-virtualization`, the guest printed `NESTED_KVM_PRESENT_OK flag=vmx mode=666
  api_version=12` and the Docker `--user 65534 --device /dev/kvm` container opened the
  node; the built-in `kvm_amd` logged "CPU 0 isn't AMD or Hygon" and stood down as
  expected. The report-only nested boot then crashed instead of reporting: the imported
  `/work` is mode 0700, so the unprivileged VMM could not exec the staged binary and the
  Python exception failed the gate. Fixed by staging the artifacts into a world-readable
  `/dev/shm/nested-l3` and treating launch errors as a probe result.
- **Second real KVM run (g5-ci-rust build `69ffbe25`, head `3b62a1d1`): strict gate PASS.**
  `NESTED_KVM_PRESENT_OK flag=vmx mode=666 api_version=12`; the Docker device container
  opened the node; `NESTED_BOOT_PROBE=pass elapsed_s=6.3` (the platform kernel booted as
  a third-level guest under the unprivileged VMM and powered off with the init receipt;
  Cloud Hypervisor only warned about THP eligibility and `CONFIG_SCHED_CORE`);
  `NESTED_KVM_ABSENT_OK` in the switch-off alternate-image boot; passt sandbox and
  network gates unchanged. The accompanying E2E build failed only in `frontend-test` on
  the known flaky `WorkspaceTabStrip` case, unrelated to this change; CI was retriggered.
- **Rebased head `92e4a6cc` (onto main `4edb068a`, no overlapping files): g5-ci-rust
  build `da592a42` failed only in `RUST_INTEG_G5D` on
  `dispatch_modules::agent_turn_owner_end_is_bounded_behind_slowly_read_replies`, a
  13 s timing bound in the dispatcher suite this change does not touch and that passed
  in build `69ffbe25`; the strict KVM gate passed again. Retriggered.
- **Independent Codex review (2026-10-27, `codex review --base origin/main`, model
  gpt-6-astra, read-only).** Two consecutive rounds, the second on the tree rebased onto
  main `11be4048`, reported no actionable defects in the configuration flow, VMM
  arguments, guest initialization or affected consumers; it did not run Rust tests or
  live KVM checks. CodeRabbit's eleven threads are answered and resolved; its two
  markdown-lint suggestions were a fence hint (applied) and an H1 in a memory file
  (declined: the memory files follow the frontmatter-plus-prose convention).
- **Rebased head `567e385f` (onto main `11be4048`): the g5-ci-e2e build `f0276a1e`
  expired in the Cloud Build queue after about an hour without a worker (pool
  congestion, no step ran) while g5-ci-rust `7327b1d5` was still running; retriggered
  with this commit.
- **Head `b8a7c7fd`: g5-ci-rust passed (`e7437266`); g5-ci-e2e `608a36e0` failed only in
  the browser-shard receipt: `auto-commit-review.spec.ts` (a 409 conflict on the review
  publish call) and `source-control-selection.spec.ts` (a timing bound) each failed once
  and passed on retry, which the receipt refuses, and the value-loop receipt then saw
  mandatory selectors as not run. Every workspace-VM, Rust E2E and provider-agent case
  passed on nested-on hosts. Retriggered with this commit.
- **Head `421f5295`: g5-ci-rust `083fd4d8` expired in the queue after an hour with
  about sixteen builds from other PRs and a QA deploy ahead of it; retriggered.
- **Head `160b13f2`: g5-ci-rust `05056877` passed; g5-ci-e2e `ae9ba904` failed only in
  the shard-2 receipt on `workspace-preview.spec.ts:324` (the run-log viewer missed its
  60 s wait for `G5_NOISY_SHUTDOWN` once, passed on retry); shard 1 passed 186 cases and
  every provider-agent, Rust E2E and workspace-VM case passed on nested-on hosts. The same
  case passed on the earlier green run `90338872`. Retriggered with this commit.
- **Not covered here.** The fixture guest's 1 GiB disk now also imports ~31 MB of
  staged platform artifacts (it fit in the first run). No Verus setup exists in this
  repository, so no production-code proof was attempted.
