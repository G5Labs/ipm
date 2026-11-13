-- Retire the organization Owner role (issue #1106).
-- Stop and drain old writers BEFORE this migration; see
-- docs/organization-owner-retirement-cutover.md. Continuity is enforced by
-- OrgService, not a database policy trigger. Historical migration bytes stay immutable.
SET LOCAL lock_timeout = '5s';

SELECT set_config(
    'g5.owner_retirement_membership_snapshot',
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
    'g5.owner_retirement_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
SELECT set_config(
    'g5.owner_retirement_had_authz_create',
    has_schema_privilege('authz_lookup_definer', 'authz', 'CREATE')::text,
    false
);
DO $$
BEGIN
    IF current_setting('g5.owner_retirement_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        EXECUTE format(
            'GRANT authz_lookup_definer TO %I WITH SET TRUE',
            SESSION_USER
        );
    END IF;
END $$;
SELECT set_config('g5.owner_retirement_had_role_update',
    has_column_privilege('authz_lookup_definer', 'public.org_memberships', 'role', 'UPDATE')::text,
    false);
SELECT set_config('g5.owner_retirement_had_generation_update',
    has_column_privilege('authz_lookup_definer', 'public.orgs', 'access_generation', 'UPDATE')::text,
    false);
DO $$
BEGIN
    IF current_setting('g5.owner_retirement_had_role_update') IS DISTINCT FROM 'true' THEN
        GRANT UPDATE (role) ON public.org_memberships TO authz_lookup_definer;
    END IF;
    -- The existing membership trigger advances current-authority generation.
    IF current_setting('g5.owner_retirement_had_generation_update') IS DISTINCT FROM 'true' THEN
        GRANT UPDATE (access_generation) ON public.orgs TO authz_lookup_definer;
    END IF;
END $$;
LOCK TABLE public.orgs, public.org_memberships IN SHARE ROW EXCLUSIVE MODE;
SET ROLE authz_lookup_definer;
DO $$
DECLARE
    invalid_tenants text;
BEGIN
    SELECT string_agg(id::text, ', ' ORDER BY id) INTO invalid_tenants
    FROM (SELECT o.id FROM public.orgs o
          WHERE o.kind = 'tenant' AND NOT EXISTS (
              SELECT 1 FROM public.org_memberships m WHERE m.org_id=o.id
              AND m.role IN ('owner', 'admin') AND m.deactivated_at IS NULL)
          ORDER BY o.id LIMIT 20) invalid;
    IF invalid_tenants IS NOT NULL THEN
        RAISE EXCEPTION 'Owner retirement requires explicit repair: tenant organizations without an active Admin after conversion: %', invalid_tenants
            USING ERRCODE = '23514', HINT = 'Assign an intended active Admin before retrying; no user is selected or reactivated automatically.';
    END IF;
END $$;
UPDATE public.org_memberships SET role='admin' WHERE role='owner';
RESET ROLE;
DO $$
BEGIN
    IF current_setting('g5.owner_retirement_had_role_update') IS DISTINCT FROM 'true' THEN
        REVOKE UPDATE (role) ON public.org_memberships FROM authz_lookup_definer;
    END IF;
    IF current_setting('g5.owner_retirement_had_generation_update') IS DISTINCT FROM 'true' THEN
        REVOKE UPDATE (access_generation) ON public.orgs FROM authz_lookup_definer;
    END IF;
END $$;
ALTER TABLE public.org_memberships DROP CONSTRAINT org_owner_always_active;
ALTER TABLE public.org_memberships DROP CONSTRAINT org_memberships_role_check;
ALTER TABLE public.org_memberships ADD CONSTRAINT org_memberships_role_check
    CHECK (role IN ('admin', 'member', 'viewer'));

-- The generated workspace-owner eligibility column already admits Admin.
-- Preserve it and its foreign keys; its historical Owner literal is unreachable.

CREATE OR REPLACE FUNCTION public.g5_resolve_eligible_workspace_creator()
 RETURNS trigger
 LANGUAGE plpgsql
 SET search_path TO 'pg_catalog', 'public'
AS $function$
BEGIN
    IF NEW.owner_user_id IS NULL AND NEW.created_by IS NOT NULL THEN
        IF NEW.org_id = ANY (authz.current_org_ids()) THEN
            -- Runtime producers already hold an explicit org context; the
            -- deferred same-org owner FK remains the eligibility authority.
            NEW.owner_user_id := NEW.created_by;
        ELSIF has_table_privilege(
            current_user, 'public.org_memberships', 'SELECT'
        ) THEN
            -- Migration/admin fixtures may be intentionally unscoped, but
            -- only an independently verified eligible creator is projected.
            IF EXISTS (
                SELECT 1
                FROM public.org_memberships AS membership
                WHERE membership.org_id = NEW.org_id
                  AND membership.user_id = NEW.created_by
                  AND membership.role IN ('admin', 'member')
            ) THEN
                NEW.owner_user_id := NEW.created_by;
            END IF;
        END IF;
    END IF;
    RETURN NEW;
END;
$function$

;
DO $$
BEGIN
    IF current_setting('g5.owner_retirement_had_authz_create') IS DISTINCT FROM 'true' THEN
        GRANT CREATE ON SCHEMA authz TO authz_lookup_definer;
    END IF;
END $$;
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
                       parent.agent_approval_review_id, p_actor_id, membership.role = 'admin'))
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
          parent.agent_approval_review_id, p_actor_id, membership.role = 'admin')
$$;
REVOKE ALL ON FUNCTION authz.readable_parent_workspace(uuid, uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.readable_parent_workspace(uuid, uuid) TO g5_tenant;

RESET ROLE;
DO $$
BEGIN
    IF current_setting('g5.owner_retirement_had_authz_create', true)
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
            current_setting('g5.owner_retirement_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.owner_retirement_had_set', true)
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

