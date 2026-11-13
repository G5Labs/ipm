# Scale the worker host boot disk with its VM slot count

Source: [G5-Lean#344](https://github.com/G5Labs/G5-Lean/issues/344), opened by @zcl-g5
on 2026-10-07, handed to the agent as "Work on G5Labs/G5-Lean#344".

## Human instructions (from the issue)

Problem, as stated:

- Every API-provisioned worker host gets the same 150 GB boot disk whatever its
  instance type, but its VM slot count grows with the instance type. All VMs on a host
  share that one disk, and the host refuses any new VM unless `scratch_disk_mib + 512`
  MiB is free (64.5 GiB on QA). Bigger hosts therefore get less disk per slot, and once
  a host's VMs hold about 65 GB in total its free slots cannot be used.
- Observed on QA 2026-10-07: on a c3-standard-22 host (5 slots, 150 GB pd-balanced),
  one workspace's `/work` grew to ~118 GiB of cargo `target/`. From 16:31 UTC the host
  refused two other workspaces with `host_reason: disk` while 4 of 5 slots were free
  (`free_mib=7525 required_mib=66048` in the `g5d-host` journal); by 18:16 UTC the host
  was 98% full.
- The 150 GB constant was chosen so the image cache and templates fit across an image
  update, not for workspace data.

Proposal, as stated:

- Size the boot disk from the host's slot count: `disk = max(200 GB, base + slots ×
  per_slot)`, where `slots` is the host's VM budget (`max_microvms`: vCPUs ÷ 4 by
  default, or `concurrentJobs` when set at Add host), `base` starts at 40 GB (OS, swap,
  image cache and template, with room for a second image during an update; roughly
  20–30 GB on QA today) and `per_slot` is 65 GB (the 64 GiB scratch disk plus 512 MiB,
  so every slot can start — "not a prediction of how much disk a VM will use").
- Resulting defaults: c3-standard-4 → 200 GB (floor), c3-standard-8 → 200 GB (floor),
  c3-standard-22 → 365 GB, c3-standard-44 → 755 GB. N2 and C2 follow the same rule.
- Add host, rotation and stale-host replacement all reach `provision_instance`, which
  today reads the provider-wide `self.boot_disk_gb`, so the size has to be passed per
  host.

Add worker host dialog, as stated:

- A "Disk size in GB (optional)" box below "Concurrent jobs (optional)", working the
  same way: empty by default; placeholder and hint show the formula's value for the
  chosen instance type and concurrent jobs and update when either changes (for example
  "Defaults to 365 GB."); empty ⇒ omitted from the request and the server uses the
  formula; the host stores NULL so rotation re-applies the current formula.
- A typed value overrides the formula, including going below the 200 GB floor. It is
  sent as `bootDiskGb`, stored in a new nullable `worker_hosts.boot_disk_gb` column and
  reused by rotation and stale-host replacement.
- Dialog and API accept whole numbers from 30 to 2000 GB (422 like `concurrentJobs`);
  GCE cannot create a disk smaller than the image and 2000 is the existing typo cap.
- Below 105 GB (base plus one slot) a VM may never be able to start, because each start
  needs 64.5 GiB free; the hint warns at that size.
- The dialog computes the default with the same constants as the server, as it already
  does for Concurrent jobs, so base, per_slot and the floor stay constants in code.
  `G5_WORKER_HOST_BOOT_DISK_GB`, which no environment sets, is removed so the dialog's
  default always matches the server's.

Done when, as stated: the size is the stored override or else the formula, the same way
on Add host, rotation and stale-host replacement; the dialog has the box and the API
validates and stores it; tests cover the size per allowlisted instance type, a
`concurrentJobs` override, and `diskSizeGb` in the GCE insert body for two slot counts;
dialog tests cover the placeholder following instance type and concurrent jobs, an empty
box being omitted, a typed value being sent and out-of-range values being blocked; API
tests cover the 422 and rotation reusing a stored override while re-applying the formula
for NULL; the `.g5` ontology records the host disk sizing rule; existing hosts get the
new size when next replaced and the rollout notes say so.

## Correction: derive the per-slot allowance

In review of the first design the human rejected the constant, in their words:

> "65 is literally written in the code twice" is not good, we should Derive

They also said the wedged QA hosts need no rollout handling:

> you do not need to worry here, I will manually remove them and re-provision

Agent reading: the per-slot allowance must have one home and be computed from the
configured scratch size (`scratch_disk_mib + 512` MiB, the host's own start gate) rather
than written as a number anywhere — including in the dialog. Since the browser cannot see
server configuration, the server publishes the sizing coefficients it applies and the
dialog computes the displayed default from those, so the client holds no copy of base,
per-slot, floor or bounds. The issue's table (365 GB at 5 slots, 755 GB at 11) is the
value this derivation produces under the 64 GiB scratch size QA, staging and prod
configure, so the human's numbers are preserved rather than replaced.

## Agent interpretation

- The numbers (40 / 65 / 200 / 30–2000) are the human's decision, not an agent estimate;
  the agent treats them as given. Per the correction above, 40 / 200 / 30 / 2000 are named
  constants with one home and 65 is not a constant at all — it is what the derivation
  yields for the configured 64 GiB scratch size.
- "Slots" is read as the resolved host VM budget the same provisioning paths already
  compute for `G5D_HOST_MAX_MICROVMS` — the operator's `concurrentJobs` when set,
  otherwise `default_concurrent_jobs(vCPUs)`. No new notion of slot count is introduced.
- `per_slot` is one VM's start requirement (`scratch_disk_mib + 512 MiB`), rounded up to
  whole GB — 65 under QA's 64 GiB scratch size, which is where the issue's number comes
  from. The API already resolves and persists `scratch_disk_mib` per host, and rotation
  copies it, so all three provisioning paths can compute it without new plumbing.
- The issue's `self.boot_disk_gb` removal is read as removing the provider-wide field
  and its environment variable entirely, not keeping both paths.
