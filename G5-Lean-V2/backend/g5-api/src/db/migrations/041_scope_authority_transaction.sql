-- SEC-H02: bind current scope authority to the consuming transaction.

-- The baseline deliberately hands back the deployment migrator's ability to
-- SET ROLE to the BYPASSRLS function owner. This migration creates and
-- reassigns SECURITY DEFINER functions, so take the same narrowly scoped
-- capability again and restore the exact membership/schema privilege state at
-- the end of the file. A superuser-only rehearsal would hide this requirement.
SELECT set_config(
    'g5.sec_h02_authz_lookup_membership_snapshot',
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
    'g5.sec_h02_authz_lookup_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
SELECT set_config(
    'g5.sec_h02_authz_lookup_had_authz_create',
    has_schema_privilege('authz_lookup_definer', 'authz', 'CREATE')::text,
    false
);
DO $$
BEGIN
    IF current_setting('g5.sec_h02_authz_lookup_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        EXECUTE format(
            'GRANT authz_lookup_definer TO %I WITH SET TRUE',
            SESSION_USER
        );
    END IF;
END $$;
GRANT USAGE, CREATE ON SCHEMA authz TO authz_lookup_definer;

-- Runtime roles can inspect only the authenticated actor's admitted scope context.
-- SELECT FOR SHARE needs UPDATE privilege on at least one column; these grants
-- belong to the NOLOGIN function owner, never to the tenant role.
GRANT SELECT ON authz.session_ctx, authz.user_sessions,
    public.org_memberships, public.org_default_scopes, public.user_scope_grants,
    public.workspace_permission_overrides TO authz_lookup_definer;
GRANT UPDATE (user_id) ON public.org_memberships TO authz_lookup_definer;
GRANT UPDATE (scope) ON public.org_default_scopes TO authz_lookup_definer;
GRANT UPDATE (id) ON public.user_scope_grants, public.workspace_permission_overrides
    TO authz_lookup_definer;

CREATE FUNCTION authz.lock_user_scope_authority(
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
    IF p_workspace_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM public.workspaces AS workspace
        WHERE workspace.id = p_workspace_id AND workspace.org_id = p_org_id
          AND workspace.deleted_at IS NULL
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
ALTER FUNCTION authz.lock_user_scope_authority(uuid, uuid, uuid, text[])
    OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.lock_user_scope_authority(uuid, uuid, uuid, text[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.lock_user_scope_authority(uuid, uuid, uuid, text[])
    TO g5_tenant, g5_app;

-- Restore the migrator's exact membership options and the function owner's
-- pre-migration CREATE privilege. Keep USAGE on authz for the new function's
-- session validation, plus the permanent table/function grants
-- above: those are the runtime authority this migration is installing.
DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.sec_h02_authz_lookup_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.sec_h02_authz_lookup_had_set', true)
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
    IF current_setting('g5.sec_h02_authz_lookup_had_authz_create', true)
        IS DISTINCT FROM 'true'
    THEN
        REVOKE CREATE ON SCHEMA authz FROM authz_lookup_definer;
    END IF;
END $$;
