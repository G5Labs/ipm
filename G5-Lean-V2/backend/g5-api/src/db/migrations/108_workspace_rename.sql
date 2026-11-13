-- Workspace rename (g5.workspace.rename, g5.workspace.former-names). The
-- branch a working workspace owns gets its own immutable column, set from the
-- name at creation, so the name can change without touching Git. Former names
-- stay reserved by the workspace and resolve to it until its deletion
-- completes (repository membership removed after the branches are deleted).

-- 1. The owned branch. Base owns each repository's default branch and has none.
ALTER TABLE public.workspaces ADD COLUMN branch_name text;

-- Mixed DDL/data requires the documented definer-role migration protocol.
-- Preserve the migrator's exact role membership; no runtime grants remain.
SELECT set_config(
    'g5.workspace_rename_membership_snapshot',
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
    'g5.workspace_rename_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
SELECT set_config(
    'g5.workspace_rename_had_authz_create',
    has_schema_privilege('authz_lookup_definer', 'authz', 'CREATE')::text,
    false
);
DO $$
BEGIN
    IF current_setting('g5.workspace_rename_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        EXECUTE format(
            'GRANT authz_lookup_definer TO %I WITH SET TRUE',
            SESSION_USER
        );
    END IF;
END $$;
-- The backfill and the duplicate guard must see every organization's rows
-- (docs/database-migrations.md): run them as the BYPASSRLS definer role.
GRANT UPDATE (branch_name) ON public.workspaces TO authz_lookup_definer;
SET ROLE authz_lookup_definer;
UPDATE public.workspaces SET branch_name = name WHERE NOT is_base;
-- Live names must be unique within an organization regardless of case; the
-- fork service checked this in the application with the Git branch collision
-- as its backstop, which a name that is no longer a branch loses.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM public.workspaces
        WHERE deleted_at IS NULL
        GROUP BY org_id, lower(name) HAVING count(*) > 1
    ) THEN
        RAISE EXCEPTION 'live workspace names that differ only in case must be reconciled before migration 108';
    END IF;
END $$;
RESET ROLE;
REVOKE UPDATE (branch_name) ON public.workspaces FROM authz_lookup_definer;

ALTER TABLE public.workspaces
    ADD CONSTRAINT workspaces_branch_name_valid CHECK (
        CASE WHEN is_base THEN branch_name IS NULL
             ELSE public.g5_git_branch_name_is_valid(branch_name) END);

-- A working workspace inserted without a branch owns its initial name, so
-- writers that only know the name keep creating valid rows.
CREATE FUNCTION public.g5_workspace_branch_name_default() RETURNS trigger
LANGUAGE plpgsql SET search_path TO pg_catalog AS $$
BEGIN
    IF NEW.is_base THEN
        NEW.branch_name := NULL;
    ELSIF NEW.branch_name IS NULL THEN
        NEW.branch_name := NEW.name;
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER workspace_branch_name_default BEFORE INSERT ON public.workspaces
    FOR EACH ROW EXECUTE FUNCTION public.g5_workspace_branch_name_default();

-- 2. The kind and the branch never change; Base keeps its name as a product
-- rule; a working workspace's name may change. A new function replaces the
-- trigger's target: the deployed migrator need not own migration 039's
-- function, which stays in place unused.
CREATE FUNCTION public.g5_workspace_identity_immutable() RETURNS trigger
LANGUAGE plpgsql SET search_path TO pg_catalog AS $$
BEGIN
    IF NEW.is_base IS DISTINCT FROM OLD.is_base THEN
        RAISE EXCEPTION 'workspace kind is immutable' USING ERRCODE = '23514';
    END IF;
    IF NEW.branch_name IS DISTINCT FROM OLD.branch_name THEN
        RAISE EXCEPTION 'workspace branch names are immutable' USING ERRCODE = '23514';
    END IF;
    IF OLD.is_base AND NEW.name IS DISTINCT FROM OLD.name THEN
        RAISE EXCEPTION 'the Base workspace name is immutable' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END $$;
DROP TRIGGER workspace_working_name_immutable ON public.workspaces;
CREATE TRIGGER workspace_identity_immutable
    BEFORE UPDATE OF name, is_base, branch_name ON public.workspaces
    FOR EACH ROW EXECUTE FUNCTION public.g5_workspace_identity_immutable();

-- 3. Live names are unique within an organization regardless of case (the
-- guard above refused duplicates while every row was visible).
CREATE UNIQUE INDEX workspaces_org_live_name_unique
    ON public.workspaces (org_id, lower(name)) WHERE deleted_at IS NULL;

