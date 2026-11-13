-- Organization admin settings (g5.data.configuration.org-admin-settings).
-- Org administrators now edit two registered org keys in public.configs from
-- the Admin page:
--
-- * max_running_vms (g5.runtime.placement.org-quota) narrows from 0-100000
--   to 0-100, the range the owner set on 2026-10-14.
-- * default_idle_timeout_secs (g5.lifecycle.idle.settings) is newly
--   registered, 60-86400 like workspace_settings.idle_timeout_secs. Every new
--   workspace starts with it; absent, the API uses 28800.
-- * Managed capacity (g5.architecture.host_demand_capacity,
--   g5.architecture.host_idle_retirement) newly registers auto_add_hosts and
--   auto_retire_idle_hosts (booleans; absent = on), auto_host_instance_type
--   (a GCE standard machine type; the API enforces Add Host's exact
--   allowlist; absent = c3-standard-4) and idle_host_retire_secs (900-86400;
--   absent = 3600).
--
-- Schema only: replacing the validator behind configs_registered_key_value
-- rewrites and revalidates no rows, and nothing is seeded (the owner chose
-- that the change applies only to workspaces created after delivery). The
-- body is 003's with these branches changed or added.
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
