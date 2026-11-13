-- g5.security.tenant.rls: tenant grants must never rely on query predicates
-- alone. Keep the existing system-role access explicit for internal writers
-- and maintenance; this does not change any table privileges.
ALTER TABLE public.notifications ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.notifications FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.notifications TO g5_tenant
    USING (workspace_id IN (
        SELECT id FROM public.workspaces
        WHERE org_id = ANY (authz.current_org_ids())
    ))
    WITH CHECK (workspace_id IN (
        SELECT id FROM public.workspaces
        WHERE org_id = ANY (authz.current_org_ids())
    ));
CREATE POLICY system_access ON public.notifications TO g5_app
    USING (true) WITH CHECK (true);

ALTER TABLE public.usage_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.usage_records FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.usage_records TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY system_access ON public.usage_records TO g5_app
    USING (true) WITH CHECK (true);

ALTER TABLE public.provider_quota_snapshots ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.provider_quota_snapshots FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.provider_quota_snapshots TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY system_access ON public.provider_quota_snapshots TO g5_app
    USING (true) WITH CHECK (true);

-- The catalog-wide assertion also exposes two older ENABLE-only tables.
-- Their pre-context authentication helpers used the table-owner exemption.
-- Move the unchanged, narrowly exposed helpers to the existing NOLOGIN
-- BYPASSRLS owner before forcing RLS; keep direct system/tenant access scoped.
-- Take and restore the deployment owner's temporary SET/schema capability
-- using the same membership-snapshot protocol as migration 008.
DO $$
DECLARE
    had_set boolean := pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET');
    had_create boolean := has_schema_privilege('authz_lookup_definer', 'authz', 'CREATE');
    membership_snapshot jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'grantor', grantor_role.rolname,
        'admin_option', membership.admin_option,
        'inherit_option', membership.inherit_option,
        'set_option', membership.set_option
    )), '[]'::jsonb)
      INTO membership_snapshot
      FROM pg_auth_members membership
      JOIN pg_roles granted_role ON granted_role.oid = membership.roleid
      JOIN pg_roles member_role ON member_role.oid = membership.member
      JOIN pg_roles grantor_role ON grantor_role.oid = membership.grantor
     WHERE granted_role.rolname = 'authz_lookup_definer'
       AND member_role.rolname = SESSION_USER;

    IF NOT had_set THEN
        EXECUTE format('GRANT authz_lookup_definer TO %I WITH SET TRUE', SESSION_USER);
    END IF;
    IF NOT had_create THEN
        GRANT CREATE ON SCHEMA authz TO authz_lookup_definer;
    END IF;

    GRANT SELECT (id, org_id, user_id, scopes, expires_at, revoked_at, key_hash)
        ON public.api_keys TO authz_lookup_definer;
    GRANT UPDATE (last_used_at, last_used_ip)
        ON public.api_keys TO authz_lookup_definer;
    GRANT SELECT (id, org_id, scope, workspace_id, user_id, expires_at, revoked_at, key_hash)
        ON public.worker_tokens TO authz_lookup_definer;
    -- The worker-token helper also reads workspaces, already granted in 008.
    REVOKE ALL ON FUNCTION authz.lookup_api_key_by_hash(text),
        authz.lookup_worker_token_by_hash(text), authz.touch_api_key_last_used(uuid, text)
        FROM PUBLIC;
    GRANT EXECUTE ON FUNCTION authz.lookup_api_key_by_hash(text),
        authz.lookup_worker_token_by_hash(text), authz.touch_api_key_last_used(uuid, text)
        TO g5_app;
    ALTER FUNCTION authz.lookup_api_key_by_hash(text) OWNER TO authz_lookup_definer;
    ALTER FUNCTION authz.lookup_worker_token_by_hash(text) OWNER TO authz_lookup_definer;
    ALTER FUNCTION authz.touch_api_key_last_used(uuid, text) OWNER TO authz_lookup_definer;

    IF NOT had_create THEN
        REVOKE CREATE ON SCHEMA authz FROM authz_lookup_definer;
    END IF;
    IF NOT had_set THEN
        FOR membership_row IN
            SELECT grantor_role.rolname AS grantor_name,
                   membership.admin_option, membership.inherit_option, membership.set_option
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
            ELSIF membership_row.admin_option IS DISTINCT FROM (prior_row->>'admin_option')::boolean
               OR membership_row.inherit_option IS DISTINCT FROM (prior_row->>'inherit_option')::boolean
               OR membership_row.set_option IS DISTINCT FROM (prior_row->>'set_option')::boolean
            THEN
                EXECUTE format(
                    'GRANT authz_lookup_definer TO %I WITH ADMIN %s, INHERIT %s, SET %s',
                    SESSION_USER, prior_row->>'admin_option',
                    prior_row->>'inherit_option', prior_row->>'set_option'
                );
            END IF;
        END LOOP;
    END IF;
END
$$;
ALTER TABLE public.api_keys FORCE ROW LEVEL SECURITY;
ALTER TABLE public.worker_tokens FORCE ROW LEVEL SECURITY;

-- The shared runner calls this after subsequent migrations and on no-op
-- replays. Inspect catalogs, not tenant rows, so the non-BYPASSRLS deployment
-- owner can verify the invariant. Include column-only and inherited grants,
-- and partitioned tables, across all application schemas.
CREATE FUNCTION authz.assert_tenant_tables_force_rls() RETURNS void
    LANGUAGE plpgsql
    SET search_path = pg_catalog
AS $$
DECLARE
    unprotected text;
BEGIN
    SELECT string_agg(format('%I.%I', n.nspname, c.relname), ', '
                      ORDER BY n.nspname, c.relname)
      INTO unprotected
      FROM pg_class c
      JOIN pg_namespace n ON n.oid = c.relnamespace
     WHERE c.relkind IN ('r', 'p')
       AND n.nspname <> 'information_schema'
       AND n.nspname !~ '^pg_'
       AND NOT (c.relrowsecurity AND c.relforcerowsecurity)
       AND (
           has_table_privilege('g5_tenant', c.oid,
               'SELECT,INSERT,UPDATE,DELETE,TRUNCATE,REFERENCES,TRIGGER')
           OR has_any_column_privilege('g5_tenant', c.oid,
               'SELECT,INSERT,UPDATE,REFERENCES')
       );
    IF unprotected IS NOT NULL THEN
        RAISE EXCEPTION 'Tables accessible to g5_tenant must enable and force row-level security: %',
            unprotected;
    END IF;
END
$$;

SELECT authz.assert_tenant_tables_force_rls();
