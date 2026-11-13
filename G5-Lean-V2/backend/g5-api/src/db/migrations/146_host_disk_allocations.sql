-- Organization-managed host/VM disk allowances (G5-Lean#1049).
CREATE OR REPLACE FUNCTION public.g5_config_value_is_registered(
    config_scope text,
    config_key text,
    config_value jsonb
) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    SET search_path TO 'pg_catalog'
    AS $$
SELECT CASE
    WHEN config_scope = 'org' AND config_key = 'host_disk_allowance_gib'
        THEN jsonb_typeof(config_value) = 'number'
             AND (config_value #>> '{}') ~ '^[0-9]+$'
             AND (config_value #>> '{}')::numeric BETWEEN 30 AND 2000
    WHEN config_scope = 'org' AND config_key = 'vm_disk_allowance_gib'
        THEN jsonb_typeof(config_value) = 'number'
             AND (config_value #>> '{}') ~ '^[0-9]+$'
             AND (config_value #>> '{}')::numeric BETWEEN 1 AND 2000
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
    WHEN config_scope = 'org' AND config_key IN ('auto_add_hosts', 'auto_retire_idle_hosts', 'automatic_continuation_enabled')
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

-- New allocations are validated together by the host-record owner. Existing
-- machines keep the historical disk calculation and exact published V. This
-- migration changes no host, guest, advertised size or explicit override.
ALTER TABLE public.worker_hosts
    ADD COLUMN host_disk_gib integer CHECK (host_disk_gib BETWEEN 30 AND 2000),
    ADD COLUMN provisioned_slots integer CHECK (provisioned_slots > 0),
    ADD COLUMN resolved_disk_gib integer CHECK (resolved_disk_gib >= 30);

-- Mixed DDL/data follows the documented definer-role migration protocol.
SELECT set_config(
    'g5.host_disks_membership_snapshot',
    COALESCE((
        SELECT jsonb_agg(
                   jsonb_build_object(
                       'grantor', grantor_role.rolname,
                       'admin_option', membership.admin_option,
                       'inherit_option', membership.inherit_option,
                       'set_option', membership.set_option
                   )
                   ORDER BY grantor_role.rolname
               )
        FROM pg_auth_members membership
        JOIN pg_roles granted_role ON granted_role.oid = membership.roleid
        JOIN pg_roles member_role ON member_role.oid = membership.member
        JOIN pg_roles grantor_role ON grantor_role.oid = membership.grantor
        WHERE granted_role.rolname = 'authz_lookup_definer'
          AND member_role.rolname = SESSION_USER
    ), '[]'::jsonb)::text,
    false
);
SELECT set_config(
    'g5.host_disks_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
DO $$
BEGIN
    IF current_setting('g5.host_disks_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        EXECUTE format(
            'GRANT authz_lookup_definer TO %I WITH SET TRUE',
            SESSION_USER
        );
    END IF;
END $$;
-- REVOKE SELECT on a table also removes its column grants. Add/revoke only
-- missing columns, preserving every existing runtime definer privilege.
SELECT set_config('g5.host_disks_select_columns', COALESCE((
    SELECT jsonb_agg(name) FROM unnest(ARRAY[
        'host_disk_gib','provisioned_slots','resolved_disk_gib','kind','instance_type',
        'slot_size_reported','slots_total','workspace_vm_limit','boot_disk_gb'
    ]) AS columns(name)
    WHERE NOT has_column_privilege('authz_lookup_definer','public.worker_hosts',name,'SELECT')
), '[]'::jsonb)::text, false);
DO $$ DECLARE column_name text; BEGIN
    FOR column_name IN SELECT jsonb_array_elements_text(current_setting('g5.host_disks_select_columns')::jsonb) LOOP
        EXECUTE format('GRANT SELECT(%I) ON public.worker_hosts TO authz_lookup_definer', column_name);
    END LOOP;
END $$;
GRANT UPDATE(host_disk_gib, provisioned_slots, resolved_disk_gib)
    ON public.worker_hosts TO authz_lookup_definer;
SET ROLE authz_lookup_definer;
UPDATE public.worker_hosts SET
    host_disk_gib = 120,
    provisioned_slots = CASE WHEN slot_size_reported THEN slots_total
        ELSE COALESCE(workspace_vm_limit,
            greatest(1, substring(instance_type FROM '[0-9]+$')::integer / 4)) END
WHERE kind = 'gce' AND instance_type ~ '^(n2|c2|c3)-standard-[0-9]+$';
UPDATE public.worker_hosts SET resolved_disk_gib = COALESCE(boot_disk_gb,
    least(2000, greatest(200, 120 + 80 * provisioned_slots)))
WHERE provisioned_slots IS NOT NULL;
RESET ROLE;
REVOKE UPDATE(host_disk_gib, provisioned_slots, resolved_disk_gib)
    ON public.worker_hosts FROM authz_lookup_definer;
DO $$ DECLARE column_name text; BEGIN
    FOR column_name IN SELECT jsonb_array_elements_text(current_setting('g5.host_disks_select_columns')::jsonb) LOOP
        EXECUTE format('REVOKE SELECT(%I) ON public.worker_hosts FROM authz_lookup_definer', column_name);
    END LOOP;
END $$;


COMMENT ON COLUMN public.worker_hosts.boot_disk_gb IS
    'Explicit administrator disk override in GiB, retained on replacement only when it meets current H + V * slots.';
COMMENT ON COLUMN public.worker_hosts.resolved_disk_gib IS
    'Creation-time resolved cloud disk in GiB. Same-host retries never recalculate it.';
COMMENT ON COLUMN public.worker_hosts.provisioned_slots IS
    'Creation-time slot count; registration must agree. Legacy rows retain their last published count.';

-- Database defense for all writers: an existing snapshot is immutable. CPU/RAM
-- publication remains measured by the host; disks and slot count do not change.
CREATE FUNCTION public.g5_guard_host_disk_snapshot() RETURNS trigger
LANGUAGE plpgsql SET search_path TO 'pg_catalog' AS $$
BEGIN
    IF OLD.host_disk_gib IS NOT NULL AND
       ROW(NEW.host_disk_gib, NEW.provisioned_slots, NEW.resolved_disk_gib, NEW.scratch_disk_mib)
       IS DISTINCT FROM
       ROW(OLD.host_disk_gib, OLD.provisioned_slots, OLD.resolved_disk_gib, OLD.scratch_disk_mib)
    THEN
        RAISE EXCEPTION 'managed host disk allocation is immutable; create a replacement'
            USING ERRCODE = '23514';
    END IF;
    IF OLD.host_disk_gib IS NOT NULL AND NEW.slots_total IS DISTINCT FROM OLD.slots_total
       AND NEW.slots_total IS DISTINCT FROM NEW.provisioned_slots THEN
        RAISE EXCEPTION 'advertised slots must match the recorded host allocation'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END
$$;
CREATE TRIGGER worker_host_disk_snapshot_guard BEFORE UPDATE ON public.worker_hosts
FOR EACH ROW EXECUTE FUNCTION public.g5_guard_host_disk_snapshot();

-- Give back the temporary SET-capable membership exactly as it was.
DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.host_disks_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.host_disks_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        FOR membership_row IN
            SELECT
                grantor_role.rolname AS grantor_name,
                membership.admin_option,
                membership.inherit_option,
                membership.set_option
            FROM pg_auth_members membership
            JOIN pg_roles granted_role ON granted_role.oid = membership.roleid
            JOIN pg_roles member_role ON member_role.oid = membership.member
            JOIN pg_roles grantor_role ON grantor_role.oid = membership.grantor
            WHERE granted_role.rolname = 'authz_lookup_definer'
              AND member_role.rolname = SESSION_USER
        LOOP
            SELECT snapshot_row
              INTO prior_row
              FROM jsonb_array_elements(membership_snapshot) AS snapshot_row
             WHERE snapshot_row->>'grantor' = membership_row.grantor_name;

            IF prior_row IS NULL THEN
                EXECUTE format(
                    'REVOKE authz_lookup_definer FROM %I',
                    SESSION_USER
                );
            ELSIF membership_row.admin_option IS DISTINCT FROM
                      (prior_row->>'admin_option')::boolean
                OR membership_row.inherit_option IS DISTINCT FROM
                      (prior_row->>'inherit_option')::boolean
                OR membership_row.set_option IS DISTINCT FROM
                      (prior_row->>'set_option')::boolean
            THEN
                EXECUTE format(
                    'GRANT authz_lookup_definer TO %I WITH ADMIN %s, INHERIT %s, SET %s',
                    SESSION_USER,
                    prior_row->>'admin_option',
                    prior_row->>'inherit_option',
                    prior_row->>'set_option'
                );
            END IF;
        END LOOP;
    END IF;
END $$;
