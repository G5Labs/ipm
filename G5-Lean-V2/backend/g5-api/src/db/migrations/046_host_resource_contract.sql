-- Host-reported equal-slot contracts are required before admission.
-- Existing rows remain unqualified until the updated host reports its contract.
ALTER TABLE public.worker_hosts ADD COLUMN slot_size_reported boolean NOT NULL DEFAULT false;
GRANT SELECT (slot_size_reported) ON public.worker_hosts TO authz_lookup_definer;

SELECT set_config(
    'g5.host_sizing_membership_snapshot',
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
DO $$
BEGIN
    EXECUTE format(
        'GRANT authz_lookup_definer TO %I WITH INHERIT TRUE', SESSION_USER
    );
END $$;

CREATE OR REPLACE FUNCTION authz.list_provisioning_worker_hosts() RETURNS TABLE(host_id uuid, org_id uuid, region text, cloud_instance_id text, has_heartbeat boolean, cloud_gone_since timestamp with time zone)
    LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    SELECT wh.host_id, wh.org_id, wh.region, wh.cloud_instance_id,
           (wh.last_heartbeat IS NOT NULL AND wh.slot_size_reported), wh.cloud_gone_since
    FROM public.worker_hosts wh
    WHERE wh.status = 'provisioning' AND wh.cloud_instance_id IS NOT NULL
$$;

CREATE OR REPLACE FUNCTION authz.transition_provisioning_worker_host(p_host_id uuid, p_to_status text) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
DECLARE
    v_host_id UUID;
BEGIN
    IF p_to_status NOT IN ('ready', 'terminated') THEN
        RAISE EXCEPTION
            'transition_provisioning_worker_host: illegal target status %', p_to_status;
    END IF;
    UPDATE public.worker_hosts
    SET status = p_to_status
    WHERE host_id = p_host_id AND status = 'provisioning'
      AND (p_to_status <> 'ready' OR slot_size_reported)
    RETURNING host_id INTO v_host_id;
    RETURN v_host_id;
END;
$$;

-- Restore all prior membership options, including an existing ADMIN-only
-- grant. The temporary inherited privileges must not survive this migration.
DO $$
DECLARE
    membership_snapshot jsonb := COALESCE(
        current_setting('g5.host_sizing_membership_snapshot', true),
        '[]'
    )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    FOR membership_row IN
        SELECT grantor_role.rolname AS grantor_name,
               membership.admin_option, membership.inherit_option,
               membership.set_option
        FROM pg_auth_members membership
        JOIN pg_roles granted_role ON granted_role.oid = membership.roleid
        JOIN pg_roles member_role ON member_role.oid = membership.member
        JOIN pg_roles grantor_role ON grantor_role.oid = membership.grantor
        WHERE granted_role.rolname = 'authz_lookup_definer'
          AND member_role.rolname = SESSION_USER
    LOOP
        SELECT snapshot_row INTO prior_row
        FROM jsonb_array_elements(membership_snapshot) AS snapshot_row
        WHERE snapshot_row->>'grantor' = membership_row.grantor_name;

        IF prior_row IS NULL THEN
            EXECUTE format('REVOKE authz_lookup_definer FROM %I', SESSION_USER);
        ELSIF membership_row.admin_option IS DISTINCT FROM
                  (prior_row->>'admin_option')::boolean
            OR membership_row.inherit_option IS DISTINCT FROM
                  (prior_row->>'inherit_option')::boolean
            OR membership_row.set_option IS DISTINCT FROM
                  (prior_row->>'set_option')::boolean
        THEN
            EXECUTE format(
                'GRANT authz_lookup_definer TO %I WITH ADMIN %s, INHERIT %s, SET %s',
                SESSION_USER, prior_row->>'admin_option',
                prior_row->>'inherit_option', prior_row->>'set_option'
            );
        END IF;
    END LOOP;
END $$;
