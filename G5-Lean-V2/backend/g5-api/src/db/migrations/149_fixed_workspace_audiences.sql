-- Fixed workspace audiences (g5.workspace.audience, issue #1107).
-- Coordinated cutover: stop old API/background instances before this migration.
-- Ordinary live/deleted workspaces become organization-readable; personal and
-- formal review identity remains authoritative. See docs/workspace-audience-cutover.md.
SET LOCAL lock_timeout = '5s';

SELECT set_config(
    'g5.fixed_audience_membership_snapshot',
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
    'g5.fixed_audience_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
SELECT set_config(
    'g5.fixed_audience_had_authz_create',
    has_schema_privilege('authz_lookup_definer', 'authz', 'CREATE')::text,
    false
);
DO $$
BEGIN
    IF current_setting('g5.fixed_audience_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        EXECUTE format(
            'GRANT authz_lookup_definer TO %I WITH SET TRUE',
            SESSION_USER
        );
    END IF;
END $$;
-- Identity fixes the audience; callers still enforce active organization membership,
-- credential/scope authority and (for live reads) workspace liveness. No table access
-- or SECURITY DEFINER authority is needed to evaluate these supplied facts.
CREATE FUNCTION authz.workspace_audience_allows(
    personal_user_id uuid, owner_user_id uuid, agent_approval_review_id uuid,
    reader_user_id uuid, org_administrator boolean
) RETURNS boolean
LANGUAGE sql IMMUTABLE PARALLEL SAFE
SET search_path TO pg_catalog
AS $$
    SELECT agent_approval_review_id IS NULL
       AND (personal_user_id IS NULL OR COALESCE(org_administrator, false)
            OR COALESCE(owner_user_id = reader_user_id, false))
$$;
REVOKE ALL ON FUNCTION authz.workspace_audience_allows(uuid, uuid, uuid, uuid, boolean) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.workspace_audience_allows(uuid, uuid, uuid, uuid, boolean)
    TO g5_tenant, g5_app, authz_lookup_definer;

GRANT USAGE, CREATE ON SCHEMA authz TO authz_lookup_definer;
SET ROLE authz_lookup_definer;
CREATE OR REPLACE FUNCTION authz.lookup_workspace_runtime_durable_members(p_workspace_id uuid, p_actor_id uuid)
RETURNS TABLE(
    is_base boolean, branch_name text, repo_id uuid, root_path text,
    upstream_branch text, default_branch text, flush_id uuid, flush_repos jsonb
)
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path TO pg_catalog, public
AS $$
    SELECT workspace.is_base,
           CASE WHEN workspace.is_base THEN repo.default_branch ELSE workspace.branch_name END,
           member.repo_id, member.root_path,
           CASE WHEN parent.is_base THEN repo.default_branch ELSE parent.branch_name END,
           repo.default_branch, receipt.flush_id, receipt.repos
    FROM public.workspaces AS workspace
    JOIN public.workspace_repos AS member
      ON member.org_id = workspace.org_id AND member.workspace_id = workspace.id
    JOIN public.repos AS repo
      ON repo.id = member.repo_id AND repo.org_id = member.org_id
    LEFT JOIN public.workspaces parent
      ON parent.id = workspace.cloned_from_id AND parent.org_id = workspace.org_id
     AND parent.deleted_at IS NULL
     AND EXISTS (SELECT 1 FROM public.workspace_repos parent_member
                 WHERE parent_member.org_id = member.org_id
                   AND parent_member.workspace_id = parent.id AND parent_member.repo_id = member.repo_id)
     AND EXISTS (SELECT 1 FROM public.org_memberships membership
                 WHERE membership.org_id = workspace.org_id AND membership.user_id = p_actor_id
                   AND membership.deactivated_at IS NULL
                   AND authz.workspace_audience_allows(parent.personal_user_id, parent.owner_user_id,
                       parent.agent_approval_review_id, p_actor_id, membership.role IN ('owner', 'admin')))
    LEFT JOIN public.workspace_vms AS vm
      ON vm.org_id = workspace.org_id AND vm.workspace_id = workspace.id
    LEFT JOIN public.workspace_flushes AS receipt
      ON receipt.org_id = workspace.org_id AND receipt.workspace_id = workspace.id
     AND receipt.flush_id = vm.last_flush_id AND receipt.complete_at IS NOT NULL
     AND receipt.pruned_at IS NULL AND receipt.error IS NULL
    WHERE workspace.id = p_workspace_id AND workspace.deleted_at IS NULL
    ORDER BY member.repo_id
$$;

CREATE OR REPLACE FUNCTION authz.readable_parent_workspace(p_workspace_id uuid, p_actor_id uuid)
RETURNS uuid LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED
SET search_path TO pg_catalog, public, authz AS $$
    SELECT parent.id
    FROM public.workspaces child
    JOIN public.workspaces parent ON parent.id = child.cloned_from_id AND parent.org_id = child.org_id
    JOIN authz.session_ctx context ON context.pid = pg_backend_pid()
        AND context.user_id = p_actor_id AND child.org_id = ANY(context.org_ids)
    JOIN public.org_memberships membership ON membership.org_id = parent.org_id
        AND membership.user_id = p_actor_id
    WHERE child.id = p_workspace_id AND child.deleted_at IS NULL AND parent.deleted_at IS NULL
      AND membership.deactivated_at IS NULL
      AND authz.workspace_audience_allows(parent.personal_user_id, parent.owner_user_id,
          parent.agent_approval_review_id, p_actor_id, membership.role IN ('owner', 'admin'))
$$;
REVOKE ALL ON FUNCTION authz.readable_parent_workspace(uuid, uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.readable_parent_workspace(uuid, uuid) TO g5_tenant;

RESET ROLE;
DO $$
BEGIN
    IF current_setting('g5.fixed_audience_had_authz_create', true)
        IS DISTINCT FROM 'true'
    THEN
        REVOKE CREATE ON SCHEMA authz FROM authz_lookup_definer;
    END IF;
END $$;

-- Give back the temporary SET-capable membership exactly as it was.
DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.fixed_audience_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.fixed_audience_had_set', true)
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

-- Remove invitation revocation with its table; preserve organization membership,
-- activity, ownership and lifecycle revocation infrastructure.
DROP TRIGGER workspaces_bump_access_generation ON public.workspaces;
CREATE TRIGGER workspaces_bump_access_generation
    AFTER UPDATE OF owner_user_id, deleted_at ON public.workspaces
    FOR EACH ROW
    WHEN (OLD.owner_user_id IS DISTINCT FROM NEW.owner_user_id
       OR OLD.deleted_at IS DISTINCT FROM NEW.deleted_at)
    EXECUTE FUNCTION public.g5_bump_access_generation();
DROP TABLE public.workspace_members;
ALTER TABLE public.workspaces DROP COLUMN visibility;
