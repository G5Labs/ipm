-- Inventory/DTO fields retired in 7ea8f691e after Cloud Hypervisor replaced
-- libkrun. Preserve runtime_identity and current credential/host state.
-- Rollout/rollback prerequisites: docs/db-audit-903.md (#903).
SET LOCAL lock_timeout = '5s';
ALTER TABLE public.worker_hosts
    DROP COLUMN libkrun_version,
    DROP COLUMN libkrunfw_version;
