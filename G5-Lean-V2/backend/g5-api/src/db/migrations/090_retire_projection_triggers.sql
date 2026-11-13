-- PR B retires producers and readers; all legacy tables/columns remain for
-- rollback. The verified Lean upgrade path contains no node comments. Refuse
-- unexpected history without converting, guessing an identity, or deleting it.
-- As in 088, borrow the definer's SET capability and only the columns needed
-- for an unfiltered check. Restore exact prior authority before committing.
SELECT set_config(
    'g5.retire_projection_authz_lookup_membership_snapshot',
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
    'g5.retire_projection_authz_lookup_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
DO $$ BEGIN
    IF current_setting('g5.retire_projection_authz_lookup_had_set') <> 'true' THEN
        EXECUTE format('GRANT authz_lookup_definer TO %I WITH SET TRUE', SESSION_USER);
    END IF;
END $$;

CREATE TEMP TABLE projection_retirement_privileges ON COMMIT DROP AS
SELECT column_name,
       has_column_privilege('authz_lookup_definer', 'public.comment_index', column_name, 'SELECT') AS had_privilege
FROM (VALUES ('scope'), ('node_id')) columns(column_name);
DO $$ DECLARE entry record; BEGIN
    FOR entry IN SELECT * FROM projection_retirement_privileges WHERE NOT had_privilege
    LOOP
        EXECUTE format('GRANT SELECT (%I) ON public.comment_index TO authz_lookup_definer', entry.column_name);
    END LOOP;
END $$;
LOCK TABLE public.comment_index IN SHARE MODE;
SET ROLE authz_lookup_definer;
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM public.comment_index WHERE scope = 'node' OR node_id IS NOT NULL) THEN
        RAISE EXCEPTION 'Cannot retire ontology references: node comments remain; preserve and resolve them before retrying migration 090';
    END IF;
END $$;
-- Keep authz.lookup_repo_org_for_projection(uuid) through PR B: the previous
-- image calls it for branch-event hints. PR C removes it after rollback ends.
RESET ROLE;
DO $$ DECLARE entry record; BEGIN
    FOR entry IN SELECT * FROM projection_retirement_privileges WHERE NOT had_privilege
    LOOP
        EXECUTE format('REVOKE SELECT (%I) ON public.comment_index FROM authz_lookup_definer', entry.column_name);
    END LOOP;
END $$;
DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.retire_projection_authz_lookup_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.retire_projection_authz_lookup_had_set', true)
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

-- Keep the invalidation triggers/functions through PR B as well. The previous
-- image skips reconciliation at an unchanged Git OID unless a binding/policy
-- change deletes its checkpoint. PR B removes the long-lived projection worker
-- and request producers; PR C removes these compatibility hooks after rollback.
