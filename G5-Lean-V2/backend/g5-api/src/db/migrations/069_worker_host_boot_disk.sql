-- Per-host boot-disk override (g5.runtime.placement.host-disk, issue #344).
-- The boot disk is shared by every VM on the host and the host refuses a start
-- unless `scratch_disk_mib + 512` MiB is free, so the size is normally derived
-- from the host's VM budget at provisioning time and nothing is stored here.
-- This column records only an EXPLICIT operator override, exactly like
-- concurrent_jobs: NULL re-applies the current sizing rule on rotation and
-- stale-host replacement, so a rule change reaches existing hosts when they are
-- next replaced.
--
-- Lower bound only, matching worker_hosts_concurrent_jobs_min: 30 GB is a
-- provider constraint (GCE cannot create a boot disk smaller than the source
-- image), so it is safe to fix in immutable SQL. The upper cap deliberately
-- lives ONLY in WorkerHostBootDiskSizing::MAX_WORKER_HOST_BOOT_DISK_GB — an
-- immutable CHECK carrying it would make raising the cap a second home and a
-- second migration. The POST route's 422 and the rotation clamp enforce it.
--
-- Additive DDL only: existing rows take NULL, so no row-level data section runs
-- under FORCE ROW LEVEL SECURITY (docs/database-migrations.md).
ALTER TABLE public.worker_hosts
    ADD COLUMN boot_disk_gb integer
    CONSTRAINT worker_hosts_boot_disk_gb_min
    CHECK ((boot_disk_gb IS NULL) OR (boot_disk_gb >= 30));

COMMENT ON COLUMN public.worker_hosts.boot_disk_gb IS 'Operator-pinned bootDiskGb override from the provision request (g5.runtime.placement.host-disk); NULL derives the size from the host VM budget and is re-resolved on rotation.';
