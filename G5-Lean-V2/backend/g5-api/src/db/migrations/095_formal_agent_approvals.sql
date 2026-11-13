-- Formal reviews retain their assignment/evidence independently of disposable
-- workspaces. Tasks, checked forks and VM custody retain their existing owners.
CREATE TABLE public.agent_approval_reviews (
    id uuid PRIMARY KEY,
    org_id uuid NOT NULL REFERENCES public.orgs(id),
    source_workspace_id uuid NOT NULL,
    approval_request_id uuid NOT NULL,
    requested_by uuid NOT NULL REFERENCES public.users(id),
    target_workspace_id uuid NOT NULL,
    review_workspace_id uuid NOT NULL UNIQUE,
    retry_of uuid REFERENCES public.agent_approval_reviews(id),
    definition_snapshot jsonb NOT NULL CHECK (jsonb_typeof(definition_snapshot) = 'object'),
    target_heads jsonb,
    invocation jsonb,
    task_id uuid UNIQUE,
    execution_provenance jsonb,
    artifact_id uuid REFERENCES public.agent_approval_artifacts(id) ON DELETE RESTRICT,
    candidate_summary text CHECK (char_length(candidate_summary) <= 1000),
    stage text NOT NULL DEFAULT 'capture' CHECK (stage IN ('capture','start','execute','harvest','seal','effective','failed','cancelled')),
    stage_started_at timestamptz NOT NULL DEFAULT now(),
    claimed_at timestamptz,
    deadline_at timestamptz NOT NULL DEFAULT now() + interval '5 minutes',
    transient_attempts integer NOT NULL DEFAULT 0 CHECK (transient_attempts >= 0),
    failure_stage text,
    failure_cause text,
    effective_at timestamptz,
    ended_at timestamptz,
    result text CHECK (result IN ('approved','rejected')),
    sealed_at timestamptz,
    cleanup_due_at timestamptz,
    cleanup_attempts integer NOT NULL DEFAULT 0 CHECK (cleanup_attempts >= 0),
    cleanup_error text,
    cleanup_completed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (org_id, source_workspace_id, approval_request_id)
        REFERENCES public.approval_requests(org_id, source_workspace_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (org_id, task_id) REFERENCES public.tasks(org_id, id) ON DELETE RESTRICT,
    UNIQUE (org_id, id),
    CONSTRAINT formal_review_effectiveness CHECK (
        (stage = 'effective' AND effective_at IS NOT NULL AND sealed_at IS NOT NULL AND artifact_id IS NOT NULL AND result IS NOT NULL)
        OR (stage <> 'effective' AND effective_at IS NULL AND result IS NULL)),
    CONSTRAINT formal_review_failure CHECK ((stage = 'failed') = (failure_stage IS NOT NULL AND failure_cause IS NOT NULL)),
    CONSTRAINT formal_review_ended CHECK ((stage IN ('effective','failed','cancelled')) = (ended_at IS NOT NULL))
);
CREATE UNIQUE INDEX agent_approval_reviews_active_request ON public.agent_approval_reviews(org_id,approval_request_id)
    WHERE stage NOT IN ('effective','failed','cancelled');
CREATE INDEX agent_approval_reviews_reconcile ON public.agent_approval_reviews(org_id,stage,cleanup_due_at)
    WHERE cleanup_completed_at IS NULL;
CREATE FUNCTION public.formal_review_cleanup_overdue(
    p_ended_at timestamptz, p_completed_at timestamptz, p_error text
) RETURNS boolean LANGUAGE sql VOLATILE AS $$
    SELECT p_ended_at IS NOT NULL AND p_completed_at IS NULL AND p_error IS NOT NULL
       AND clock_timestamp() >= p_ended_at + interval '24 hours'
$$;
ALTER TABLE public.agent_approval_reviews ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.agent_approval_reviews FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.agent_approval_reviews
    USING (org_id = ANY(authz.current_org_ids())) WITH CHECK (org_id = ANY(authz.current_org_ids()));
GRANT SELECT,INSERT,UPDATE ON public.agent_approval_reviews TO g5_tenant;
GRANT SELECT ON public.agent_approval_reviews TO g5_app;

ALTER TABLE public.workspaces ADD COLUMN agent_approval_review_id uuid,
    ADD CONSTRAINT workspaces_formal_review FOREIGN KEY (org_id,agent_approval_review_id)
        REFERENCES public.agent_approval_reviews(org_id,id) ON DELETE RESTRICT;
-- The history link names the original source; the task belongs to the review.
ALTER TABLE public.approval_request_tasks DROP CONSTRAINT approval_request_tasks_org_id_source_workspace_id_task_id_fkey;
ALTER TABLE public.approval_request_tasks ADD CONSTRAINT approval_request_tasks_task
    FOREIGN KEY (org_id,task_id) REFERENCES public.tasks(org_id,id) ON DELETE RESTRICT;
ALTER TABLE public.approval_statuses DROP CONSTRAINT approval_statuses_human_actor;
ALTER TABLE public.approval_statuses ADD COLUMN agent_review_id uuid,
    ADD CONSTRAINT approval_statuses_formal_review FOREIGN KEY (org_id,agent_review_id)
        REFERENCES public.agent_approval_reviews(org_id,id) ON DELETE RESTRICT,
    ADD CONSTRAINT approval_statuses_actor_authority CHECK (
        (actor_type='human' AND actor_user_id IS NOT NULL AND actor_task_id IS NULL AND agent_review_id IS NULL)
        OR (actor_type='agent' AND actor_user_id IS NULL AND actor_task_id IS NOT NULL AND agent_review_id IS NOT NULL AND outcome IN ('approved','rejected')));
CREATE UNIQUE INDEX approval_statuses_agent_review ON public.approval_statuses(agent_review_id) WHERE agent_review_id IS NOT NULL;

-- Snapshot and restore the temporary definer membership, including PG16+
-- membership options. The deployment migrator needs no BYPASSRLS capability.
DO $$
DECLARE had_set boolean := pg_has_role(SESSION_USER,'authz_lookup_definer','SET');
        had_create boolean := has_schema_privilege('authz_lookup_definer','authz','CREATE');
        snapshot jsonb; row record; prior jsonb; function_signature text;
BEGIN
    SELECT COALESCE(jsonb_agg(jsonb_build_object('grantor',g.rolname,'admin_option',m.admin_option,'inherit_option',m.inherit_option,'set_option',m.set_option)),'[]'::jsonb)
      INTO snapshot FROM pg_auth_members m JOIN pg_roles r ON r.oid=m.roleid JOIN pg_roles u ON u.oid=m.member JOIN pg_roles g ON g.oid=m.grantor
     WHERE r.rolname='authz_lookup_definer' AND u.rolname=SESSION_USER;
    IF NOT had_set THEN EXECUTE format('GRANT authz_lookup_definer TO %I WITH SET TRUE',SESSION_USER); END IF;
    IF NOT had_create THEN GRANT CREATE ON SCHEMA authz TO authz_lookup_definer; END IF;
    -- Context checks must also cover proposed workspace rows not yet visible
    -- to a definer lookup. Ownership lookups never inherit tenant filtering.
    GRANT SELECT ON public.agent_approval_reviews TO authz_lookup_definer;
    EXECUTE $fn$CREATE FUNCTION authz.formal_review_context_allows(p_workspace uuid, p_write boolean) RETURNS boolean
        LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED
        SET search_path TO pg_catalog,authz AS $body$
        SELECT COALESCE((SELECT user_id = '00000000-0000-0000-0000-000000000001'::uuid
            AND ('*' = ANY(scopes)
                 OR ('formal-review:' || p_workspace::text) = ANY(scopes)
                 OR (NOT p_write AND ('formal-review-read:' || p_workspace::text) = ANY(scopes)))
            FROM authz.session_ctx WHERE pid = pg_backend_pid()), false)
        $body$ $fn$;
    EXECUTE $fn$CREATE FUNCTION authz.formal_review_allows(p_workspace uuid, p_write boolean) RETURNS boolean
        LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED
        SET search_path TO pg_catalog,authz AS $body$
        SELECT p_workspace IS NULL
            OR NOT EXISTS(SELECT 1 FROM public.workspaces WHERE id = p_workspace AND agent_approval_review_id IS NOT NULL)
            OR authz.formal_review_context_allows(p_workspace, p_write)
        $body$ $fn$;
    EXECUTE $fn$CREATE FUNCTION authz.formal_review_task_workspace(p_task uuid) RETURNS uuid
        LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED
        SET search_path TO pg_catalog,authz AS $body$
        SELECT workspace_id FROM public.tasks WHERE id = p_task
        $body$ $fn$;
    EXECUTE $fn$CREATE FUNCTION authz.formal_review_session_workspace(p_session uuid) RETURNS uuid
        LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED
        SET search_path TO pg_catalog,authz AS $body$
        SELECT workspace_id FROM public.chat_sessions WHERE id = p_session
        $body$ $fn$;
    EXECUTE $fn$CREATE FUNCTION authz.formal_review_fork_workspace(p_fork uuid) RETURNS uuid
        LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED
        SET search_path TO pg_catalog,authz AS $body$
        SELECT review_workspace_id FROM public.agent_approval_reviews WHERE review_workspace_id = p_fork
        $body$ $fn$;
    FOREACH function_signature IN ARRAY ARRAY[
        'authz.formal_review_context_allows(uuid,boolean)',
        'authz.formal_review_allows(uuid,boolean)',
        'authz.formal_review_task_workspace(uuid)',
        'authz.formal_review_session_workspace(uuid)',
        'authz.formal_review_fork_workspace(uuid)'
    ] LOOP
        EXECUTE format('ALTER FUNCTION %s OWNER TO authz_lookup_definer', function_signature);
        EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC', function_signature);
        EXECUTE format('GRANT EXECUTE ON FUNCTION %s TO g5_app,g5_tenant', function_signature);
    END LOOP;
    IF NOT had_create THEN REVOKE CREATE ON SCHEMA authz FROM authz_lookup_definer; END IF;
    IF NOT had_set THEN
        FOR row IN SELECT g.rolname AS grantor,m.admin_option,m.inherit_option,m.set_option FROM pg_auth_members m
            JOIN pg_roles r ON r.oid=m.roleid JOIN pg_roles u ON u.oid=m.member JOIN pg_roles g ON g.oid=m.grantor
            WHERE r.rolname='authz_lookup_definer' AND u.rolname=SESSION_USER
        LOOP
            SELECT v INTO prior FROM jsonb_array_elements(snapshot) v WHERE v->>'grantor'=row.grantor;
            IF prior IS NULL THEN EXECUTE format('REVOKE authz_lookup_definer FROM %I',SESSION_USER);
            ELSIF row.admin_option IS DISTINCT FROM (prior->>'admin_option')::boolean OR row.inherit_option IS DISTINCT FROM (prior->>'inherit_option')::boolean OR row.set_option IS DISTINCT FROM (prior->>'set_option')::boolean THEN
                EXECUTE format('GRANT authz_lookup_definer TO %I WITH ADMIN %s, INHERIT %s, SET %s',SESSION_USER,prior->>'admin_option',prior->>'inherit_option',prior->>'set_option');
            END IF;
        END LOOP;
    END IF;
END $$;
-- Separate command policies keep a review-bound reader unable to mutate.
-- Independent parent links compose as AND, including nullable task/session links.
-- Existing g5_app infrastructure authority is deliberately unchanged.
DO $$
DECLARE binding record; read_guard text; write_guard text;
BEGIN
    FOR binding IN
        SELECT 'workspaces'::text AS table_name, 'formal_review_private'::text AS prefix,
               'id'::text AS workspace_expression
        UNION ALL
        SELECT c.table_name, 'formal_review_private', 'workspace_id'
        FROM information_schema.columns c JOIN pg_tables t
          ON t.schemaname=c.table_schema AND t.tablename=c.table_name
        WHERE c.table_schema='public' AND c.column_name='workspace_id'
          AND t.rowsecurity AND c.table_name <> 'workspaces'
        UNION ALL
        SELECT 'workspace_forks', 'formal_review_private',
               'authz.formal_review_fork_workspace(id)'
        UNION ALL
        SELECT c.table_name, 'formal_review_private_task',
               'authz.formal_review_task_workspace(task_id)'
        FROM information_schema.columns c JOIN pg_tables t
          ON t.schemaname=c.table_schema AND t.tablename=c.table_name
        WHERE c.table_schema='public' AND c.column_name='task_id' AND t.rowsecurity
          AND c.table_name NOT IN ('tasks','agent_approval_reviews','agent_approval_artifacts','approval_request_tasks')
        UNION ALL
        SELECT c.table_name, 'formal_review_private_' || c.column_name,
               format('authz.formal_review_session_workspace(%I)', c.column_name)
        FROM information_schema.columns c JOIN pg_tables t
          ON t.schemaname=c.table_schema AND t.tablename=c.table_name
        WHERE c.table_schema='public' AND c.column_name IN ('session_id','chat_session_id')
          AND t.rowsecurity AND c.table_name NOT IN ('tasks','chat_sessions')
    LOOP
        IF binding.table_name = 'workspaces' THEN
            read_guard := '(agent_approval_review_id IS NULL OR authz.formal_review_context_allows(id,false))';
            write_guard := '(agent_approval_review_id IS NULL OR authz.formal_review_context_allows(id,true))';
        ELSE
            read_guard := format('authz.formal_review_allows(%s,false)', binding.workspace_expression);
            write_guard := format('authz.formal_review_allows(%s,true)', binding.workspace_expression);
        END IF;
        EXECUTE format('CREATE POLICY %I ON public.%I AS RESTRICTIVE FOR SELECT TO g5_tenant USING (%s)',
                       binding.prefix || '_select', binding.table_name, read_guard);
        EXECUTE format('CREATE POLICY %I ON public.%I AS RESTRICTIVE FOR INSERT TO g5_tenant WITH CHECK (%s)',
                       binding.prefix || '_insert', binding.table_name, write_guard);
        EXECUTE format('CREATE POLICY %I ON public.%I AS RESTRICTIVE FOR UPDATE TO g5_tenant USING (%s) WITH CHECK (%s)',
                       binding.prefix || '_update', binding.table_name, write_guard, write_guard);
        EXECUTE format('CREATE POLICY %I ON public.%I AS RESTRICTIVE FOR DELETE TO g5_tenant USING (%s)',
                       binding.prefix || '_delete', binding.table_name, write_guard);
    END LOOP;
END $$;

CREATE FUNCTION public.guard_formal_review_history() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.org_id<>OLD.org_id OR NEW.source_workspace_id<>OLD.source_workspace_id OR NEW.approval_request_id<>OLD.approval_request_id
       OR NEW.review_workspace_id<>OLD.review_workspace_id OR NEW.target_workspace_id<>OLD.target_workspace_id
       OR NEW.retry_of IS DISTINCT FROM OLD.retry_of OR NEW.requested_by<>OLD.requested_by OR NEW.definition_snapshot<>OLD.definition_snapshot
       OR (OLD.invocation IS NOT NULL AND NEW.invocation IS DISTINCT FROM OLD.invocation)
       OR (OLD.target_heads IS NOT NULL AND NEW.target_heads IS DISTINCT FROM OLD.target_heads)
       OR (OLD.task_id IS NOT NULL AND NEW.task_id IS DISTINCT FROM OLD.task_id)
       OR (OLD.artifact_id IS NOT NULL AND NEW.artifact_id IS DISTINCT FROM OLD.artifact_id)
       OR (OLD.candidate_summary IS NOT NULL AND NEW.candidate_summary IS DISTINCT FROM OLD.candidate_summary)
       OR (OLD.claimed_at IS NOT NULL AND NEW.claimed_at IS DISTINCT FROM OLD.claimed_at)
       OR (OLD.ended_at IS NOT NULL AND NEW.ended_at IS DISTINCT FROM OLD.ended_at)
       OR (OLD.execution_provenance IS NOT NULL AND NEW.execution_provenance IS DISTINCT FROM OLD.execution_provenance)
       OR (OLD.stage IN ('effective','failed','cancelled') AND (NEW.stage<>OLD.stage OR NEW.result IS DISTINCT FROM OLD.result OR NEW.effective_at IS DISTINCT FROM OLD.effective_at OR NEW.sealed_at IS DISTINCT FROM OLD.sealed_at OR NEW.failure_stage IS DISTINCT FROM OLD.failure_stage OR NEW.failure_cause IS DISTINCT FROM OLD.failure_cause)) THEN
        RAISE EXCEPTION 'formal review assignment and terminal history are immutable' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER formal_review_history BEFORE UPDATE ON public.agent_approval_reviews FOR EACH ROW EXECUTE FUNCTION public.guard_formal_review_history();

-- Agent statuses are admitted only against durably sealed evidence in the same
-- transaction. A forged task success or an unsealed candidate cannot open a gate.
CREATE FUNCTION public.guard_formal_approval_status() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.actor_type='agent' AND NOT EXISTS (
        SELECT 1 FROM public.agent_approval_reviews r JOIN public.agent_approval_artifacts a ON a.id=r.artifact_id AND a.org_id=r.org_id
        JOIN public.workspaces w ON w.id=r.review_workspace_id AND w.org_id=r.org_id
        WHERE a.approval_request_id=r.approval_request_id AND a.source_workspace_id=r.source_workspace_id AND a.task_id=r.task_id AND r.id=NEW.agent_review_id AND r.org_id=NEW.org_id AND r.source_workspace_id=NEW.source_workspace_id
          AND r.task_id=NEW.actor_task_id AND r.stage='effective' AND r.sealed_at IS NOT NULL
          AND r.result=NEW.outcome AND a.result=NEW.outcome AND w.mutation_policy='read_only'
          AND w.agent_approval_review_id=r.id AND (r.invocation->'resolution'->'sourceHeads')=NEW.reviewed_git_heads
          AND r.invocation->'resolution'->>'definitionId'=NEW.approval_definition_id::text
          AND (r.invocation->'resolution'->>'definitionVersion')::integer=NEW.approval_definition_version
          AND EXISTS(SELECT 1 FROM public.approval_requests q WHERE q.id=r.approval_request_id AND q.org_id=r.org_id AND q.phase=NEW.phase)
    ) THEN RAISE EXCEPTION 'agent approval requires its sealed formal evidence' USING ERRCODE='23514'; END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER formal_approval_status BEFORE INSERT OR UPDATE ON public.approval_statuses FOR EACH ROW EXECUTE FUNCTION public.guard_formal_approval_status();

CREATE FUNCTION public.guard_approval_task_workspace() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM public.tasks task WHERE task.id=NEW.task_id AND task.org_id=NEW.org_id AND
        (task.workspace_id=NEW.source_workspace_id OR EXISTS(SELECT 1 FROM public.agent_approval_reviews review
            JOIN public.workspaces w ON w.id=review.review_workspace_id AND w.org_id=review.org_id AND w.agent_approval_review_id=review.id
            WHERE review.approval_request_id=NEW.approval_request_id AND review.org_id=NEW.org_id
              AND review.source_workspace_id=NEW.source_workspace_id AND review.review_workspace_id=task.workspace_id)))
    THEN RAISE EXCEPTION 'approval task does not belong to its source or assigned review workspace' USING ERRCODE='23514'; END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER approval_task_workspace BEFORE INSERT OR UPDATE ON public.approval_request_tasks FOR EACH ROW EXECUTE FUNCTION public.guard_approval_task_workspace();
