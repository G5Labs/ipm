-- Membership activity deliberately does not change the role-derived ownership key.
ALTER TABLE public.org_memberships
    ADD COLUMN deactivated_at timestamptz,
    ADD COLUMN activity_revision bigint NOT NULL DEFAULT 0 CHECK (activity_revision >= 0),
    ADD COLUMN deactivation_cleanup_id uuid,
    ADD CONSTRAINT org_owner_always_active CHECK (role <> 'owner' OR
        (deactivated_at IS NULL AND deactivation_cleanup_id IS NULL));
CREATE INDEX org_memberships_pending_cleanup ON public.org_memberships(org_id, user_id)
    WHERE deactivation_cleanup_id IS NOT NULL;
CREATE INDEX workspaces_org_owner ON public.workspaces(org_id, owner_user_id, id);
-- Admission can fail before a workspace has VM state. Keep its retry schedule
-- with the workspace, fenced by the membership's cleanup generation.
ALTER TABLE public.workspaces
    ADD COLUMN member_cleanup_retry_id uuid,
    ADD COLUMN member_cleanup_retry_at timestamptz,
    ADD CONSTRAINT workspaces_member_cleanup_retry_pair CHECK (
        (member_cleanup_retry_id IS NULL) = (member_cleanup_retry_at IS NULL));

DROP TRIGGER org_memberships_bump_access_generation ON public.org_memberships;
CREATE TRIGGER org_memberships_bump_access_generation
    AFTER INSERT OR DELETE OR UPDATE OF role, deactivated_at ON public.org_memberships
    FOR EACH ROW EXECUTE FUNCTION public.g5_bump_access_generation();

SELECT set_config(
    'g5.member_activity_authz_lookup_membership_snapshot',
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
    'g5.member_activity_authz_lookup_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
SELECT set_config(
    'g5.member_activity_authz_lookup_had_authz_create',
    has_schema_privilege('authz_lookup_definer', 'authz', 'CREATE')::text,
    false
);
DO $$
BEGIN
    IF current_setting('g5.member_activity_authz_lookup_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        EXECUTE format(
            'GRANT authz_lookup_definer TO %I WITH SET TRUE',
            SESSION_USER
        );
    END IF;
END $$;
GRANT USAGE, CREATE ON SCHEMA authz TO authz_lookup_definer;

CREATE OR REPLACE FUNCTION authz.lock_user_scope_authority(
    p_user_id uuid, p_org_id uuid, p_workspace_id uuid, p_scopes text[]
) RETURNS TABLE (role text, granted_scopes text[])
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    actor_role text;
    grant_row record;
    additive_scopes text[] := '{}';
    expiring_grants jsonb := '[]';
BEGIN
    -- No caller-controlled GUC is authority. Bind to the verified ScopedDb
    -- backend context and its live session before touching private auth tables.
    IF NOT EXISTS (
        SELECT 1 FROM authz.session_ctx AS ctx
        JOIN authz.user_sessions AS session ON session.session_id = ctx.session_id
        WHERE ctx.pid = pg_backend_pid() AND ctx.user_id = p_user_id
          AND p_org_id = ANY(ctx.org_ids)
          AND session.user_id = ctx.user_id AND session.org_ids = ctx.org_ids
          AND session.scopes = ctx.scopes AND session.expires_at > clock_timestamp()
    ) THEN
        RETURN;
    END IF;
    -- Workspace liveness is part of the resolved authority context. Retain it
    -- through admission/commit, including collaborative and system-pool paths
    -- that do not enter the owner-only workspace mutation helper.
    IF p_workspace_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM public.workspaces AS workspace
        WHERE workspace.id = p_workspace_id AND workspace.org_id = p_org_id
          AND workspace.deleted_at IS NULL
        FOR SHARE OF workspace
    ) THEN
        RETURN;
    END IF;

    SELECT membership.role INTO actor_role
    FROM public.org_memberships AS membership
    WHERE membership.org_id = p_org_id AND membership.user_id = p_user_id
      AND membership.deactivated_at IS NULL
    FOR SHARE;
    IF NOT FOUND THEN
        RETURN;
    END IF;

    -- Keep every row contributing requested authority locked until the caller's
    -- mutation commits. Rows deleted/changed while admission waits are rechecked
    -- by FOR SHARE; only the locked versions contribute to the result.
    FOR grant_row IN
        SELECT defaults.scope FROM public.org_default_scopes AS defaults
        WHERE defaults.org_id = p_org_id AND defaults.scope = ANY(p_scopes)
        ORDER BY defaults.scope FOR SHARE
    LOOP
        additive_scopes := array_append(additive_scopes, grant_row.scope);
    END LOOP;
    FOR grant_row IN
        SELECT grants.scope, grants.expires_at FROM public.user_scope_grants AS grants
        WHERE grants.org_id = p_org_id AND grants.user_id = p_user_id
          AND grants.scope = ANY(p_scopes)
        ORDER BY grants.scope FOR SHARE
    LOOP
        expiring_grants := expiring_grants || jsonb_build_object(
            'scope', grant_row.scope, 'expires_at', grant_row.expires_at);
    END LOOP;
    IF p_workspace_id IS NOT NULL THEN
        FOR grant_row IN
            SELECT overrides.scope FROM public.workspace_permission_overrides AS overrides
            WHERE overrides.workspace_id = p_workspace_id AND overrides.user_id = p_user_id
              AND overrides.scope = ANY(p_scopes)
            ORDER BY overrides.scope FOR SHARE
        LOOP
            additive_scopes := array_append(additive_scopes, grant_row.scope);
        END LOOP;
    END IF;
    -- Evaluate expiry after all lock waits, against wall time rather than the
    -- transaction-start timestamp. Role defaults remain defined by AuthService.
    RETURN QUERY SELECT actor_role, additive_scopes || ARRAY(
        SELECT grants.scope FROM jsonb_to_recordset(expiring_grants)
            AS grants(scope text, expires_at timestamptz)
        WHERE grants.expires_at IS NULL OR grants.expires_at > clock_timestamp()
    );
