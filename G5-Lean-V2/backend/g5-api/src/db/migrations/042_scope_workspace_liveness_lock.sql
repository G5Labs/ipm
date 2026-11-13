-- SEC-H02: serialize workspace authority with soft deletion.
-- Keep migration 039 immutable. Replacing its existing definer-owned function
-- needs inherited owner privileges, temporarily acquired and restored below.
-- CREATE OR REPLACE preserves the function owner and EXECUTE ACL; migration
-- 008 already grants the definer SELECT and UPDATE(id) on workspaces.

SELECT set_config(
    'g5.sec_h02_workspace_membership_snapshot',
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

CREATE OR REPLACE FUNCTION authz.lock_user_scope_authority(
    p_user_id uuid, p_org_id uuid, p_workspace_id uuid, p_scopes text[]
) RETURNS TABLE (role text, granted_scopes text[])
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    actor_role text;
    grant_row record;
    additive_scopes text[] := '{}';
    expiring_grants jsonb := '[]';
BEGIN
    -- No caller-controlled GUC is authority. Bind to the verified ScopedDb
    -- backend context and its live session before touching private auth tables.
    IF NOT EXISTS (
        SELECT 1 FROM authz.session_ctx AS ctx
        JOIN authz.user_sessions AS session ON session.session_id = ctx.session_id
        WHERE ctx.pid = pg_backend_pid() AND ctx.user_id = p_user_id
          AND p_org_id = ANY(ctx.org_ids)
          AND session.user_id = ctx.user_id AND session.org_ids = ctx.org_ids
          AND session.scopes = ctx.scopes AND session.expires_at > clock_timestamp()
    ) THEN
        RETURN;
    END IF;
    -- Workspace liveness is part of the resolved authority context. Retain it
    -- through admission/commit, including collaborative and system-pool paths
    -- that do not enter the owner-only workspace mutation helper.
    IF p_workspace_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM public.workspaces AS workspace
        WHERE workspace.id = p_workspace_id AND workspace.org_id = p_org_id
          AND workspace.deleted_at IS NULL
        FOR SHARE OF workspace
    ) THEN
        RETURN;
    END IF;

    SELECT membership.role INTO actor_role
    FROM public.org_memberships AS membership
    WHERE membership.org_id = p_org_id AND membership.user_id = p_user_id
    FOR SHARE;
    IF NOT FOUND THEN
        RETURN;
    END IF;

    -- Keep every row contributing requested authority locked until the caller's
    -- mutation commits. Rows deleted/changed while admission waits are rechecked
    -- by FOR SHARE; only the locked versions contribute to the result.
    FOR grant_row IN
        SELECT defaults.scope FROM public.org_default_scopes AS defaults
        WHERE defaults.org_id = p_org_id AND defaults.scope = ANY(p_scopes)
        ORDER BY defaults.scope FOR SHARE
    LOOP
        additive_scopes := array_append(additive_scopes, grant_row.scope);
    END LOOP;
    FOR grant_row IN
        SELECT grants.scope, grants.expires_at FROM public.user_scope_grants AS grants
        WHERE grants.org_id = p_org_id AND grants.user_id = p_user_id
          AND grants.scope = ANY(p_scopes)
        ORDER BY grants.scope FOR SHARE
    LOOP
        expiring_grants := expiring_grants || jsonb_build_object(
            'scope', grant_row.scope, 'expires_at', grant_row.expires_at);
    END LOOP;
    IF p_workspace_id IS NOT NULL THEN
        FOR grant_row IN
            SELECT overrides.scope FROM public.workspace_permission_overrides AS overrides
            WHERE overrides.workspace_id = p_workspace_id AND overrides.user_id = p_user_id
              AND overrides.scope = ANY(p_scopes)
            ORDER BY overrides.scope FOR SHARE
        LOOP
            additive_scopes := array_append(additive_scopes, grant_row.scope);
        END LOOP;
    END IF;
    -- Evaluate expiry after all lock waits, against wall time rather than the
    -- transaction-start timestamp. Role defaults remain defined by AuthService.
    RETURN QUERY SELECT actor_role, additive_scopes || ARRAY(
        SELECT grants.scope FROM jsonb_to_recordset(expiring_grants)
            AS grants(scope text, expires_at timestamptz)
        WHERE grants.expires_at IS NULL OR grants.expires_at > clock_timestamp()
    );
END;
$$;

-- Restore all prior membership options, including an existing ADMIN-only
-- grant. The temporary inherited privileges must not survive this migration.
DO $$
DECLARE
    membership_snapshot jsonb := COALESCE(
        current_setting('g5.sec_h02_workspace_membership_snapshot', true),
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
