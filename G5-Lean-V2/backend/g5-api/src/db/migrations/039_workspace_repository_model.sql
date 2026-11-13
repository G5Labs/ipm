-- Workspace membership is application identity; Git owns repository versions.
-- This release requires the coordinated empty-database reset. No legacy rows or
-- operation payloads are converted, and all earlier migrations remain immutable.

SELECT set_config(
    'g5.cleanup_authz_lookup_membership_snapshot',
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
    'g5.cleanup_authz_lookup_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
SELECT set_config(
    'g5.cleanup_authz_lookup_had_authz_create',
    has_schema_privilege('authz_lookup_definer', 'authz', 'CREATE')::text,
    false
);
DO $$
BEGIN
    IF current_setting('g5.cleanup_authz_lookup_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        EXECUTE format(
            'GRANT authz_lookup_definer TO %I WITH SET TRUE',
            SESSION_USER
        );
    END IF;
END $$;
GRANT USAGE, CREATE ON SCHEMA authz TO authz_lookup_definer;


-- The parent-access function reads only the authenticated session context.
-- Its definer needs schema USAGE permanently, but CREATE only during migration.
GRANT SELECT ON authz.session_ctx TO authz_lookup_definer;
GRANT SELECT ON public.workspaces, public.workspace_members, public.org_memberships,
    public.workspace_repos, public.repos, public.workspace_vms, public.workspace_flushes
    TO authz_lookup_definer;
SET ROLE authz_lookup_definer;
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM public.workspaces) THEN
        RAISE EXCEPTION 'workspace model cleanup requires the coordinated database reset';
    END IF;
END $$;
RESET ROLE;

ALTER TABLE public.repo_mirror_refs
    DROP COLUMN oid, DROP COLUMN provider_oid, DROP COLUMN previous_oid;
ALTER TABLE public.projection_state DROP CONSTRAINT projection_state_projection_check;
ALTER TABLE public.projection_state ADD CONSTRAINT projection_state_projection_check
    CHECK (projection IN ('docs', 'ontology'));

CREATE TABLE public.remote_integrations (
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    repo_id uuid NOT NULL,
    remote_branch text NOT NULL CHECK (public.g5_git_branch_name_is_valid(remote_branch)),
    auto_pull_enabled boolean NOT NULL DEFAULT false,
    sync_paused_reason text,
    last_sync_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, repo_id),
    UNIQUE (org_id, workspace_id, repo_id),
    CONSTRAINT remote_integrations_member_fk FOREIGN KEY (org_id, workspace_id, repo_id)
        REFERENCES public.workspace_repos(org_id, workspace_id, repo_id) ON DELETE CASCADE
);
ALTER TABLE public.remote_integrations ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.remote_integrations FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.remote_integrations
    USING (org_id = ANY(authz.current_org_ids()))
    WITH CHECK (org_id = ANY(authz.current_org_ids()));
GRANT SELECT, INSERT, UPDATE, DELETE ON public.remote_integrations TO g5_tenant;

