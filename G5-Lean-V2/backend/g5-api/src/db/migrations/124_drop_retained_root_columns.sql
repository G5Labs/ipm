-- Contract step for root deletion at VM release (PR 365, issue 903).
-- The API stopped reading/writing these columns in 0187bf263. The pre-upgrade
-- hook still runs alongside the serving API: check serving/rollback images
-- and historical-value retention before rollout (docs/db-audit-903.md).
-- Keep HostHello/heartbeat retained_roots and VmSpec retain_hours compatibility.
-- DDL only; FORCE RLS stays enabled throughout.
-- Fail promptly on busy tables; retry the hook after the blocker ends.
SET LOCAL lock_timeout = '5s';
ALTER TABLE public.workspace_vms
    DROP CONSTRAINT workspace_vms_retained_root_pair,
    DROP COLUMN retained_root_host_id,
    DROP COLUMN retained_root_cache_generation;
ALTER TABLE public.worker_hosts
    DROP CONSTRAINT worker_hosts_retained_roots_array,
    DROP COLUMN retained_roots;
ALTER TABLE public.workspace_settings
    DROP CONSTRAINT workspace_settings_retain_hours_check,
    DROP COLUMN retain_hours;