END;
$$;


-- Read/hold membership only within the caller's verified tenant context. The
-- target can differ from the actor (administration and transfer recipients).
CREATE FUNCTION authz.lock_org_membership(p_org_id uuid, p_user_id uuid)
RETURNS TABLE(role text, deactivated_at timestamptz, activity_revision bigint,
              deactivation_cleanup_id uuid)
LANGUAGE sql VOLATILE SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
    SELECT m.role, m.deactivated_at, m.activity_revision, m.deactivation_cleanup_id
    FROM public.org_memberships m
    WHERE m.org_id = p_org_id AND m.user_id = p_user_id
      AND p_org_id = ANY(authz.current_org_ids())
    FOR SHARE OF m
$$;
ALTER FUNCTION authz.lock_org_membership(uuid, uuid) OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.lock_org_membership(uuid, uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.lock_org_membership(uuid, uuid) TO g5_app, g5_tenant;

-- A captured RLS capability must not reopen a disabled person's organization.
-- Service actors without a membership retain their existing restricted scope.
CREATE FUNCTION authz.session_memberships_active() RETURNS boolean
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
    SELECT COALESCE(cardinality(c.org_ids) = 0 OR cardinality(authz.current_org_ids()) > 0, false)
    FROM authz.session_ctx c WHERE c.pid = pg_backend_pid()
$$;
ALTER FUNCTION authz.session_memberships_active() OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.session_memberships_active() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.session_memberships_active() TO g5_app, g5_tenant;

-- The runtime facade already holds workspace authority. Hold the owner's
-- membership through bounded command admission; lifecycle cleanup bypasses this
-- predicate, never the incarnation checks in the facade.
CREATE FUNCTION authz.lock_workspace_member_activity(p_workspace_id uuid) RETURNS boolean
LANGUAGE plpgsql VOLATILE SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE owner_org uuid; owner_id uuid; permitted boolean;
BEGIN
    SELECT org_id, owner_user_id INTO owner_org, owner_id FROM public.workspaces
    WHERE id = p_workspace_id AND deleted_at IS NULL FOR SHARE;
    IF NOT FOUND THEN RETURN false; END IF;
    SELECT deactivated_at IS NULL AND deactivation_cleanup_id IS NULL INTO permitted
    FROM public.org_memberships WHERE org_id = owner_org AND user_id = owner_id FOR SHARE;
    RETURN COALESCE(permitted, false);
END
$$;
ALTER FUNCTION authz.lock_workspace_member_activity(uuid) OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.lock_workspace_member_activity(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.lock_workspace_member_activity(uuid) TO g5_app;

ALTER TABLE public.workspace_vms
    DROP CONSTRAINT workspace_vms_stop_reason_check,
    DROP CONSTRAINT workspace_vms_pending_stop_reason_check,
    ADD CONSTRAINT workspace_vms_stop_reason_check CHECK (stop_reason IS NULL OR stop_reason IN
      ('user','transfer','idle','drain','rollout','credential','capacity','delete','member_deactivated')),
    ADD CONSTRAINT workspace_vms_pending_stop_reason_check CHECK (pending_stop_reason IS NULL OR pending_stop_reason IN
      ('user','transfer','idle','drain','rollout','credential','capacity','delete','member_deactivated'));

GRANT SELECT ON public.workspaces, public.workspace_vms, public.tasks TO authz_lookup_definer;
GRANT UPDATE (deactivation_cleanup_id) ON public.org_memberships TO authz_lookup_definer;
CREATE FUNCTION authz.finish_member_cleanup(p_org uuid, p_user uuid, p_cleanup uuid) RETURNS boolean
LANGUAGE plpgsql VOLATILE SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
BEGIN
    IF NOT COALESCE(p_org = ANY(authz.current_org_ids()), false) THEN RETURN false; END IF;
    PERFORM 1 FROM public.org_memberships WHERE org_id = p_org AND user_id = p_user
        AND deactivation_cleanup_id = p_cleanup FOR UPDATE;
    IF NOT FOUND THEN RETURN false; END IF;
    IF EXISTS (SELECT 1 FROM public.workspaces w
        WHERE w.org_id = p_org AND w.owner_user_id = p_user AND (
          EXISTS (SELECT 1 FROM public.workspace_vms vm WHERE vm.workspace_id = w.id
             AND (vm.status IN ('starting','running','stopping') OR vm.vm_id IS NOT NULL
                  OR vm.start_intent->>'status' IN ('pending','queued_for_capacity','starting')))
          OR EXISTS (SELECT 1 FROM public.tasks t WHERE t.workspace_id = w.id
                     AND t.status IN ('queued','running')))) THEN RETURN false; END IF;
    UPDATE public.org_memberships SET deactivation_cleanup_id = NULL
        WHERE org_id = p_org AND user_id = p_user AND deactivation_cleanup_id = p_cleanup;
    RETURN FOUND;
END
$$;
ALTER FUNCTION authz.finish_member_cleanup(uuid, uuid, uuid) OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.finish_member_cleanup(uuid, uuid, uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.finish_member_cleanup(uuid, uuid, uuid) TO g5_app, g5_tenant;

CREATE FUNCTION authz.settled_member_cleanups(p_limit bigint)
RETURNS TABLE(org_id uuid, user_id uuid, cleanup_id uuid)
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
    SELECT m.org_id, m.user_id, m.deactivation_cleanup_id
    FROM public.org_memberships m WHERE m.org_id = ANY(authz.current_org_ids())
      AND m.deactivation_cleanup_id IS NOT NULL
      AND NOT EXISTS (SELECT 1 FROM public.workspaces w
        LEFT JOIN public.workspace_vms vm ON vm.workspace_id = w.id
        WHERE w.org_id = m.org_id AND w.owner_user_id = m.user_id AND (
          vm.status IN ('starting','running','stopping') OR vm.vm_id IS NOT NULL
          OR vm.start_intent->>'status' IN ('pending','queued_for_capacity','starting')
          OR EXISTS (SELECT 1 FROM public.tasks t WHERE t.workspace_id = w.id
                     AND t.status IN ('queued','running'))))
    ORDER BY m.org_id, m.user_id LIMIT LEAST(GREATEST(p_limit,1),32)
$$;
ALTER FUNCTION authz.settled_member_cleanups(bigint) OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.settled_member_cleanups(bigint) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.settled_member_cleanups(bigint) TO g5_app, g5_tenant;

-- Start from the pending-membership index instead of projecting membership
-- once for every workspace on every lifecycle tick. Include deleted custody.
CREATE FUNCTION authz.pending_member_cleanup_workspaces(p_limit bigint)
RETURNS TABLE(id uuid, org_id uuid, owner_user_id uuid, cleanup_id uuid)
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
    SELECT w.id, w.org_id, w.owner_user_id, m.deactivation_cleanup_id
    FROM public.org_memberships m
    JOIN public.workspaces w ON w.org_id = m.org_id AND w.owner_user_id = m.user_id
    LEFT JOIN public.workspace_vms vm ON vm.workspace_id = w.id
    WHERE m.org_id = ANY(authz.current_org_ids()) AND m.deactivation_cleanup_id IS NOT NULL
      AND (w.member_cleanup_retry_id IS DISTINCT FROM m.deactivation_cleanup_id
        OR w.member_cleanup_retry_at <= statement_timestamp())
      AND (vm.status IN ('starting','running')
        OR vm.start_intent->>'status' IN ('pending','queued_for_capacity','starting')
        OR EXISTS (SELECT 1 FROM public.tasks t WHERE t.workspace_id = w.id AND t.status = 'queued'))
    -- Even after retries become due, visit unattempted work first. Repeated
    -- failures rotate to the end of the retry queue rather than pinning a page.
    ORDER BY CASE WHEN w.member_cleanup_retry_id = m.deactivation_cleanup_id
                  THEN w.member_cleanup_retry_at END NULLS FIRST,
             m.org_id, m.user_id, w.id
    LIMIT LEAST(GREATEST(p_limit,1),32)
$$;
ALTER FUNCTION authz.pending_member_cleanup_workspaces(bigint) OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.pending_member_cleanup_workspaces(bigint) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.pending_member_cleanup_workspaces(bigint) TO g5_app, g5_tenant;

-- Selection is only an optimization; mutations hold the membership below.
CREATE FUNCTION authz.workspace_execution_allowed(p_workspace uuid) RETURNS boolean
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
    SELECT EXISTS (SELECT 1 FROM public.workspaces w JOIN public.org_memberships m
      ON m.org_id = w.org_id AND m.user_id = w.owner_user_id
      WHERE w.id = p_workspace AND w.org_id = ANY(authz.current_org_ids())
        AND m.deactivated_at IS NULL AND m.deactivation_cleanup_id IS NULL)
$$;
ALTER FUNCTION authz.workspace_execution_allowed(uuid) OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.workspace_execution_allowed(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.workspace_execution_allowed(uuid) TO g5_app, g5_tenant;

CREATE FUNCTION authz.lock_workspace_cleanup_pending(p_workspace uuid) RETURNS boolean
LANGUAGE plpgsql VOLATILE SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE owner_org uuid; owner_id uuid; pending boolean;
BEGIN
    SELECT org_id, owner_user_id INTO owner_org, owner_id FROM public.workspaces
      WHERE id = p_workspace AND org_id = ANY(authz.current_org_ids()) FOR SHARE;
    IF NOT FOUND THEN RETURN true; END IF;
    SELECT deactivation_cleanup_id IS NOT NULL OR deactivated_at IS NOT NULL INTO pending
      FROM public.org_memberships WHERE org_id = owner_org AND user_id = owner_id FOR SHARE;
    RETURN COALESCE(pending, true);
END
$$;
ALTER FUNCTION authz.lock_workspace_cleanup_pending(uuid) OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.lock_workspace_cleanup_pending(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.lock_workspace_cleanup_pending(uuid) TO g5_app, g5_tenant;

-- The role-derived foreign key remains unchanged. Only acquisition requires
-- activity, so an inactive owner's existing references remain valid.
CREATE FUNCTION authz.guard_workspace_owner_activity() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
BEGIN
    IF NEW.owner_user_id IS NOT NULL THEN
        PERFORM 1 FROM public.org_memberships WHERE org_id = NEW.org_id
          AND user_id = NEW.owner_user_id AND deactivated_at IS NULL FOR SHARE;
        IF NOT FOUND THEN RAISE EXCEPTION 'Workspace owner must be an active organization member'
            USING ERRCODE = '23514'; END IF;
    END IF;
    RETURN NEW;
END
$$;
ALTER FUNCTION authz.guard_workspace_owner_activity() OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.guard_workspace_owner_activity() FROM PUBLIC;
CREATE TRIGGER workspace_owner_activity_insert BEFORE INSERT ON public.workspaces
    FOR EACH ROW EXECUTE FUNCTION authz.guard_workspace_owner_activity();
CREATE TRIGGER workspace_owner_activity_update BEFORE UPDATE OF owner_user_id, org_id ON public.workspaces
    FOR EACH ROW WHEN (OLD.owner_user_id IS DISTINCT FROM NEW.owner_user_id OR OLD.org_id <> NEW.org_id)
    EXECUTE FUNCTION authz.guard_workspace_owner_activity();

CREATE FUNCTION authz.guard_vm_member_activity() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
BEGIN
    IF NEW.status IN ('starting','running') AND (TG_OP = 'INSERT' OR
       OLD.status IS DISTINCT FROM NEW.status OR OLD.vm_id IS DISTINCT FROM NEW.vm_id OR OLD.epoch <> NEW.epoch) THEN
        IF NOT authz.lock_workspace_member_activity(NEW.workspace_id) THEN
            RAISE EXCEPTION 'Workspace execution requires active membership and completed cleanup'
                USING ERRCODE = '23514';
        END IF;
    END IF;
    RETURN NEW;
END
$$;
ALTER FUNCTION authz.guard_vm_member_activity() OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.guard_vm_member_activity() FROM PUBLIC;
CREATE TRIGGER workspace_vm_member_activity BEFORE INSERT OR UPDATE ON public.workspace_vms
    FOR EACH ROW EXECUTE FUNCTION authz.guard_vm_member_activity();

CREATE OR REPLACE FUNCTION public.g5_workspace_vm_identity_is_fenced() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog'
    AS $$
BEGIN
    IF OLD.vm_id IS NOT NULL AND NEW.vm_id = OLD.vm_id AND NEW.epoch = OLD.epoch THEN
        IF OLD.boot_spec IS NOT NULL AND NEW.boot_spec IS DISTINCT FROM OLD.boot_spec THEN
            RAISE EXCEPTION 'boot source is immutable within an incarnation' USING ERRCODE = '23514';
        END IF;
        IF OLD.stop_operation_id IS NOT NULL
           AND NEW.stop_operation_id IS DISTINCT FROM OLD.stop_operation_id
           AND NOT (
               OLD.status = 'stopping' AND NEW.status = 'running'
               AND NEW.stop_operation_id IS NULL
               AND NEW.stop_generation IS NULL
               AND NEW.stop_reason IS NULL
               AND NEW.stop_retry_at IS NULL
               AND NEW.stop_deadline_at IS NULL
               AND NEW.stop_command_sent_at IS NULL
           )
           AND NOT (
               OLD.status IN ('starting', 'running')
               AND OLD.pending_stop_reason IS NOT NULL
               AND OLD.pending_stop_deadline_at IS NOT NULL
               AND OLD.nudged_at IS NOT NULL
               AND NEW.status = 'stopping'
               AND NEW.stop_operation_id IS NOT NULL
               AND (
                   NEW.stop_reason IN ('user', 'transfer', 'idle', 'member_deactivated')
                   OR (
                       NEW.stop_reason = 'delete'
                       AND NEW.stop_retry_at <= statement_timestamp()
                   )
               )
               AND NEW.stop_generation IS NOT NULL
               AND NEW.stop_deadline_at IS NOT NULL
               AND NEW.pending_stop_reason IS NULL
               AND NEW.pending_stop_deadline_at IS NULL
               AND NEW.nudged_at IS NULL
               AND NEW.stop_command_sent_at IS NULL
           )
           AND NOT (
               OLD.status = 'stopping'
               AND NEW.status = 'stopping'
               AND NEW.stop_reason = 'delete'
               AND NEW.stop_operation_id IS NOT NULL
               AND NEW.stop_generation > OLD.stop_generation
               AND NEW.stop_retry_at <= statement_timestamp()
               AND NEW.pending_stop_reason IS NULL
               AND NEW.pending_stop_deadline_at IS NULL
               AND NEW.nudged_at IS NULL
           )
        THEN
            RAISE EXCEPTION 'unreleased stop operation cannot be overwritten' USING ERRCODE = '23514';
        END IF;
    END IF;

    IF OLD.start_intent IS NOT NULL
       AND OLD.start_intent ->> 'status' IN ('pending', 'queued_for_capacity', 'starting')
       AND NEW.start_intent IS NOT NULL
       AND NEW.start_intent ->> 'id' IS DISTINCT FROM OLD.start_intent ->> 'id'
       AND NEW.start_intent ->> 'supersedes_intent_id' IS DISTINCT FROM OLD.start_intent ->> 'id'
    THEN
        RAISE EXCEPTION 'active start intent requires explicit fenced supersession'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

SELECT set_config(
    'g5.member_activity_authz_ctx_membership_snapshot',
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
        WHERE granted_role.rolname = 'authz_ctx_writer'
          AND member_role.rolname = SESSION_USER
    ), '[]'::jsonb)::text,
    false
);
SELECT set_config(
    'g5.member_activity_authz_ctx_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_ctx_writer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
SELECT set_config(
    'g5.member_activity_authz_ctx_had_authz_create',
    has_schema_privilege('authz_ctx_writer', 'authz', 'CREATE')::text,
    false
);
DO $$
BEGIN
    IF current_setting('g5.member_activity_authz_ctx_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        EXECUTE format(
            'GRANT authz_ctx_writer TO %I WITH SET TRUE',
            SESSION_USER
        );
    END IF;
END $$;
GRANT USAGE, CREATE ON SCHEMA authz TO authz_ctx_writer;

CREATE FUNCTION authz.active_session_org_ids() RETURNS uuid[]
LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED SET search_path = pg_catalog, pg_temp AS $$
    SELECT ARRAY(SELECT org FROM unnest(c.org_ids) org WHERE NOT EXISTS (
        SELECT 1 FROM public.org_memberships m WHERE m.org_id = org AND m.user_id = c.user_id
          AND m.deactivated_at IS NOT NULL))
    FROM authz.session_ctx c WHERE c.pid = pg_backend_pid()
$$;
ALTER FUNCTION authz.active_session_org_ids() OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.active_session_org_ids() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.active_session_org_ids() TO authz_ctx_writer;
CREATE OR REPLACE FUNCTION authz.current_org_ids() RETURNS uuid[]
LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED SET search_path = pg_catalog, pg_temp AS $$
    SELECT authz.active_session_org_ids()
$$;

-- A STABLE function in an RLS row filter can still execute once per row. Give
-- every existing policy a statement-local InitPlan, while preserving its roles,
-- command, permissiveness and all other predicates. Explicit uuid[] avoids ANY
-- being parsed as a subquery comparison against an array-valued row.
DO $$
DECLARE
    policy_row record;
    alteration text;
BEGIN
    FOR policy_row IN
        SELECT ns.nspname, tbl.relname, pol.polname,
               pg_get_expr(pol.polqual, pol.polrelid) AS using_expr,
               pg_get_expr(pol.polwithcheck, pol.polrelid) AS check_expr
        FROM pg_policy pol
        JOIN pg_class tbl ON tbl.oid = pol.polrelid
        JOIN pg_namespace ns ON ns.oid = tbl.relnamespace
        WHERE ns.nspname = 'public'
          AND (pg_get_expr(pol.polqual, pol.polrelid) LIKE '%authz.current_org_ids()%'
            OR pg_get_expr(pol.polwithcheck, pol.polrelid) LIKE '%authz.current_org_ids()%')
    LOOP
        alteration := format('ALTER POLICY %I ON %I.%I',
                             policy_row.polname, policy_row.nspname, policy_row.relname);
        IF policy_row.using_expr IS NOT NULL THEN
            alteration := alteration || format(' USING (%s)', replace(
                policy_row.using_expr, 'authz.current_org_ids()',
                '(SELECT authz.current_org_ids())::uuid[]'));
        END IF;
        IF policy_row.check_expr IS NOT NULL THEN
            alteration := alteration || format(' WITH CHECK (%s)', replace(
                policy_row.check_expr, 'authz.current_org_ids()',
                '(SELECT authz.current_org_ids())::uuid[]'));
        END IF;
        EXECUTE alteration;
    END LOOP;
END $$;

-- Restore the migrator's exact membership options and the function owner's
-- pre-migration CREATE privilege. Keep USAGE on authz for the new function's
-- session validation, plus the permanent table/function grants
-- above: those are the runtime authority this migration is installing.
DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.member_activity_authz_ctx_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.member_activity_authz_ctx_had_set', true)
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
            WHERE granted_role.rolname = 'authz_ctx_writer'
              AND member_role.rolname = SESSION_USER
        LOOP
            SELECT snapshot_row
              INTO prior_row
              FROM jsonb_array_elements(membership_snapshot) AS snapshot_row
             WHERE snapshot_row->>'grantor' = membership_row.grantor_name;

            IF prior_row IS NULL THEN
                EXECUTE format(
                    'REVOKE authz_ctx_writer FROM %I',
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
                    'GRANT authz_ctx_writer TO %I WITH ADMIN %s, INHERIT %s, SET %s',
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
    IF current_setting('g5.member_activity_authz_ctx_had_authz_create', true)
        IS DISTINCT FROM 'true'
    THEN
        REVOKE CREATE ON SCHEMA authz FROM authz_ctx_writer;
    END IF;
END $$;

-- Restore the migrator's exact membership options and the function owner's
-- pre-migration CREATE privilege. Keep USAGE on authz for the new function's
-- session validation, plus the permanent table/function grants
-- above: those are the runtime authority this migration is installing.
DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.member_activity_authz_lookup_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.member_activity_authz_lookup_had_set', true)
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
    IF current_setting('g5.member_activity_authz_lookup_had_authz_create', true)
        IS DISTINCT FROM 'true'
    THEN
        REVOKE CREATE ON SCHEMA authz FROM authz_lookup_definer;
    END IF;
END $$;