-- Replace signatures directly: application and schema deploy together after reset.
SET ROLE authz_lookup_definer;
DROP FUNCTION authz.lookup_workspace_runtime_durable_members(uuid);
CREATE FUNCTION authz.lookup_workspace_runtime_durable_members(p_workspace_id uuid, p_actor_id uuid)
RETURNS TABLE(
    is_base boolean, branch_name text, repo_id uuid, root_path text,
    upstream_branch text, default_branch text, flush_id uuid, flush_repos jsonb
)
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path TO pg_catalog, public
AS $$
    SELECT workspace.is_base,
           CASE WHEN workspace.is_base THEN repo.default_branch ELSE workspace.name END,
           member.repo_id, member.root_path,
           CASE WHEN parent.is_base THEN repo.default_branch ELSE parent.name END,
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
ALTER FUNCTION authz.lookup_workspace_runtime_durable_members(uuid, uuid) OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.lookup_workspace_runtime_durable_members(uuid, uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.lookup_workspace_runtime_durable_members(uuid, uuid) TO g5_app;

CREATE OR REPLACE FUNCTION authz.lookup_workspace_runtime_authority(p_workspace_id uuid)
RETURNS TABLE(org_id uuid, workspace_id uuid, owner_user_id uuid, branch_name text,
              deleted_at timestamptz, pending_transfer boolean)
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path TO pg_catalog, public
AS $$
    SELECT workspace.org_id, workspace.id, workspace.owner_user_id,
           CASE WHEN workspace.is_base THEN NULL ELSE workspace.name END,
           workspace.deleted_at,
           EXISTS (SELECT 1 FROM public.workspace_ownership_transfers transfer
                   WHERE transfer.workspace_id = workspace.id AND transfer.status = 'pending')
    FROM public.workspaces workspace WHERE workspace.id = p_workspace_id
    FOR SHARE OF workspace
$$;

-- Parent access is evaluated as the scoped actor. Tenant connections cannot
-- read the system-owned membership table directly; expose only the authorized
-- parent identity, never an unscoped membership lookup.
CREATE FUNCTION authz.readable_parent_workspace(p_workspace_id uuid, p_actor_id uuid)
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
      AND (membership.role IN ('owner', 'admin') OR parent.owner_user_id = p_actor_id
           OR parent.visibility = 'org' OR EXISTS (
               SELECT 1 FROM public.workspace_members invite
               WHERE invite.workspace_id = parent.id AND invite.org_id = parent.org_id
                 AND invite.user_id = p_actor_id))
$$;
REVOKE ALL ON FUNCTION authz.readable_parent_workspace(uuid, uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.readable_parent_workspace(uuid, uuid) TO g5_tenant;

RESET ROLE;

-- Re-key retained content and operation evidence; the coordinated reset means
-- there are no legacy identities to convert or old payloads to decode.
ALTER TABLE public.doc_folders
    DROP COLUMN workspace_branch_id, DROP COLUMN workspace_branch_repo_id,
    ADD COLUMN repo_id uuid NOT NULL,
    ADD CONSTRAINT doc_folders_member_fk FOREIGN KEY (org_id, workspace_id, repo_id)
        REFERENCES public.workspace_repos(org_id, workspace_id, repo_id) ON DELETE CASCADE,
    ADD CONSTRAINT doc_folders_member_path_key UNIQUE (workspace_id, repo_id, path);
ALTER TABLE public.docs
    DROP COLUMN workspace_branch_id, DROP COLUMN workspace_branch_repo_id,
    ADD COLUMN repo_id uuid,
    ADD CONSTRAINT docs_member_fk FOREIGN KEY (org_id, workspace_id, repo_id)
        REFERENCES public.workspace_repos(org_id, workspace_id, repo_id) ON DELETE CASCADE;
CREATE INDEX docs_member_path ON public.docs(workspace_id, repo_id, file_path) WHERE file_path IS NOT NULL;
CREATE UNIQUE INDEX docs_explainer_repo_path_unique ON public.docs(workspace_id, repo_id, file_path)
    WHERE doc_type = 'explainer' AND file_path IS NOT NULL;
CREATE UNIQUE INDEX docs_review_repo_path_unique ON public.docs(workspace_id, repo_id, file_path)
    WHERE doc_type = 'review' AND file_path ~ '^docs/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/reviews/agent-approval-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.md$';
ALTER TABLE public.ontology_nodes
    DROP COLUMN workspace_branch_id, DROP COLUMN workspace_branch_repo_id,
    ADD COLUMN workspace_id uuid NOT NULL, ADD COLUMN repo_id uuid NOT NULL,
    ADD CONSTRAINT ontology_nodes_member_fk FOREIGN KEY (org_id, workspace_id, repo_id)
        REFERENCES public.workspace_repos(org_id, workspace_id, repo_id) ON DELETE CASCADE,
    ADD CONSTRAINT ontology_nodes_member_parent_name_key UNIQUE(workspace_id, repo_id, parent_id, name),
    ADD CONSTRAINT ontology_nodes_scope_id_key UNIQUE(org_id, workspace_id, repo_id, id);
ALTER TABLE public.ontology_nodes DROP CONSTRAINT ontology_nodes_parent_id_fkey;
ALTER TABLE public.ontology_nodes ADD CONSTRAINT ontology_nodes_parent_fk
    FOREIGN KEY (org_id, workspace_id, repo_id, parent_id)
    REFERENCES public.ontology_nodes(org_id, workspace_id, repo_id, id) ON DELETE CASCADE;
CREATE UNIQUE INDEX ontology_nodes_root_name_key ON public.ontology_nodes(workspace_id, repo_id, name) WHERE parent_id IS NULL;
CREATE UNIQUE INDEX ontology_nodes_component_key ON public.ontology_nodes(workspace_id, repo_id, component_id) WHERE component_id IS NOT NULL;
CREATE INDEX ontology_nodes_member_parent ON public.ontology_nodes(workspace_id, repo_id, parent_id);
ALTER TABLE public.intent_record
    DROP COLUMN workspace_branch_id,
    ADD COLUMN workspace_id uuid NOT NULL, ADD COLUMN repo_id uuid NOT NULL,
    ADD CONSTRAINT intent_record_member_fk FOREIGN KEY (org_id, workspace_id, repo_id)
        REFERENCES public.workspace_repos(org_id, workspace_id, repo_id) ON DELETE CASCADE,
    ADD CONSTRAINT intent_record_workspace_id_key UNIQUE(org_id, workspace_id, id);
CREATE UNIQUE INDEX intent_record_code_key ON public.intent_record(workspace_id, repo_id, file_type, file_name, code_block_name) WHERE file_type = 'code';
CREATE UNIQUE INDEX intent_record_document_key ON public.intent_record(workspace_id, repo_id, file_type, file_name, intent_record_number) WHERE file_type IN ('view', 'ontology');
ALTER TABLE public.ontology_grounding
    DROP COLUMN workspace_branch_id,
    ADD COLUMN workspace_id uuid NOT NULL, ADD COLUMN repo_id uuid NOT NULL,
    ADD CONSTRAINT ontology_grounding_member_fk FOREIGN KEY (org_id, workspace_id, repo_id)
        REFERENCES public.workspace_repos(org_id, workspace_id, repo_id) ON DELETE CASCADE;
ALTER TABLE public.ontology_grounding
    DROP CONSTRAINT ontology_grounding_source_intent_record_id_fkey,
    DROP CONSTRAINT ontology_grounding_target_intent_record_id_fkey,
    ADD CONSTRAINT ontology_grounding_source_fk FOREIGN KEY(org_id, workspace_id, source_intent_record_id)
        REFERENCES public.intent_record(org_id, workspace_id, id) ON DELETE CASCADE,
    ADD CONSTRAINT ontology_grounding_target_fk FOREIGN KEY(org_id, workspace_id, target_intent_record_id)
        REFERENCES public.intent_record(org_id, workspace_id, id) ON DELETE CASCADE;
CREATE UNIQUE INDEX ontology_grounding_edge_key ON public.ontology_grounding(workspace_id, repo_id, source_intent_record_id, target_intent_record_id);

ALTER TABLE public.branch_integration_queue RENAME TO remote_integration_queue;
DROP POLICY tenant_isolation ON public.remote_integration_queue;
CREATE POLICY tenant_isolation ON public.remote_integration_queue
    USING (org_id = ANY(authz.current_org_ids()))
    WITH CHECK (org_id = ANY(authz.current_org_ids()));
ALTER TABLE public.remote_integration_queue
    DROP COLUMN workspace_branch_repo_id,
    ADD COLUMN workspace_id uuid NOT NULL, ADD COLUMN repo_id uuid NOT NULL,
    ADD PRIMARY KEY(workspace_id, repo_id),
    ADD CONSTRAINT remote_integration_queue_policy_fk FOREIGN KEY(org_id, workspace_id, repo_id)
        REFERENCES public.remote_integrations(org_id, workspace_id, repo_id) ON DELETE CASCADE;
CREATE INDEX remote_integration_queue_approved_order ON public.remote_integration_queue(requested_at, workspace_id, repo_id)
    WHERE trigger_kind = 'approved_rebase_publish';
ALTER TABLE public.branch_integration_remote_pair_leases RENAME TO remote_ref_leases;
ALTER TABLE public.branch_sync_runs RENAME TO remote_sync_runs;
ALTER TABLE public.remote_sync_runs
    DROP COLUMN workspace_branch_repo_id, DROP COLUMN advisory_sha,
    ADD COLUMN workspace_id uuid NOT NULL, ADD COLUMN repo_id uuid NOT NULL,
    ADD CONSTRAINT remote_sync_runs_member_fk FOREIGN KEY(org_id, workspace_id, repo_id)
        REFERENCES public.workspace_repos(org_id, workspace_id, repo_id) ON DELETE CASCADE;
CREATE INDEX remote_sync_runs_member_recent ON public.remote_sync_runs(workspace_id, repo_id, started_at DESC);
ALTER TABLE public.job_merge_runs
    DROP COLUMN workspace_branch_repo_id,
    ADD COLUMN repo_id uuid,
    ADD COLUMN resolution_task_id uuid,
    ADD CONSTRAINT job_merge_runs_resolution_task_fk FOREIGN KEY(org_id, workspace_id, resolution_task_id)
        REFERENCES public.tasks(org_id, workspace_id, id),
    ADD CONSTRAINT job_merge_runs_member_fk FOREIGN KEY(org_id, workspace_id, repo_id)
        REFERENCES public.workspace_repos(org_id, workspace_id, repo_id) ON DELETE CASCADE;
CREATE INDEX job_merge_runs_member ON public.job_merge_runs(workspace_id, repo_id, created_at DESC);
CREATE UNIQUE INDEX job_merge_runs_active_remote_member ON public.job_merge_runs(workspace_id, repo_id)
    WHERE source_kind = 'remote_tracking' AND repo_id IS NOT NULL
      AND status IN ('pending', 'running', 'waiting_for_human', 'rolling_back');

DROP TABLE public.workspace_branch_repos;
DROP TABLE public.workspace_branches;
DROP FUNCTION public.maintain_workspace_branch_repo_base_version();
DROP FUNCTION public.sync_workspace_branch_repo_from_branch();
DROP FUNCTION public.wbr_fill_default_root_path();
DROP FUNCTION public.invalidate_bound_ref_projections();
ALTER TABLE public.workspace_repos
    DROP COLUMN base_branch, DROP COLUMN head_sha_cache, DROP COLUMN backfilled_from_legacy_at;
ALTER TABLE public.workspaces
    DROP COLUMN branch_name, DROP COLUMN docs_branch_name, DROP COLUMN docs_tracking_mode,
    ADD CONSTRAINT workspaces_working_name_valid CHECK (is_base OR public.g5_git_branch_name_is_valid(name)),
    DROP CONSTRAINT workspaces_cloned_from_id_fkey,
    ADD CONSTRAINT workspaces_parent_same_org_fk FOREIGN KEY(org_id, cloned_from_id)
        REFERENCES public.workspaces(org_id, id) ON DELETE NO ACTION DEFERRABLE INITIALLY IMMEDIATE;
ALTER TABLE public.jobs DROP COLUMN branch_name, DROP COLUMN base_branch, DROP COLUMN head_commit;

CREATE FUNCTION public.g5_workspace_working_name_immutable() RETURNS trigger
LANGUAGE plpgsql SET search_path TO pg_catalog AS $$
BEGIN
    IF (NOT OLD.is_base OR NOT NEW.is_base) AND NEW.name IS DISTINCT FROM OLD.name THEN
        RAISE EXCEPTION 'working workspace names are immutable' USING ERRCODE = '23514';
    END IF;
    IF NEW.is_base IS DISTINCT FROM OLD.is_base THEN
        RAISE EXCEPTION 'workspace kind is immutable' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER workspace_working_name_immutable BEFORE UPDATE OF name, is_base ON public.workspaces
    FOR EACH ROW EXECUTE FUNCTION public.g5_workspace_working_name_immutable();

CREATE FUNCTION public.invalidate_workspace_repo_projections() RETURNS trigger
LANGUAGE plpgsql SET search_path TO pg_catalog, public AS $$
DECLARE selected_branch text;
BEGIN
    SELECT CASE WHEN workspace.is_base THEN repo.default_branch ELSE workspace.name END
      INTO selected_branch
      FROM public.workspaces workspace JOIN public.repos repo ON repo.org_id = workspace.org_id
     WHERE workspace.id = NEW.workspace_id AND workspace.org_id = NEW.org_id AND repo.id = NEW.repo_id;
    PERFORM pg_advisory_xact_lock(hashtextextended('projection:' || NEW.repo_id::text || ':refs/heads/' || selected_branch, 0));
    DELETE FROM public.projection_state WHERE repo_id = NEW.repo_id AND ref = 'refs/heads/' || selected_branch;
    RETURN NEW;
END $$;
CREATE TRIGGER invalidate_workspace_repo_projections AFTER INSERT OR UPDATE OF repo_id, role, root_path ON public.workspace_repos
    FOR EACH ROW EXECUTE FUNCTION public.invalidate_workspace_repo_projections();


-- Changing repository policy changes the Base authority ref. Invalidate both
-- projections so the new binding is reconciled even if the ref already existed.
CREATE FUNCTION public.invalidate_default_ref_projections() RETURNS trigger
LANGUAGE plpgsql SET search_path TO pg_catalog, public AS $$
DECLARE selected_ref text;
BEGIN
    IF NEW.default_branch IS DISTINCT FROM OLD.default_branch THEN
        FOR selected_ref IN SELECT DISTINCT 'refs/heads/' || branch_name
                            FROM unnest(ARRAY[OLD.default_branch, NEW.default_branch]) branch_name
                            ORDER BY 1
        LOOP
            PERFORM pg_advisory_xact_lock(hashtextextended('projection:' || NEW.id::text || ':' || selected_ref, 0));
            DELETE FROM public.projection_state WHERE repo_id = NEW.id AND ref = selected_ref;
        END LOOP;
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER invalidate_default_ref_projections AFTER UPDATE OF default_branch ON public.repos
    FOR EACH ROW EXECUTE FUNCTION public.invalidate_default_ref_projections();


-- Restore the migrator's exact membership options and the function owner's
-- pre-migration schema CREATE privilege. Keep permanent USAGE/table/function grants
-- above: those are the runtime authority this migration is installing.
DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.cleanup_authz_lookup_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.cleanup_authz_lookup_had_set', true)
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
    IF current_setting('g5.cleanup_authz_lookup_had_authz_create', true)
        IS DISTINCT FROM 'true'
    THEN
        REVOKE CREATE ON SCHEMA authz FROM authz_lookup_definer;
    END IF;
END $$;

-- Review evidence retains exact Git cuts keyed by workspace/repository.
CREATE OR REPLACE FUNCTION public.approval_review_head_vector_v1_is_valid(value jsonb) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $_$
    SELECT CASE
        WHEN jsonb_typeof(value) <> 'array' THEN false
        ELSE jsonb_array_length(value) > 0
            AND NOT EXISTS (
                SELECT 1
                FROM jsonb_array_elements(value) AS heads(head)
                WHERE NOT (
                    approval_review_jsonb_has_exact_keys_v1(
                        head,
                        ARRAY['repoId', 'branchName', 'workspaceId', 'headCommit']
                    )
                    AND jsonb_typeof(head -> 'repoId') = 'string'
                    AND btrim(head ->> 'repoId') <> ''
                    AND jsonb_typeof(head -> 'branchName') = 'string'
                    AND btrim(head ->> 'branchName') <> ''
                    AND jsonb_typeof(head -> 'workspaceId') = 'string'
                    AND (head ->> 'workspaceId') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    AND jsonb_typeof(head -> 'headCommit') = 'string'
                    AND (head ->> 'headCommit') ~ '^([0-9a-f]{40}|[0-9a-f]{64})$'
                )
            )
            AND (
                SELECT COUNT(*) = COUNT(DISTINCT head ->> 'repoId')
                FROM jsonb_array_elements(value) AS heads(head)
            )
    END
$_$;

CREATE OR REPLACE FUNCTION public.approval_review_head_vector_v2_is_valid(value jsonb) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $_$
    SELECT CASE
        WHEN jsonb_typeof(value) <> 'array' THEN false
        ELSE jsonb_array_length(value) BETWEEN 1 AND 1024
            AND octet_length(value::TEXT) <= 262144
            AND NOT EXISTS (
                SELECT 1
                FROM jsonb_array_elements(value) AS heads(head)
                WHERE NOT (
                    approval_review_jsonb_has_exact_keys_v1(
                        head,
                        ARRAY['repoId', 'branchName', 'workspaceId', 'headCommit']
                    )
                    AND jsonb_typeof(head -> 'repoId') = 'string'
                    AND (head ->> 'repoId') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    AND jsonb_typeof(head -> 'branchName') = 'string'
                    AND btrim(head ->> 'branchName') <> ''
                    AND octet_length(head ->> 'branchName') <= 1024
                    AND jsonb_typeof(head -> 'workspaceId') = 'string'
                    AND (head ->> 'workspaceId') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    AND jsonb_typeof(head -> 'headCommit') = 'string'
                    AND (head ->> 'headCommit') ~ '^([0-9a-f]{40}|[0-9a-f]{64})$'
                )
            )
            AND (
                SELECT COUNT(*) = COUNT(DISTINCT head ->> 'repoId')
                FROM jsonb_array_elements(value) AS heads(head)
            )
    END
$_$;