-- 4. Former names, reserved by the workspace until its deletion completes.
CREATE TABLE public.workspace_former_names (
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    name text NOT NULL CHECK (public.g5_git_branch_name_is_valid(name)),
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, name),
    CONSTRAINT workspace_former_names_workspace_fk FOREIGN KEY (org_id, workspace_id)
        REFERENCES public.workspaces(org_id, id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX workspace_former_names_org_name_unique
    ON public.workspace_former_names (org_id, lower(name));
ALTER TABLE public.workspace_former_names ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_former_names FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.workspace_former_names
    USING (org_id = ANY(authz.current_org_ids()))
    WITH CHECK (org_id = ANY(authz.current_org_ids()));
CREATE POLICY formal_review_private_select ON public.workspace_former_names
    AS RESTRICTIVE FOR SELECT TO g5_tenant
    USING (authz.formal_review_allows(workspace_id, false));
CREATE POLICY formal_review_private_insert ON public.workspace_former_names
    AS RESTRICTIVE FOR INSERT TO g5_tenant
    WITH CHECK (authz.formal_review_allows(workspace_id, true));
CREATE POLICY formal_review_private_update ON public.workspace_former_names
    AS RESTRICTIVE FOR UPDATE TO g5_tenant
    USING (authz.formal_review_allows(workspace_id, true))
    WITH CHECK (authz.formal_review_allows(workspace_id, true));
CREATE POLICY formal_review_private_delete ON public.workspace_former_names
    AS RESTRICTIVE FOR DELETE TO g5_tenant
    USING (authz.formal_review_allows(workspace_id, true));
GRANT SELECT, INSERT, UPDATE, DELETE ON public.workspace_former_names TO g5_tenant;

-- 5. Every rename, durably: the former-name set cannot show a rename back or
-- the order of renames.
CREATE TABLE public.workspace_renames (
    seq bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    previous_name text NOT NULL,
    name text NOT NULL,
    actor_user_id uuid,
    renamed_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT workspace_renames_workspace_fk FOREIGN KEY (org_id, workspace_id)
        REFERENCES public.workspaces(org_id, id) ON DELETE CASCADE
);
CREATE INDEX workspace_renames_workspace ON public.workspace_renames (workspace_id, seq DESC);
ALTER TABLE public.workspace_renames ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.workspace_renames FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.workspace_renames
    USING (org_id = ANY(authz.current_org_ids()))
    WITH CHECK (org_id = ANY(authz.current_org_ids()));
CREATE POLICY formal_review_private_select ON public.workspace_renames
    AS RESTRICTIVE FOR SELECT TO g5_tenant
    USING (authz.formal_review_allows(workspace_id, false));
CREATE POLICY formal_review_private_insert ON public.workspace_renames
    AS RESTRICTIVE FOR INSERT TO g5_tenant
    WITH CHECK (authz.formal_review_allows(workspace_id, true));
CREATE POLICY formal_review_private_update ON public.workspace_renames
    AS RESTRICTIVE FOR UPDATE TO g5_tenant
    USING (authz.formal_review_allows(workspace_id, true))
    WITH CHECK (authz.formal_review_allows(workspace_id, true));
CREATE POLICY formal_review_private_delete ON public.workspace_renames
    AS RESTRICTIVE FOR DELETE TO g5_tenant
    USING (authz.formal_review_allows(workspace_id, true));
GRANT SELECT, INSERT ON public.workspace_renames TO g5_tenant;
GRANT USAGE ON SEQUENCE public.workspace_renames_seq_seq TO g5_tenant;

-- 6. Runtime lookups read the owned branch, never the name. Same signatures
-- as migration 039; the definer role already holds SELECT on workspaces and
-- takes CREATE on the schema only for these replacements.
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
                   AND (membership.role IN ('owner', 'admin') OR parent.owner_user_id = p_actor_id
                        OR parent.visibility = 'org' OR EXISTS (
                            SELECT 1 FROM public.workspace_members invite
                            WHERE invite.workspace_id = parent.id AND invite.org_id = parent.org_id
                              AND invite.user_id = p_actor_id)))
    LEFT JOIN public.workspace_vms AS vm
      ON vm.org_id = workspace.org_id AND vm.workspace_id = workspace.id
    LEFT JOIN public.workspace_flushes AS receipt
      ON receipt.org_id = workspace.org_id AND receipt.workspace_id = workspace.id
     AND receipt.flush_id = vm.last_flush_id AND receipt.complete_at IS NOT NULL
     AND receipt.pruned_at IS NULL AND receipt.error IS NULL
    WHERE workspace.id = p_workspace_id AND workspace.deleted_at IS NULL
    ORDER BY member.repo_id
$$;

CREATE OR REPLACE FUNCTION authz.lookup_workspace_runtime_authority(p_workspace_id uuid)
RETURNS TABLE(org_id uuid, workspace_id uuid, owner_user_id uuid, branch_name text,
              deleted_at timestamptz, pending_transfer boolean)
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path TO pg_catalog, public
AS $$
    SELECT workspace.org_id, workspace.id, workspace.owner_user_id,
           CASE WHEN workspace.is_base THEN NULL ELSE workspace.branch_name END,
           workspace.deleted_at,
           EXISTS (SELECT 1 FROM public.workspace_ownership_transfers transfer
                   WHERE transfer.workspace_id = workspace.id AND transfer.status = 'pending')
    FROM public.workspaces workspace WHERE workspace.id = p_workspace_id
    FOR SHARE OF workspace
$$;
RESET ROLE;
DO $$
BEGIN
    IF current_setting('g5.workspace_rename_had_authz_create', true)
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
            current_setting('g5.workspace_rename_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.workspace_rename_had_set', true)
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
