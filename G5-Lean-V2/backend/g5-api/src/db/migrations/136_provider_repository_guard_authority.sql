-- Organization bootstrap may update repository identity without reading PRs.
-- Only the trigger needs to inspect pending attempts: do not grant their table
-- to bootstrap/system callers. Use the existing NOLOGIN lookup owner, preserving
-- the migrator's SET membership and the owner's temporary schema capability.
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

    GRANT SELECT (repo_id, merge_pending)
        ON public.workspace_pull_request_provider_repos TO authz_lookup_definer;
    ALTER FUNCTION public.protect_pending_provider_repository() SET SCHEMA authz;
    ALTER FUNCTION authz.protect_pending_provider_repository() SECURITY DEFINER;
    ALTER FUNCTION authz.protect_pending_provider_repository()
        SET search_path = pg_catalog, pg_temp;
    ALTER FUNCTION authz.protect_pending_provider_repository() OWNER TO authz_lookup_definer;
    REVOKE ALL ON FUNCTION authz.protect_pending_provider_repository() FROM PUBLIC;

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
END $$;
