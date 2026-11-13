# Evidence

Implemented, tested, observed and deployed evidence are recorded separately. This
file describes the design as corrected on 2026-11-04 ([plan](ontology-plan.md)).

## Implemented

- Organization settings H (host disk allowance, default 120 GiB) and V (VM disk
  allowance, default 80 GiB), with atomic host allocation snapshots (H, V, slots,
  resolved disk) for manual creation, demand growth, retries and replacement.
  Registration and VM specifications use the recorded V. Development and CI
  launchers configure their fixture V through the administrator API and consume
  the returned snapshot.
- Start admission in the host supervisor: the workspace filesystem must have at
  least V available, checked once. The former V + 512 MiB gate, the cold image's
  second V, the archive multiplier and the bootstrap margins are removed.
- Private disks stay sparse reflink clones exactly as on main; Cloud Hypervisor
  keeps its sparse disk default. No unsharing, preallocation, extent
  verification, quota project, reader lease or pressure reclamation remains.
- Every production microVM boot restores source through trusted guest SCM.
- Typed capacity diagnostics preserve Start through replies and exit-proven
  durable events; canonical helper stderr is distinguished from guest serial and
  arbitrary image or source text.

## CI E2E root cause (2026-11-04)

The PR's earlier physical reservation unshared and fsynced the whole private disk
on every Start (commits `0e7a3b0a3` and `9feac3bd1`), copying the template's data:

- Host logs show `cloned pristine block workload template preparation_seconds`
  of 6.4–23 s per Start. A local replay of the CI template (10 GiB ext4, 883 MiB
  allocated) takes 5.2–6.3 s with the unshare and 5–12 ms as a reflink.
- Starts without a capacity wait: median 16 s on this PR against 2.0 s in three
  main-based builds from other PRs on the same day.
- Capacity waits per run: 2,345–2,931 s (48–61 waits, up to 177 s) in this PR's
  runs `619a0e10`, `8c8ebbf2` and `b313a159`, against 162–517 s (7–16 waits, at
  most 34 s) in those comparison builds.
- First-attempt test failures: 9–15 per PR run against none in 15 main-based
  builds of other PRs. Some failures had no capacity wait, consistent with the
  extra Start latency and about 0.9 GB of writes per Start.

The fixture organization's default VM quota (10) is below the browser lane's
declared concurrency (13) on main as well. It lengthened the waits but did not
cause the regression, and is tracked separately.

## Tested locally (2026-11-04)

- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`:
  passed.
- `cargo test -p g5-contracts -p g5d-foundation --lib`: 391 and 301 passed.
- `cargo test -p g5-api-core --lib startup_script`: 9 passed, including the cache
  door floor at V and the absence of any quota or reservation setup.
- `cargo test -p g5d-isolation --lib --features cloud-hypervisor`: 132 passed.
  The ignored real-XFS template tests also pass on a loop-mounted XFS, including
  `private_disks_stay_sparse_until_guest_writes`: warm, pinned and cold clones keep
  virtual size V while allocating under V/4, and a 64 MiB write adds its blocks.
- `G5_TEST_REFLINK_ROOT=<XFS loop> cargo test -p g5d-host --lib`: 120 passed,
  including admission refusal for an unsatisfiable V followed by Ready at a V
  that fits. Without a reflink root, five fixtures fail before admission for lack
  of reflink support.
- `cargo test -p g5-api --test integration -- api_worker_hosts api_host_capacity
  api_org_admin_settings migration_146_host_disk_allocations` against Postgres 17:
  199 passed, covering settings, minimum and larger overrides, refusal below the
  minimum, retry snapshots, replacement and migration of live allocations.
- Script regressions: `test_cloudbuild_waitfor_order.py` (32),
  `test_ci_docker_contract.py` (18), `test_seed_host_allocation.py` (3) and
  `infra/tests/test_release_environment_values.py` (6) pass.

The real-boot sparse measurement in `scripts/test_cloud_hypervisor_seed.py`
needs Linux/KVM and runs in the Rust CI gate; it has not run in this workspace.

## Earlier evidence

Earlier commits carried physical reservation and the shared quota. Their results,
including all 2,065 API integration tests and 249 XFS-lane tests on `cd4f253`
and the Linux/KVM guest restore cases with the literal 156.8 GiB cold-image
reproduction, predate this correction. They must be re-established on the
corrected candidate by the Rust and E2E CI gates.

## Deployed

Nothing. No existing host or live VM is resized or replaced; settings changes
never resize existing disks.

## Gaps

- No candidate worker image has booted this candidate. API Ready at the literal
  156.8 GiB free-space level remains unobserved outside the Linux/KVM fixture.
- Optional repository seed preparation keeps main's gate (every slot's V plus
  three preparation disks), so a default-sized host does not prepare seeds; this
  is main's behavior, unchanged.
- The free-space check is admission-time only, by request: a sparse disk can
  later meet a full host filesystem.

## Constants affecting this feature

- `WORKER_HOST_BOOT_DISK_BASE_GB` = 120 and `WORKER_HOST_BOOT_DISK_PER_SLOT_GB` = 80:
  defaults for H and V.
- `MIN_HOST_DISK_ALLOWANCE_GIB` = 30, `MIN_VM_DISK_ALLOWANCE_GIB` = 1 and
  `MAX_WORKER_HOST_BOOT_DISK_GB` = 2000: setting and disk validation bounds.
- `WORKER_HOST_SOURCE_PREPARATION_DISKS` = 3: optional seed preparation's
  staging budget, from main.
- `WORKER_HOST_CACHE_DOOR_GB` = 20: the cache door's byte bound within H; its
  free-space floor is the host's V.
- `default_workspace_vm_limit`: one slot per four vCPUs when no slot count is
  given, from main.
