-- 008_lean_mvp_execution_cutover.sql — finalize the workspace-VM task and
-- shared-session model after every retained producer has moved off per-task
-- workers and sticky chat runtimes.

-- Keep the last retained-root identity after an incarnation releases its
-- active host placement. Delete and ownership transfer must prove that this
-- exact generation has been purged before refs or owner authority change.
ALTER TABLE public.workspace_vms
    ADD COLUMN retained_root_host_id uuid,
    ADD COLUMN retained_root_cache_generation bigint,
    ADD CONSTRAINT workspace_vms_retained_root_pair CHECK (
        (retained_root_host_id IS NULL) =
        (retained_root_cache_generation IS NULL)
        AND (
            retained_root_cache_generation IS NULL
            OR retained_root_cache_generation >= 0
        )
    );

-- Explicit force-delete is the one owner action allowed to proceed after a
-- failed durability capture. Keep that reason distinct from operator drain or
-- capacity actions, and retain the resulting loss record on the workspace
-- tombstone after its operational VM row is removed.
ALTER TABLE public.workspace_vms
    DROP CONSTRAINT workspace_vms_stop_reason_check,
    DROP CONSTRAINT workspace_vms_pending_stop_reason_check,
    ADD CONSTRAINT workspace_vms_stop_reason_check CHECK (
        stop_reason IS NULL OR stop_reason IN (
            'user', 'restart', 'transfer', 'idle', 'drain', 'rollout',
            'credential', 'capacity', 'delete'
        )
    ),
    ADD CONSTRAINT workspace_vms_pending_stop_reason_check CHECK (
        pending_stop_reason IS NULL OR pending_stop_reason IN (
            'user', 'restart', 'transfer', 'idle', 'drain', 'rollout',
            'credential', 'capacity', 'delete'
        )
    );

ALTER TABLE public.workspaces
    ADD COLUMN deletion_operation_id uuid,
    ADD COLUMN deletion_loss jsonb,
    ADD CONSTRAINT workspaces_deletion_loss_shape CHECK (
        deletion_loss IS NULL OR jsonb_typeof(deletion_loss) = 'object'
    );

