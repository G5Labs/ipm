-- Jobs and their execution graph are retired. Hosts are reprovisioned at cutover.
-- Preserve independent tasks, conversations, usage, human decisions and workspace merges.
-- The deployment migrator lacks BYPASSRLS. Borrow only the existing definer's
-- SET capability and table privileges, then restore their exact prior state.
SELECT set_config(
    'g5.retire_authz_lookup_membership_snapshot',
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
    'g5.retire_authz_lookup_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
DO $$ BEGIN
    IF current_setting('g5.retire_authz_lookup_had_set') <> 'true' THEN
        EXECUTE format('GRANT authz_lookup_definer TO %I WITH SET TRUE', SESSION_USER);
    END IF;
END $$;
CREATE TEMP TABLE retirement_privileges ON COMMIT DROP AS
SELECT table_name, privilege,
       has_table_privilege('authz_lookup_definer', 'public.' || table_name, privilege) AS had_privilege
FROM (VALUES ('tasks'), ('task_dependencies'), ('task_attempts'), ('provider_requests'), ('session_inputs'), ('approval_review_runs'), ('approval_request_tasks'), ('approval_requests'), ('approval_statuses'), ('merge_evaluations'), ('job_merge_runs')) tables(table_name)
CROSS JOIN (VALUES ('SELECT'), ('UPDATE'), ('DELETE')) privileges(privilege);
GRANT SELECT, UPDATE, DELETE ON public.tasks, public.task_dependencies, public.task_attempts, public.provider_requests, public.session_inputs, public.approval_review_runs, public.approval_request_tasks, public.approval_requests, public.approval_statuses, public.merge_evaluations, public.job_merge_runs TO authz_lookup_definer;
-- Credential helpers retain column grants from 087. A table-level REVOKE
-- would remove those too, so borrow only missing authority on scopes.
CREATE TEMP TABLE retirement_api_scope_privileges ON COMMIT DROP AS
SELECT privilege,
       has_column_privilege('authz_lookup_definer', 'public.api_keys', 'scopes', privilege) AS had_privilege
FROM (VALUES ('SELECT'), ('UPDATE')) privileges(privilege);
DO $$ DECLARE entry record; BEGIN
    FOR entry IN SELECT * FROM retirement_api_scope_privileges WHERE NOT had_privilege
    LOOP
        EXECUTE format('GRANT %s (scopes) ON public.api_keys TO authz_lookup_definer', entry.privilege);
    END LOOP;
END $$;
CREATE TEMP TABLE retired_execution_tasks (id uuid PRIMARY KEY) ON COMMIT DROP;
CREATE TEMP TABLE retired_review_requests (id uuid PRIMARY KEY) ON COMMIT DROP;
GRANT SELECT, INSERT ON retired_execution_tasks, retired_review_requests TO authz_lookup_definer;
SET ROLE authz_lookup_definer;
-- API keys are tenant protected. Preserve equivalent authority, deduplicate,
-- and discard retired cancel-only authority instead of widening it to execute.
UPDATE api_keys SET scopes = ARRAY(
    SELECT DISTINCT CASE scope
        WHEN 'jobs:read' THEN 'tasks:read'
        WHEN 'jobs:execute' THEN 'tasks:execute'
        ELSE scope END AS scope
    FROM unnest(api_keys.scopes) scope
    WHERE scope NOT LIKE 'jobs:%' OR scope IN ('jobs:read', 'jobs:execute')
    ORDER BY scope
) WHERE EXISTS (SELECT 1 FROM unnest(api_keys.scopes) scope WHERE scope LIKE 'jobs:%');
INSERT INTO retired_execution_tasks
WITH RECURSIVE retired(id) AS (
    SELECT id FROM tasks WHERE job_id IS NOT NULL
    UNION
    SELECT dependency.task_id FROM task_dependencies dependency
    JOIN retired ON retired.id = dependency.depends_on_task_id
)
SELECT id FROM retired;

UPDATE task_attempts SET ended_at = GREATEST(now(), started_at), reason = 'cancelled',
    outcome = '{"status":"cancelled","reason":"execution_retired"}',
    codex_auth_challenge_enc = NULL,
    codex_auth_state = CASE WHEN codex_auth_state IS NULL THEN NULL ELSE '{"phase":"ended"}'::jsonb END
WHERE task_id IN (SELECT id FROM retired_execution_tasks) AND ended_at IS NULL;

UPDATE tasks SET status = 'cancelled', ended_at = GREATEST(now(), started_at),
    outcome = '{"status":"cancelled","reason":"execution_retired"}',
    hold_reason = NULL, continuation_reason = NULL,
    vm_id = NULL, epoch = NULL, start_confirmed = false, updated_at = now()
WHERE id IN (SELECT id FROM retired_execution_tasks)
  AND status IN ('queued', 'running', 'waiting');

UPDATE provider_requests SET status = 'invalidated', ended_at = now(), end_reason = 'execution_retired'
WHERE task_id IN (SELECT id FROM retired_execution_tasks) AND status IN ('pending','reply_pending');
UPDATE session_inputs SET delivery = 'withdrawn', withdrawn_at = now(),
    reservation_id = NULL, reservation_revision = NULL,
    delivery_audit = delivery_audit || '[{"event":"execution_retired"}]'::jsonb
WHERE task_id IN (SELECT id FROM retired_execution_tasks)
  AND delivery IN ('queued','reserved','delivery_unknown');

INSERT INTO retired_review_requests
SELECT DISTINCT approval_request_id AS id FROM approval_review_runs
WHERE NOT EXISTS (SELECT 1 FROM approval_request_tasks link
    WHERE link.approval_request_id = approval_review_runs.approval_request_id);
RESET ROLE;
DROP TABLE approval_review_runs;
SET ROLE authz_lookup_definer;
DELETE FROM approval_requests WHERE id IN (SELECT id FROM retired_review_requests);
DELETE FROM approval_statuses WHERE actor_type = 'agent';
DELETE FROM merge_evaluations WHERE merge_run_id IN
    (SELECT id FROM job_merge_runs WHERE source_kind = 'job' OR source_job_id IS NOT NULL OR merge_job_id IS NOT NULL);
DELETE FROM job_merge_runs WHERE source_kind = 'job' OR source_job_id IS NOT NULL OR merge_job_id IS NOT NULL;
UPDATE job_merge_runs SET phase = 'workspace_rebase_preflight' WHERE phase = 'job_rebase_preflight';
RESET ROLE;
-- Remove only privileges introduced above; existing definer grants survive.
DO $$ DECLARE entry record; BEGIN
    FOR entry IN SELECT * FROM retirement_api_scope_privileges WHERE NOT had_privilege
    LOOP
        EXECUTE format('REVOKE %s (scopes) ON public.api_keys FROM authz_lookup_definer', entry.privilege);
    END LOOP;
END $$;
DO $$ DECLARE entry record; BEGIN
    FOR entry IN SELECT * FROM retirement_privileges WHERE NOT had_privilege
    LOOP
        IF to_regclass('public.' || entry.table_name) IS NOT NULL THEN
            EXECUTE format('REVOKE %s ON public.%I FROM authz_lookup_definer', entry.privilege, entry.table_name);
        END IF;
    END LOOP;
END $$;
DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.retire_authz_lookup_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.retire_authz_lookup_had_set', true)
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

-- The scope registry and grant tables are global permission metadata, owned
-- by the migrator. Register task authority before changing the referencing rows.
INSERT INTO oauth_scopes(scope, description, category) VALUES
    ('tasks:read', 'Read tasks and their execution state', 'tasks'),
    ('tasks:execute', 'Execute and control tasks', 'tasks')
ON CONFLICT (scope) DO UPDATE SET description = EXCLUDED.description, category = EXCLUDED.category;

-- An explicit task grant wins if both names already exist. Otherwise rename in
-- place so grant IDs, actor provenance and expiration are retained.
DELETE FROM org_default_scopes old USING org_default_scopes current
WHERE old.org_id = current.org_id AND old.scope IN ('jobs:read', 'jobs:execute')
  AND current.scope = replace(old.scope, 'jobs:', 'tasks:');
UPDATE org_default_scopes SET scope = replace(scope, 'jobs:', 'tasks:')
WHERE scope IN ('jobs:read', 'jobs:execute');
DELETE FROM org_default_scopes WHERE scope LIKE 'jobs:%';

DELETE FROM user_scope_grants old USING user_scope_grants current
WHERE old.org_id = current.org_id AND old.user_id = current.user_id
  AND old.scope IN ('jobs:read', 'jobs:execute')
  AND current.scope = replace(old.scope, 'jobs:', 'tasks:');
UPDATE user_scope_grants SET scope = replace(scope, 'jobs:', 'tasks:')
WHERE scope IN ('jobs:read', 'jobs:execute');
DELETE FROM user_scope_grants WHERE scope LIKE 'jobs:%';

DELETE FROM workspace_permission_overrides old USING workspace_permission_overrides current
WHERE old.workspace_id = current.workspace_id AND old.user_id = current.user_id
  AND old.scope IN ('jobs:read', 'jobs:execute')
  AND current.scope = replace(old.scope, 'jobs:', 'tasks:');
UPDATE workspace_permission_overrides SET scope = replace(scope, 'jobs:', 'tasks:')
WHERE scope IN ('jobs:read', 'jobs:execute');
DELETE FROM workspace_permission_overrides WHERE scope LIKE 'jobs:%';
DELETE FROM oauth_scopes WHERE scope LIKE 'jobs:%';

DROP TRIGGER trg_approval_statuses_v3_run_match ON approval_statuses;
DROP TRIGGER trg_approval_statuses_v4_run_match ON approval_statuses;
ALTER TABLE approval_statuses DROP COLUMN agent_job_id, DROP COLUMN agent_approval_run_details;
-- Task reviews currently produce reports, not formal decisions. Keep the gate closed.
ALTER TABLE approval_statuses ADD CONSTRAINT approval_statuses_human_actor
    CHECK (actor_type = 'human' AND actor_user_id IS NOT NULL AND actor_task_id IS NULL);

ALTER TABLE job_merge_runs DROP COLUMN source_job_id, DROP COLUMN merge_job_id;
ALTER TABLE job_merge_runs DROP CONSTRAINT job_merge_runs_source_kind_check;
ALTER TABLE job_merge_runs ADD CONSTRAINT workspace_merge_runs_source_kind_check
    CHECK (source_kind IN ('workspace','remote_tracking'));
ALTER TABLE job_merge_runs ALTER COLUMN source_kind SET DEFAULT 'workspace';
ALTER TABLE job_merge_runs ALTER COLUMN phase SET DEFAULT 'workspace_rebase_preflight';
ALTER TABLE job_merge_runs RENAME TO workspace_merge_runs;

-- Drop these columns before the parent table: the old task FK cascaded deletes.
ALTER TABLE tasks DROP COLUMN job_id, DROP COLUMN advisory_predecessor_job_id;
ALTER TABLE usage_records DROP COLUMN job_id;
ALTER TABLE notifications DROP COLUMN job_id;
DROP TABLE jobs;
ALTER TABLE worker_hosts RENAME COLUMN concurrent_jobs TO workspace_vm_limit;
ALTER TABLE worker_hosts RENAME CONSTRAINT worker_hosts_concurrent_jobs_min TO worker_hosts_workspace_vm_limit_min;

-- Remove the closed evidence decoders and guards used only by the retired executor.
DO $$ DECLARE entry record; BEGIN
    FOR entry IN SELECT p.oid::regprocedure::text AS signature
        FROM pg_proc p JOIN pg_namespace ns ON ns.oid = p.pronamespace
        WHERE ns.nspname = 'public' AND
          (p.proname LIKE 'approval_review_%' OR p.proname LIKE 'agent_approval_%'
           OR p.proname IN ('reject_approval_context_update',
               'validate_agent_approval_status_v3_run','validate_agent_approval_status_v4_run'))
    LOOP EXECUTE format('DROP FUNCTION IF EXISTS %s CASCADE', entry.signature); END LOOP;
END $$;

-- Keep constraint/index names aligned with their live workspace owner.
DO $$ DECLARE entry record; BEGIN
    FOR entry IN SELECT conname FROM pg_constraint WHERE conrelid = 'workspace_merge_runs'::regclass
        AND conname LIKE '%job_merge%'
    LOOP EXECUTE format('ALTER TABLE workspace_merge_runs RENAME CONSTRAINT %I TO %I',
        entry.conname, replace(entry.conname, 'job_merge', 'workspace_merge')); END LOOP;
    FOR entry IN SELECT indexname FROM pg_indexes WHERE tablename = 'workspace_merge_runs'
        AND indexname LIKE '%job_merge%'
    LOOP EXECUTE format('ALTER INDEX %I RENAME TO %I',
        entry.indexname, replace(entry.indexname, 'job_merge', 'workspace_merge')); END LOOP;
END $$;

CREATE INDEX tasks_workspace_timeline ON tasks (workspace_id, created_at DESC, id DESC);
CREATE INDEX tasks_workspace_active ON tasks (workspace_id, created_at DESC, id DESC)
    WHERE status IN ('queued','running','waiting');
