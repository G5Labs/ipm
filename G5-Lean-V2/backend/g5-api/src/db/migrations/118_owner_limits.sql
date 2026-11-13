-- Per-owner limits (g5.organization.owner-limits,
-- g5.data.configuration.org-admin-settings). Organization administrators now
-- edit two more registered org keys from the Admin page:
--
-- * max_running_vms_per_owner (0-100; absent = 5): the VMs one owner may hold
--   starting, running or stopping, plus their earlier waiting Starts
--   (g5.runtime.placement.owner-limit).
-- * max_workspaces_per_owner (0-1000; absent = 50): the live non-Base
--   workspaces one owner may hold, plus their pending forks and pending
--   transfers to them (g5.workspace.owner-limit).
--
-- The validator body is 110's with the two keys added.
--
-- Schema only: nothing is seeded and no existing row is rewritten. Both are
-- admission checks, so owners already over a limit keep what they hold.

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
    WHEN config_scope = 'org' AND config_key = 'max_running_vms_per_owner'
        THEN jsonb_typeof(config_value) = 'number'
             AND (config_value #>> '{}') ~ '^[0-9]+$'
             AND (config_value #>> '{}')::numeric BETWEEN 0 AND 100
    WHEN config_scope = 'org' AND config_key = 'max_workspaces_per_owner'
        THEN jsonb_typeof(config_value) = 'number'
             AND (config_value #>> '{}') ~ '^[0-9]+$'
             AND (config_value #>> '{}')::numeric BETWEEN 0 AND 1000
    WHEN config_scope = 'org' AND config_key IN ('default_idle_timeout_secs', 'default_personal_idle_timeout_secs')
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
