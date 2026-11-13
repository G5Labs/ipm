-- g5.architecture.host_cloud_linkage (issue #400): hello can make an
-- unlinked host ready before a crashed or timed-out bring-up is repaired.
-- Recreate the read function to add status to its projection. Older callers
-- select the existing named columns and remain compatible during deployment.

-- Temporarily assume the existing function owner, as in migration 039.
SELECT set_config(
    'g5.orphan_linkage_authz_lookup_membership_snapshot',
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
    'g5.orphan_linkage_authz_lookup_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
SELECT set_config(
    'g5.orphan_linkage_authz_lookup_had_authz_create',
    has_schema_privilege('authz_lookup_definer', 'authz', 'CREATE')::text,
    false
);
DO $$
BEGIN
    IF current_setting('g5.orphan_linkage_authz_lookup_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        EXECUTE format(
            'GRANT authz_lookup_definer TO %I WITH SET TRUE',
            SESSION_USER
        );
    END IF;
END $$;
GRANT USAGE, CREATE ON SCHEMA authz TO authz_lookup_definer;

SET ROLE authz_lookup_definer;

DROP FUNCTION authz.list_orphan_bringup_worker_hosts();

CREATE FUNCTION authz.list_orphan_bringup_worker_hosts()
RETURNS TABLE (
    host_id uuid,
    org_id uuid,
    region text,
    created_at timestamptz,
    orphan_check_misses integer,
    status text
)
LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED
SET search_path TO 'public', 'authz', 'pg_catalog'
AS $$
    SELECT wh.host_id, wh.org_id, wh.region, wh.created_at,
           wh.orphan_check_misses, wh.status
    FROM public.worker_hosts wh
    WHERE wh.status <> 'terminated'
      AND wh.cloud_instance_id IS NULL
      AND wh.provision_error_code IS NULL
      AND wh.created_at < now() - interval '60 seconds'
      AND (wh.bringup_heartbeat_at IS NULL
           OR wh.bringup_heartbeat_at < now() - interval '90 seconds')
$$;

ALTER FUNCTION authz.list_orphan_bringup_worker_hosts() OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.list_orphan_bringup_worker_hosts() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.list_orphan_bringup_worker_hosts() TO g5_app;
COMMENT ON FUNCTION authz.list_orphan_bringup_worker_hosts() IS
    'Live unlinked worker hosts with no classified provisioning error, past creation grace and without a fresh bring-up lease (g5.architecture.host_cloud_linkage). SECURITY DEFINER + BYPASSRLS owner for cross-org repair.';

CREATE OR REPLACE FUNCTION authz.attach_orphan_worker_host_cloud_refs(
    p_host_id uuid,
    p_vpc_id text,
    p_cloud_instance_id text
) RETURNS uuid
LANGUAGE sql SECURITY DEFINER
SET search_path TO 'public', 'authz', 'pg_catalog'
AS $$
    UPDATE public.worker_hosts
    SET vpc_id = p_vpc_id,
        cloud_instance_id = p_cloud_instance_id
    WHERE host_id = p_host_id
      AND status <> 'terminated'
      AND cloud_instance_id IS NULL
      AND (bringup_heartbeat_at IS NULL
           OR bringup_heartbeat_at < now() - interval '90 seconds')
    RETURNING host_id
$$;

COMMENT ON FUNCTION authz.attach_orphan_worker_host_cloud_refs(uuid, text, text) IS
    'Attach a found-alive instance to any live unlinked host (g5.architecture.host_cloud_linkage), rechecking the bring-up lease and termination/linkage guards atomically. SECURITY DEFINER + BYPASSRLS owner.';

-- Miss counting, reset and abandoned termination retain their provisioning-only
-- SQL guards, including protection against hello racing a reconciliation pass.

RESET ROLE;

-- Restore the exact membership and schema CREATE privileges.
DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.orphan_linkage_authz_lookup_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.orphan_linkage_authz_lookup_had_set', true)
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

DO $$
BEGIN
    IF current_setting('g5.orphan_linkage_authz_lookup_had_authz_create', true)
        IS DISTINCT FROM 'true'
    THEN
        REVOKE CREATE ON SCHEMA authz FROM authz_lookup_definer;
    END IF;
END $$;
