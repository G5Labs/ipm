-- Managed capacity free-slot floor (g5.architecture.host_demand_capacity,
-- g5.data.configuration.org-admin-settings). Organization administrators now
-- edit a seventh registered org key from the Admin page:
--
-- * min_free_slots (0-100; absent = 1): the free current slots managed
--   capacity keeps ready in the organization's demand region, and the reserve
--   empty-host removal keeps.
--
-- worker_hosts.capacity_floor_presence marks a host automatic retirement
-- removed while the floor was set: it keeps its region "present" for the floor
-- until an hour after its cloud cleanup, so retiring the region's last empty
-- stale host does not end the floor; an operator's removal clears the marker
-- for the whole region, which ends the floor there.
--
-- worker_hosts.capacity_floor_claim marks a host the capacity sweep claimed
-- for that floor rather than for a queued Start (capacity_start_intent_id).
-- Claim launch, retry and abandonment key on either marker; rotation never
-- adopts a floor claim that never registered, since the floor re-claims. The
-- 082 capacity guard is replaced so the manual cross-region stockout retry
-- (provision_next_retry_at) never adopts a floor claim either: its body is
-- 082's with the floor marker added to that one condition.
--
-- Schema only: the column default is metadata, replacing the validator
-- behind configs_registered_key_value rewrites and revalidates no rows, and
-- nothing is seeded. The function body is 083's with one branch added.
ALTER TABLE public.worker_hosts
    ADD COLUMN capacity_floor_claim boolean NOT NULL DEFAULT false,
    ADD COLUMN capacity_floor_presence boolean NOT NULL DEFAULT false;

CREATE OR REPLACE FUNCTION public.g5_guard_host_capacity() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    -- Fence replacement selections from an older API replica too: its INSERT
    -- and pairing write share a transaction, so refusing the pair rolls back
    -- the would-be successor before it can launch cloud work.
    IF TG_OP = 'UPDATE' AND OLD.status = 'terminated'
       AND OLD.drain_reason = 'automatic_idle'
       AND NEW.rotation_replaced_by IS NOT NULL
       AND NEW.rotation_replaced_by IS DISTINCT FROM OLD.rotation_replaced_by THEN
        RAISE EXCEPTION 'idle-retired host cannot acquire a replacement' USING ERRCODE = '23514';
    END IF;
    IF NEW.status <> 'ready' OR NOT NEW.slot_size_reported
       OR NEW.reported_live_vm_ids IS NULL
       OR cardinality(NEW.reported_live_vm_ids) <> 0
       OR NEW.last_heartbeat IS NULL
       OR NEW.last_heartbeat < now() - interval '30 seconds' THEN
        NEW.idle_since := NULL;
    END IF;
    -- Net-growth retries are demand- or floor-conditioned. Older retry workers
    -- must not consume them or retarget them through the manual cross-region
    -- policy.
    IF NEW.capacity_start_intent_id IS NOT NULL OR NEW.capacity_floor_claim THEN
        NEW.provision_next_retry_at := NULL;
    END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION public.g5_config_value_is_registered(
    config_scope text,
    config_key text,
    config_value jsonb
) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    SET search_path TO 'pg_catalog'
    AS $$
SELECT CASE
    WHEN config_scope = 'org' AND config_key = 'worker_host_region'
        THEN jsonb_typeof(config_value) = 'string'
             AND length(btrim(config_value #>> '{}')) BETWEEN 1 AND 128
    WHEN config_scope = 'org' AND config_key = 'max_running_vms'
        THEN jsonb_typeof(config_value) = 'number'
             AND (config_value #>> '{}') ~ '^[0-9]+$'
             AND (config_value #>> '{}')::numeric BETWEEN 0 AND 100
    WHEN config_scope = 'org' AND config_key = 'default_idle_timeout_secs'
        THEN jsonb_typeof(config_value) = 'number'
             AND (config_value #>> '{}') ~ '^[0-9]+$'
             AND (config_value #>> '{}')::numeric BETWEEN 60 AND 86400
    WHEN config_scope = 'org' AND config_key IN ('auto_add_hosts', 'auto_retire_idle_hosts')
        THEN jsonb_typeof(config_value) = 'boolean'
    WHEN config_scope = 'org' AND config_key = 'auto_host_instance_type'
        THEN jsonb_typeof(config_value) = 'string'
             AND (config_value #>> '{}') ~ '^(n2|c2|c3)-standard-[0-9]{1,3}$'
    WHEN config_scope = 'org' AND config_key = 'idle_host_retire_secs'
        THEN jsonb_typeof(config_value) = 'number'
             AND (config_value #>> '{}') ~ '^[0-9]+$'
             AND (config_value #>> '{}')::numeric BETWEEN 900 AND 86400
    WHEN config_scope = 'org' AND config_key = 'min_free_slots'
        THEN jsonb_typeof(config_value) = 'number'
             AND (config_value #>> '{}') ~ '^[0-9]+$'
             AND (config_value #>> '{}')::numeric BETWEEN 0 AND 100
    WHEN config_scope = 'org' AND config_key = 'allowed_images'
        THEN jsonb_typeof(config_value) = 'array'
             AND NOT EXISTS (
                 SELECT 1 FROM jsonb_array_elements(config_value) AS image
                 WHERE jsonb_typeof(image) <> 'string'
                    OR (image #>> '{}') !~ '^[a-z0-9._-]+(:[0-9]+)?(/[a-z0-9._-]+)+@sha256:[0-9a-f]{64}$'
             )
    WHEN config_scope = 'org' AND config_key = 'workspace_egress_policy'
        THEN public.g5_workspace_egress_policy_is_valid(config_value)
    WHEN config_scope IN ('org', 'user') AND config_key LIKE 'feature.%'
        THEN jsonb_typeof(config_value) = 'boolean'
    WHEN config_scope = 'user' AND config_key = 'agent.default_provider'
        THEN config_value #>> '{}' IN ('claude_code', 'codex')
    WHEN config_scope = 'user' AND config_key = 'agent.default_model'
        THEN jsonb_typeof(config_value) = 'string'
             AND length(btrim(config_value #>> '{}')) BETWEEN 1 AND 256
    WHEN config_scope = 'user' AND config_key = 'agent.permission_policy'
        THEN jsonb_typeof(config_value) = 'object'
    ELSE false
END
$$;