-- The baseline deliberately hands back the deployment migrator's ability to
-- SET ROLE to the BYPASSRLS function owner. This migration creates and
-- reassigns SECURITY DEFINER functions, so take the same narrowly scoped
-- capability again and restore the exact membership/schema privilege state at
-- the end of the file. A superuser-only rehearsal would hide this requirement.
SELECT set_config(
    'g5.mvp1_authz_lookup_membership_snapshot',
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
    'g5.mvp1_authz_lookup_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
SELECT set_config(
    'g5.mvp1_authz_lookup_had_authz_usage',
    has_schema_privilege('authz_lookup_definer', 'authz', 'USAGE')::text,
    false
);
SELECT set_config(
    'g5.mvp1_authz_lookup_had_authz_create',
    has_schema_privilege('authz_lookup_definer', 'authz', 'CREATE')::text,
    false
);
DO $$
BEGIN
    IF current_setting('g5.mvp1_authz_lookup_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        EXECUTE format(
            'GRANT authz_lookup_definer TO %I WITH SET TRUE',
            SESSION_USER
        );
    END IF;
END $$;
GRANT USAGE, CREATE ON SCHEMA authz TO authz_lookup_definer;

-- ScopedDb clears its per-backend RLS context immediately before COMMIT. The
-- P4.1 cross-table constraint triggers are deferred until that COMMIT, so they
-- must resolve their referenced tenant rows through the existing NOLOGIN,
-- BYPASSRLS trigger owner. Keep the functions narrow and leave the runtime
-- system role unable to query these tenant tables directly.
GRANT SELECT ON public.workspaces, public.repos, public.tasks,
    public.chat_sessions, public.workspace_repos,
    public.workspace_ownership_transfers, public.workspace_vms,
    public.workspace_flushes TO authz_lookup_definer;
-- PostgreSQL requires UPDATE privilege on at least one column for SELECT FOR
-- SHARE. The definer is NOLOGIN and the only exposed function is read-only.
GRANT UPDATE (id) ON public.workspaces TO authz_lookup_definer;

-- The authenticated host-control stream runs on the restricted system pool
-- before any tenant context exists. It is the authoritative writer for host
-- heartbeats, inventory and drained state, so give only that system role's
-- inherited g5_app identity access to the host registry.
CREATE POLICY system_access ON public.worker_hosts
    TO g5_app USING (true) WITH CHECK (true);
GRANT SELECT, UPDATE ON public.worker_hosts TO g5_app;

-- WorkspaceRuntime runs on the restricted system pool. Keep its authority and
-- durable-read access narrow instead of adding a broad g5_app RLS bypass. The
-- authority lookup takes the same shared workspace-row lock that the caller's
-- transaction previously took directly, so ownership transfer still fences
-- every admitted runtime RPC until that transaction commits.
CREATE FUNCTION authz.lookup_workspace_runtime_authority(p_workspace_id uuid)
RETURNS TABLE(
    org_id uuid,
    workspace_id uuid,
    owner_user_id uuid,
    branch_name text,
    deleted_at timestamptz,
    pending_transfer boolean
)
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path TO pg_catalog, public
AS $$
    SELECT workspace.org_id,
           workspace.id,
           workspace.owner_user_id,
           workspace.branch_name,
           workspace.deleted_at,
           EXISTS (
               SELECT 1
               FROM public.workspace_ownership_transfers AS transfer
               WHERE transfer.workspace_id = workspace.id
                 AND transfer.status = 'pending'
           )
    FROM public.workspaces AS workspace
    WHERE workspace.id = p_workspace_id
    FOR SHARE OF workspace
$$;
ALTER FUNCTION authz.lookup_workspace_runtime_authority(uuid)
    OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.lookup_workspace_runtime_authority(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.lookup_workspace_runtime_authority(uuid) TO g5_app;

CREATE FUNCTION authz.lookup_workspace_runtime_durable_members(p_workspace_id uuid)
RETURNS TABLE(
    is_base boolean,
    branch_name text,
    repo_id uuid,
    root_path text,
    base_branch text,
    head_sha_cache text,
    default_branch text,
    flush_id uuid,
    flush_repos jsonb
)
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path TO pg_catalog, public
AS $$
    SELECT workspace.is_base,
           workspace.branch_name,
           member.repo_id,
           member.root_path,
           member.base_branch,
           member.head_sha_cache,
           repo.default_branch,
           receipt.flush_id,
           receipt.repos
    FROM public.workspaces AS workspace
    JOIN public.workspace_repos AS member
      ON member.org_id = workspace.org_id
     AND member.workspace_id = workspace.id
    JOIN public.repos AS repo
      ON repo.id = member.repo_id
     AND repo.org_id = member.org_id
    LEFT JOIN public.workspace_vms AS vm
      ON vm.org_id = workspace.org_id
     AND vm.workspace_id = workspace.id
    LEFT JOIN public.workspace_flushes AS receipt
      ON receipt.org_id = workspace.org_id
     AND receipt.flush_id = vm.last_flush_id
     AND receipt.complete_at IS NOT NULL
     AND receipt.pruned_at IS NULL
    WHERE workspace.id = p_workspace_id
      AND workspace.deleted_at IS NULL
    ORDER BY member.repo_id
$$;
ALTER FUNCTION authz.lookup_workspace_runtime_durable_members(uuid)
    OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.lookup_workspace_runtime_durable_members(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.lookup_workspace_runtime_durable_members(uuid) TO g5_app;

-- PostgreSQL requires a prospective function owner to have CREATE on its
-- schema. The deployment migrator cannot grant CREATE on the platform-owned
-- public schema, so replace the P4.1 trigger functions under the authz schema
-- that the migrator owns and can delegate narrowly to the BYPASSRLS definer.
DROP TRIGGER workspace_forks_same_org_members ON public.workspace_forks;
DROP TRIGGER session_inputs_same_session_task ON public.session_inputs;
DROP TRIGGER workspace_vms_validate_start_intent ON public.workspace_vms;
DROP TRIGGER workspace_flushes_same_org_members ON public.workspace_flushes;

CREATE FUNCTION authz.g5_validate_workspace_fork_members() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'pg_catalog', 'public'
    AS $$
DECLARE
    member jsonb;
BEGIN
    FOR member IN SELECT * FROM jsonb_array_elements(NEW.repo_manifest)
    LOOP
        IF NOT EXISTS (
            SELECT 1 FROM public.repos
            WHERE org_id = NEW.org_id AND id = (member ->> 'repo_id')::uuid
        ) THEN
            RAISE EXCEPTION 'workspace fork member is not an existing same-org repository'
                USING ERRCODE = '23503';
        END IF;
    END LOOP;
    RETURN NULL;
END;
$$;
ALTER FUNCTION authz.g5_validate_workspace_fork_members()
    OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.g5_validate_workspace_fork_members() FROM PUBLIC;

CREATE FUNCTION authz.g5_validate_session_input_binding() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'pg_catalog', 'public'
    AS $$
BEGIN
    IF NEW.task_id IS NOT NULL AND NOT EXISTS (
        SELECT 1
        FROM public.tasks AS task
        JOIN public.chat_sessions AS session
          ON session.org_id = NEW.org_id AND session.id = NEW.session_id
        WHERE task.org_id = NEW.org_id
          AND task.id = NEW.task_id
          AND task.workspace_id = session.workspace_id
          AND (task.chat_session_id IS NULL OR task.chat_session_id = NEW.session_id)
    ) THEN
        RAISE EXCEPTION 'session input task must belong to the same workspace/session'
            USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END;
$$;
ALTER FUNCTION authz.g5_validate_session_input_binding()
    OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.g5_validate_session_input_binding() FROM PUBLIC;

CREATE FUNCTION authz.g5_validate_runtime_intent_binding() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'pg_catalog', 'public'
    AS $$
DECLARE
    workspace_owner uuid;
    workspace_generation bigint;
BEGIN
    SELECT owner_user_id, lifecycle_generation
    INTO workspace_owner, workspace_generation
    FROM public.workspaces
    WHERE org_id = NEW.org_id AND id = NEW.workspace_id;

    IF NEW.start_intent IS NOT NULL
       AND NEW.start_intent ->> 'status' IN ('pending', 'queued_for_capacity', 'starting')
       AND (
           workspace_owner IS NULL
           OR (NEW.start_intent ->> 'owner_user_id')::uuid <> workspace_owner
           OR (NEW.start_intent ->> 'lifecycle_generation')::bigint <> workspace_generation
           OR (NEW.start_intent ->> 'expires_at')::timestamptz <= now()
           OR (NEW.start_intent ->> 'created_at')::timestamptz >=
              (NEW.start_intent ->> 'expires_at')::timestamptz
       )
    THEN
        RAISE EXCEPTION 'active start intent must bind the current owner and lifecycle generation'
            USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END;
$$;
ALTER FUNCTION authz.g5_validate_runtime_intent_binding()
    OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.g5_validate_runtime_intent_binding() FROM PUBLIC;

CREATE FUNCTION authz.g5_validate_flush_repositories() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'pg_catalog', 'public'
    AS $$
DECLARE
    required_repo_id uuid;
BEGIN
    FOREACH required_repo_id IN ARRAY NEW.required_repo_ids
    LOOP
        IF NOT EXISTS (
            SELECT 1 FROM public.workspace_repos AS mapped
            WHERE mapped.org_id = NEW.org_id
              AND mapped.workspace_id = NEW.workspace_id
              AND mapped.repo_id = required_repo_id
        ) THEN
            RAISE EXCEPTION 'flush repository is not a same-org workspace member'
                USING ERRCODE = '23503';
        END IF;
    END LOOP;
    RETURN NULL;
END;
$$;
ALTER FUNCTION authz.g5_validate_flush_repositories()
    OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.g5_validate_flush_repositories() FROM PUBLIC;

CREATE CONSTRAINT TRIGGER workspace_forks_same_org_members
    AFTER INSERT OR UPDATE OF org_id, repo_manifest ON public.workspace_forks
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION authz.g5_validate_workspace_fork_members();
CREATE CONSTRAINT TRIGGER session_inputs_same_session_task
    AFTER INSERT OR UPDATE OF org_id, session_id, task_id, attempt ON public.session_inputs
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION authz.g5_validate_session_input_binding();
CREATE CONSTRAINT TRIGGER workspace_vms_validate_start_intent
    AFTER INSERT OR UPDATE OF org_id, workspace_id, start_intent ON public.workspace_vms
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION authz.g5_validate_runtime_intent_binding();
CREATE CONSTRAINT TRIGGER workspace_flushes_same_org_members
    AFTER INSERT OR UPDATE OF org_id, workspace_id, required_repo_ids ON public.workspace_flushes
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION authz.g5_validate_flush_repositories();

DROP FUNCTION public.g5_validate_workspace_fork_members();
DROP FUNCTION public.g5_validate_session_input_binding();
DROP FUNCTION public.g5_validate_runtime_intent_binding();
DROP FUNCTION public.g5_validate_flush_repositories();

COMMENT ON FUNCTION authz.g5_validate_runtime_intent_binding() IS
    'Deferred cross-table workspace VM intent validator; SECURITY DEFINER is required because ScopedDb clears tenant RLS context before COMMIT';

-- Preserve the retired mutable-question records as immutable task history.
-- provider_requests is the only active question/permission state after this
-- migration.
INSERT INTO public.task_events(id, org_id, task_id, entry_type, content, metadata, created_at)
SELECT question.id,
       question.org_id,
       question.task_id,
       'output',
       COALESCE(
           NULLIF(question.payload ->> 'prompt', ''),
           NULLIF(question.payload ->> 'question', ''),
           'Historical ' || question.kind
       ),
       jsonb_strip_nulls(jsonb_build_object(
           'kind', 'legacy_question_audit',
           'question_kind', question.kind,
           'payload', question.payload,
           'status', question.status,
           'answer', question.answer,
           'expires_at', question.expires_at,
           'answered_by', question.answered_by,
           'answered_at', question.answered_at,
           'consumed_at', question.consumed_at,
           'successor_task_id', question.successor_task_id
       )),
       question.created_at
FROM public.task_questions AS question
ON CONFLICT (id) DO NOTHING;

-- Reconcile every task created while the additive P4.1 compatibility schema
-- was live. Jobs are optional, so the workspace and creator fall back to the
-- current workspace authority for jobless rows.
UPDATE public.tasks AS task
SET workspace_id = COALESCE(task.workspace_id, job.workspace_id),
    created_by = COALESCE(task.created_by, job.created_by, workspace.owner_user_id),
    kind = COALESCE(task.kind, CASE task.task_type
        WHEN 'chat' THEN 'agent'
        WHEN 'regular' THEN 'agent'
        WHEN 'sync' THEN 'agent'
        WHEN 'explainer_generate' THEN 'agent'
        WHEN 'bootstrap' THEN 'command'
        WHEN 'golden_builder' THEN 'command'
        WHEN 'merge' THEN 'command'
        WHEN 'preview' THEN 'preview'
        ELSE NULL
    END),
    spec = CASE
        WHEN task.spec ? 'dependency_failure_policy' THEN task.spec
        ELSE task.spec || jsonb_build_object('dependency_failure_policy', 'fail_dependents')
    END
FROM public.jobs AS job
LEFT JOIN public.workspaces AS workspace
  ON workspace.org_id = job.org_id AND workspace.id = job.workspace_id
WHERE task.job_id = job.id;

UPDATE public.tasks AS task
SET created_by = workspace.owner_user_id
FROM public.workspaces AS workspace
WHERE task.job_id IS NULL
  AND task.org_id = workspace.org_id
  AND task.workspace_id = workspace.id
  AND task.created_by IS NULL;

-- Legacy execution states have no valid workspace incarnation. Preserve their
-- terminal outcome; otherwise hold them until an owner explicitly starts or
-- continues the workspace.
UPDATE public.tasks
SET hold_reason = CASE
        WHEN status IN ('claimed', 'paused', 'blocked', 'recovering', 'recovery_blocked')
            THEN COALESCE(hold_reason, 'start_required')
        WHEN status = 'running' AND vm_id IS NULL
            THEN COALESCE(hold_reason, 'start_required')
        ELSE hold_reason
    END,
    start_confirmed = CASE WHEN status = 'running' AND vm_id IS NULL THEN false
                           ELSE start_confirmed END,
    status = CASE WHEN status = 'running' AND vm_id IS NULL THEN 'waiting'
                  ELSE CASE status
                      WHEN 'completed' THEN 'succeeded'
                      WHEN 'rolled_back' THEN 'cancelled'
                      WHEN 'pending' THEN 'queued'
                      WHEN 'claimed' THEN 'waiting'
                      WHEN 'paused' THEN 'waiting'
                      WHEN 'blocked' THEN 'waiting'
                      WHEN 'recovering' THEN 'waiting'
                      WHEN 'recovery_blocked' THEN 'waiting'
                      ELSE status
                  END
             END;

-- A shared session is mandatory for every agent task. Reuse the deterministic
-- legacy task link selected by P4.1; create a session only for the remaining
-- reconciled rows.
WITH missing AS (
    SELECT task.id AS task_id,
           task.org_id,
           task.workspace_id,
           task.created_by,
           gen_random_uuid() AS session_id
    FROM public.tasks AS task
    WHERE task.kind = 'agent'
      AND task.chat_session_id IS NULL
      AND task.workspace_id IS NOT NULL
      AND task.created_by IS NOT NULL
), inserted AS (
    INSERT INTO public.chat_sessions(
        id, workspace_id, scope_type, title, org_id, created_by,
        permission_policy, session_manifest_generation, created_at, updated_at
    )
    SELECT session_id,
           workspace_id,
           'workspace',
           'Migrated agent session',
           org_id,
           created_by,
           jsonb_build_object(
               'schema_version', 'g5.agent.permission-policy/v1',
               'permission_mode', 'default'
           ),
           0,
           now(),
           now()
    FROM missing
    RETURNING id
)
UPDATE public.tasks AS task
SET chat_session_id = missing.session_id
FROM missing
WHERE task.id = missing.task_id
  AND EXISTS (SELECT 1 FROM inserted WHERE inserted.id = missing.session_id);

DO $$
DECLARE
    unresolved_tasks bigint;
    unresolved_agent_sessions bigint;
    unresolved_workspaces bigint;
BEGIN
    SELECT count(*) INTO unresolved_tasks
    FROM public.tasks
    WHERE workspace_id IS NULL OR kind IS NULL OR created_by IS NULL;

    SELECT count(*) INTO unresolved_agent_sessions
    FROM public.tasks
    WHERE kind = 'agent' AND chat_session_id IS NULL;

    SELECT count(*) INTO unresolved_workspaces
    FROM public.workspaces
    WHERE deleted_at IS NULL
      AND (owner_user_id IS NULL
           OR (NOT is_base AND (
               NOT public.g5_git_branch_name_is_valid(branch_name)
               OR name IS DISTINCT FROM branch_name
           )));

    IF unresolved_tasks <> 0
       OR unresolved_agent_sessions <> 0
       OR unresolved_workspaces <> 0 THEN
        RAISE EXCEPTION
            'Lean MVP cutover blocked: unresolved_tasks=%, unresolved_agent_sessions=%, unresolved_workspaces=%; inspect p4_1_task_migration_preflight and p4_1_workspace_migration_preflight',
            unresolved_tasks, unresolved_agent_sessions, unresolved_workspaces
            USING ERRCODE = '23514';
    END IF;
END;
$$;

ALTER TABLE public.tasks
    ADD COLUMN base_sha text;

UPDATE public.tasks
SET base_sha = NULLIF(base_commit, '')
WHERE base_commit IS NOT NULL;

DROP TRIGGER IF EXISTS trg_tasks_guard_sticky_execution_admission ON public.tasks;
DROP TRIGGER IF EXISTS tasks_project_legacy_target_fields ON public.tasks;
DROP TRIGGER IF EXISTS tasks_stamp_async_decision_policy ON public.tasks;
DROP TRIGGER IF EXISTS chat_sessions_project_legacy_task_link ON public.chat_sessions;
DROP TRIGGER IF EXISTS task_execution_attempts_project_v9_attempt
    ON public.task_execution_attempts;
DROP TRIGGER IF EXISTS trg_clear_terminal_task_agent_auth ON public.tasks;

DROP FUNCTION IF EXISTS authz.guard_sticky_task_execution_admission();
DROP FUNCTION IF EXISTS authz.prepare_prior_generation_sticky_runtime_retirement(text);
DROP FUNCTION IF EXISTS authz.retire_prelocked_prior_generation_sticky_runtimes(
    text, bigint, uuid[], uuid[]
);
DROP FUNCTION IF EXISTS authz.retire_prior_generation_sticky_runtimes(text, bigint);
DROP FUNCTION IF EXISTS public.g5_project_legacy_task_fields();
DROP FUNCTION IF EXISTS public.stamp_task_async_decision_policy();
DROP FUNCTION IF EXISTS public.g5_link_legacy_task_session();
DROP FUNCTION IF EXISTS public.g5_project_legacy_execution_attempt();
DROP FUNCTION IF EXISTS public.clear_terminal_task_agent_auth();
DROP VIEW IF EXISTS public.p4_1_task_migration_preflight;

ALTER TABLE public.tasks DROP CONSTRAINT tasks_status_check;
ALTER TABLE public.tasks
    ADD CONSTRAINT tasks_status_check CHECK (
        status IN ('queued', 'running', 'waiting', 'succeeded', 'failed', 'cancelled')
    ),
    ADD CONSTRAINT tasks_agent_session_required CHECK (
        kind <> 'agent' OR chat_session_id IS NOT NULL
    ),
    ALTER COLUMN workspace_id SET NOT NULL,
    ALTER COLUMN kind SET NOT NULL,
    ALTER COLUMN created_by SET NOT NULL;

ALTER TABLE public.workspaces
    ADD CONSTRAINT workspaces_active_owner_required CHECK (
        deleted_at IS NOT NULL OR owner_user_id IS NOT NULL
    ),
    ADD CONSTRAINT workspaces_writable_branch_required CHECK (
        deleted_at IS NOT NULL
        OR is_base
        OR (
            public.g5_git_branch_name_is_valid(branch_name)
            AND name = branch_name
        )
    );

-- VM state is authoritative for working workspace lifecycle. The legacy
-- workspace-level status can diverge from that state and has no retained
-- producer or consumer after this cutover.
ALTER TABLE public.workspaces
    DROP COLUMN status;

-- Workspace branches now have exactly one authority owner: their workspace.
-- Keep the small set of disposable remote-tracking merge-base rows explicitly
-- marked as transport state, then remove the temporary job/task scope model.
ALTER TABLE public.workspace_branches
    ADD COLUMN is_transient boolean NOT NULL DEFAULT false;

UPDATE public.workspace_branches
SET is_transient = true
WHERE authority_scope_kind IS NULL
  AND branch_name LIKE 'remote-tracking-base-%';

DELETE FROM public.workspace_branches
WHERE authority_scope_kind IN ('job', 'task');

DROP TRIGGER IF EXISTS trg_validate_workspace_branch_authority_scope
    ON public.workspace_branches;
DROP TRIGGER IF EXISTS workspace_branches_project_v9_mapping
    ON public.workspace_branches;
DROP TRIGGER IF EXISTS workspace_branch_repos_project_v9_mapping
    ON public.workspace_branch_repos;
DROP TRIGGER IF EXISTS workspace_repos_legacy_authority_guard
    ON public.workspace_repos;
DROP TRIGGER IF EXISTS task_dependencies_legacy_authority_guard
    ON public.task_dependencies;
DROP VIEW IF EXISTS public.p4_1_workspace_migration_preflight;
DROP FUNCTION IF EXISTS public.validate_workspace_branch_authority_scope();
DROP FUNCTION IF EXISTS public.g5_project_legacy_workspace_branch();
DROP FUNCTION IF EXISTS public.g5_project_legacy_workspace_binding();
DROP FUNCTION IF EXISTS public.g5_reject_direct_compat_projection_write();
DROP INDEX IF EXISTS public.idx_workspace_branches_authority_scope;
DROP INDEX IF EXISTS public.uq_workspace_branches_authority_scope_repo;

ALTER TABLE public.workspace_branches
    DROP CONSTRAINT IF EXISTS chk_workspace_branches_authority_scope,
    DROP CONSTRAINT IF EXISTS workspace_branches_authority_scope_kind_check,
    DROP COLUMN authority_scope_id,
    DROP COLUMN authority_scope_kind,
    DROP COLUMN job_id,
    DROP COLUMN task_id;

CREATE UNIQUE INDEX uq_workspace_branches_workspace_repo
    ON public.workspace_branches(workspace_id, repo_id)
    WHERE NOT is_transient;

COMMENT ON COLUMN public.workspace_branches.is_transient IS
    'True only for disposable transport rows such as remote-tracking merge bases; never workspace authority';

-- The scheduler's narrow RLS exceptions followed the retired task/session
-- compatibility columns. Re-key them to the authoritative workflow profile
-- and shared workspace-session shape before those columns are removed.
DROP POLICY IF EXISTS issue_discovery_scheduler_tasks ON public.tasks;
CREATE POLICY issue_discovery_scheduler_tasks ON public.tasks TO g5_app
    USING (
        kind = 'agent'
        AND spec #>> '{workflow,profile}' = 'issue_ontology_discovery'
    )
    WITH CHECK (
        kind = 'agent'
        AND spec #>> '{workflow,profile}' = 'issue_ontology_discovery'
    );

DROP POLICY IF EXISTS issue_discovery_scheduler_chat_messages ON public.chat_messages;
CREATE POLICY issue_discovery_scheduler_chat_messages ON public.chat_messages TO g5_app
    USING (session_id IN (
        SELECT chat_sessions.id
        FROM public.chat_sessions
        WHERE chat_sessions.scope_type = 'workspace'
          AND chat_sessions.title LIKE 'Issue ontology discovery:%'
    ))
    WITH CHECK (session_id IN (
        SELECT chat_sessions.id
        FROM public.chat_sessions
        WHERE chat_sessions.scope_type = 'workspace'
          AND chat_sessions.title LIKE 'Issue ontology discovery:%'
    ));

DROP POLICY IF EXISTS issue_discovery_scheduler_chat_sessions ON public.chat_sessions;
CREATE POLICY issue_discovery_scheduler_chat_sessions ON public.chat_sessions TO g5_app
    USING (
        scope_type = 'workspace'
        AND title LIKE 'Issue ontology discovery:%'
    )
    WITH CHECK (
        scope_type = 'workspace'
        AND title LIKE 'Issue ontology discovery:%'
    );

ALTER TABLE public.tasks
    DROP CONSTRAINT IF EXISTS tasks_provider_check,
    DROP COLUMN branch_name,
    DROP COLUMN base_commit,
    DROP COLUMN workspace_ref,
    DROP COLUMN worker_id,
    DROP COLUMN assigned_at,
    DROP COLUMN lease_expires_at,
    DROP COLUMN lease_renewals,
    DROP COLUMN task_observations,
    DROP COLUMN agent_auth_enc,
    DROP COLUMN agent_auth_worker_id,
    DROP COLUMN agent_auth_submitted_by,
    DROP COLUMN agent_auth_updated_at,
    DROP COLUMN async_decision_policy,
    DROP COLUMN task_type;

ALTER TABLE public.chat_sessions
    DROP COLUMN session_type,
    DROP COLUMN task_id,
    DROP COLUMN status,
    DROP COLUMN branch,
    DROP COLUMN required_worker_id,
    DROP COLUMN branch_mode,
    DROP COLUMN attach_preview_task_id;

ALTER TABLE public.workspace_settings
    DROP COLUMN max_concurrent_tasks,
    DROP COLUMN max_cloud_workers,
    DROP COLUMN allow_cloud_workers,
    DROP COLUMN worker_idle_timeout_min,
    DROP COLUMN tasks_per_worker,
    DROP COLUMN worker_scaling_interval_sec,
    DROP COLUMN chat_sticky_idle_timeout_min,
    DROP COLUMN question_timeout_min;

DROP TABLE public.runtime_attribution_events CASCADE;
DROP TABLE public.user_preferences CASCADE;
DROP TABLE public.chat_sticky_runtimes CASCADE;
DROP TABLE public.agent_approval_handoffs CASCADE;
DROP TABLE public.chat_message_context_resolutions CASCADE;
DROP TABLE public.chat_session_context_links CASCADE;
DROP TABLE public.task_question_windows CASCADE;
DROP TABLE public.task_questions CASCADE;
DROP TABLE public.task_execution_attempts CASCADE;
DROP TABLE public.builder_task_specs CASCADE;
DROP TABLE public.session_build_telemetry CASCADE;
DROP TABLE public.workspace_prebuild_state CASCADE;
DROP TABLE public.repo_manifests CASCADE;
DROP TABLE public.job_preview_sessions CASCADE;
DROP TABLE public.workers CASCADE;

-- Dropping workers removes the former dual-registry SECURITY DEFINER lookup by
-- dependency. Recreate it against the sole authoritative host registry before
-- any host reconnects; host authentication runs before tenant RLS context is
-- available.
CREATE OR REPLACE FUNCTION authz.lookup_worker_host_for_auth(p_host_id uuid)
RETURNS TABLE(org_id uuid, status text)
    LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    SELECT host.org_id, host.status
    FROM public.worker_hosts AS host
    WHERE host.host_id = p_host_id
$$;

ALTER FUNCTION authz.lookup_worker_host_for_auth(uuid) OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.lookup_worker_host_for_auth(uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.lookup_worker_host_for_auth(uuid) TO g5_app;
COMMENT ON FUNCTION authz.lookup_worker_host_for_auth(uuid) IS
    'Pre-org-binding host lookup against worker_hosts, owned by authz_lookup_definer so host JWT validation can run before tenant context exists';

DROP FUNCTION IF EXISTS authz.guard_worker_insert_dead_host();
DROP FUNCTION IF EXISTS authz.guard_worker_revival_dead_host();
DROP FUNCTION IF EXISTS authz.worker_identity_bound_to_terminated_host(text, uuid);

COMMENT ON COLUMN public.tasks.base_sha IS
    'Informational source revision captured at task creation; never a private task branch authority';

-- Restore the migrator's exact membership options and the function owner's
-- pre-migration schema privileges. Keep the permanent table/function grants
-- above: those are the runtime authority this migration is installing.
DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.mvp1_authz_lookup_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.mvp1_authz_lookup_had_set', true)
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
    IF current_setting('g5.mvp1_authz_lookup_had_authz_create', true)
        IS DISTINCT FROM 'true'
    THEN
        REVOKE CREATE ON SCHEMA authz FROM authz_lookup_definer;
    END IF;
    IF current_setting('g5.mvp1_authz_lookup_had_authz_usage', true)
        IS DISTINCT FROM 'true'
    THEN
        REVOKE USAGE ON SCHEMA authz FROM authz_lookup_definer;
    END IF;
END $$;
