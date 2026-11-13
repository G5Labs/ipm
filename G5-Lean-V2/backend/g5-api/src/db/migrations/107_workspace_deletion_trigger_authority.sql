-- Organization bootstrap may insert Base repository mappings but cannot lock
-- workspace rows. Run only the admission trigger with the existing restricted
-- NOLOGIN lookup owner's SELECT and UPDATE(id) privileges; leave the callable
-- deletion check SECURITY INVOKER and the caller's table permissions unchanged.
-- Preserve the migrator's role membership and the definer's schema privileges.
SELECT set_config(
    'g5.deletion_admission_authz_lookup_membership_snapshot',
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
    'g5.deletion_admission_authz_lookup_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
SELECT set_config(
    'g5.deletion_admission_authz_lookup_had_authz_create',
    has_schema_privilege('authz_lookup_definer', 'authz', 'CREATE')::text,
    false
);
DO $$
BEGIN
    IF current_setting('g5.deletion_admission_authz_lookup_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        EXECUTE format(
            'GRANT authz_lookup_definer TO %I WITH SET TRUE',
            SESSION_USER
        );
    END IF;
END $$;
GRANT USAGE, CREATE ON SCHEMA authz TO authz_lookup_definer;

ALTER FUNCTION public.g5_guard_workspace_deletion_admission() SET SCHEMA authz;
ALTER FUNCTION authz.g5_guard_workspace_deletion_admission() SECURITY DEFINER;
ALTER FUNCTION authz.g5_guard_workspace_deletion_admission()
    SET search_path = pg_catalog, pg_temp;
ALTER FUNCTION authz.g5_guard_workspace_deletion_admission() OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.g5_guard_workspace_deletion_admission() FROM PUBLIC;

DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.deletion_admission_authz_lookup_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.deletion_admission_authz_lookup_had_set', true)
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
    IF current_setting('g5.deletion_admission_authz_lookup_had_authz_create', true)
        IS DISTINCT FROM 'true'
    THEN
        REVOKE CREATE ON SCHEMA authz FROM authz_lookup_definer;
    END IF;
END $$;
