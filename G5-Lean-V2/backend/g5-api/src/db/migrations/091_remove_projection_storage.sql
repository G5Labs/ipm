-- PR C contracts the schema after PR B removed every projection producer.
-- Refuse unexpected node discussion without losing its history, including rows
-- hidden from the NOSUPERUSER/NOBYPASSRLS migrator. Keep the table locked from
-- the unfiltered check through contraction; restore borrowed authority exactly.
SELECT set_config(
    'g5.retire_projection_storage_authz_lookup_membership_snapshot',
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
    'g5.retire_projection_storage_authz_lookup_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
DO $$ BEGIN
    IF current_setting('g5.retire_projection_storage_authz_lookup_had_set') <> 'true' THEN
        EXECUTE format('GRANT authz_lookup_definer TO %I WITH SET TRUE', SESSION_USER);
    END IF;
END $$;

CREATE TEMP TABLE projection_storage_retirement_privileges ON COMMIT DROP AS
SELECT column_name,
       has_column_privilege('authz_lookup_definer', 'public.comment_index', column_name, 'SELECT') AS had_privilege
FROM (VALUES ('scope'), ('node_id')) columns(column_name);
DO $$ DECLARE entry record; BEGIN
    FOR entry IN SELECT * FROM projection_storage_retirement_privileges WHERE NOT had_privilege
    LOOP
        EXECUTE format('GRANT SELECT (%I) ON public.comment_index TO authz_lookup_definer', entry.column_name);
    END LOOP;
END $$;
LOCK TABLE public.comment_index IN SHARE MODE;
SET ROLE authz_lookup_definer;
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM public.comment_index WHERE scope = 'node' OR node_id IS NOT NULL) THEN
        RAISE EXCEPTION 'Cannot retire ontology references: node comments remain; preserve and resolve them before retrying migration 091';
    END IF;
END $$;
-- The lookup belongs to authz_lookup_definer, not the deployment migrator.
DROP FUNCTION authz.lookup_repo_org_for_projection(uuid);
RESET ROLE;
DO $$ DECLARE entry record; BEGIN
    FOR entry IN SELECT * FROM projection_storage_retirement_privileges WHERE NOT had_privilege
    LOOP
        EXECUTE format('REVOKE SELECT (%I) ON public.comment_index FROM authz_lookup_definer', entry.column_name);
    END LOOP;
END $$;
DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.retire_projection_storage_authz_lookup_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.retire_projection_storage_authz_lookup_had_set', true)
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

-- Remove the only remaining projection writers before removing their storage.
DROP TRIGGER invalidate_workspace_repo_projections ON public.workspace_repos;
DROP TRIGGER invalidate_default_ref_projections ON public.repos;
DROP FUNCTION public.invalidate_workspace_repo_projections();
DROP FUNCTION public.invalidate_default_ref_projections();

DROP INDEX public.idx_comment_node;
ALTER TABLE public.comment_index
    DROP CONSTRAINT comment_index_node_has_no_repo,
    DROP CONSTRAINT comment_index_scope_check,
    DROP COLUMN node_id,
    ADD CONSTRAINT comment_index_scope_check CHECK (scope IN ('line', 'suggestion'));
ALTER TABLE public.tasks DROP COLUMN node_id;
ALTER TABLE public.docs
    DROP CONSTRAINT docs_scope_mode_check,
    DROP COLUMN scope_node_id,
    DROP COLUMN scope_mode;

-- Referencing columns are gone; plain DROP refuses any unexpected dependency.
-- Retain docs, tasks, file discussion and every substantive field they contain.
DROP TABLE public.doc_node_links;
DROP TABLE public.ontology_nodes;
DROP TABLE public.projection_state;
