# G5 on G5 with VM isolation: the full-stack preview's host in microvm mode

Status: designed 2026-10-28 and presented through the question set `nested-virt-followup`
in this workspace's Project Docs. The owner then wrote "continue" without recording
answers; the implementation (§6) follows the recommended options, which are recorded on
the question set as agent proposals pending the owner's confirmation, and Q4 (the damaged
request document) stays open.

Request: the owner's Project Doc `request.md` (workspace
`3927fd16-a974-437f-8185-a653ea04a22d`), recorded with its damaged spans in
[chat intent](../../chat_intent/nested-virt-followup/request.md). Follow-up to the
[nested-virtualization design](../nested-virtualization/ontology-plan.md) (PR #853).

Goal: a workspace VM on a nested-enabled host opens the `full-stack` preview with the
previewed worker host in the default `microvm` mode, and the previewed instance opens and
executes a workspace inside a real Cloud Hypervisor guest. Managed hosts, CI images and
process-mode development change only by the added `runtime/platform/` directory and the
guest kernel options. Owner decisions already made: option A (the host's own pinned platform
artifacts travel on the runtime disk), no kernel baked into g5dev, no download path; nested
boot time is not a concern.

## 1. Governing ontology and what changes

| Block | Role here | Change |
|---|---|---|
| [g5.architecture.microvm_backend.nested](../../ontology/software_architecture/hosts.g5.md#g5.architecture.microvm_backend.nested) | Says the in-guest backend selection still requires the platform artifacts, and that platform-supervised inner microVMs "remain out of scope" | amended: the runtime disk now carries the platform; an inner host inside a nested-enabled guest is in scope for development previews (§3.2, §3.5) |
| [g5.architecture.microvm_backend](../../ontology/software_architecture/hosts.g5.md#g5.architecture.microvm_backend) | Owns the launch: VMM, pinned kernel, trusted init, passt, Landlock, egress policy | unchanged; the inner host runs the same launch unmodified |
| [g5.host.runtime-bundle](../../ontology/software_architecture/hosts.g5.md#g5.host.runtime-bundle) | The read-only runtime bundle's contents | amended: it also carries the platform artifacts for inner hosts |
| [g5.runtime.boot.backend.process-local](../../ontology/software_architecture/hosts.g5.md#g5.runtime.boot.backend.process-local) | Describes the full-stack preview as a process-mode host | amended: microvm by default in a nested-enabled Linux guest; process mode moves to a sibling profile (§3.5) |
| [g5.security.nested-virtualization](../../ontology/concepts/security.g5.md#g5.security.nested-virtualization) | Exposure analysis; says Landlock is unchanged | amended with a note: the guest kernel now has Landlock and the nf_tables families the inner host's egress policy needs (§3.1) |
| [g5.host.isolation](../../ontology/software_architecture/hosts.g5.md#g5.host.isolation) | No fallback from microvm to process | unchanged; the profile fails explicitly (§3.5) |
| [g5.runtime.local-mode-scope](../../ontology/software_architecture/hosts.g5.md#g5.runtime.local-mode-scope) | Process mode is development-only | unchanged |
| [g5.quality.delivery.ci.image-key](../../ontology/features/operations.g5.md#g5.quality.delivery.ci.image-key) | CI builder keyed on `infra/guest/kernel.config` | unchanged, relied on |

## 2. Facts the design relies on (verified 2026-10-28 in this workspace VM and the checkout)

Facts from the request that the checkout confirms:

- The in-guest `g5d` selects Cloud Hypervisor only when `cloud_hypervisor::available()`
  holds (Linux, usable `/dev/kvm`, and the three artifacts from `G5D_CLOUD_HYPERVISOR_BIN`,
  `G5D_GUEST_KERNEL`, `G5D_GUEST_INITRAMFS`, defaults under `/opt/g5-host/`); passt comes
  from `G5D_PASST_BIN` or PATH. `g5d vm-exec` passes `--landlock` unconditionally and
  requires the `g5-vmm` identity, the `g5-passt` identity, the `kvm` group and
  `/usr/sbin/nft` (`guest_egress::install`), so it needs root.
- The g5dev image is `FROM g5/base:26.04` and never runs
  `install-cloud-hypervisor-platform.sh`; only `Dockerfile.g5d-base` (host plane) does.
  This VM has no `/opt/g5/platform`, no `g5-passt`/`g5-vmm` account, no `kvm` group, no
  passt and no cloud-hypervisor. It does have `/usr/sbin/nft` (the `nftables` package is
  already installed, so item 4 of the request only has to make that explicit).
- The runtime disk is assembled by `scripts/build-runtime-disk.sh` from the runtime
  directory in three places: Packer `60-layout-dirs.sh` (after `30-build-g5d.sh` built
  `/opt/g5-host`), `scripts/stage-local-worker-runtime.sh` (inside an `ubuntu:26.04`
  container that sees nothing of the host) and `scripts/prepare-cloud-hypervisor-test.py`
  (the minimal CI fixture runtime). The local dev host's freshness stamp
  (`g5_worker_host_runtime_source_hash`) already hashes `scripts/build-runtime-disk.sh`,
  `scripts/stage-guest-bootstrap.py` and `infra/guest`.
- The guest kernel config has no `CONFIG_SECURITY_LANDLOCK`; this VM has no
  `/sys/kernel/security/lsm` entry listing it.

Facts the request does not list, found here, that the goal depends on:

- **The guest kernel cannot install the egress policy.** `nft -f -` with the exact
  `guest_egress::nft_policy` text fails in this VM at `add table inet …` with
  `Operation not supported` (no `CONFIG_NF_TABLES_INET`); in the `ip` family the `ct`
  expressions and `fib daddr type local` fail the same way (no `CONFIG_NFT_CT`,
  `CONFIG_NFT_FIB_IPV4/IPV6/INET`), while `meta skuid`, sets, `reject` and `accept` work.
  Without these an inner `vm-exec` fails before spawning passt, Landlock or not.
- **A microvm host requires reflink; the guest has none.** `workload_template::prepare`
  refuses admission ("host filesystem must support reflinks before admission"),
  `clone_locked` rejects an unqualified host, and `g5d host` enables copy-instead-of-reflink
  only in process mode (`backend/g5d/src/cli/host.rs`). The guest's root, `/work` and
  `/tmp` are one ext4 disk: `cp --reflink=always` fails with `Operation not supported`,
  there is no loop device (`losetup` fails; no `CONFIG_BLK_DEV_LOOP`) and no XFS
  (`/proc/filesystems`). The strict CI gate makes its reflink root the same way the design
  proposes: an XFS image file mounted through loop.
- **`just dev-up` does not stage the Linux guest runtime.** In microvm mode
  `g5_worker_host_prepare_runtime` only refreshes an already staged runtime and otherwise
  points at `just worker-host-setup`, which runs `scripts/stage-local-worker-runtime.sh`: a
  Docker build of `Dockerfile.g5d`'s builder stage (`FROM g5/base-rust-warm:26.04`, pulled
  through the cache door mirror when the host publishes one) plus pinned downloads of node,
  Claude Code, Codex, gh, gcloud, kubectl and the Rust test tools, then the runtime disk
  build in a container. The process-mode profile needs none of this because it links the
  host build itself.
- **sudo in the guest.** `ubuntu ALL=(ALL) NOPASSWD: ALL` implies `SETENV`, so
  `sudo -n -E` works and keeps `HOME=/home/agent`; `secure_path` still resets `PATH`, and
  `--preserve-env=PATH` keeps it (both verified here). The dispatcher runs the preview
  command as uid 1000 with `env_clear` plus PATH, HOME, the profile's `env` and `PORT`.
- **The CI fixture.** `prepare-cloud-hypervisor-test.py` builds a busybox root with
  `root` as the only account, no `nft`, no GNU `cp`, no e2fsprogs/xfsprogs, and the L2
  guest disk is ext4 (1 GiB with Docker). The smoke step sets only `G5D_BIN`, so the
  runtime the L2 guest mounts is the minimal `root/runtime` (g5d plus bootstrap), built by
  `build-runtime-disk.sh` on a CI image that has `/opt/g5-host` and `/usr/bin/passt`.

## 3. Design

### 3.1 Guest kernel (`infra/guest/kernel.config`)

Add, built in (no module support):

```text
CONFIG_SECURITY_LANDLOCK=y
CONFIG_LSM="landlock,yama"
CONFIG_NF_TABLES_INET=y
CONFIG_NFT_CT=y
CONFIG_NFT_FIB_IPV4=y
CONFIG_NFT_FIB_IPV6=y
CONFIG_NFT_FIB_INET=y
CONFIG_BLK_DEV_LOOP=y
CONFIG_XFS_FS=y
```

- Landlock: Cloud Hypervisor's `--landlock` creates its ruleset as a hard requirement, so
  this is the one setting the request names. `CONFIG_LSM` is written explicitly rather
  than trusting the Kconfig default string, because `build-cloud-hypervisor-runtime.sh`
  fails the build on any seed line missing from the emitted `.config`, which turns "Landlock
  is initialized" into a build-time check; `yama` is kept because the default list has it.
- The nf_tables lines are the minimum for `guest_egress::nft_policy` (§2). They were
  found by running the policy in this VM, not inferred; the kernel build step will show
  the emitted values.
- Loop and XFS are the reflink root for the inner host (§3.5) and for the CI gate's inner
  `vm-exec` (§3.6). They are the only part of the kernel change that depends on an owner
  decision (Q1); the design is written for the recommended answer.

Memory and size: Landlock and the netfilter expressions are small; XFS adds about 1 MiB of
kernel text. `vmlinux` stays within the 31 MB the fixture guest already imports. The
host's own strict KVM gate boots this kernel unchanged, which is the request's
"host's own gate still passes" check.

### 3.2 Platform artifacts on the runtime disk (option A)

Owner of the staging: `scripts/build-runtime-disk.sh`, so every producer of a runtime disk
(Packer, local staging, the CI fixture, self-hosted release assets) ships the same
directory from the same inputs without three copies of the logic. Before building the ext4
image it stages, when a platform is present:

```text
runtime/platform/cloud-hypervisor   from ${G5D_CLOUD_HYPERVISOR_BIN:-/opt/g5-host/bin/cloud-hypervisor}
runtime/platform/vmlinux            from ${G5D_GUEST_KERNEL:-/opt/g5-host/boot/vmlinux}
runtime/platform/initramfs.cpio     from ${G5D_GUEST_INITRAMFS:-/opt/g5-host/boot/initramfs.cpio}
runtime/platform/kernel.config      beside vmlinux (platform_kernel_config() reads it there)
runtime/platform/passt              from ${G5D_PASST_BIN:-/usr/bin/passt}
runtime/platform/SHA256SUMS         digests of the five files
```

Guest path `/opt/g5/platform/…`, on the read-only, executable runtime mount
(`init.c` mounts `/opt/g5` with `MS_RDONLY` only). The same pins as the host, so a deploy
updates both together: no new pin, no new version file. `versions.env` is copied too for
`g5d doctor`-style reporting.

Absence rule: when neither the environment nor `/opt/g5-host` supplies the artifacts (a
macOS or ARM developer's staging container, or a Linux dev host that never installed the
platform) the directory is omitted and the script prints one line saying so; nothing else
changes, which keeps process-mode development identical. A partial set (some files, not
all) fails the build. The platform's `SHA256SUMS` from `/opt/g5-host/boot` is verified
before copying when present.

Producers:

- Packer `60-layout-dirs.sh`: runs `build-runtime-disk.sh` after `/opt/g5-host` exists;
  nothing to add. `75-write-image-info.sh` records the platform digests next to the g5d
  digest (worker-image.json) so a deployed image shows what inner hosts will run.
- `scripts/stage-local-worker-runtime.sh`: the runtime-disk container gains
  `-v /opt/g5-host:/opt/g5-host:ro` and `-v /usr/bin/passt:/usr/bin/passt:ro` when those
  exist on the staging machine, so a Linux dev host's staged runtime carries its own
  platform; macOS stays as today (absent).
- `scripts/prepare-cloud-hypervisor-test.py`: no change; the CI image has `/opt/g5-host`.
- Freshness: `g5_worker_host_runtime_source_hash` additionally hashes
  `/opt/g5-host/boot/SHA256SUMS` and `passt --version` when present, so a platform
  rebuild on a Linux dev host restages the runtime on the next `dev-up`. The Packer and
  CI paths are rebuilt per image by construction.

Why not stage from `30-build-g5d.sh` or the Packer stage alone: the local and CI runtime
disks would then lack the platform and the CI gate (§3.6) could not use "the platform
directory from the runtime disk" as the request asks.

### 3.3 Discovery in the dev host launcher (`scripts/worker-host-env.sh`)

In microvm mode on Linux, when `/opt/g5/platform/cloud-hypervisor` exists and the
corresponding variable is unset, export `G5D_CLOUD_HYPERVISOR_BIN`, `G5D_GUEST_KERNEL`,
`G5D_GUEST_INITRAMFS` and `G5D_PASST_BIN` to the platform directory, and echo the
resolved paths once. Explicit values win. `/opt/g5-host` (a real Linux host) still
resolves through the Rust defaults, so managed and self-hosted hosts are untouched. No Rust
change: `platform_kernel_config()` finds `kernel.config` beside the kernel, and the nested
kernel guard reads it there.

The host launcher also feeds the same four paths to `g5d vm-exec --report-version`
(`g5_worker_host_check_host_vm_exec`), which is what `just worker-host-setup` fails on
today without them.

### 3.4 Guest image (`infra/docker/Dockerfile.g5d-g5dev`)

Add to the apt list `nftables` (explicit, already present transitively) and `xfsprogs`
(Q1), and create what `58-install-passt.sh` and `install-cloud-hypervisor-platform.sh`
create on hosts, with the same commands and the same non-root checks:

```text
groupadd --system kvm            (if absent)
useradd --system --no-create-home --shell /usr/sbin/nologin g5-passt
groupadd --system g5-vmm; useradd --system --gid g5-vmm --no-create-home --shell /usr/sbin/nologin g5-vmm
usermod --append --groups kvm g5-vmm
```

No passt build or download (the binary comes from the runtime disk), no AppArmor profile
(the guest kernel has no AppArmor; passt's own seccomp/namespace sandbox is unchanged),
no kernel build. The image gate asserts the two accounts exist and are non-root, as the
installer does. Image contract test `scripts/test_g5dev_cache_writability.py` stays; a
sibling check lists the accounts and packages.

### 3.5 Preview profiles

**`full-stack.yaml` becomes the microvm profile.** The `command` and `cleanup_command`
drop `G5D_ISOLATION_MODE=process G5D_ALLOW_PROCESS_ISOLATION=1`; the default is microvm.
`G5D_NESTED_VIRTUALIZATION` for the inner host stays `off` (the inner guests do not need
KVM; this is a per-host switch the previewed instance may turn on itself later). The
header comment is rewritten: a workspace VM on a nested-enabled host boots real guests;
the process-mode sibling is for macOS, ARM and hosts with the switch off.

**Explicit failure, no fallback.** Before `dev-up`, the command checks `/dev/kvm` is
usable and `/opt/g5/platform/cloud-hypervisor` exists; otherwise it exits non-zero with
one line naming the sibling profile. The preview's stderr tail reaches the task outcome
(bake-permission deliverable 2), so the owner sees the reason in the UI. This follows
[g5.host.isolation](../../ontology/software_architecture/hosts.g5.md#g5.host.isolation): a
backend failure never selects process mode.

**Sibling `full-stack-process.yaml`** carries today's `full-stack` command verbatim
(process mode, `G5D_ALLOW_PROCESS_ISOLATION=1`, same ports, same stack identity, same
seeds) with the description "… process-mode worker host (macOS, ARM, or hosts without nested
virtualization)". The profile picker already shows every profile by name
([ui](../../ontology/features/ui.g5.md), "Keep full-stack, full-stack-no-worker and
frontend-only distinguishable"). Chosen over an in-profile mode switch because a profile is
the owner's deliberate choice and the error above points at it by name.

**Root boundary (Q3; recommended: only the host process).** `just dev-up` keeps running as
`ubuntu`: cargo builds, Docker Compose, the Docker-staged runtime, `.dev/`, `target/` and
`CARGO_HOME` stay owned by uid 1000 (the 2026-10-14 root-owned-cache failure is the known
trap, and the chat agent builds in the same checkout after the preview). The
`worker-host-start`, `worker-host` and `worker-host-stop` recipes gain one helper in
`worker-host-env.sh`, `g5_worker_host_as_root`, which prefixes a command with
`sudo -n --preserve-env=<the G5D_*/G5_* keys the host reads> --preserve-env=PATH` when
`G5D_ISOLATION_MODE=microvm`, the OS is Linux and the effective uid is not 0, and runs it
directly otherwise (macOS and process mode never see sudo). It wraps exactly: the
`g5d host` launch, the `kill`/`kill -0` calls of the stop and restart paths, the stray
reaper's `lsof`/`kill`, and the reflink-root mount below. Everything else in the recipes
stays unprivileged. A microvm host has always needed root on Linux (passt and VMM identity
drops, nft); the helper makes that explicit instead of asking developers to `sudo just`.
The full-stack preview builds its native host into a separate Cargo output directory
under its host root. Concurrent checkout builds, including tests without VM support,
must not replace the executable that the running host re-executes for VM preparation.

**Reflink root (Q1; recommended: XFS image through loop).** A new small script,
`scripts/dev-host-reflink-root.sh`, run by the profile before `dev-up` (and by
`worker-host-setup` on Linux when `G5_DEV_HOST_REFLINK_ROOT=1`): creates a sparse XFS
image at `/var/lib/g5-dev-host/root.xfs` sized to the smaller of 48 GiB and 80% of the
free space of its filesystem, `mkfs.xfs -m reflink=1`, mounts it at
`/var/lib/g5-dev-host/root` and chowns it to uid 1000; idempotent (an existing mount is
reused). The profile exports `G5_DEV_HOST_DIR=/var/lib/g5-dev-host/root`, which the env
script already honours for the default org, so the host's state, caches, workspace
roots, rootfs templates and the staged runtime all live on reflink-capable storage. The
location is outside `/work` on purpose: `/work` is the workspace's archived tree, and a
multi-gigabyte mount inside it would be walked by the import bound and could be archived.
`/var/lib/g5-dev-host` is ephemeral with the VM, as the preview is. The agent harness
bounds the file size of its process tree (the dispatch module envelope, 4 GiB for a
preview's processes, 16 GiB for an agent turn's, inherited through sudo), which killed the first workspace run of the
script with SIGXFSZ (2026-10-28); the script lifts the bound once it is root and otherwise
sizes the image within it, refusing below 8 GiB. The same envelope (4 GiB files, 4 GiB
data) reached the host through sudo and killed its block template preparation (scratch
disk × slots, 18 GiB); the root exec helper lifts both through `prlimit` in place, so the
launched pid is still the host. The stray reaper
scopes by `$G5_DEV_HOST_DIR/`, so the override keeps today's scoping. Per-org hosts
(`G5_WORKER_ORGS`) keep their `.dev/g5d-host-<slug>` directories; the preview does not
use them, and a later change can route them under the same root.

**Inner guest runtime (Q2; recommended: stage it with `worker-host-setup`).** The profile
runs `just worker-host-setup` before `dev-up` when the staged runtime is absent (the
recipe is idempotent and the stamp logic then keeps it current). This is the canonical
path and produces the branch's own guest `g5d` through `Dockerfile.g5d`, exactly as a
Linux developer's machine does; inside the guest the builder tier comes from the cache door
mirror and `cargo` hits the organization's cache, so the dominant cost is the download of
the pinned CLIs (about 3 GB) and the runtime-disk build, paid once per preview VM boot.
First-time runtime staging and the application build use the same host cache profile,
with explicit caller settings taking precedence; staging must select it before resolving
its builder dependencies, even though it runs before `dev-up`.
The alternative, assembling the inner runtime from the native host build plus the mounted
`/opt/g5/bin` closure, is faster (no Docker, no downloads) but is a second staging path
with its own parity rules (CLI pins must match the deployed runtime), so it is not
recommended for this PR; the owner can choose it if preview start time matters more than
one staging path.

Resource envelope: three inner VM slots (`G5D_HOST_MAX_MICROVMS: 3`, unchanged) at the
default 10 GiB scratch each fit the 48 GiB image only sparsely; the profile sets
`G5D_SCRATCH_DISK_MIB: "6144"` so three live guests and their templates stay within the
image. Memory per inner VM follows the host defaults; a c3-standard-22 workspace is the
documented target.

### 3.6 CI gate (`scripts/test-guest-nested-kvm.py`, strict KVM lane)

The third-level boot goes through the real path. Inside the L2 fixture guest (booted with
`--nested-virtualization`), a new `--vm-exec` probe:

1. Hard: creates a Landlock ruleset through `landlock_create_ruleset` (ctypes) and reads
   the ABI version; fails the gate when the syscall is unsupported. This is the request's
   "Landlock ruleset creation itself must be a hard check", independent of the boot.
2. Hard: `/opt/g5/platform/passt --version` runs, and `nft -f -` installs and deletes a
   policy with the exact `nft_policy` expressions for an unprivileged uid.
3. Reflink root: create a sparse XFS image at `/tmp/nested-l3.xfs` on the guest
   disk, allocate its loop with a separate `losetup --find --show` process, then
   mount that device as XFS at `/tmp/nested-l3`. Detach only that loop afterward
   (lazy cleanup on unmount after success, immediate cleanup after mount failure).
   This is a hard prerequisite: without reflinks the inner `vm-exec` cannot start.
4. Inner `vm-exec`: with `G5D_ROOTFS_STATE_DIR`/`G5D_OCI_CACHE_DIR` on that root,
   `g5d oci prepare-workload` from the fixture's OCI layout (copied into `/work`, a few
   hundred MB compressed), a runtime directory whose `runtime.ext4` is copied
   from the separately staged minimal fixture runtime (not the full outer
   guest runtime), the platform variables pointing at
   `/opt/g5/platform`, and `/opt/g5/bin/g5d vm-exec --rootfs … --cpus 1 --mem-mib 512
   --block-workload -V …:/work -V …:/opt/g5:ro -- /bin/sh -c 'echo L3_OK'`. Success is
   the guest exit receipt and `L3_OK` on the inner serial relay. Report-only unless
   `G5_NESTED_BOOT_STRICT=1`, as today, because three-deep boot reliability on C3 build
   workers is the recorded caveat; but a `vm-exec` failure whose error names Landlock or
   passt is hard regardless of the switch (it is a platform defect, not depth flakiness).
   The current `--boot` mode (bare VMM, no Landlock) is removed: it proved nothing this
   probe does not.

Fixture additions in `prepare-cloud-hypervisor-test.py`: `g5-vmm`, `g5-passt` and
`kvm` entries in `/etc/passwd` and `/etc/group`; GNU `cp` and `tar` (coreutils, for
`--reflink=always`); `nft`, `mkfs.xfs`, `mkfs.ext4`, `e2fsck`, `mount`/`losetup` with
their library closures, copied like the Docker binaries. The L2 disk grows from 1 GiB to
2 GiB to hold the OCI layout copy and the inner template; the host's capacity check
already admits that size. `test-cloud-hypervisor-guest.sh` (QA acceptance on a real
g5dev image) runs probes 1–3 and, with the real runtime, the inner `vm-exec` too.

Every strict lane (`smoke-cloud-hypervisor`, the Packer image gate, the E2E
prerequisites) runs this unchanged; the Packer gate's runtime comes from the candidate
image and so carries the staged platform.

### 3.7 Ontology amendments (applied with the implementation; wording for the recommended answers)

- `g5.architecture.microvm_backend.nested`: replace "Platform-supervised inner microVMs
  from the in-guest `g5d` remain out of scope" with: the runtime disk carries the host's
  pinned VMM, kernel, initramfs, kernel config and passt under `/opt/g5/platform`, so an
  inner host inside a nested-enabled guest runs the unmodified launch (Landlock, passt
  identity, egress policy); the guest kernel declares Landlock, the `inet` nf_tables
  family with `ct` and `fib`, loop and XFS for that purpose; inner hosts are a development
  preview path, never a placement target of the control plane.
- `g5.host.runtime-bundle`: "… libraries, pinned module catalog and the platform artifacts
  an inner host needs".
- `g5.runtime.boot.backend.process-local`: the full-stack preview in a nested-enabled
  Linux guest starts a microvm host on a loop-mounted XFS root under a root-only boundary
  for the host process; the process-mode stack is the `full-stack-process` profile; a
  guest without KVM or platform fails the microvm profile explicitly.
- `g5.security.nested-virtualization`: note that the guest kernel now has Landlock and the
  egress-policy nf_tables families; this adds guest-side mechanisms only and changes no
  host boundary.
- Metadata: lineage to this design and the question set; `human-verified` for the owner's
  answers, `agent` for mechanism.

## 4. Verification plan

1. **In this workspace (no `/dev/kvm`).** Shell tests for `worker-host-env.sh` discovery
   against a fake `/opt/g5/platform` and for `g5_worker_host_as_root` (never on macOS or
   in process mode); a unit test of `build-runtime-disk.sh` staging with a fake platform
   directory (present, absent, partial); the g5dev Dockerfile builds (amd64, in the deploy
   pipeline; locally the image gate's account and package assertions are reviewed); the
   kernel config emits `CONFIG_SECURITY_LANDLOCK=y`, the `CONFIG_LSM` string, the nf_tables
   options, loop and XFS (full kernel build here if the apt packages install; otherwise
   the config-only step as in #853); `nft -f -` with the policy text against the new kernel
   runs in CI's L2 guest (§3.6 probe 2).
2. **CI.** Strict KVM lanes green with the new probes; the E2E lane green (its hosts are
   unchanged except for the runtime disk's new directory).
3. **QA after deploy.** From a workspace on a c3-standard-22 host: open `full-stack`;
   `dev-up` reports "native microVM worker host running"; create a workspace in the
   previewed instance and run a command in it; on the previewed host confirm the inner
   `cloud-hypervisor` process has `--landlock` and runs as `g5-vmm`, and `passt` runs as
   `g5-passt`; close the preview and confirm `dev-down` removes the inner VM, containers
   and volumes. Record wall times for the staging, the host start and the first inner VM
   boot for information only.

## 5. Decisions for the owner

| # | Decision | Recommendation |
|---|---|---|
| 1 | Reflink root for the inner host (and the CI probe) | Add loop and XFS to the guest kernel; mount an XFS image under `/var/lib/g5-dev-host` (§3.5) |
| 2 | Where the inner guest runtime comes from | `just worker-host-setup` inside the preview (§3.5) |
| 3 | Root boundary in the guest | Only the `g5d host` process and its stop/reap/mount through a recipe helper (§3.5) |
| 4 | The damaged request document | Confirm the reconstruction in the question's context, or re-save `request.md` |

Decided in the design as the request delegates: a sibling `full-stack-process` profile with
an explicit, named failure in `full-stack`; the extra nf_tables kernel options (a verified
blocker, not a choice); Landlock and the `CONFIG_LSM` line; staging in
`build-runtime-disk.sh`; the inner host's `G5D_NESTED_VIRTUALIZATION` stays off.

## 6. Implementation (2026-10-28, on the recommended options)

- **Kernel.** `infra/guest/kernel.config` gains Landlock as the only LSM
  (`CONFIG_LSM="landlock"`, no Yama: the previous kernel ran no LSM, so adding one would
  change ptrace behaviour), `NF_TABLES_INET`, `NFT_CT`, `NFT_FIB_IPV4/IPV6/INET`,
  `BLK_DEV_LOOP` and `XFS_FS`. A config-only build of the pinned 6.12.112 source in this
  VM (`make allnoconfig` with the seed) emits every setting, plus the selected
  `NFT_REJECT_INET`, `NFT_FIB`, `LIBCRC32C` and eight loop devices.
- **Platform on the runtime disk.** New `scripts/stage-runtime-platform.sh`, called by
  `build-runtime-disk.sh` after the bootstrap staging: copies the five files from the
  `G5D_*` overrides or `/opt/g5-host`, verifies the host's `boot/SHA256SUMS` when the
  layout is the host's, writes `platform/SHA256SUMS` and `versions.env`, ships nothing on a
  machine without a platform, fails on a partial one. `stage-local-worker-runtime.sh`
  mounts `/opt/g5-host`, passt and the resolved overrides into the runtime-disk container
  (inside a workspace VM those are the mounted `/opt/g5/platform` files).
  `75-write-image-info.sh` records `runtime_platform_sha256` and fails without it.
  `g5_worker_host_runtime_source_hash` folds the resolved platform files' digests in.
- **Discovery and root boundary.** `scripts/worker-host-env.sh`: discovery from
  `G5_DEV_HOST_PLATFORM_DIR` (default `/opt/g5/platform`) for a Linux microvm host when
  unset; `g5_worker_host_needs_root`, `g5_worker_host_as_root`,
  `g5_worker_host_exec_as_root` (`sudo -n --preserve-env=PATH,HOME,<exported G5D_*/G5_*
  keys,RUST_LOG>`), `g5_worker_host_alive` (ps, never a signal) and
  `g5_worker_host_check_reflink_root`. The worker-host recipes launch the host through the
  exec helper (so the pid file holds the host, not a wrapper), signal through the root
  helper and probe liveness with ps; the stray reaper does the same for kills and `lsof`.
- **Reflink root and preview preparation.** `scripts/dev-host-reflink-root.sh` (sparse XFS
  image under `/var/lib/g5-dev-host`, loop-mounted, reflink-probed, idempotent, reused
  across preview opens in the same VM) and `scripts/preview-microvm-host.sh` (the ordered
  refusals naming `full-stack-process`, the mount, and `just worker-host-setup` once per
  VM).
- **Profiles.** `full-stack.yaml` is the microvm profile (`G5_DEV_HOST_DIR` on the reflink
  root, `G5D_SCRATCH_DISK_MIB=6144`, the preparation script before `dev-up`);
  `full-stack-process.yaml` carries the previous command verbatim.
- **Guest image.** `nftables` and `xfsprogs` in the apt list; the `g5-passt`/`g5-vmm`
  identities and `kvm` group with the installers' non-root checks; the final-stage gate
  asserts them with `nft --version` and `mkfs.xfs -V`.
- **CI gate.** `test-guest-nested-kvm.py` replaces `--boot` with `--inner-host-prereqs`
  (Landlock ABI through `landlock_create_ruleset`, platform passt, the exact
  `nft_policy` text installed and removed, the loop XFS root; all hard) and `--vm-exec DIR`
  (template from the fixture OCI layout on the XFS root, the staged minimal
  `G5_TEST_INNER_RUNTIME/runtime.ext4` as the inner runtime disk (originally a copy of
  `/dev/vdb`, replaced after the owner's review in §7), `/opt/g5/bin/g5d vm-exec` with the
  platform variables; report-only unless
  strict, hard when the output names Landlock or passt). The fixture gains the identities,
  GNU `cp`/`tar`, `nft`, e2fsprogs, xfsprogs and mount tools, and a 3 GiB disk; the first
  smoke boot gets 3 GiB of memory and copies the OCI layout into `/work/nested`. The guest
  acceptance runs the prerequisites, and the inner boot when given `G5_TEST_OCI_LAYOUT`.
- **Tests here.** `scripts/test-stage-runtime-platform.sh` (new),
  `scripts/test-preview-microvm-host.sh` (new), `scripts/test_g5dev_inner_host.py` (new),
  `scripts/test-worker-host-env-process-build.sh` (discovery, root boundary, reflink
  check), `scripts/test-dev-up-preview-profile.sh` (microvm default and the sibling; its
  seed assertion was already stale against the profile text on main and is repaired),
  `scripts/test_just_commands.py` (fixture stubs for the helpers),
  `infra/docker/test-dev-build-profile.sh`, `scripts/test_stage_local_worker_runtime.py`,
  `scripts/test_g5dev_cache_writability.py`: all pass in this VM.
- **Not run here.** The strict KVM lanes (no `/dev/kvm` in this VM), the g5dev image
  build (amd64, deploy pipeline), the kernel's full build, and the QA acceptance from a
  workspace on a nested-enabled host. `dev-host-reflink-root.sh` needs root, loop and XFS,
  so its success path is first exercised by the CI probe's equivalent steps and the QA
  acceptance.

## 7. PR #914 and CI (2026-10-28)

- Published as G5Labs/G5-Lean #914 at head `1189c73a`.
- `g5-ci-e2e` build `c8d10f56` failed in `frontend-test` only: a 15 s timeout in
  `src/components/files/blockEdit.test.ts` ("keeps the ancestry of every untouched token")
  and a text assertion in `src/pages/WorkspacePagePersonal.test.tsx`. This PR changes no
  frontend file; the same lane failed on frontend flakes during #853. Retriggered with the
  commit that records this, after the Rust lane reported on the head.
- **Owner review on #914 (Kaihui via Claude, 2026-10-28 03:32Z): one blocking defect,
  fixed.** The probe copied the guest's own runtime disk (`/dev/vdb`) into the 3 GiB XFS
  image; in the Packer image gate that runtime is the complete baked one (about 4 GB), so
  the copy could not fit and the traceback would have failed the gate outside the strict
  switch, blocking new QA worker images. Fix as suggested: `prepare-cloud-hypervisor-test.py`
  always builds the minimal inner runtime (candidate g5d plus bootstrap through
  `build-runtime-disk.sh`) and exports `G5_TEST_INNER_RUNTIME`; the CI gate and the guest
  acceptance stage its `runtime.ext4` under `/work/nested/runtime`; the probe copies that
  file onto the reflink root and stages everything inside its error handling, so a
  staging failure is a reported probe result. The non-blocking note on `PLATFORM_DEFECT`
  is taken: the match now names Landlock, the passt launch, the egress policy install and
  a non-zero network-backend exit, not every mention of passt. The review also confirms
  Q4 (the reconstruction matches the original handover); recorded as a proposal on the
  question set with the review as its source. Rebased onto main `d360bf7e` (nine chat
  commits, no overlapping files).
- **Codex review round 1 (`codex review --base origin/main`, gpt-6-astra, high; read-only):
  three findings, all taken.** (P1) `package-self-hosted-release-assets.sh` did not ship
  `stage-runtime-platform.sh`, which `build-runtime-disk.sh` now calls unconditionally, so a
  fresh self-hosted install would have failed at runtime staging: packaged, and the bundle
  contract test now requires every `$repo/scripts/...` helper the disk builder names to
  be installed into the bundle. (P2) Stop and reap privileges followed the requested
  isolation mode, so a developer flipping a running root microvm host to process mode
  could not signal it: `g5_worker_host_kill`/`g5_worker_host_lsof` now choose sudo from
  the target process's owner, used by the recipes and the stray reaper; the launch keeps
  `g5_worker_host_as_root`. (P2) The preview test's exported `test` function did not
  intercept the `[` builtin, so the "no KVM" case would have failed on a KVM-enabled
  machine: the check reads `G5_DEV_HOST_KVM_DEVICE` (default `/dev/kvm`) and the test
  injects a missing node or `/dev/null`, which also lets the platform and Landlock
  refusals run everywhere.
- **E2E on the rebased head (`e7ff414a`) failed again only on `blockEdit.test.ts`
  ("keeps the ancestry of every untouched token").** PR #913's build `f6f1f4bf` failed on
  the same test the same way, so it is a main-wide flake of a test this PR does not touch
  (the file takes about 90 s under CI load against a 15 s per-test timeout). Locally
  (idle 4-vCPU workspace VM, three runs after `npm ci`) the test passes in 5.6–5.8 s and
  the whole file in about 38 s, so the CI failures are the 15 s per-test timeout under a
  loaded runner, not a defect this PR could introduce or fix.
- **Codex review round 2: three P2 findings, all taken.** The `full-stack` profile could
  still start a process host when the environment or the checkout's `.env` selected
  process mode (the dev recipes source `.env` with `set -a`): the profile now pins
  `G5D_ISOLATION_MODE=microvm` and clears the acknowledgement in its own shell, and
  `preview-microvm-host.sh` refuses an ambient or dotenv selection first, naming the
  sibling. The local staging container could receive the same passt destination twice
  (discovery and an equal `G5D_PASST_BIN`), which Docker rejects: mounts are now keyed by
  destination, overrides first. `TimeoutExpired` carries bytes under `text=True`, so the
  timeout branch lost the boot's output and the platform-defect match: decoded now.
  Regressions added for each.
- **Codex review round 3: two findings, both taken.** (P1) The Packer image gate's
  container installed neither `git` nor `patchelf`, which the now unconditional minimal
  inner-runtime build needs (`stage-guest-bootstrap.py`, the guest executable patch):
  added to that container's package list; the CI builder image already has both. (P2)
  A root microvm host leaves root-owned journal, cache and workspace state under the
  shared host directory, and a later unprivileged process host there fails deep inside
  the host: `g5_worker_host_check_state_ownership` refuses a process launch on Linux
  over state the caller does not own (numeric uid, staged runtimes exempt), naming the
  first path and the `chown` repair; process mode still never uses sudo.
- **Codex review round 4: one P2 finding, taken.** The guest acceptance did not forward
  `G5_NESTED_BOOT_STRICT` into its guest, so a strict qualification run would still have
  treated an inner boot failure as report-only; it is passed through `-e` as the CI gate
  does.
- **Codex review round 5: one P2 finding, taken.** `stage-runtime-platform.sh` was not in
  the local dev host's guest-runtime freshness inputs, so a change to it alone would not
  have restaged the runtime: added, with a regression that every helper the disk
  builder invokes is a freshness input.
- **Codex review round 6: one P2 finding, taken.** Under sudo the launch pid is sudo's
  supervisor (PAM sessions fork the command), so a forced stop would have killed sudo and
  orphaned the root host, and the stray reaper's host branch needed `lsof`, which the
  g5dev image lacks. `worker-host-start` now records the g5d child of the launch
  (`g5_worker_host_resolve_launched_pid`), and the reaper matches processes to the host
  directory through `/proc` (cwd, open files, mounts) where lsof is absent, through the
  owner's privilege. Regressions cover both.
- **CodeRabbit threads (first two heads), reconciled.** Taken: the mount deduplication
  and the network-backend classification (already fixed in rounds 1–2), the reflink clone
  proven on an existing mount, a partial third-level fixture (one of `G5_TEST_OCI_LAYOUT`
  and `G5_TEST_INNER_RUNTIME`) rejected instead of silently downgraded, fixed-string
  matching in the freshness regression, the design's §6 wording, and the trailing spaces
  on the edited runtime-bundle line. Obsolete: sizing the XFS image for a `/dev/vdb` copy
  (no copy remains).
- **The main-wide frontend flake, bounded.** After six consecutive E2E failures on this PR
  and one on #913, all on `blockEdit.test.ts` "keeps the ancestry of every untouched
  token" (5–6 s idle, past the suite's 15 s default under CI load), that one test carries
  its own 90 s budget. It is the only frontend change in this PR and is outside its scope;
  it is a separate commit so the owner can drop it.
- **Codex review round 7: one P1 finding, taken.** The `/proc` fallback also read
  `mountinfo`, where the preview's XFS host root appears for every process in the
  namespace, so the stray reaper on the g5dev image would have classified the enclosing
  `g5d preview-serve` and dispatcher as strays and killed them before starting the inner
  host. Only a process's own cwd and open files count now; the regression checks that a
  mount every process sees does not make an unrelated process a stray.
- **Codex review round 8: two P2 findings, taken.** The platform-defect match now names
  the passt launcher's own failures ("exited before readiness", "readiness timed out",
  "startup: …"), so a passt that cannot start fails the gate instead of being
  report-only; the pid-resolution regression stubs a non-root caller because the script
  suite also runs as root in Docker CI.
- **Codex review round 9: one P1 finding, taken.** The local staging bound the machine's
  own `/opt/g5-host` and passt into the runtime-disk container; on a Docker-out-of-Docker
  caller (the E2E lane's CI builder, whose daemon launches sibling containers) a bind
  source names the daemon host, where a missing file source becomes an empty directory
  and breaks the container. The platform is now copied under the staging output
  (`platform-input/`, beside `runtime/`, the directory the existing runtime mount already
  proves shareable) and that copy is mounted with the `G5D_*` variables pointing into it;
  all five files or none, removed after the disk build. Regression rewritten accordingly.
- **First real KVM runs of the inner-host probe (g5-ci-rust builds `b2fc6ef2` at
  `f6cf924f` and `782a7179` at `313b41c1`): the inner host works.** Inside the nested-on
  fixture guest, under `g5d`'s own launch: `NESTED_LANDLOCK_OK abi=6`,
  `NESTED_PASST_POLICY_OK passt=passt 0.0~git20260120.386b5f5-1` (the exact egress policy
  installed and removed by the guest kernel's new nf_tables families),
  `NESTED_REFLINK_ROOT_OK root=/tmp/nested-l3` (loop XFS on the ext4 guest disk) and
  `NESTED_VM_EXEC_PROBE=pass elapsed_s=14.3 vm_exec_exit=0`: the in-guest `g5d vm-exec`
  prepared a template from the fixture layout on the reflink root, launched the platform
  VMM from `/opt/g5/platform` with `--landlock`, the `g5-vmm` and `g5-passt` identities and
  the egress policy, and the third-level guest printed `L3_OK` and its exit receipt.
  `NESTED_KVM_PRESENT_OK flag=vmx mode=666 api_version=12`, the Docker device container and
  the switch-off negative control passed as before. Both builds then failed later in the
  same script, on the exit-status boot: the 8 GiB XFS store no longer admitted a third
  private clone beside the 3 GiB fixture template ("backing filesystem has 6368309248
  available bytes, below requested total"); the sparse store is now 16 GiB.
- **Codex review round 10: one P1 finding, taken.** Restaging inside a workspace VM copied
  the runtime disk's flat `SHA256SUMS` into the host-layout input, where the stager would
  have failed to verify it and aborted `worker-host-setup`. The source list is now verified
  in its own layout (host or flat) and a host-layout list is regenerated for the copy; the
  regression restages a flat platform through the real stager and rejects a tampered one.
- **Codex review round 11: no actionable findings** (static image and profile contract
  tests and shell syntax passed in its sandbox; Docker and nested-KVM integration are
  the CI lanes' job). Eleven rounds in total, sixteen findings, all taken.
- **Codex review round 12 (confirming, after the test repair): one P2, taken.** The
  staging regression's no-platform case relied on the machine lacking `/opt/g5-host`,
  which the CI builder image and Linux development hosts have; it now uses absent
  fixture paths. Twelve rounds, seventeen findings, all taken.
- **Codex review round 13 (final tree): no actionable defects.** Converged: two
  consecutive clean rounds bracket one test-only repair.
- **g5-ci-rust `05400900` at `e5adef89`: the strict KVM gate passed in full** (the 16 GiB
  store admitted every boot after the inner-host probe). The build failed only in the
  Rust source contract `deployment_and_ci_configuration_never_selects_process_isolation`,
  which scans `scripts/` for process-mode selections and allowlists the refusal tests by
  name: the new `scripts/test-preview-microvm-host.sh` drives the microvm preview's check
  with an ambient process selection to prove the refusal and is now allowlisted like its
  siblings.
- **Rebased head `5108aa7d` (onto main `9e72c262`, 39 commits, no overlapping files).**
  g5-ci-e2e `3359135a` failed only in `frontend-test` on
  `OnboardingTour.test.tsx` "ends on the Guided step when the full sidebar's header is
  activated by keyboard" (an assertion, not a timeout), a file from main's new
  onboarding-tour commits that this PR does not touch; it passes locally three times.
  Retriggered with this commit.
- **Head `5446a831`: g5-ci-rust `cd9e9bc1` passed the strict KVM gate again and failed
  only on `g5d-isolation isolation::workload_root::tests::helper_parent_death_kills_writer`
  at its 5 s "helper started" wait (a pre-existing parent-death test in a file this PR
  does not touch; it passed in the same build's other lane and in `05400900`).
  Retriggered. g5-ci-e2e `6da59e0b` passed in full on the same head.

### Post-merge image-gate repair: loop mount compatibility (2026-10-28)

Packer build `387f2aa3-a605-40ab-8b7f-0c2ef4384b6e` failed in the nested
guest's XFS mount after KVM, Docker, Landlock and the passt policy passed. The
kernel uses `allnoconfig`, leaving `CONFIG_BLK_DEV_WRITE_MOUNTED` disabled.
Ubuntu 24.04's older libmount holds a writable loop-device descriptor while
mounting, which conflicts with that protection and returns `EBUSY`. The PR CI
builder's newer userspace passed the same test; that did not qualify Packer's
Ubuntu 24.04 fixture.

Both the probe and the preview helper now allocate a loop with a separate
`losetup` process, then mount its device explicitly as XFS after the setup
descriptor has closed. They detach only the allocated device after success
(lazy cleanup at unmount) or failure, preserving the original mount error if
cleanup also fails. The kernel protection, reflink requirement, hard
prerequisite gate, ext4 workspace disks and existing mounted-root reuse remain
unchanged. This repairs the existing design rather than changing its contract.

Eleven synthetic regression tests cover both entry points, repeated mount
reuse, allocation failure, mount failure and cleanup failure, including a
failed detach after a successful mount. The original nine tests failed before
the repair and pass afterward; the two added cleanup tests also pass. The six
inner-host unit tests and preview prerequisite/profile shell tests pass.

Claude Code and independent source review found no blocking defects. A failed
detach stops that invocation; abrupt termination can still leave a loop until
guest teardown or manual cleanup. This repair does not introduce broader
interruption recovery or reuse another invocation's loop association.

Disposable Cloud Build `90bce5af-5908-46cc-9162-25b5f8098aec` passed the
compatibility test with Ubuntu 24.04 userspace and the pinned Linux 6.12.112
guest kernel (`CONFIG_BLK_DEV_WRITE_MOUNTED` disabled), booted under QEMU TCG.
The original combined mount reproduced `EBUSY`; both patched helpers passed
real mounting, repeat invocation, reflink, mount-flag, AUTOCLEAR and unmount
release checks, plus failed-mount cleanup. The preview helper also preserved
the requested ownership. The runner required the probe's success marker and
zero child status. The minimal harness panicked during shutdown afterward;
that occurred after the probe completed, not during a test operation.
This proves the filesystem compatibility repair, not nested KVM execution or
the complete Packer image gate. No image was published or deployed.

PR #920's initial Rust CI stopped at `cargo-deny` on RUSTSEC-2026-0414 before
the VM gate. The follow-up updates `pulldown-cmark` to 0.13.5 and Tokio to
1.53.3 for RUSTSEC-2026-0386 from the refreshed advisory database. Only those
two lockfile versions/checksums change; no advisory exemption is added.
`cargo deny --locked check`, 117 Markdown unit tests and 28 integration tests
pass. A bounded empty-display-wikilink regression fails immediately on
0.13.4 and passes on 0.13.5. The earlier gate description above now matches
the implemented loop lifecycle and separate inner runtime, resolving the
CodeRabbit documentation finding.
