-- =============================================================================
-- 001_lean_baseline.sql — the G5-Lean schema baseline
-- =============================================================================
--
-- This is the only migration. P0.2 (docs/lean-implementation-plan.md) produced
-- it from a fresh database that had been migrated with the 289 historical
-- migrations (main at 148019f; tag `lean-before`): `pg_dump --schema-only`
-- with owners and privileges kept and `_sqlx_migrations` excluded, then
-- reassembled so that a non-superuser migrator can apply it. A second fresh
-- database migrated with this file alone yields an identical schema dump.
--
-- Editing convention (plan §2.6): there is no deployed data, so this file is
-- edited in place until the first production deployment; forward migrations
-- resume after that. Keep each PR's edits inside a delimited section:
--
--     -- P4.1 begin
--     ...
--     -- P4.1 end
--
-- so rebases stay trivial, and regenerate the SQLx offline cache
-- (`just db-prepare`) whenever queries change.
--
-- Layout (pg_dump dependency order): session settings and cluster roles,
-- schema `authz`, extensions, functions, tables and sequences, column
-- defaults, constraints, indexes, triggers, policies, row-level security,
-- privileges, then the scaffolding cleanup and apply-time assertions. There
-- is no seed data: the historical migrations inserted no rows.
--
-- Migrator requirements (unchanged from the historical migrations): the role
-- applying this file must be able to create roles (CREATEROLE or SUPERUSER),
-- must be able to SET ROLE to `authz_ctx_writer` (granted out of band by
-- scripts/ci-provision-migrator.sh and by the Helm migrate job's init
-- container), needs BYPASSRLS to create the BYPASSRLS role
-- `authz_lookup_definer` when that role does not exist yet, and needs the
-- `vector` and `pgcrypto` extensions pre-created or the privilege to create
-- them. SQLx applies the file inside one transaction; apply it with
-- `psql -1` when running it by hand.
-- =============================================================================

-- Functions are created before the tables they reference (pg_dump order), so
-- SQL-language bodies must not be validated at creation time. Both settings
-- are reset at the end of the file and roll back with a failed apply.
SET check_function_bodies = false;
SET client_min_messages = warning;

-- -----------------------------------------------------------------------------
-- Cluster roles (idempotent: they may already exist in a shared cluster)
-- -----------------------------------------------------------------------------
-- g5_tenant        tenant pool: DML on RLS-protected tables, FORCE RLS applies
-- g5_app           system pool: DML on system tables, authz function access
-- authz_writer     mints authz.user_sessions
-- authz_ctx_writer owns authz.set_ctx/clear_ctx/current_org_ids, manages
--                  authz.session_ctx
-- authz_lookup_definer  BYPASSRLS owner of the SECURITY DEFINER functions that
--                  must see rows without an org binding (host authentication,
--                  worker-host reconciliation, sticky-runtime retirement)
-- Login roles are provisioned out of band and inherit from these.
DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'g5_tenant') THEN
        CREATE ROLE g5_tenant NOLOGIN;
    END IF;
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'g5_app') THEN
        CREATE ROLE g5_app NOLOGIN;
    END IF;
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'authz_writer') THEN
        CREATE ROLE authz_writer NOLOGIN;
    END IF;
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'authz_ctx_writer') THEN
        CREATE ROLE authz_ctx_writer NOLOGIN;
    END IF;
END $$;

-- Migrator scaffolding for `authz_lookup_definer`. `ALTER FUNCTION ... OWNER
-- TO authz_lookup_definer` requires the migrator to be able to SET ROLE to
-- that role. Snapshot the migrator's SET path and its exact membership rows
-- BEFORE the role may be created, so a PG16+ automatic creator membership is
-- not mistaken for a pre-existing grant; the cleanup section at the end of
-- this file restores exactly this state. The settings are session-scoped on
-- purpose (they survive whether or not the file runs inside one transaction).
SELECT set_config(
    'g5.baseline_had_authz_lookup_definer_set',
    CASE
        WHEN to_regrole('authz_lookup_definer') IS NULL THEN 'false'
        WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET') THEN 'true'
        ELSE 'false'
    END,
    false
);
SELECT set_config(
    'g5.baseline_authz_lookup_definer_membership_snapshot',
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
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'authz_lookup_definer') THEN
        CREATE ROLE authz_lookup_definer WITH BYPASSRLS NOLOGIN;
    END IF;
    -- WITH SET TRUE is load-bearing: an existing membership row may carry
    -- set_option = false, and a plain re-GRANT does not change omitted
    -- options. Let PostgreSQL pick an eligible direct or indirect ADMIN
    -- grantor rather than forcing one.
    IF current_setting('g5.baseline_had_authz_lookup_definer_set', true)
        IS DISTINCT FROM 'true'
    THEN
        EXECUTE format('GRANT authz_lookup_definer TO %I WITH SET TRUE', SESSION_USER);
    END IF;
END $$;

-- SCHEMA authz
CREATE SCHEMA authz;

-- Ownership reassignment below requires the new owner to hold CREATE on the
-- containing schema at that moment. authz_ctx_writer keeps its schema
-- privileges permanently (repeated in the privileges section);
-- authz_lookup_definer's are temporary, and the cleanup section revokes
-- whatever this file had to add.
GRANT USAGE, CREATE ON SCHEMA authz TO authz_ctx_writer;
SELECT set_config(
    'g5.baseline_authz_lookup_definer_had_authz_usage',
    CASE WHEN has_schema_privilege('authz_lookup_definer', 'authz', 'USAGE') THEN 'true' ELSE 'false' END,
    false
);
SELECT set_config(
    'g5.baseline_authz_lookup_definer_had_authz_create',
    CASE WHEN has_schema_privilege('authz_lookup_definer', 'authz', 'CREATE') THEN 'true' ELSE 'false' END,
    false
);
GRANT USAGE, CREATE ON SCHEMA authz TO authz_lookup_definer;

-- EXTENSION pgcrypto
CREATE EXTENSION IF NOT EXISTS pgcrypto WITH SCHEMA public;

-- EXTENSION vector
CREATE EXTENSION IF NOT EXISTS vector WITH SCHEMA public;

-- FUNCTION authz.apply_retired_codex_auth_config_row(text, uuid, uuid, uuid, jsonb, bytea, text[], jsonb, bytea, text[]) (owner authz_lookup_definer)
CREATE FUNCTION authz.apply_retired_codex_auth_config_row(p_scope_kind text, p_org_id uuid, p_workspace_id uuid, p_user_id uuid, p_expected_settings jsonb, p_expected_secrets_enc bytea, p_expected_secret_keys text[], p_next_settings jsonb, p_next_secrets_enc bytea, p_next_secret_keys text[]) RETURNS boolean
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'pg_catalog'
    AS $$
DECLARE
    affected INTEGER;
BEGIN
    IF jsonb_typeof(p_next_settings) <> 'object'
       OR EXISTS (
            SELECT 1 FROM jsonb_object_keys(p_next_settings) AS keys(setting_key)
            WHERE btrim(setting_key) = 'CODEX_AUTH_JSON'
       )
       OR EXISTS (
            SELECT 1 FROM unnest(p_next_secret_keys) AS keys(secret_key)
            WHERE btrim(secret_key) = 'CODEX_AUTH_JSON'
       ) THEN
        RAISE EXCEPTION 'retired CODEX_AUTH_JSON key remains in sanitized config output';
    END IF;

    IF p_scope_kind = 'org' THEN
        UPDATE public.org_configs
        SET settings_json = p_next_settings,
            secrets_enc = p_next_secrets_enc,
            secret_keys = p_next_secret_keys,
            updated_at = now()
        WHERE org_id = p_org_id
          AND settings_json = p_expected_settings
          AND secrets_enc IS NOT DISTINCT FROM p_expected_secrets_enc
          AND secret_keys = p_expected_secret_keys;
    ELSIF p_scope_kind = 'workspace' THEN
        UPDATE public.workspace_configs
        SET settings_json = p_next_settings,
            secrets_enc = p_next_secrets_enc,
            secret_keys = p_next_secret_keys,
            updated_at = now()
        WHERE workspace_id = p_workspace_id
          AND settings_json = p_expected_settings
          AND secrets_enc IS NOT DISTINCT FROM p_expected_secrets_enc
          AND secret_keys = p_expected_secret_keys;
    ELSIF p_scope_kind = 'user' THEN
        UPDATE public.user_configs
        SET settings_json = p_next_settings,
            secrets_enc = p_next_secrets_enc,
            secret_keys = p_next_secret_keys,
            updated_at = now()
        WHERE org_id = p_org_id
          AND user_id = p_user_id
          AND settings_json = p_expected_settings
          AND secrets_enc IS NOT DISTINCT FROM p_expected_secrets_enc
          AND secret_keys = p_expected_secret_keys;
    ELSE
        RAISE EXCEPTION 'unknown Runtime Config scope: %', p_scope_kind;
    END IF;

    GET DIAGNOSTICS affected = ROW_COUNT;
    RETURN affected = 1;
END
$$;

ALTER FUNCTION authz.apply_retired_codex_auth_config_row(p_scope_kind text, p_org_id uuid, p_workspace_id uuid, p_user_id uuid, p_expected_settings jsonb, p_expected_secrets_enc bytea, p_expected_secret_keys text[], p_next_settings jsonb, p_next_secrets_enc bytea, p_next_secret_keys text[]) OWNER TO authz_lookup_definer;

-- FUNCTION authz.attach_orphan_worker_host_cloud_refs(uuid, text, text) (owner authz_lookup_definer)
CREATE FUNCTION authz.attach_orphan_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text) RETURNS uuid
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    UPDATE public.worker_hosts
    SET vpc_id = p_vpc_id,
        cloud_instance_id = p_cloud_instance_id
    WHERE host_id = p_host_id
      AND status = 'provisioning'
      AND cloud_instance_id IS NULL
      AND (bringup_heartbeat_at IS NULL
           OR bringup_heartbeat_at < now() - interval '90 seconds')
    RETURNING host_id
$$;

ALTER FUNCTION authz.attach_orphan_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION attach_orphan_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text)
COMMENT ON FUNCTION authz.attach_orphan_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text) IS 'Orphan-pass attach ([D39]/[D60]): links a found-alive instance to a row whose bring-up lease is absent or stale, re-checking the lease inside the guard so a concurrently-renewed bring-up is never preempted. SECURITY DEFINER + BYPASSRLS owner.';

-- FUNCTION authz.clear_ctx() (owner authz_ctx_writer)
CREATE FUNCTION authz.clear_ctx() RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'authz', 'pg_catalog'
    AS $$
BEGIN
    DELETE FROM authz.session_ctx WHERE pid = pg_backend_pid();
END;
$$;

ALTER FUNCTION authz.clear_ctx() OWNER TO authz_ctx_writer;

-- COMMENT authz.FUNCTION clear_ctx()
COMMENT ON FUNCTION authz.clear_ctx() IS 'Removes the session context for the current connection. Called on transaction commit and by after_acquire pool hook.';

-- FUNCTION authz.current_org_ids() (owner authz_ctx_writer)
CREATE FUNCTION authz.current_org_ids() RETURNS uuid[]
    LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED
    SET search_path TO 'authz', 'pg_catalog'
    AS $$
    SELECT org_ids FROM authz.session_ctx WHERE pid = pg_backend_pid();
$$;

ALTER FUNCTION authz.current_org_ids() OWNER TO authz_ctx_writer;

-- COMMENT authz.FUNCTION current_org_ids()
COMMENT ON FUNCTION authz.current_org_ids() IS 'Returns the org_ids array for the current connection''s session. Used by RLS policies. Returns NULL when no session → fail-closed. STABLE + PARALLEL RESTRICTED required for query planner safety.';

-- FUNCTION authz.get_worker_host_cloud_linkage(uuid) (owner authz_lookup_definer)
CREATE FUNCTION authz.get_worker_host_cloud_linkage(p_host_id uuid) RETURNS TABLE(status text, cloud_instance_id text)
    LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    SELECT wh.status, wh.cloud_instance_id
    FROM public.worker_hosts wh
    WHERE wh.host_id = p_host_id
$$;

ALTER FUNCTION authz.get_worker_host_cloud_linkage(p_host_id uuid) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION get_worker_host_cloud_linkage(p_host_id uuid)
COMMENT ON FUNCTION authz.get_worker_host_cloud_linkage(p_host_id uuid) IS 'Bring-up compensation ownership check ([D61], Codex round-2 blocker 2): reads the row''s current status + cloud_instance_id under FORCE RLS so the detached bring-up can verify nothing owns the instance it is about to terminate. SECURITY DEFINER + BYPASSRLS owner.';

-- FUNCTION authz.guard_sticky_task_execution_admission() (owner authz_lookup_definer)
CREATE FUNCTION authz.guard_sticky_task_execution_admission() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public', 'pg_catalog'
    AS $$
DECLARE
    v_fenced BOOLEAN;
BEGIN
    IF NOT (
        (NEW.status = 'claimed' AND OLD.status <> 'claimed')
        OR (NEW.status = 'running' AND OLD.status <> 'running')
        OR (OLD.worker_id IS NULL AND NEW.worker_id IS NOT NULL)
        OR (OLD.assigned_at IS NULL AND NEW.assigned_at IS NOT NULL)
        OR (OLD.lease_expires_at IS NULL AND NEW.lease_expires_at IS NOT NULL)
        OR (
            OLD.active_execution_attempt_id IS NULL
            AND NEW.active_execution_attempt_id IS NOT NULL
        )
    ) THEN
        RETURN NEW;
    END IF;

    PERFORM pg_advisory_xact_lock(
        hashtextextended(
            'g5:sticky-task-claim-transition:' || NEW.id::text,
            0
        )
    );

    SELECT EXISTS (
        SELECT 1
        FROM public.chat_sticky_runtimes retiring_runtime
        WHERE retiring_runtime.task_id = NEW.id
          AND retiring_runtime.org_id = NEW.org_id
          AND (
              retiring_runtime.status = 'reaping'
              OR (
                  retiring_runtime.teardown_operation_id IS NOT NULL
                  AND retiring_runtime.teardown_workspace_released_at IS NULL
              )
          )
    ) INTO v_fenced;

    IF v_fenced THEN
        RAISE EXCEPTION
            'task % execution admission is fenced by unresolved sticky-runtime workspace custody',
            NEW.id
            USING ERRCODE = '40001';
    END IF;

    RETURN NEW;
END;
$$;

ALTER FUNCTION authz.guard_sticky_task_execution_admission() OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION guard_sticky_task_execution_admission()
COMMENT ON FUNCTION authz.guard_sticky_task_execution_admission() IS 'Rolling-upgrade guard for legacy task claim/ACK UPDATEs: takes the issue #2226 per-task transition mutex and raises serialization_failure on first claimed/running or nullable execution-authority acquisition while same-task teardown custody remains fenced (task_service D56).';

-- FUNCTION authz.guard_worker_insert_dead_host() (owner authz_lookup_definer)
CREATE FUNCTION authz.guard_worker_insert_dead_host() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public', 'pg_catalog'
    AS $$
BEGIN
    -- Serialize against a concurrent host termination: the shared row lock
    -- conflicts with the termination UPDATE's lock, so either this insert
    -- commits first (and the release-time conjunction check sees the live
    -- worker) or termination commits first (and the re-read below sees the
    -- terminal status and refuses). An unlocked MVCC read would leave a
    -- window where an in-flight insert that read the host as live commits a
    -- custodian after the watchdog already stamped release.
    PERFORM 1
    FROM public.worker_hosts wh
    WHERE wh.org_id = NEW.org_id
      AND (wh.host_id = NEW.host_id OR wh.host_id = NEW.external_host_id)
    FOR SHARE OF wh;

    IF EXISTS (
        SELECT 1
        FROM public.worker_hosts wh
        WHERE wh.org_id = NEW.org_id
          AND (wh.host_id = NEW.host_id OR wh.host_id = NEW.external_host_id)
          AND wh.status = 'terminated'
    ) THEN
        RAISE EXCEPTION
            'worker host for % is terminated; register under a fresh host identity instead (issue #2310)',
            NEW.id
            USING ERRCODE = '55006'; -- object_in_use: lets the registration boundary map this refusal to 409
    END IF;
    RETURN NEW;
END;
$$;

ALTER FUNCTION authz.guard_worker_insert_dead_host() OWNER TO authz_lookup_definer;

-- FUNCTION authz.guard_worker_revival_dead_host() (owner authz_lookup_definer)
CREATE FUNCTION authz.guard_worker_revival_dead_host() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public', 'pg_catalog'
    AS $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM public.worker_hosts wh
        WHERE wh.org_id = OLD.org_id
          AND (wh.host_id = OLD.host_id OR wh.host_id = OLD.external_host_id)
          AND wh.status = 'terminated'
    ) THEN
        RAISE EXCEPTION
            'worker % is terminated and bound to a terminated worker host; register a new worker identity instead of reviving it (issue #2310)',
            OLD.id
            USING ERRCODE = '55006'; -- object_in_use: lets the registration boundary map this refusal to 409
    END IF;
    RETURN NEW;
END;
$$;

ALTER FUNCTION authz.guard_worker_revival_dead_host() OWNER TO authz_lookup_definer;

-- FUNCTION authz.list_orphan_bringup_worker_hosts() (owner authz_lookup_definer)
CREATE FUNCTION authz.list_orphan_bringup_worker_hosts() RETURNS TABLE(host_id uuid, org_id uuid, region text, created_at timestamp with time zone, orphan_check_misses integer)
    LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    SELECT wh.host_id, wh.org_id, wh.region, wh.created_at, wh.orphan_check_misses
    FROM public.worker_hosts wh
    WHERE wh.status = 'provisioning'
      AND wh.cloud_instance_id IS NULL
      AND wh.provision_error_code IS NULL
      AND wh.created_at < now() - interval '60 seconds'
      -- [D60]: a fresh bring-up lease means the detached task is alive
      -- (possibly deep in a [D59] multi-zone stockout retry) — the row is in
      -- flight, not orphaned. NULL keeps pre-171 semantics for legacy rows.
      AND (wh.bringup_heartbeat_at IS NULL
           OR wh.bringup_heartbeat_at < now() - interval '90 seconds')
$$;

ALTER FUNCTION authz.list_orphan_bringup_worker_hosts() OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION list_orphan_bringup_worker_hosts()
COMMENT ON FUNCTION authz.list_orphan_bringup_worker_hosts() IS 'Reconciler orphan-pass read of provisioning hosts whose bring-up never persisted cloud_instance_id AND whose bring-up liveness lease (bringup_heartbeat_at) is absent or stale ([D39]/[D60]). SECURITY DEFINER + BYPASSRLS owner so the system pool can read cross-org under FORCE RLS.';

-- FUNCTION authz.list_provisioning_worker_hosts() (owner authz_lookup_definer)
CREATE FUNCTION authz.list_provisioning_worker_hosts() RETURNS TABLE(host_id uuid, org_id uuid, region text, cloud_instance_id text, has_heartbeat boolean, cloud_gone_since timestamp with time zone)
    LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    SELECT wh.host_id, wh.org_id, wh.region, wh.cloud_instance_id,
           (wh.last_heartbeat IS NOT NULL), wh.cloud_gone_since
    FROM public.worker_hosts wh
    WHERE wh.status = 'provisioning' AND wh.cloud_instance_id IS NOT NULL
$$;

ALTER FUNCTION authz.list_provisioning_worker_hosts() OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION list_provisioning_worker_hosts()
COMMENT ON FUNCTION authz.list_provisioning_worker_hosts() IS 'Reconciler read of provisioning hosts with a backing cloud instance ([D20]); since migration 171 also projects region + cloud_gone_since for the [D61] fallback-aware wall-clock gone confirmation. SECURITY DEFINER + BYPASSRLS owner so the system pool can read across orgs under FORCE RLS.';

-- FUNCTION authz.list_retired_codex_auth_config_rows() (owner authz_lookup_definer)
CREATE FUNCTION authz.list_retired_codex_auth_config_rows() RETURNS TABLE(scope_kind text, org_id uuid, workspace_id uuid, user_id uuid, settings_json jsonb, secrets_enc bytea, secret_keys text[])
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'pg_catalog'
    AS $$
    SELECT 'org'::TEXT, c.org_id, NULL::UUID, NULL::UUID,
           c.settings_json, c.secrets_enc, c.secret_keys
    FROM public.org_configs c
    WHERE c.secrets_enc IS NOT NULL
       OR EXISTS (
              SELECT 1 FROM unnest(c.secret_keys) AS keys(secret_key)
              WHERE btrim(secret_key) = 'CODEX_AUTH_JSON'
          )
       OR EXISTS (
              SELECT 1 FROM jsonb_object_keys(
                  CASE WHEN jsonb_typeof(c.settings_json) = 'object'
                       THEN c.settings_json ELSE '{}'::jsonb END
              ) AS keys(setting_key)
              WHERE btrim(setting_key) = 'CODEX_AUTH_JSON'
          )
    UNION ALL
    SELECT 'workspace'::TEXT, NULL::UUID, c.workspace_id, NULL::UUID,
           c.settings_json, c.secrets_enc, c.secret_keys
    FROM public.workspace_configs c
    WHERE c.secrets_enc IS NOT NULL
       OR EXISTS (
              SELECT 1 FROM unnest(c.secret_keys) AS keys(secret_key)
              WHERE btrim(secret_key) = 'CODEX_AUTH_JSON'
          )
       OR EXISTS (
              SELECT 1 FROM jsonb_object_keys(
                  CASE WHEN jsonb_typeof(c.settings_json) = 'object'
                       THEN c.settings_json ELSE '{}'::jsonb END
              ) AS keys(setting_key)
              WHERE btrim(setting_key) = 'CODEX_AUTH_JSON'
          )
    UNION ALL
    SELECT 'user'::TEXT, c.org_id, NULL::UUID, c.user_id,
           c.settings_json, c.secrets_enc, c.secret_keys
    FROM public.user_configs c
    WHERE c.secrets_enc IS NOT NULL
       OR EXISTS (
              SELECT 1 FROM unnest(c.secret_keys) AS keys(secret_key)
              WHERE btrim(secret_key) = 'CODEX_AUTH_JSON'
          )
       OR EXISTS (
              SELECT 1 FROM jsonb_object_keys(
                  CASE WHEN jsonb_typeof(c.settings_json) = 'object'
                       THEN c.settings_json ELSE '{}'::jsonb END
              ) AS keys(setting_key)
              WHERE btrim(setting_key) = 'CODEX_AUTH_JSON'
          )
$$;

ALTER FUNCTION authz.list_retired_codex_auth_config_rows() OWNER TO authz_lookup_definer;

-- FUNCTION authz.list_worker_host_regions() (owner authz_lookup_definer)
CREATE FUNCTION authz.list_worker_host_regions() RETURNS TABLE(region text)
    LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    SELECT DISTINCT wh.region FROM public.worker_hosts wh
$$;

ALTER FUNCTION authz.list_worker_host_regions() OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION list_worker_host_regions()
COMMENT ON FUNCTION authz.list_worker_host_regions() IS 'Distinct regions ever recorded on worker_hosts rows ([D65] Pass 3): the orphan sweep lists instances in these regions plus the [D66] fallback basket. SECURITY DEFINER + BYPASSRLS owner for cross-org read under FORCE RLS.';

-- FUNCTION authz.lookup_api_key_by_hash(text)
CREATE FUNCTION authz.lookup_api_key_by_hash(p_key_hash text) RETURNS TABLE(id uuid, org_id uuid, user_id uuid, scopes text[], expires_at timestamp with time zone, revoked_at timestamp with time zone)
    LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    SELECT
        ak.id,
        ak.org_id,
        ak.user_id,
        ak.scopes,
        ak.expires_at,
        ak.revoked_at
    FROM public.api_keys ak
    WHERE ak.key_hash = p_key_hash
$$;

-- COMMENT authz.FUNCTION lookup_api_key_by_hash(p_key_hash text)
COMMENT ON FUNCTION authz.lookup_api_key_by_hash(p_key_hash text) IS 'Looks up an API key by SHA-256 hash for auth middleware before tenant context exists.';

-- FUNCTION authz.lookup_worker_host_for_auth(uuid) (owner authz_lookup_definer)
CREATE FUNCTION authz.lookup_worker_host_for_auth(p_host_id uuid) RETURNS TABLE(org_id uuid, status text)
    LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    SELECT candidate.org_id, candidate.status
    FROM (
        -- Preserve cloud-managed host precedence if an external UUID were ever
        -- to collide with a worker_hosts primary key.
        SELECT
            0 AS registry_priority,
            TRUE AS live,
            NULL::timestamptz AS last_heartbeat,
            wh.org_id,
            wh.status
        FROM public.worker_hosts wh
        WHERE wh.host_id = p_host_id

        UNION ALL

        SELECT
            1 AS registry_priority,
            (w.status <> 'terminated') AS live,
            w.last_heartbeat,
            w.org_id,
            w.status
        FROM public.workers w
        WHERE w.external_host_id = p_host_id
          AND w.org_id IS NOT NULL
    ) AS candidate
    ORDER BY candidate.registry_priority,
             candidate.live DESC,
             candidate.last_heartbeat DESC NULLS LAST
    LIMIT 1
$$;

ALTER FUNCTION authz.lookup_worker_host_for_auth(p_host_id uuid) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION lookup_worker_host_for_auth(p_host_id uuid)
COMMENT ON FUNCTION authz.lookup_worker_host_for_auth(p_host_id uuid) IS 'Pre-org-binding DB-liveness lookup for cloud worker_hosts and externally managed self-hosted supervisors represented by workers.external_host_id. Owned by authz_lookup_definer (BYPASSRLS); g5_app receives EXECUTE only.';

-- FUNCTION authz.lookup_worker_token_by_hash(text)
CREATE FUNCTION authz.lookup_worker_token_by_hash(p_key_hash text) RETURNS TABLE(id uuid, org_id uuid, scope text, workspace_id uuid, user_id uuid, expires_at timestamp with time zone, revoked_at timestamp with time zone)
    LANGUAGE sql STABLE SECURITY DEFINER PARALLEL RESTRICTED
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    SELECT
        wt.id,
        wt.org_id,
        wt.scope,
        wt.workspace_id,
        wt.user_id,
        wt.expires_at,
        wt.revoked_at
    FROM public.worker_tokens wt
    LEFT JOIN public.workspaces w ON w.id = wt.workspace_id
    WHERE wt.key_hash = p_key_hash
      AND (wt.workspace_id IS NULL OR w.deleted_at IS NULL)
$$;

-- COMMENT authz.FUNCTION lookup_worker_token_by_hash(p_key_hash text)
COMMENT ON FUNCTION authz.lookup_worker_token_by_hash(p_key_hash text) IS 'Looks up a worker registration token by SHA-256 hash for bootstrap auth. Runs as SECURITY DEFINER so runtime system users do not need global RLS visibility.';

-- FUNCTION authz.mark_worker_host_cloud_gone(uuid, text) (owner authz_lookup_definer)
CREATE FUNCTION authz.mark_worker_host_cloud_gone(p_host_id uuid, p_expected_cloud_instance_id text) RETURNS timestamp with time zone
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    UPDATE public.worker_hosts
    SET cloud_gone_since = COALESCE(cloud_gone_since, now())
    WHERE host_id = p_host_id
      AND status = 'provisioning'
      AND cloud_instance_id = p_expected_cloud_instance_id
    RETURNING cloud_gone_since
$$;

ALTER FUNCTION authz.mark_worker_host_cloud_gone(p_host_id uuid, p_expected_cloud_instance_id text) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION mark_worker_host_cloud_gone(p_host_id uuid, p_expected_cloud_instance_id text)
COMMENT ON FUNCTION authz.mark_worker_host_cloud_gone(p_host_id uuid, p_expected_cloud_instance_id text) IS 'Wall-clock gone stamp ([D61]): records the start of the confirmed-gone window for the EXPECTED linkage only (CAS — a repointed row is not re-stamped under a stale expectation). COALESCE keeps the earliest observation. SECURITY DEFINER + BYPASSRLS owner.';

-- FUNCTION authz.prepare_prior_generation_sticky_runtime_retirement(text) (owner authz_lookup_definer)
CREATE FUNCTION authz.prepare_prior_generation_sticky_runtime_retirement(p_worker_id text) RETURNS TABLE(prepared_job_ids uuid[], prepared_task_ids uuid[])
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public', 'pg_catalog'
    AS $$
DECLARE
    v_org_id UUID;
    v_job_id UUID;
    v_task_id UUID;
    v_candidate_job_ids UUID[] := '{}'::UUID[];
    v_candidate_task_ids UUID[] := '{}'::UUID[];
    v_candidate_task_job_ids UUID[] := '{}'::UUID[];
    v_rechecked_job_ids UUID[] := '{}'::UUID[];
    v_rechecked_task_ids UUID[] := '{}'::UUID[];
    v_rechecked_task_job_ids UUID[] := '{}'::UUID[];
BEGIN
    SELECT worker.org_id
      INTO v_org_id
      FROM public.workers worker
     WHERE worker.id = p_worker_id
       AND worker.org_id IS NOT NULL;

    IF NOT FOUND THEN
        RETURN QUERY SELECT '{}'::UUID[], '{}'::UUID[];
        RETURN;
    END IF;

    IF EXISTS (
        SELECT 1
        FROM public.chat_sticky_runtimes csr
        LEFT JOIN public.tasks task
          ON task.id = csr.task_id
         AND task.org_id = csr.org_id
        LEFT JOIN public.jobs job
          ON job.id = task.job_id
         AND job.org_id = task.org_id
        WHERE csr.worker_id = p_worker_id
          AND csr.teardown_reconciliation_kind IS NULL
          AND (
              (csr.status IN ('active', 'reaping') AND csr.teardown_operation_id IS NULL)
              OR (
                  csr.status = 'reaping'
                  AND csr.teardown_operation_id IS NOT NULL
                  AND csr.teardown_workspace_released_at IS NULL
              )
          )
          AND (
              csr.org_id IS DISTINCT FROM v_org_id
              OR task.id IS NULL
              OR job.id IS NULL
          )
    ) THEN
        RAISE EXCEPTION
            'registration retirement candidate has no exact worker-org task/job mapping'
            USING ERRCODE = '40001';
    END IF;

    WITH candidates AS (
        SELECT DISTINCT task.id AS task_id, task.job_id
        FROM public.chat_sticky_runtimes csr
        JOIN public.tasks task
          ON task.id = csr.task_id
         AND task.org_id = csr.org_id
        JOIN public.jobs job
          ON job.id = task.job_id
         AND job.org_id = task.org_id
        WHERE csr.org_id = v_org_id
          AND csr.worker_id = p_worker_id
          AND csr.teardown_reconciliation_kind IS NULL
          AND (
              (
                  csr.status IN ('active', 'reaping')
                  AND csr.teardown_operation_id IS NULL
              ) OR (
                  csr.status = 'reaping'
                  AND csr.teardown_operation_id IS NOT NULL
                  AND csr.teardown_workspace_released_at IS NULL
              )
          )
    )
    SELECT COALESCE(array_agg(DISTINCT job_id ORDER BY job_id), '{}'::UUID[]),
           COALESCE(array_agg(task_id ORDER BY task_id), '{}'::UUID[]),
           COALESCE(array_agg(job_id ORDER BY task_id), '{}'::UUID[])
      INTO v_candidate_job_ids,
           v_candidate_task_ids,
           v_candidate_task_job_ids
      FROM candidates;

    FOREACH v_job_id IN ARRAY v_candidate_job_ids
    LOOP
        PERFORM job.id
          FROM public.jobs job
         WHERE job.id = v_job_id
           AND job.org_id = v_org_id
         FOR UPDATE OF job;
    END LOOP;

    FOREACH v_task_id IN ARRAY v_candidate_task_ids
    LOOP
        PERFORM task.id
        FROM public.tasks task
         WHERE task.id = v_task_id
           AND task.org_id = v_org_id
           AND task.job_id = ANY(v_candidate_job_ids)
         FOR UPDATE OF task;
    END LOOP;

    WITH candidates AS (
        SELECT DISTINCT task.id AS task_id, task.job_id
        FROM public.chat_sticky_runtimes csr
        JOIN public.tasks task
          ON task.id = csr.task_id
         AND task.org_id = csr.org_id
        JOIN public.jobs job
          ON job.id = task.job_id
         AND job.org_id = task.org_id
        WHERE csr.org_id = v_org_id
          AND csr.worker_id = p_worker_id
          AND csr.teardown_reconciliation_kind IS NULL
          AND (
              (
                  csr.status IN ('active', 'reaping')
                  AND csr.teardown_operation_id IS NULL
              ) OR (
                  csr.status = 'reaping'
                  AND csr.teardown_operation_id IS NOT NULL
                  AND csr.teardown_workspace_released_at IS NULL
              )
          )
    )
    SELECT COALESCE(array_agg(DISTINCT job_id ORDER BY job_id), '{}'::UUID[]),
           COALESCE(array_agg(task_id ORDER BY task_id), '{}'::UUID[]),
           COALESCE(array_agg(job_id ORDER BY task_id), '{}'::UUID[])
      INTO v_rechecked_job_ids,
           v_rechecked_task_ids,
           v_rechecked_task_job_ids
      FROM candidates;

    IF v_rechecked_job_ids IS DISTINCT FROM v_candidate_job_ids
       OR v_rechecked_task_ids IS DISTINCT FROM v_candidate_task_ids
       OR v_rechecked_task_job_ids IS DISTINCT FROM v_candidate_task_job_ids
    THEN
        RAISE EXCEPTION
            'registration retirement candidate mapping changed during parent-job/task preparation'
            USING ERRCODE = '40001';
    END IF;

    RETURN QUERY SELECT v_candidate_job_ids, v_candidate_task_ids;
END;
$$;

ALTER FUNCTION authz.prepare_prior_generation_sticky_runtime_retirement(p_worker_id text) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION prepare_prior_generation_sticky_runtime_retirement(p_worker_id text)
COMMENT ON FUNCTION authz.prepare_prior_generation_sticky_runtime_retirement(p_worker_id text) IS 'Discovers prior-generation registration-retirement candidates, locks sorted parent jobs before sorted tasks, revalidates their exact mapping, and returns both canonical cuts before worker mutation (task_service D56, issue #2226).';

-- FUNCTION authz.repoint_worker_host_cloud_instance(uuid, text, text) (owner authz_lookup_definer)
CREATE FUNCTION authz.repoint_worker_host_cloud_instance(p_host_id uuid, p_expected_cloud_instance_id text, p_new_cloud_instance_id text) RETURNS uuid
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    UPDATE public.worker_hosts
    SET cloud_instance_id = p_new_cloud_instance_id,
        cloud_gone_since = NULL
    WHERE host_id = p_host_id
      AND status = 'provisioning'
      AND cloud_instance_id = p_expected_cloud_instance_id
      AND cloud_instance_id IS DISTINCT FROM p_new_cloud_instance_id
    RETURNING host_id
$$;

ALTER FUNCTION authz.repoint_worker_host_cloud_instance(p_host_id uuid, p_expected_cloud_instance_id text, p_new_cloud_instance_id text) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION repoint_worker_host_cloud_instance(p_host_id uuid, p_expected_cloud_instance_id text, p_new_cloud_instance_id text)
COMMENT ON FUNCTION authz.repoint_worker_host_cloud_instance(p_host_id uuid, p_expected_cloud_instance_id text, p_new_cloud_instance_id text) IS 'Repoints the linkage to the zone the instance was actually found in ([D59] fallback moved it; [D61]). CAS-guarded on status + the expected old linkage; clears cloud_gone_since. SECURITY DEFINER + BYPASSRLS owner.';

-- FUNCTION authz.reset_worker_host_cloud_gone(uuid, text) (owner authz_lookup_definer)
CREATE FUNCTION authz.reset_worker_host_cloud_gone(p_host_id uuid, p_expected_cloud_instance_id text) RETURNS uuid
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    UPDATE public.worker_hosts
    SET cloud_gone_since = NULL
    WHERE host_id = p_host_id
      AND status = 'provisioning'
      AND cloud_instance_id = p_expected_cloud_instance_id
      AND cloud_gone_since IS NOT NULL
    RETURNING host_id
$$;

ALTER FUNCTION authz.reset_worker_host_cloud_gone(p_host_id uuid, p_expected_cloud_instance_id text) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION reset_worker_host_cloud_gone(p_host_id uuid, p_expected_cloud_instance_id text)
COMMENT ON FUNCTION authz.reset_worker_host_cloud_gone(p_host_id uuid, p_expected_cloud_instance_id text) IS 'Clears the confirmed-gone window after an alive observation ([D61]), CAS-guarded on the expected linkage. SECURITY DEFINER + BYPASSRLS owner.';

-- FUNCTION authz.reset_worker_host_orphan_check_miss(uuid) (owner authz_lookup_definer)
CREATE FUNCTION authz.reset_worker_host_orphan_check_miss(p_host_id uuid) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
DECLARE
    v_misses INT;
BEGIN
    UPDATE public.worker_hosts
    SET orphan_check_misses = 0
    WHERE host_id = p_host_id
      AND status = 'provisioning'
      AND cloud_instance_id IS NULL
      AND provision_error_code IS NULL
      -- Only emit a write when there's something to reset. Avoids dead-row
      -- churn when the transient observation came in BEFORE any miss was
      -- accumulated (the common case for a Stopping-on-first-tick scenario).
      AND orphan_check_misses > 0
    RETURNING orphan_check_misses INTO v_misses;
    RETURN v_misses;
END;
$$;

ALTER FUNCTION authz.reset_worker_host_orphan_check_miss(p_host_id uuid) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION reset_worker_host_orphan_check_miss(p_host_id uuid)
COMMENT ON FUNCTION authz.reset_worker_host_orphan_check_miss(p_host_id uuid) IS 'Reconciler orphan-pass miss-counter RESET (round-5 follow-up to round-4 subagent + codex-connector P2). Called from the transient-cloud-state arm when get_instance_by_name returns Stopping/Unknown — a live-but-transient observation must break the missing streak so ORPHAN_TERMINATE_MISS_THRESHOLD continues to mean "consecutive missing observations" per [D39]. Same orphan-guard predicate as siblings; returns NULL when the row no longer qualifies, or returns 0 only if it actually reset a non-zero counter (skip-when-zero avoids dead-row churn).';

-- FUNCTION authz.retire_prelocked_prior_generation_sticky_runtimes(text, bigint, uuid[], uuid[]) (owner authz_lookup_definer)
CREATE FUNCTION authz.retire_prelocked_prior_generation_sticky_runtimes(p_worker_id text, p_current_generation bigint, p_prepared_job_ids uuid[], p_prepared_task_ids uuid[]) RETURNS TABLE(retired_count bigint, recovering_task_id uuid, recovering_job_id uuid, recovering_workspace_id uuid)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public', 'pg_catalog'
    AS $$
DECLARE
    v_canonical_prepared_job_ids UUID[];
    v_canonical_prepared_task_ids UUID[];
    v_prepared_mapping_job_ids UUID[] := '{}'::UUID[];
    v_prepared_mapping_task_ids UUID[] := '{}'::UUID[];
    v_fresh_job_ids UUID[] := '{}'::UUID[];
    v_fresh_task_ids UUID[] := '{}'::UUID[];
    v_final_job_ids UUID[] := '{}'::UUID[];
    v_final_task_ids UUID[] := '{}'::UUID[];
    v_final_runtime_ids UUID[] := '{}'::UUID[];
    v_final_runtime_task_ids UUID[] := '{}'::UUID[];
    v_locked_runtime_ids UUID[] := '{}'::UUID[];
    v_locked_runtime_task_ids UUID[] := '{}'::UUID[];
    v_fenced_task_ids UUID[] := '{}'::UUID[];
    v_fenced_runtime_ids UUID[] := '{}'::UUID[];
    v_fenced_runtime_task_ids UUID[] := '{}'::UUID[];
    v_worker_org_id UUID;
    v_revalidated_worker_org_id UUID;
    v_worker_generation BIGINT;
    v_worker_revalidated BOOLEAN := false;
    v_retired_count BIGINT := 0;
    v_task_id UUID;
    v_reject BOOLEAN := false;
BEGIN
    -- Both prepare surfaces return non-NULL, sorted, duplicate-free cuts. A
    -- malformed direct call fails closed before acquiring any advisory lock.
    SELECT COALESCE(array_agg(job_id ORDER BY job_id), '{}'::UUID[])
      INTO v_canonical_prepared_job_ids
      FROM (
          SELECT DISTINCT prepared.job_id
          FROM unnest(COALESCE(p_prepared_job_ids, '{}'::UUID[]))
               AS prepared(job_id)
          WHERE prepared.job_id IS NOT NULL
      ) canonical;
    SELECT COALESCE(array_agg(task_id ORDER BY task_id), '{}'::UUID[])
      INTO v_canonical_prepared_task_ids
      FROM (
          SELECT DISTINCT prepared.task_id
          FROM unnest(COALESCE(p_prepared_task_ids, '{}'::UUID[]))
               AS prepared(task_id)
          WHERE prepared.task_id IS NOT NULL
      ) canonical;

    IF p_prepared_job_ids IS NULL
       OR p_prepared_task_ids IS NULL
       OR p_prepared_job_ids IS DISTINCT FROM v_canonical_prepared_job_ids
       OR p_prepared_task_ids IS DISTINCT FROM v_canonical_prepared_task_ids
    THEN
        v_reject := true;
    END IF;

    SELECT worker.org_id, worker.worker_session_generation
      INTO v_worker_org_id, v_worker_generation
      FROM public.workers worker
     WHERE worker.id = p_worker_id
       AND worker.org_id IS NOT NULL;

    IF NOT FOUND
       OR p_current_generation <= 0
       OR v_worker_generation IS DISTINCT FROM p_current_generation
    THEN
        v_reject := true;
    END IF;

    SELECT v_reject OR EXISTS (
        SELECT 1
        FROM public.chat_sticky_runtimes csr
        LEFT JOIN public.tasks task
          ON task.id = csr.task_id
         AND task.org_id = csr.org_id
        LEFT JOIN public.jobs job
          ON job.id = task.job_id
         AND job.org_id = task.org_id
        WHERE csr.worker_id = p_worker_id
          AND csr.worker_session_generation IS DISTINCT FROM p_current_generation
          AND csr.teardown_reconciliation_kind IS NULL
          AND (
              (csr.status IN ('active', 'reaping') AND csr.teardown_operation_id IS NULL)
              OR (
                  csr.status = 'reaping'
                  AND csr.teardown_operation_id IS NOT NULL
                  AND csr.teardown_workspace_released_at IS NULL
              )
          )
          AND (
              csr.org_id IS DISTINCT FROM v_worker_org_id
              OR task.id IS NULL
              OR job.id IS NULL
          )
    ) INTO v_reject;

    -- The supplied task cut must still be the complete exact mapping onto the
    -- supplied parent cut in the worker's organization.
    SELECT COALESCE(array_agg(DISTINCT task.job_id ORDER BY task.job_id), '{}'::UUID[]),
           COALESCE(array_agg(task.id ORDER BY task.id), '{}'::UUID[])
      INTO v_prepared_mapping_job_ids, v_prepared_mapping_task_ids
      FROM public.tasks task
      JOIN public.jobs job
        ON job.id = task.job_id
       AND job.org_id = task.org_id
     WHERE task.org_id = v_worker_org_id
       AND task.id = ANY(v_canonical_prepared_task_ids)
       AND job.id = ANY(v_canonical_prepared_job_ids);

    IF v_prepared_mapping_job_ids IS DISTINCT FROM v_canonical_prepared_job_ids
       OR v_prepared_mapping_task_ids IS DISTINCT FROM v_canonical_prepared_task_ids
    THEN
        v_reject := true;
    END IF;

    -- Recompute after the caller has locked/rotated the worker. The cut may
    -- shrink, but any job/task outside the prepared cuts proves lock-order
    -- growth and rejects the whole registration transaction.
    WITH candidates AS (
        SELECT DISTINCT task.id AS task_id, task.job_id
        FROM public.chat_sticky_runtimes csr
        JOIN public.tasks task
          ON task.id = csr.task_id
         AND task.org_id = csr.org_id
        JOIN public.jobs job
          ON job.id = task.job_id
         AND job.org_id = task.org_id
        WHERE csr.org_id = v_worker_org_id
          AND csr.worker_id = p_worker_id
          AND csr.worker_session_generation IS DISTINCT FROM p_current_generation
          AND csr.teardown_reconciliation_kind IS NULL
          AND (
              (
                  csr.status IN ('active', 'reaping')
                  AND csr.teardown_operation_id IS NULL
              ) OR (
                  csr.status = 'reaping'
                  AND csr.teardown_operation_id IS NOT NULL
                  AND csr.teardown_workspace_released_at IS NULL
              )
          )
    )
    SELECT COALESCE(array_agg(DISTINCT job_id ORDER BY job_id), '{}'::UUID[]),
           COALESCE(array_agg(task_id ORDER BY task_id), '{}'::UUID[])
      INTO v_fresh_job_ids, v_fresh_task_ids
      FROM candidates;

    IF v_fresh_job_ids IS DISTINCT FROM v_canonical_prepared_job_ids
       OR v_fresh_task_ids IS DISTINCT FROM v_canonical_prepared_task_ids
    THEN
        v_reject := true;
    END IF;

    -- Reject predecessor custody or task execution authority before taking
    -- task advisories. Exact-host custody and hostless grant-time root leases
    -- retain migration 247's retirement/adoption semantics.
    SELECT v_reject OR EXISTS (
        SELECT 1
        FROM public.chat_sticky_runtimes csr
        LEFT JOIN public.workers worker
          ON worker.id = csr.worker_id
         AND worker.org_id = csr.org_id
        LEFT JOIN public.tasks task
          ON task.id = csr.task_id
         AND task.org_id = csr.org_id
        WHERE csr.org_id = v_worker_org_id
          AND csr.task_id = ANY(v_fresh_task_ids)
          AND csr.worker_id = p_worker_id
          AND csr.worker_session_generation IS DISTINCT FROM p_current_generation
          AND csr.teardown_reconciliation_kind IS NULL
          AND (
              (
                  csr.status IN ('active', 'reaping')
                  AND csr.teardown_operation_id IS NULL
              ) OR (
                  csr.status = 'reaping'
                  AND csr.teardown_operation_id IS NOT NULL
                  AND csr.teardown_workspace_released_at IS NULL
              )
          )
          AND (
              task.id IS NULL
              OR task.status IN ('claimed', 'running')
              OR task.worker_id IS NOT NULL
              OR task.assigned_at IS NOT NULL
              OR task.lease_expires_at IS NOT NULL
              OR task.active_execution_attempt_id IS NOT NULL
              OR EXISTS (
                  SELECT 1
                  FROM public.task_execution_attempts attempt
                  WHERE attempt.task_id = csr.task_id
                    AND attempt.org_id = csr.org_id
                    AND attempt.ended_at IS NULL
              )
              OR (
                  csr.host_id IS NULL
                  AND NOT csr.worker_workspace_root_lease_v1
              )
              OR (
                  csr.host_id IS NOT NULL
                  AND worker.host_id IS DISTINCT FROM csr.host_id
                  AND worker.external_host_id IS DISTINCT FROM csr.host_id
              )
          )
    ) INTO v_reject;

    IF v_reject THEN
        RETURN QUERY SELECT -1::BIGINT, NULL::UUID, NULL::UUID, NULL::UUID;
        RETURN;
    END IF;

    FOREACH v_task_id IN ARRAY v_canonical_prepared_task_ids
    LOOP
        PERFORM pg_advisory_xact_lock(
            hashtextextended(
                'g5:sticky-task-claim-transition:' || v_task_id::text,
                0
            )
        );
    END LOOP;

    -- Revalidate worker authority, both cuts, and execution authority after the
    -- advisories. Growth, disappearance, remap, or org change rejects the
    -- complete registration transaction.
    WITH candidates AS (
        SELECT DISTINCT csr.id AS runtime_id, task.id AS task_id, task.job_id
        FROM public.chat_sticky_runtimes csr
        JOIN public.tasks task
          ON task.id = csr.task_id
         AND task.org_id = csr.org_id
        JOIN public.jobs job
          ON job.id = task.job_id
         AND job.org_id = task.org_id
        WHERE csr.org_id = v_worker_org_id
          AND csr.worker_id = p_worker_id
          AND csr.worker_session_generation IS DISTINCT FROM p_current_generation
          AND csr.teardown_reconciliation_kind IS NULL
          AND (
              (
                  csr.status IN ('active', 'reaping')
                  AND csr.teardown_operation_id IS NULL
              ) OR (
                  csr.status = 'reaping'
                  AND csr.teardown_operation_id IS NOT NULL
                  AND csr.teardown_workspace_released_at IS NULL
              )
          )
    )
    SELECT COALESCE(array_agg(DISTINCT job_id ORDER BY job_id), '{}'::UUID[]),
           COALESCE(array_agg(DISTINCT task_id ORDER BY task_id), '{}'::UUID[]),
           COALESCE(array_agg(runtime_id ORDER BY runtime_id), '{}'::UUID[]),
           COALESCE(array_agg(task_id ORDER BY runtime_id), '{}'::UUID[])
      INTO v_final_job_ids,
           v_final_task_ids,
           v_final_runtime_ids,
           v_final_runtime_task_ids
      FROM candidates;

    SELECT worker.org_id, worker.worker_session_generation
      INTO v_revalidated_worker_org_id, v_worker_generation
      FROM public.workers worker
     WHERE worker.id = p_worker_id
       AND worker.org_id = v_worker_org_id;
    v_worker_revalidated := FOUND;

    SELECT COALESCE(array_agg(DISTINCT task.job_id ORDER BY task.job_id), '{}'::UUID[]),
           COALESCE(array_agg(task.id ORDER BY task.id), '{}'::UUID[])
      INTO v_prepared_mapping_job_ids, v_prepared_mapping_task_ids
      FROM public.tasks task
      JOIN public.jobs job
        ON job.id = task.job_id
       AND job.org_id = task.org_id
     WHERE task.org_id = v_worker_org_id
       AND task.id = ANY(v_canonical_prepared_task_ids)
       AND job.id = ANY(v_canonical_prepared_job_ids);

    IF NOT v_worker_revalidated
       OR v_revalidated_worker_org_id IS DISTINCT FROM v_worker_org_id
       OR v_worker_generation IS DISTINCT FROM p_current_generation
       OR v_prepared_mapping_job_ids IS DISTINCT FROM v_canonical_prepared_job_ids
       OR v_prepared_mapping_task_ids IS DISTINCT FROM v_canonical_prepared_task_ids
       OR v_final_job_ids IS DISTINCT FROM v_canonical_prepared_job_ids
       OR v_final_task_ids IS DISTINCT FROM v_canonical_prepared_task_ids
       OR EXISTS (
           SELECT 1
           FROM public.chat_sticky_runtimes csr
           LEFT JOIN public.tasks task
             ON task.id = csr.task_id
            AND task.org_id = csr.org_id
           LEFT JOIN public.jobs job
             ON job.id = task.job_id
            AND job.org_id = task.org_id
           WHERE csr.worker_id = p_worker_id
             AND csr.worker_session_generation IS DISTINCT FROM p_current_generation
             AND csr.teardown_reconciliation_kind IS NULL
             AND (
                 (csr.status IN ('active', 'reaping') AND csr.teardown_operation_id IS NULL)
                 OR (
                     csr.status = 'reaping'
                     AND csr.teardown_operation_id IS NOT NULL
                     AND csr.teardown_workspace_released_at IS NULL
                 )
             )
             AND (
                 csr.org_id IS DISTINCT FROM v_worker_org_id
                 OR task.id IS NULL
                 OR job.id IS NULL
             )
       )
       OR EXISTS (
           SELECT 1
           FROM public.chat_sticky_runtimes csr
           LEFT JOIN public.workers worker
             ON worker.id = csr.worker_id
            AND worker.org_id = csr.org_id
           LEFT JOIN public.tasks task
             ON task.id = csr.task_id
            AND task.org_id = csr.org_id
           WHERE csr.org_id = v_worker_org_id
             AND csr.task_id = ANY(v_final_task_ids)
             AND csr.worker_id = p_worker_id
             AND csr.worker_session_generation IS DISTINCT FROM p_current_generation
             AND csr.teardown_reconciliation_kind IS NULL
             AND (
                 (
                     csr.status IN ('active', 'reaping')
                     AND csr.teardown_operation_id IS NULL
                 ) OR (
                     csr.status = 'reaping'
                     AND csr.teardown_operation_id IS NOT NULL
                     AND csr.teardown_workspace_released_at IS NULL
                 )
             )
             AND (
                 task.id IS NULL
                 OR task.status IN ('claimed', 'running')
                 OR task.worker_id IS NOT NULL
                 OR task.assigned_at IS NOT NULL
                 OR task.lease_expires_at IS NOT NULL
                 OR task.active_execution_attempt_id IS NOT NULL
                 OR EXISTS (
                     SELECT 1
                     FROM public.task_execution_attempts attempt
                     WHERE attempt.task_id = csr.task_id
                       AND attempt.org_id = csr.org_id
                       AND attempt.ended_at IS NULL
                 )
                 OR (
                     csr.host_id IS NULL
                     AND NOT csr.worker_workspace_root_lease_v1
                 )
                 OR (
                     csr.host_id IS NOT NULL
                     AND worker.host_id IS DISTINCT FROM csr.host_id
                     AND worker.external_host_id IS DISTINCT FROM csr.host_id
                 )
             )
       )
    THEN
        RETURN QUERY SELECT -1::BIGINT, NULL::UUID, NULL::UUID, NULL::UUID;
        RETURN;
    END IF;

    -- Runtime rows are last in the complete parent-job → task → worker →
    -- advisory → runtime cut and are acquired in canonical UUID order.
    PERFORM csr.id
      FROM public.chat_sticky_runtimes csr
     WHERE csr.org_id = v_worker_org_id
       AND csr.id = ANY(v_final_runtime_ids)
       AND csr.worker_id = p_worker_id
       AND csr.worker_session_generation IS DISTINCT FROM p_current_generation
       AND csr.teardown_reconciliation_kind IS NULL
       AND (
           (
               csr.status IN ('active', 'reaping')
               AND csr.teardown_operation_id IS NULL
           ) OR (
               csr.status = 'reaping'
               AND csr.teardown_operation_id IS NOT NULL
               AND csr.teardown_workspace_released_at IS NULL
           )
       )
     ORDER BY csr.id
     FOR UPDATE OF csr;

    -- A candidate may disappear or a legacy writer may create another one
    -- while this transaction waits for its captured runtime rows. Recompute
    -- the complete worker candidate identity without an id filter and reject
    -- shrink, growth, remap, tenant drift, or authority loss before writes.
    SELECT COALESCE(array_agg(csr.id ORDER BY csr.id), '{}'::UUID[]),
           COALESCE(array_agg(csr.task_id ORDER BY csr.id), '{}'::UUID[])
      INTO v_locked_runtime_ids, v_locked_runtime_task_ids
      FROM public.chat_sticky_runtimes csr
     WHERE csr.worker_id = p_worker_id
       AND csr.worker_session_generation IS DISTINCT FROM p_current_generation
       AND csr.teardown_reconciliation_kind IS NULL
       AND (
           (
               csr.status IN ('active', 'reaping')
               AND csr.teardown_operation_id IS NULL
           ) OR (
               csr.status = 'reaping'
               AND csr.teardown_operation_id IS NOT NULL
               AND csr.teardown_workspace_released_at IS NULL
           )
       );

    IF v_locked_runtime_ids IS DISTINCT FROM v_final_runtime_ids
       OR v_locked_runtime_task_ids IS DISTINCT FROM v_final_runtime_task_ids
       OR EXISTS (
           SELECT 1
           FROM public.chat_sticky_runtimes csr
           LEFT JOIN public.workers worker
             ON worker.id = csr.worker_id
            AND worker.org_id = csr.org_id
           LEFT JOIN public.tasks task
             ON task.id = csr.task_id
            AND task.org_id = csr.org_id
           LEFT JOIN public.jobs job
             ON job.id = task.job_id
            AND job.org_id = task.org_id
           WHERE csr.worker_id = p_worker_id
             AND csr.worker_session_generation IS DISTINCT FROM p_current_generation
             AND csr.teardown_reconciliation_kind IS NULL
             AND (
                 (
                     csr.status IN ('active', 'reaping')
                     AND csr.teardown_operation_id IS NULL
                 ) OR (
                     csr.status = 'reaping'
                     AND csr.teardown_operation_id IS NOT NULL
                     AND csr.teardown_workspace_released_at IS NULL
                 )
             )
             AND (
                 csr.org_id IS DISTINCT FROM v_worker_org_id
                 OR task.id IS NULL
                 OR job.id IS NULL
                 OR (
                     csr.host_id IS NULL
                     AND NOT csr.worker_workspace_root_lease_v1
                 )
                 OR (
                     csr.host_id IS NOT NULL
                     AND worker.host_id IS DISTINCT FROM csr.host_id
                     AND worker.external_host_id IS DISTINCT FROM csr.host_id
                 )
             )
       )
    THEN
        RETURN QUERY SELECT -1::BIGINT, NULL::UUID, NULL::UUID, NULL::UUID;
        RETURN;
    END IF;

    WITH adopted AS (
        UPDATE public.chat_sticky_runtimes csr
        SET updated_at = now(),
            teardown_reconciliation_kind = 'registration_retirement',
            teardown_retired_worker_session_generation = COALESCE(
                NULLIF(csr.worker_session_generation, 0),
                GREATEST(p_current_generation - 1, 1)
            ),
            teardown_reconciliation_receipt = NULL
        WHERE csr.org_id = v_worker_org_id
          AND csr.id = ANY(v_final_runtime_ids)
          AND csr.worker_id = p_worker_id
          AND csr.host_id IS NULL
          AND csr.status = 'reaping'
          AND csr.worker_session_generation IS DISTINCT FROM p_current_generation
          AND csr.worker_workspace_root_lease_v1
          AND csr.teardown_operation_id IS NOT NULL
          AND csr.teardown_workspace_released_at IS NULL
          AND csr.teardown_reconciliation_kind IS NULL
        RETURNING csr.id AS runtime_id, csr.task_id
    ), retired AS (
        UPDATE public.chat_sticky_runtimes csr
        SET status = 'reaping',
            updated_at = now(),
            teardown_operation_id = gen_random_uuid(),
            teardown_generation = GREATEST(csr.teardown_generation, 1),
            teardown_requested_at = now(),
            teardown_deadline_at = now() + INTERVAL '30 seconds',
            teardown_repository_scopes = '[]'::jsonb,
            teardown_reconciliation_kind = 'registration_retirement',
            teardown_retired_worker_session_generation = COALESCE(
                NULLIF(csr.worker_session_generation, 0),
                GREATEST(p_current_generation - 1, 1)
            ),
            teardown_reconciliation_receipt = NULL
        WHERE csr.org_id = v_worker_org_id
          AND csr.id = ANY(v_final_runtime_ids)
          AND csr.worker_id = p_worker_id
          AND csr.status IN ('active', 'reaping')
          AND csr.worker_session_generation IS DISTINCT FROM p_current_generation
          AND csr.teardown_operation_id IS NULL
          AND csr.teardown_reconciliation_kind IS NULL
        RETURNING csr.id AS runtime_id, csr.task_id
    ), retained AS (
        -- An exact-host ordinary teardown operation is already durably fenced
        -- and remains owned by the current host supervisor. Registration must
        -- treat it as handled without rewriting its operation/reconciliation
        -- evidence or counting it as a newly retired runtime.
        SELECT csr.id AS runtime_id, csr.task_id
        FROM public.chat_sticky_runtimes csr
        JOIN public.workers worker
          ON worker.id = csr.worker_id
         AND worker.org_id = csr.org_id
        WHERE csr.org_id = v_worker_org_id
          AND csr.id = ANY(v_final_runtime_ids)
          AND csr.worker_id = p_worker_id
          AND csr.host_id IS NOT NULL
          AND (
              worker.host_id = csr.host_id
              OR worker.external_host_id = csr.host_id
          )
          AND csr.status = 'reaping'
          AND csr.worker_session_generation IS DISTINCT FROM p_current_generation
          AND csr.teardown_operation_id IS NOT NULL
          AND csr.teardown_workspace_released_at IS NULL
          AND csr.teardown_reconciliation_kind IS NULL
    ), fenced AS (
        SELECT adopted.runtime_id, adopted.task_id, true AS newly_retired FROM adopted
        UNION ALL
        SELECT retired.runtime_id, retired.task_id, true AS newly_retired FROM retired
        UNION ALL
        SELECT retained.runtime_id, retained.task_id, false AS newly_retired FROM retained
    )
    SELECT COUNT(*) FILTER (WHERE fenced.newly_retired)::BIGINT,
           COALESCE(array_agg(fenced.runtime_id ORDER BY fenced.runtime_id), '{}'::UUID[]),
           COALESCE(array_agg(fenced.task_id ORDER BY fenced.runtime_id), '{}'::UUID[]),
           COALESCE(array_agg(DISTINCT fenced.task_id ORDER BY fenced.task_id), '{}'::UUID[])
      INTO v_retired_count,
           v_fenced_runtime_ids,
           v_fenced_runtime_task_ids,
           v_fenced_task_ids
      FROM fenced;

    IF v_fenced_runtime_ids IS DISTINCT FROM v_final_runtime_ids
       OR v_fenced_runtime_task_ids IS DISTINCT FROM v_final_runtime_task_ids
       OR v_fenced_task_ids IS DISTINCT FROM v_final_task_ids
    THEN
        RETURN QUERY SELECT -1::BIGINT, NULL::UUID, NULL::UUID, NULL::UUID;
        RETURN;
    END IF;

    -- Only a truly scheduler-ready pending task is adopted. The validation
    -- above rejects authority, and this guarded write repeats the exact shape
    -- so no malformed task is coerced into recovery.
    RETURN QUERY
    WITH recovering AS (
        UPDATE public.tasks task
        SET status = 'recovering',
            updated_at = now()
        WHERE task.org_id = v_worker_org_id
          AND task.id = ANY(v_fenced_task_ids)
          AND task.status = 'pending'
          AND task.worker_id IS NULL
          AND task.assigned_at IS NULL
          AND task.lease_expires_at IS NULL
          AND task.active_execution_attempt_id IS NULL
          AND NOT EXISTS (
              SELECT 1
              FROM public.task_execution_attempts attempt
              WHERE attempt.task_id = task.id
                AND attempt.org_id = task.org_id
                AND attempt.ended_at IS NULL
          )
        RETURNING task.id, task.job_id
    ), projected AS (
        SELECT recovering.id AS task_id,
               recovering.job_id,
               job.workspace_id
        FROM recovering
        JOIN public.jobs job
          ON job.id = recovering.job_id
         AND job.org_id = v_worker_org_id
    )
    SELECT v_retired_count,
           projected.task_id,
           projected.job_id,
           projected.workspace_id
    FROM projected
    UNION ALL
    SELECT v_retired_count, NULL::UUID, NULL::UUID, NULL::UUID
    WHERE NOT EXISTS (SELECT 1 FROM projected)
    ORDER BY 2 NULLS LAST;
END;
$$;

ALTER FUNCTION authz.retire_prelocked_prior_generation_sticky_runtimes(p_worker_id text, p_current_generation bigint, p_prepared_job_ids uuid[], p_prepared_task_ids uuid[]) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION retire_prelocked_prior_generation_sticky_runtimes(p_worker_id text, p_current_generation bigint, p_prepared_job_ids uuid[], p_prepared_task_ids uuid[])
COMMENT ON FUNCTION authz.retire_prelocked_prior_generation_sticky_runtimes(p_worker_id text, p_current_generation bigint, p_prepared_job_ids uuid[], p_prepared_task_ids uuid[]) IS 'Under caller-held parent-job/task and exact worker-generation locks, validates both prepared cuts and worker org, takes sorted task advisories/runtime rows, retires prior-generation custody, moves eligible fenced pending tasks to recovering, and returns exact post-commit publication projection; -1 is the retry sentinel (task_service D56, issue #2226).';

-- FUNCTION authz.retire_prior_generation_sticky_runtimes(text, bigint) (owner authz_lookup_definer)
CREATE FUNCTION authz.retire_prior_generation_sticky_runtimes(p_worker_id text, p_current_generation bigint) RETURNS bigint
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'public', 'pg_catalog'
    AS $$
    SELECT CASE
        WHEN EXISTS (
            SELECT 1
            FROM public.chat_sticky_runtimes csr
            WHERE csr.worker_id = p_worker_id
              AND csr.worker_session_generation IS DISTINCT FROM p_current_generation
              AND csr.teardown_reconciliation_kind IS NULL
              AND (
                  (
                      csr.status IN ('active', 'reaping')
                      AND csr.teardown_operation_id IS NULL
                  ) OR (
                      csr.status = 'reaping'
                      AND csr.teardown_operation_id IS NOT NULL
                      AND csr.teardown_workspace_released_at IS NULL
                  )
              )
        ) THEN -1::BIGINT
        ELSE 0::BIGINT
    END;
$$;

ALTER FUNCTION authz.retire_prior_generation_sticky_runtimes(p_worker_id text, p_current_generation bigint) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION retire_prior_generation_sticky_runtimes(p_worker_id text, p_current_generation bigint)
COMMENT ON FUNCTION authz.retire_prior_generation_sticky_runtimes(p_worker_id text, p_current_generation bigint) IS 'Rolling-only worker-first probe: takes no task/advisory/runtime locks and returns -1 when any prior-generation retirement candidate exists, otherwise 0 (task_service D56, issue #2226).';

-- FUNCTION authz.schedule_worker_host_provision_retry(uuid) (owner authz_lookup_definer)
CREATE FUNCTION authz.schedule_worker_host_provision_retry(p_host_id uuid) RETURNS timestamp with time zone
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    UPDATE public.worker_hosts
    SET provision_next_retry_at =
            now() + make_interval(secs => LEAST(120.0 * power(2.0, provision_retry_count), 600.0))
    WHERE host_id = p_host_id
      AND status = 'provisioning'
      AND cloud_instance_id IS NULL
    RETURNING provision_next_retry_at
$$;

ALTER FUNCTION authz.schedule_worker_host_provision_retry(p_host_id uuid) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION schedule_worker_host_provision_retry(p_host_id uuid)
COMMENT ON FUNCTION authz.schedule_worker_host_provision_retry(p_host_id uuid) IS 'Stockout retry scheduling ([D66], #1880): bring-up stamps the next attempt with min(120s x 2^provision_retry_count, 600s) backoff after classifying zone_capacity_exhausted. Guarded to unlinked provisioning rows. SECURITY DEFINER + BYPASSRLS owner for system-pool access under FORCE RLS.';

-- FUNCTION authz.set_ctx(uuid) (owner authz_ctx_writer)
CREATE FUNCTION authz.set_ctx(p_session_id uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'authz', 'pg_catalog'
    AS $$
DECLARE
    v_user_id  UUID;
    v_org_ids  UUID[];
    v_scopes   TEXT[];
BEGIN
    -- Validate session exists and is not expired.
    SELECT user_id, org_ids, scopes
      INTO v_user_id, v_org_ids, v_scopes
      FROM authz.user_sessions
     WHERE session_id = p_session_id
       AND expires_at > now();

    IF NOT FOUND THEN
        RAISE EXCEPTION 'Invalid or expired session: %', p_session_id
            USING ERRCODE = 'P0002';  -- no_data_found
    END IF;

    -- Write or replace context for this connection PID.
    INSERT INTO authz.session_ctx (pid, org_ids, user_id, scopes, session_id, set_at)
    VALUES (pg_backend_pid(), v_org_ids, v_user_id, v_scopes, p_session_id, now())
    ON CONFLICT (pid) DO UPDATE
       SET org_ids    = EXCLUDED.org_ids,
           user_id    = EXCLUDED.user_id,
           scopes     = EXCLUDED.scopes,
           session_id = EXCLUDED.session_id,
           set_at     = EXCLUDED.set_at;
END;
$$;

ALTER FUNCTION authz.set_ctx(p_session_id uuid) OWNER TO authz_ctx_writer;

-- COMMENT authz.FUNCTION set_ctx(p_session_id uuid)
COMMENT ON FUNCTION authz.set_ctx(p_session_id uuid) IS 'Activates an authz session on the current transaction''s connection. Must be called inside BEGIN..COMMIT. Validates session TTL without touching authz.user_sessions.';

-- FUNCTION authz.set_worker_host_cloud_refs(uuid, text, text) (owner authz_lookup_definer)
CREATE FUNCTION authz.set_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text) RETURNS uuid
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    UPDATE public.worker_hosts
    SET vpc_id = p_vpc_id, cloud_instance_id = p_cloud_instance_id
    WHERE host_id = p_host_id AND status = 'provisioning'
    RETURNING host_id
$$;

ALTER FUNCTION authz.set_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION set_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text)
COMMENT ON FUNCTION authz.set_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text) IS 'PR-rev-7-9c POST cloud-ref persistence (vpc_id, cloud_instance_id). Guarded to provisioning rows. SECURITY DEFINER + BYPASSRLS owner for post-commit system-pool write under FORCE RLS.';

-- FUNCTION authz.set_worker_host_cloud_refs(uuid, text, text, text) (owner authz_lookup_definer)
CREATE FUNCTION authz.set_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text, p_boot_image_id text) RETURNS uuid
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    UPDATE public.worker_hosts
    SET vpc_id = p_vpc_id,
        cloud_instance_id = p_cloud_instance_id,
        boot_image_id = p_boot_image_id,
        provision_error_code = NULL,
        provision_next_retry_at = NULL,
        cloud_gone_since = NULL
    WHERE host_id = p_host_id AND status = 'provisioning'
    RETURNING host_id
$$;

ALTER FUNCTION authz.set_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text, p_boot_image_id text) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION set_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text, p_boot_image_id text)
COMMENT ON FUNCTION authz.set_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text, p_boot_image_id text) IS 'POST/rotation/retry cloud-ref persistence including boot_image_id ([D56]) and, since migration 228, the atomic clear of provision_error_code / provision_next_retry_at / cloud_gone_since on success ([D66]). Guarded to provisioning rows. SECURITY DEFINER + BYPASSRLS owner.';

-- FUNCTION authz.set_worker_host_orphan_check_miss(uuid) (owner authz_lookup_definer)
CREATE FUNCTION authz.set_worker_host_orphan_check_miss(p_host_id uuid) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
DECLARE
    v_misses INT;
BEGIN
    UPDATE public.worker_hosts
    SET orphan_check_misses = orphan_check_misses + 1
    WHERE host_id = p_host_id
      AND status = 'provisioning'
      AND cloud_instance_id IS NULL
      AND provision_error_code IS NULL
      -- [D60] lease fence: an in-flight bring-up is never counted.
      AND (bringup_heartbeat_at IS NULL
           OR bringup_heartbeat_at < now() - interval '90 seconds')
    RETURNING orphan_check_misses INTO v_misses;
    RETURN v_misses;
END;
$$;

ALTER FUNCTION authz.set_worker_host_orphan_check_miss(p_host_id uuid) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION set_worker_host_orphan_check_miss(p_host_id uuid)
COMMENT ON FUNCTION authz.set_worker_host_orphan_check_miss(p_host_id uuid) IS 'Reconciler orphan-pass miss-counter increment ([D39]), lease-fenced since migration 171 ([D60]): a row whose bring-up lease is fresh is in flight and is not counted. Returns NULL when the row no longer qualifies. SECURITY DEFINER + BYPASSRLS owner.';

-- FUNCTION authz.set_worker_host_provision_error_code(uuid, text) (owner authz_lookup_definer)
CREATE FUNCTION authz.set_worker_host_provision_error_code(p_host_id uuid, p_error_code text) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
DECLARE
    v_host_id UUID;
BEGIN
    -- Whitelist the codeset in the function body too — the column CHECK is the
    -- ultimate guardrail, but a typed reject here surfaces a precise error to
    -- the caller instead of a constraint-violation traceback in logs.
    IF p_error_code IS NOT NULL
       AND p_error_code NOT IN ('zone_capacity_exhausted',
                                'abandoned_bringup',
                                'never_registered',
                                'base_image_below_glibc_floor') THEN
        RAISE EXCEPTION
            'set_worker_host_provision_error_code: unknown code %', p_error_code;
    END IF;
    -- Guarded to provisioning rows: a row that already left provisioning (drain,
    -- terminate, ready) must not be retroactively stamped with a bring-up error.
    UPDATE public.worker_hosts
    SET provision_error_code = p_error_code
    WHERE host_id = p_host_id AND status = 'provisioning'
    RETURNING host_id INTO v_host_id;
    RETURN v_host_id;
END;
$$;

ALTER FUNCTION authz.set_worker_host_provision_error_code(p_host_id uuid, p_error_code text) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION set_worker_host_provision_error_code(p_host_id uuid, p_error_code text)
COMMENT ON FUNCTION authz.set_worker_host_provision_error_code(p_host_id uuid, p_error_code text) IS 'Set worker_hosts.provision_error_code from the system pool after a cloud bring-up failure. Guarded to provisioning rows. SECURITY DEFINER + BYPASSRLS owner so the post-commit write reaches the row under FORCE RLS.';

-- FUNCTION authz.terminate_abandoned_worker_host(uuid) (owner authz_lookup_definer)
CREATE FUNCTION authz.terminate_abandoned_worker_host(p_host_id uuid) RETURNS uuid
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    SELECT authz.terminate_abandoned_worker_host(p_host_id, 10)
$$;

ALTER FUNCTION authz.terminate_abandoned_worker_host(p_host_id uuid) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION terminate_abandoned_worker_host(p_host_id uuid)
COMMENT ON FUNCTION authz.terminate_abandoned_worker_host(p_host_id uuid) IS 'Rolling-deploy compatibility wrapper for pre-171 replicas: delegates to the 2-arg lease-fenced, threshold-atomic terminate with the historical threshold (10). New code uses the 2-arg form.';

-- FUNCTION authz.terminate_abandoned_worker_host(uuid, integer) (owner authz_lookup_definer)
CREATE FUNCTION authz.terminate_abandoned_worker_host(p_host_id uuid, p_miss_threshold integer) RETURNS uuid
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    UPDATE public.worker_hosts
    SET status = 'terminated',
        last_heartbeat = NULL,
        provision_error_code = 'abandoned_bringup'
    WHERE host_id = p_host_id
      AND status = 'provisioning'
      AND cloud_instance_id IS NULL
      AND provision_error_code IS NULL
      -- [D60] lease fence + atomic threshold re-check (Codex round-2
      -- blocker 1): a lease revival or a transient-observation counter reset
      -- between the caller's increment and this terminate makes it a no-op.
      AND (bringup_heartbeat_at IS NULL
           OR bringup_heartbeat_at < now() - interval '90 seconds')
      AND orphan_check_misses >= p_miss_threshold
    RETURNING host_id
$$;

ALTER FUNCTION authz.terminate_abandoned_worker_host(p_host_id uuid, p_miss_threshold integer) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION terminate_abandoned_worker_host(p_host_id uuid, p_miss_threshold integer)
COMMENT ON FUNCTION authz.terminate_abandoned_worker_host(p_host_id uuid, p_miss_threshold integer) IS 'Reconciler orphan-pass terminate after threshold confirmed-missing observations ([D39]); since migration 171 lease-fenced and threshold-atomic ([D60]). Sets status=terminated, last_heartbeat=NULL, provision_error_code=abandoned_bringup. SECURITY DEFINER + BYPASSRLS owner.';

-- FUNCTION authz.terminate_gone_worker_host(uuid, text, integer) (owner authz_lookup_definer)
CREATE FUNCTION authz.terminate_gone_worker_host(p_host_id uuid, p_expected_cloud_instance_id text, p_confirm_secs integer) RETURNS timestamp with time zone
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    UPDATE public.worker_hosts
    SET status = 'terminated',
        last_heartbeat = NULL
    WHERE host_id = p_host_id
      AND status = 'provisioning'
      AND cloud_instance_id = p_expected_cloud_instance_id
      AND cloud_gone_since IS NOT NULL
      AND cloud_gone_since < now() - make_interval(secs => p_confirm_secs)
    RETURNING cloud_gone_since
$$;

ALTER FUNCTION authz.terminate_gone_worker_host(p_host_id uuid, p_expected_cloud_instance_id text, p_confirm_secs integer) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION terminate_gone_worker_host(p_host_id uuid, p_expected_cloud_instance_id text, p_confirm_secs integer)
COMMENT ON FUNCTION authz.terminate_gone_worker_host(p_host_id uuid, p_expected_cloud_instance_id text, p_confirm_secs integer) IS 'Terminates a provisioning row whose cloud instance has been confirmed gone for at least p_confirm_secs of wall-clock time ([D61]) — atomic CAS on status + expected linkage + lapsed window, so no replica interleaving can kill a repointed/alive row. SECURITY DEFINER + BYPASSRLS owner.';

-- FUNCTION authz.touch_api_key_last_used(uuid, text)
CREATE FUNCTION authz.touch_api_key_last_used(p_id uuid, p_last_used_ip text) RETURNS boolean
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
BEGIN
    UPDATE public.api_keys
    SET
        last_used_at = now(),
        last_used_ip = CASE
            WHEN p_last_used_ip IS NULL OR btrim(p_last_used_ip) = '' THEN NULL
            ELSE p_last_used_ip::inet
        END
    WHERE id = p_id;

    RETURN FOUND;
END;
$$;

-- COMMENT authz.FUNCTION touch_api_key_last_used(p_id uuid, p_last_used_ip text)
COMMENT ON FUNCTION authz.touch_api_key_last_used(p_id uuid, p_last_used_ip text) IS 'Best-effort updater for API key last_used metadata from auth middleware.';

-- FUNCTION authz.touch_worker_host_bringup(uuid) (owner authz_lookup_definer)
CREATE FUNCTION authz.touch_worker_host_bringup(p_host_id uuid) RETURNS uuid
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
    UPDATE public.worker_hosts
    SET bringup_heartbeat_at = now()
    WHERE host_id = p_host_id
      AND status = 'provisioning'
    RETURNING host_id
$$;

ALTER FUNCTION authz.touch_worker_host_bringup(p_host_id uuid) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION touch_worker_host_bringup(p_host_id uuid)
COMMENT ON FUNCTION authz.touch_worker_host_bringup(p_host_id uuid) IS 'Bring-up liveness lease renewal ([D60]): bring_up_cloud_resources ticks this at start and every 30s. Guarded to provisioning rows. SECURITY DEFINER + BYPASSRLS owner for system-pool access under FORCE RLS.';

-- FUNCTION authz.transition_provisioning_worker_host(uuid, text) (owner authz_lookup_definer)
CREATE FUNCTION authz.transition_provisioning_worker_host(p_host_id uuid, p_to_status text) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public', 'authz', 'pg_catalog'
    AS $$
DECLARE
    v_host_id UUID;
BEGIN
    IF p_to_status NOT IN ('ready', 'terminated') THEN
        RAISE EXCEPTION
            'transition_provisioning_worker_host: illegal target status %', p_to_status;
    END IF;
    UPDATE public.worker_hosts
    SET status = p_to_status
    WHERE host_id = p_host_id AND status = 'provisioning'
    RETURNING host_id INTO v_host_id;
    RETURN v_host_id;
END;
$$;

ALTER FUNCTION authz.transition_provisioning_worker_host(p_host_id uuid, p_to_status text) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION transition_provisioning_worker_host(p_host_id uuid, p_to_status text)
COMMENT ON FUNCTION authz.transition_provisioning_worker_host(p_host_id uuid, p_to_status text) IS 'PR-rev-7-9c reconciler guarded transition provisioning -> {ready,terminated}. No-op (returns NULL) if the host is not provisioning. SECURITY DEFINER + BYPASSRLS owner for cross-org system-pool access under FORCE RLS.';

-- FUNCTION authz.worker_identity_bound_to_terminated_host(text, uuid) (owner authz_lookup_definer)
CREATE FUNCTION authz.worker_identity_bound_to_terminated_host(p_worker_id text, p_org_id uuid) RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'public', 'pg_catalog'
    AS $$
    SELECT EXISTS (
        SELECT 1
        FROM public.workers w
        JOIN public.worker_hosts wh
          ON wh.org_id = w.org_id
         AND (wh.host_id = w.host_id OR wh.host_id = w.external_host_id)
        WHERE w.id = p_worker_id
          AND w.org_id = p_org_id
          AND w.status = 'terminated'
          AND wh.status = 'terminated'
    );
$$;

ALTER FUNCTION authz.worker_identity_bound_to_terminated_host(p_worker_id text, p_org_id uuid) OWNER TO authz_lookup_definer;

-- COMMENT authz.FUNCTION worker_identity_bound_to_terminated_host(p_worker_id text, p_org_id uuid)
COMMENT ON FUNCTION authz.worker_identity_bound_to_terminated_host(p_worker_id text, p_org_id uuid) IS 'RLS-safe registration-side guard (issue #2310): true when a terminated worker identity is bound to a terminated worker host, in which case same-id revival must be refused so the watchdog''s fenced-custodian release can never race a resurrected custodian.';

-- FUNCTION public.agent_approval_artifact_v2_is_valid(jsonb, boolean)
CREATE FUNCTION public.agent_approval_artifact_v2_is_valid(value jsonb, accepted boolean) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $_$
    SELECT CASE
        WHEN jsonb_typeof(value) <> 'object' THEN false
        WHEN accepted AND NOT approval_review_jsonb_has_exact_keys_v1(
            value,
            ARRAY[
                'outputRepoId', 'path', 'sourceCommitSha', 'blobOid',
                'byteSize', 'contentSha256', 'mediaType',
                'publicationOperationId', 'publishedAt'
            ]
        ) THEN false
        WHEN NOT accepted AND NOT approval_review_jsonb_has_exact_keys_v1(
            value,
            ARRAY[
                'outputRepoId', 'path', 'jobCommitSha', 'blobOid',
                'byteSize', 'contentSha256', 'mediaType', 'generatedAt'
            ]
        ) THEN false
        ELSE jsonb_typeof(value -> 'outputRepoId') = 'string'
            AND (value ->> 'outputRepoId') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            AND jsonb_typeof(value -> 'path') = 'string'
            AND (value ->> 'path') ~* '^docs/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/reviews/agent-approval-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.md$'
            AND (
                (accepted AND jsonb_typeof(value -> 'sourceCommitSha') = 'string'
                    AND (value ->> 'sourceCommitSha') ~ '^([0-9a-f]{40}|[0-9a-f]{64})$')
                OR
                (NOT accepted AND jsonb_typeof(value -> 'jobCommitSha') = 'string'
                    AND (value ->> 'jobCommitSha') ~ '^([0-9a-f]{40}|[0-9a-f]{64})$')
            )
            AND jsonb_typeof(value -> 'blobOid') = 'string'
            AND (value ->> 'blobOid') ~ '^([0-9a-f]{40}|[0-9a-f]{64})$'
            AND jsonb_typeof(value -> 'byteSize') = 'number'
            AND (value ->> 'byteSize') ~ '^[0-9]+$'
            AND (value ->> 'byteSize')::NUMERIC BETWEEN 1 AND 2097152
            AND jsonb_typeof(value -> 'contentSha256') = 'string'
            AND (value ->> 'contentSha256') ~ '^[0-9a-f]{64}$'
            AND value ->> 'mediaType' = 'text/markdown; charset=utf-8'
            AND (
                (accepted
                    AND jsonb_typeof(value -> 'publicationOperationId') = 'string'
                    AND (value ->> 'publicationOperationId') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    AND jsonb_typeof(value -> 'publishedAt') = 'string'
                    AND btrim(value ->> 'publishedAt') <> '')
                OR
                (NOT accepted
                    AND jsonb_typeof(value -> 'generatedAt') = 'string'
                    AND btrim(value ->> 'generatedAt') <> '')
            )
    END
$_$;

-- FUNCTION public.agent_approval_run_details_is_valid(jsonb)
CREATE FUNCTION public.agent_approval_run_details_is_valid(value jsonb) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $$
    SELECT agent_approval_run_details_v1_is_valid(value)
        OR agent_approval_run_details_v2_is_valid(value)
$$;

-- FUNCTION public.agent_approval_run_details_v1_is_valid(jsonb)
CREATE FUNCTION public.agent_approval_run_details_v1_is_valid(value jsonb) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $_$
    SELECT CASE
        WHEN NOT approval_review_jsonb_has_exact_keys_v1(
            value,
            ARRAY[
                'schemaVersion', 'targetWorkspaceId', 'baseHeads', 'artifact',
                'validationDisposition', 'executionReceipt', 'attemptCount',
                'contractVersions', 'failure'
            ]
        ) THEN false
        ELSE octet_length(value::TEXT) <= 65536
            AND value -> 'schemaVersion' = '1'::JSONB
            AND jsonb_typeof(value -> 'targetWorkspaceId') = 'string'
            AND (value ->> 'targetWorkspaceId') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            AND approval_review_head_vector_v1_is_valid(value -> 'baseHeads')
            AND (
                value -> 'artifact' = 'null'::JSONB
                OR (
                    approval_review_jsonb_has_exact_keys_v1(
                        value -> 'artifact',
                        ARRAY['path', 'commitSha', 'blobOid']
                    )
                    AND jsonb_typeof(value -> 'artifact' -> 'path') = 'string'
                    AND (value -> 'artifact' ->> 'path') ~* '^docs/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/approval-review-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.md$'
                    AND jsonb_typeof(value -> 'artifact' -> 'commitSha') = 'string'
                    AND (value -> 'artifact' ->> 'commitSha') ~ '^([0-9a-f]{40}|[0-9a-f]{64})$'
                    AND jsonb_typeof(value -> 'artifact' -> 'blobOid') = 'string'
                    AND (value -> 'artifact' ->> 'blobOid') ~ '^([0-9a-f]{40}|[0-9a-f]{64})$'
                )
            )
            AND value ->> 'validationDisposition' IN ('valid', 'invalid', 'absent')
            AND (
                value -> 'executionReceipt' = 'null'::JSONB
                OR approval_review_execution_receipt_v1_is_valid(value -> 'executionReceipt')
            )
            AND jsonb_typeof(value -> 'attemptCount') = 'number'
            AND (value ->> 'attemptCount') ~ '^[0-9]+$'
            AND (value ->> 'attemptCount')::NUMERIC BETWEEN 1 AND 4294967295
            AND value -> 'contractVersions'
                = '{"reviewContext":1,"reviewPolicy":1,"output":1}'::JSONB
            AND (
                value -> 'failure' = 'null'::JSONB
                OR (
                    approval_review_jsonb_has_exact_keys_v1(
                        value -> 'failure',
                        ARRAY['code', 'message']
                    )
                    AND value -> 'failure' ->> 'code' IN (
                        'execution_failed', 'timed_out', 'cancelled',
                        'worker_lost', 'capability_unavailable', 'output_invalid',
                        'collection_failed', 'publication_failed',
                        'artifact_validation_failed'
                    )
                    AND jsonb_typeof(value -> 'failure' -> 'message') = 'string'
                    AND btrim(value -> 'failure' ->> 'message') <> ''
                    AND octet_length(value -> 'failure' ->> 'message') <= 1024
                )
            )
    END
$_$;

-- FUNCTION public.agent_approval_run_details_v2_is_valid(jsonb)
CREATE FUNCTION public.agent_approval_run_details_v2_is_valid(value jsonb) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $_$
    SELECT CASE
        WHEN NOT approval_review_jsonb_has_exact_keys_v1(
            value,
            ARRAY[
                'schemaVersion', 'agentJobId', 'targetWorkspaceId',
                'baseHeads', 'artifact', 'validationDisposition',
                'executionReceipt', 'attemptCount', 'contractVersions',
                'failure'
            ]
        ) THEN false
        ELSE value -> 'schemaVersion' = '2'::JSONB
            AND jsonb_typeof(value -> 'agentJobId') = 'string'
            AND (value ->> 'agentJobId')
                ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            AND value -> 'contractVersions' -> 'reviewContext' = '2'::JSONB
            AND agent_approval_run_details_v1_is_valid(
                jsonb_set(
                    jsonb_set(
                        value - 'agentJobId',
                        '{schemaVersion}',
                        '1'::JSONB
                    ),
                    '{contractVersions,reviewContext}',
                    '1'::JSONB,
                    false
                )
            )
            AND approval_review_head_vector_v1_is_canonical(value -> 'baseHeads')
    END
$_$;

-- FUNCTION public.agent_approval_run_details_v3_is_valid(jsonb)
CREATE FUNCTION public.agent_approval_run_details_v3_is_valid(value jsonb) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $_$
    SELECT CASE
        WHEN NOT approval_review_jsonb_has_exact_keys_v1(
            value,
            ARRAY[
                'schemaVersion', 'agentJobId', 'resolutionId',
                'targetWorkspaceId', 'targetHeads', 'assignedSourceHeads',
                'generatedArtifact', 'publishedSourceHeads', 'acceptedArtifact',
                'validationDisposition', 'executionReceipt', 'attemptCount',
                'contractVersions', 'failure'
            ]
        ) THEN false
        ELSE octet_length(value::TEXT) <= 65536
            AND value -> 'schemaVersion' = '3'::JSONB
            AND jsonb_typeof(value -> 'agentJobId') = 'string'
            AND (value ->> 'agentJobId') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            AND jsonb_typeof(value -> 'resolutionId') = 'string'
            AND (value ->> 'resolutionId') ~ '^apr_[0-9a-f]{64}$'
            AND jsonb_typeof(value -> 'targetWorkspaceId') = 'string'
            AND (value ->> 'targetWorkspaceId') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            AND approval_review_head_vector_v2_is_valid(value -> 'targetHeads')
            AND approval_review_head_vector_v2_is_valid(value -> 'assignedSourceHeads')
            AND (
                value -> 'generatedArtifact' = 'null'::JSONB
                OR agent_approval_artifact_v2_is_valid(value -> 'generatedArtifact', false)
            )
            AND (
                value -> 'publishedSourceHeads' = 'null'::JSONB
                OR approval_review_head_vector_v2_is_valid(value -> 'publishedSourceHeads')
            )
            AND (
                value -> 'acceptedArtifact' = 'null'::JSONB
                OR agent_approval_artifact_v2_is_valid(value -> 'acceptedArtifact', true)
            )
            AND value ->> 'validationDisposition' IN ('valid', 'invalid', 'absent')
            AND (
                value -> 'executionReceipt' = 'null'::JSONB
                OR approval_review_execution_receipt_v1_is_valid(value -> 'executionReceipt')
            )
            AND jsonb_typeof(value -> 'attemptCount') = 'number'
            AND (value ->> 'attemptCount') ~ '^[0-9]+$'
            AND (value ->> 'attemptCount')::NUMERIC BETWEEN 1 AND 4294967295
            AND value -> 'contractVersions' = '{
                "requestContext":3,
                "resolution":2,
                "reviewPolicy":1,
                "output":2,
                "runDetails":3
            }'::JSONB
            AND (
                value -> 'failure' = 'null'::JSONB
                OR (
                    approval_review_jsonb_has_exact_keys_v1(
                        value -> 'failure',
                        ARRAY['code', 'message']
                    )
                    AND value -> 'failure' ->> 'code' IN (
                        'execution_failed', 'timed_out', 'cancelled',
                        'worker_lost', 'capability_unavailable', 'output_invalid',
                        'collection_failed', 'publication_failed',
                        'artifact_validation_failed'
                    )
                    AND jsonb_typeof(value -> 'failure' -> 'message') = 'string'
                    AND btrim(value -> 'failure' ->> 'message') <> ''
                    AND octet_length(value -> 'failure' ->> 'message') <= 1024
                )
            )
    END
$_$;

-- FUNCTION public.agent_approval_run_details_v4_is_valid(jsonb)
CREATE FUNCTION public.agent_approval_run_details_v4_is_valid(value jsonb) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $_$
    SELECT CASE
        WHEN NOT approval_review_jsonb_has_exact_keys_v1(
            value,
            ARRAY[
                'schemaVersion', 'agentJobId', 'resolutionId',
                'targetWorkspaceId', 'targetHeads', 'assignedSourceHeads',
                'generatedArtifact', 'draftProjection',
                'validationDisposition', 'executionReceipt', 'attemptCount',
                'contractVersions', 'failure'
            ]
        ) THEN false
        ELSE octet_length(value::TEXT) <= 65536
            AND value -> 'schemaVersion' = '4'::JSONB
            AND jsonb_typeof(value -> 'agentJobId') = 'string'
            AND (value ->> 'agentJobId') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            AND jsonb_typeof(value -> 'resolutionId') = 'string'
            AND (value ->> 'resolutionId') ~ '^apr_[0-9a-f]{64}$'
            AND jsonb_typeof(value -> 'targetWorkspaceId') = 'string'
            AND (value ->> 'targetWorkspaceId') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            AND approval_review_head_vector_v2_is_valid(value -> 'targetHeads')
            AND approval_review_head_vector_v2_is_valid(value -> 'assignedSourceHeads')
            AND agent_approval_artifact_v2_is_valid(value -> 'generatedArtifact', false)
            AND approval_review_jsonb_has_exact_keys_v1(
                value -> 'draftProjection',
                ARRAY[
                    'outputDocId', 'outputRepoId', 'path', 'byteSize',
                    'contentSha256', 'projectedAt'
                ]
            )
            AND jsonb_typeof(value -> 'draftProjection' -> 'outputDocId') = 'string'
            AND (value -> 'draftProjection' ->> 'outputDocId') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            AND jsonb_typeof(value -> 'draftProjection' -> 'outputRepoId') = 'string'
            AND (value -> 'draftProjection' ->> 'outputRepoId') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            AND jsonb_typeof(value -> 'draftProjection' -> 'path') = 'string'
            AND (value -> 'draftProjection' ->> 'path') ~* '^docs/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/reviews/agent-approval-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.md$'
            AND jsonb_typeof(value -> 'draftProjection' -> 'byteSize') = 'number'
            AND (value -> 'draftProjection' ->> 'byteSize') ~ '^[0-9]+$'
            AND (value -> 'draftProjection' ->> 'byteSize')::NUMERIC BETWEEN 1 AND 2097152
            AND jsonb_typeof(value -> 'draftProjection' -> 'contentSha256') = 'string'
            AND (value -> 'draftProjection' ->> 'contentSha256') ~ '^[0-9a-f]{64}$'
            AND jsonb_typeof(value -> 'draftProjection' -> 'projectedAt') = 'string'
            AND btrim(value -> 'draftProjection' ->> 'projectedAt') <> ''
            AND value -> 'draftProjection' ->> 'outputRepoId'
                = value -> 'generatedArtifact' ->> 'outputRepoId'
            AND value -> 'draftProjection' ->> 'path'
                = value -> 'generatedArtifact' ->> 'path'
            AND value -> 'draftProjection' -> 'byteSize'
                = value -> 'generatedArtifact' -> 'byteSize'
            AND value -> 'draftProjection' ->> 'contentSha256'
                = value -> 'generatedArtifact' ->> 'contentSha256'
            AND value ->> 'validationDisposition' = 'valid'
            AND (
                value -> 'executionReceipt' = 'null'::JSONB
                OR approval_review_execution_receipt_v1_is_valid(value -> 'executionReceipt')
            )
            AND jsonb_typeof(value -> 'attemptCount') = 'number'
            AND (value ->> 'attemptCount') ~ '^[0-9]+$'
            AND (value ->> 'attemptCount')::NUMERIC BETWEEN 1 AND 4294967295
            AND value -> 'contractVersions' = '{
                "requestContext":3,
                "resolution":2,
                "reviewPolicy":1,
                "output":2,
                "runDetails":4
            }'::JSONB
            AND value -> 'failure' = 'null'::JSONB
    END
$_$;

-- FUNCTION public.approval_review_context_is_valid(jsonb, uuid, uuid)
CREATE FUNCTION public.approval_review_context_is_valid(value jsonb, source_workspace_id uuid, approval_job_id uuid) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $$
    SELECT CASE
        WHEN value ? 'stage' THEN
            jsonb_typeof(value -> 'stage') = 'string'
            AND value ->> 'stage' IN ('planning', 'finalizing')
            AND (
                approval_review_context_v1_is_valid(
                    value - 'stage', source_workspace_id, approval_job_id
                )
                OR approval_review_context_v2_is_valid(
                    value - 'stage', source_workspace_id, approval_job_id
                )
            )
        ELSE
            approval_review_context_v1_is_valid(
                value, source_workspace_id, approval_job_id
            )
            OR approval_review_context_v2_is_valid(
                value, source_workspace_id, approval_job_id
            )
    END
$$;

-- FUNCTION public.approval_review_context_v1_is_valid(jsonb, uuid, uuid)
CREATE FUNCTION public.approval_review_context_v1_is_valid(value jsonb, source_workspace_id uuid, approval_job_id uuid) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $_$
    SELECT CASE
        WHEN NOT approval_review_jsonb_has_exact_keys_v1(
            value,
            ARRAY[
                'schemaVersion', 'requestKey', 'definitionId', 'definitionVersion',
                'targetWorkspaceId', 'currentHeads', 'baseHeads', 'outputDocPath'
            ]
        ) THEN false
        ELSE octet_length(value::TEXT) <= 262144
            AND value -> 'schemaVersion' = '1'::JSONB
            AND jsonb_typeof(value -> 'requestKey') = 'string'
            AND (value ->> 'requestKey') ~ '^[0-9a-f]{64}$'
            AND jsonb_typeof(value -> 'definitionId') = 'string'
            AND (value ->> 'definitionId') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            AND jsonb_typeof(value -> 'definitionVersion') = 'number'
            AND (value ->> 'definitionVersion') ~ '^[0-9]+$'
            AND (value ->> 'definitionVersion')::NUMERIC BETWEEN 1 AND 2147483647
            AND jsonb_typeof(value -> 'targetWorkspaceId') = 'string'
            AND (value ->> 'targetWorkspaceId') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            AND approval_review_head_vector_v1_is_valid(value -> 'currentHeads')
            AND approval_review_head_vector_v1_is_valid(value -> 'baseHeads')
            AND jsonb_typeof(value -> 'outputDocPath') = 'string'
            AND value ->> 'outputDocPath' = format(
                'docs/%s/approval-review-%s.md',
                source_workspace_id,
                approval_job_id
            )
    END
$_$;

-- FUNCTION public.approval_review_context_v2_is_valid(jsonb, uuid, uuid)
CREATE FUNCTION public.approval_review_context_v2_is_valid(value jsonb, source_workspace_id uuid, approval_job_id uuid) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $_$
    SELECT CASE
        WHEN NOT approval_review_jsonb_has_exact_keys_v1(
            value,
            ARRAY[
                'schemaVersion', 'approvalRequestId', 'requestKey',
                'definitionId', 'definitionVersion', 'targetWorkspaceId',
                'currentHeads', 'baseHeads', 'outputDocPath'
            ]
        ) THEN false
        ELSE value -> 'schemaVersion' = '2'::JSONB
            AND jsonb_typeof(value -> 'approvalRequestId') = 'string'
            AND (value ->> 'approvalRequestId')
                ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            AND approval_review_context_v1_is_valid(
                jsonb_set(
                    value - 'approvalRequestId',
                    '{schemaVersion}',
                    '1'::JSONB
                ),
                source_workspace_id,
                approval_job_id
            )
            AND approval_review_head_vector_v1_is_canonical(value -> 'currentHeads')
            AND approval_review_head_vector_v1_is_canonical(value -> 'baseHeads')
    END
$_$;

-- FUNCTION public.approval_review_context_v3_is_valid(jsonb)
CREATE FUNCTION public.approval_review_context_v3_is_valid(value jsonb) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $_$
    SELECT (
        NOT value ? 'stage'
        OR (
            jsonb_typeof(value -> 'stage') = 'string'
            AND value ->> 'stage' IN ('planning', 'finalizing')
        )
    )
    AND CASE
        WHEN NOT approval_review_jsonb_has_exact_keys_v1(
            value - 'stage',
            ARRAY[
                'schemaVersion', 'approvalRequestId', 'requestKey',
                'definitionId', 'definitionVersion'
            ]
        ) THEN false
        ELSE octet_length(value::TEXT) <= 262144
            AND value -> 'schemaVersion' = '3'::JSONB
            AND jsonb_typeof(value -> 'approvalRequestId') = 'string'
            AND (value ->> 'approvalRequestId') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            AND jsonb_typeof(value -> 'requestKey') = 'string'
            AND (value ->> 'requestKey') ~ '^[0-9a-f]{64}$'
            AND jsonb_typeof(value -> 'definitionId') = 'string'
            AND (value ->> 'definitionId') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            AND jsonb_typeof(value -> 'definitionVersion') = 'number'
            AND (value ->> 'definitionVersion') ~ '^[0-9]+$'
            AND (value ->> 'definitionVersion')::NUMERIC BETWEEN 1 AND 2147483647
    END
$_$;

-- FUNCTION public.approval_review_execution_receipt_v1_is_valid(jsonb)
CREATE FUNCTION public.approval_review_execution_receipt_v1_is_valid(value jsonb) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $_$
    SELECT CASE
        WHEN NOT approval_review_jsonb_has_exact_keys_v1(
            value,
            ARRAY[
                'schemaVersion', 'resolutionId', 'provider', 'providerRoute',
                'model', 'upstreamModel', 'permissionMode', 'effort',
                'authMode', 'egressMode'
            ]
        ) THEN false
        ELSE value -> 'schemaVersion' = '1'::JSONB
            AND jsonb_typeof(value -> 'resolutionId') = 'string'
            AND (value ->> 'resolutionId') ~ '^alr_[0-9a-f]{64}$'
            -- P3.5 begin: provider identity is independent of test binary selection.
            AND value ->> 'provider' IN ('claude_code', 'codex')
            -- P3.5 end
            AND value ->> 'providerRoute' IN (
                'none', 'openrouter', 'openai_direct', 'anthropic_direct',
                'compatible_endpoint'
            )
            AND jsonb_typeof(value -> 'model') = 'string'
            AND btrim(value ->> 'model') <> ''
            AND jsonb_typeof(value -> 'upstreamModel') = 'string'
            AND btrim(value ->> 'upstreamModel') <> ''
            AND jsonb_typeof(value -> 'permissionMode') = 'string'
            AND btrim(value ->> 'permissionMode') <> ''
            AND jsonb_typeof(value -> 'effort') = 'string'
            AND btrim(value ->> 'effort') <> ''
            AND value ->> 'authMode' IN (
                'no_credential', 'open_router_api_key', 'open_ai_api_key',
                'chat_gpt_subscription', 'anthropic_api_key',
                'anthropic_bearer', 'anthropic_oauth'
            )
            AND value ->> 'egressMode' IN (
                'no_network', 'host_llm_proxy', 'direct_provider'
            )
    END
$_$;

-- FUNCTION public.approval_review_head_vector_v1_is_canonical(jsonb)
CREATE FUNCTION public.approval_review_head_vector_v1_is_canonical(value jsonb) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $$
    SELECT approval_review_head_vector_v1_is_valid(value)
       AND value = approval_review_head_vector_v1_normalize(value)
$$;

-- FUNCTION public.approval_review_head_vector_v1_is_valid(jsonb)
CREATE FUNCTION public.approval_review_head_vector_v1_is_valid(value jsonb) RETURNS boolean
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
                        ARRAY['repoId', 'branchName', 'workspaceBranchId', 'headCommit']
                    )
                    AND jsonb_typeof(head -> 'repoId') = 'string'
                    AND btrim(head ->> 'repoId') <> ''
                    AND jsonb_typeof(head -> 'branchName') = 'string'
                    AND btrim(head ->> 'branchName') <> ''
                    AND jsonb_typeof(head -> 'workspaceBranchId') = 'string'
                    AND (head ->> 'workspaceBranchId') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
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

-- FUNCTION public.approval_review_head_vector_v1_normalize(jsonb)
CREATE FUNCTION public.approval_review_head_vector_v1_normalize(value jsonb) RETURNS jsonb
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $$
    SELECT CASE
        WHEN jsonb_typeof(value) <> 'array' THEN NULL
        ELSE COALESCE(
            (
                SELECT jsonb_agg(
                    head ORDER BY (head ->> 'repoId') COLLATE "C"
                )
                FROM jsonb_array_elements(value) AS heads(head)
            ),
            '[]'::JSONB
        )
    END
$$;

-- FUNCTION public.approval_review_head_vector_v2_is_valid(jsonb)
CREATE FUNCTION public.approval_review_head_vector_v2_is_valid(value jsonb) RETURNS boolean
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
                        ARRAY['repoId', 'branchName', 'workspaceBranchId', 'headCommit']
                    )
                    AND jsonb_typeof(head -> 'repoId') = 'string'
                    AND (head ->> 'repoId') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    AND jsonb_typeof(head -> 'branchName') = 'string'
                    AND btrim(head ->> 'branchName') <> ''
                    AND octet_length(head ->> 'branchName') <= 1024
                    AND jsonb_typeof(head -> 'workspaceBranchId') = 'string'
                    AND (head ->> 'workspaceBranchId') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
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

-- FUNCTION public.approval_review_jsonb_has_exact_keys_v1(jsonb, text[])
CREATE FUNCTION public.approval_review_jsonb_has_exact_keys_v1(value jsonb, expected_keys text[]) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $$
    SELECT CASE
        WHEN jsonb_typeof(value) <> 'object' THEN false
        ELSE (
            SELECT COALESCE(array_agg(key ORDER BY key), ARRAY[]::TEXT[])
            FROM jsonb_object_keys(value) AS keys(key)
        ) = (
            SELECT COALESCE(array_agg(key ORDER BY key), ARRAY[]::TEXT[])
            FROM unnest(expected_keys) AS keys(key)
        )
    END
$$;

-- FUNCTION public.approval_review_runs_transition_guard()
CREATE FUNCTION public.approval_review_runs_transition_guard() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    legal_transition BOOLEAN;
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.state <> 'pending_assignment' THEN
            RAISE EXCEPTION 'approval review run must be inserted pending assignment'
                USING ERRCODE = '23514',
                      CONSTRAINT = 'approval_review_runs_initial_state';
        END IF;
        NEW.updated_at := now();
        RETURN NEW;
    END IF;

    legal_transition := NEW.state = OLD.state OR CASE OLD.state
        WHEN 'pending_assignment' THEN NEW.state IN ('assigned', 'failed', 'cancelled')
        WHEN 'assigned' THEN NEW.state IN ('generated', 'failed', 'cancelled')
        WHEN 'generated' THEN NEW.state IN ('merged_to_draft', 'failed', 'cancelled')
        WHEN 'merged_to_draft' THEN NEW.state IN ('settled', 'failed', 'cancelled')
        ELSE false
    END;
    IF NOT legal_transition THEN
        RAISE EXCEPTION 'illegal approval review run transition % -> %', OLD.state, NEW.state
            USING ERRCODE = '23514',
                  CONSTRAINT = 'approval_review_runs_legal_transition';
    END IF;

    IF OLD.resolution_id IS NOT NULL AND ROW(
        NEW.resolution_id, NEW.resolved_bootstrap_task_id,
        NEW.target_workspace_id, NEW.source_heads,
        NEW.target_heads, NEW.output_repo_id, NEW.output_doc_path, NEW.resolved_at
    ) IS DISTINCT FROM ROW(
        OLD.resolution_id, OLD.resolved_bootstrap_task_id,
        OLD.target_workspace_id, OLD.source_heads,
        OLD.target_heads, OLD.output_repo_id, OLD.output_doc_path, OLD.resolved_at
    ) THEN
        RAISE EXCEPTION 'approval review assignment evidence is immutable'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'approval_review_runs_assignment_immutable';
    END IF;

    IF ROW(NEW.resolved_attempt_id, NEW.resolved_worker_id) IS DISTINCT FROM
       ROW(OLD.resolved_attempt_id, OLD.resolved_worker_id)
       AND NOT (
           OLD.state = 'assigned'
           AND NEW.state = 'assigned'
           AND OLD.resolved_attempt_id IS NULL
           AND OLD.resolved_worker_id IS NULL
           AND NEW.resolved_attempt_id IS NOT NULL
           AND NEW.resolved_worker_id IS NOT NULL
       )
    THEN
        RAISE EXCEPTION 'approval review first execution evidence is immutable'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'approval_review_runs_first_execution_immutable';
    END IF;

    IF OLD.generated_result IS NOT NULL AND ROW(
        NEW.generated_result, NEW.generated_task_id, NEW.generated_attempt_id,
        NEW.generated_job_commit_sha, NEW.generated_blob_oid,
        NEW.generated_byte_size, NEW.generated_content_sha256,
        NEW.generated_media_type, NEW.generated_at
    ) IS DISTINCT FROM ROW(
        OLD.generated_result, OLD.generated_task_id, OLD.generated_attempt_id,
        OLD.generated_job_commit_sha, OLD.generated_blob_oid,
        OLD.generated_byte_size, OLD.generated_content_sha256,
        OLD.generated_media_type, OLD.generated_at
    ) THEN
        RAISE EXCEPTION 'approval review generated evidence is immutable'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'approval_review_runs_generated_immutable';
    END IF;

    IF OLD.published_source_heads IS NOT NULL AND ROW(
        NEW.published_source_heads, NEW.artifact_commit_sha, NEW.artifact_blob_oid,
        NEW.artifact_byte_size, NEW.artifact_content_sha256,
        NEW.artifact_media_type, NEW.publication_operation_id,
        NEW.published_at, NEW.approval_status_id
    ) IS DISTINCT FROM ROW(
        OLD.published_source_heads, OLD.artifact_commit_sha, OLD.artifact_blob_oid,
        OLD.artifact_byte_size, OLD.artifact_content_sha256,
        OLD.artifact_media_type, OLD.publication_operation_id,
        OLD.published_at, OLD.approval_status_id
    ) THEN
        RAISE EXCEPTION 'approval review accepted evidence is immutable'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'approval_review_runs_publication_immutable';
    END IF;

    -- Settlement writes the run first and then inserts its status in the same
    -- transaction. Require a fresh status id so the immediate status trigger
    -- below is the operation that closes the cross-table invariant; a caller
    -- cannot attach a pre-existing status row and bypass that validation.
    IF OLD.state <> 'settled'
       AND NEW.state = 'settled'
       AND EXISTS (
           SELECT 1
           FROM approval_statuses status
           WHERE status.org_id = NEW.org_id
             AND status.id = NEW.approval_status_id
       )
    THEN
        RAISE EXCEPTION 'approval review settlement must allocate a fresh status id'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'approval_review_runs_fresh_status';
    END IF;

    NEW.updated_at := now();
    RETURN NEW;
END
$$;

-- FUNCTION public.approval_review_runs_verdict_immutable_guard()
CREATE FUNCTION public.approval_review_runs_verdict_immutable_guard() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF OLD.approval_status_id IS NOT NULL
       AND NEW.approval_status_id IS DISTINCT FROM OLD.approval_status_id
    THEN
        RAISE EXCEPTION 'approval review verdict evidence is immutable'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'approval_review_runs_verdict_immutable';
    END IF;
    RETURN NEW;
END
$$;

-- FUNCTION public.clear_terminal_task_agent_auth()
CREATE FUNCTION public.clear_terminal_task_agent_auth() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NEW.status IN ('completed', 'failed', 'cancelled', 'rolled_back') THEN
        NEW.agent_auth_enc := NULL;
        NEW.agent_auth_worker_id := NULL;
        NEW.agent_auth_submitted_by := NULL;
        NEW.agent_auth_updated_at := NULL;
    END IF;
    RETURN NEW;
END;
$$;

-- FUNCTION public.cli_device_workspace_org(uuid)
CREATE FUNCTION public.cli_device_workspace_org(p_workspace_id uuid) RETURNS uuid
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'public'
    AS $$
    SELECT org_id
    FROM workspaces
    WHERE id = p_workspace_id
      AND deleted_at IS NULL
$$;

-- FUNCTION public.g5_default_repo_mount_root(text, text)
CREATE FUNCTION public.g5_default_repo_mount_root(repo_slug text, repo_name text) RETURNS text
    LANGUAGE sql IMMUTABLE
    AS $$
    SELECT NULLIF(
        btrim(
            regexp_replace(
                lower(
                    btrim(
                        btrim(
                            CASE
                                WHEN btrim(COALESCE(repo_slug, '')) = ''
                                    THEN COALESCE(repo_name, '')
                                ELSE repo_slug
                            END
                        ),
                        '/'
                    )
                ),
                '[^a-z0-9._-]', '-', 'g'
            ),
            '-'
        ),
        ''
    );
$$;

-- FUNCTION public.guard_approval_request_update()
CREATE FUNCTION public.guard_approval_request_update() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NEW.id IS DISTINCT FROM OLD.id
       OR NEW.seq IS DISTINCT FROM OLD.seq
       OR NEW.org_id IS DISTINCT FROM OLD.org_id
       OR NEW.source_workspace_id IS DISTINCT FROM OLD.source_workspace_id
       OR NEW.approval_definition_id IS DISTINCT FROM OLD.approval_definition_id
       OR NEW.approval_definition_version IS DISTINCT FROM OLD.approval_definition_version
       OR NEW.request_key IS DISTINCT FROM OLD.request_key
       OR NEW.actor_type IS DISTINCT FROM OLD.actor_type
       OR NEW.actor_user_id IS DISTINCT FROM OLD.actor_user_id
       OR NEW.requested_by IS DISTINCT FROM OLD.requested_by
       OR NEW.expires_at IS DISTINCT FROM OLD.expires_at
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'approval_requests identity fields are immutable'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'approval_requests_immutable';
    END IF;

    IF OLD.state <> 'pending'
       OR NEW.state NOT IN ('settled', 'cancelled', 'expired', 'superseded') THEN
        RAISE EXCEPTION 'approval_requests permits one pending-to-terminal transition'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'approval_requests_transition';
    END IF;

    IF NEW.closed_at IS DISTINCT FROM transaction_timestamp()
       OR NEW.updated_at IS DISTINCT FROM transaction_timestamp() THEN
        RAISE EXCEPTION 'approval_requests close timestamps must use transaction time'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'approval_requests_server_timestamps';
    END IF;

    NEW.closed_at := transaction_timestamp();
    NEW.updated_at := transaction_timestamp();

    RETURN NEW;
END;
$$;

-- FUNCTION public.human_escalation_questions_parent_org_guard()
CREATE FUNCTION public.human_escalation_questions_parent_org_guard() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public'
    AS $$
DECLARE
    parent_org_id UUID;
BEGIN
    SELECT he.org_id INTO parent_org_id
    FROM human_escalations he
    WHERE he.id = NEW.escalation_id;

    IF parent_org_id IS NULL THEN
        RAISE EXCEPTION 'parent human escalation % not found', NEW.escalation_id
            USING ERRCODE = '23503';
    END IF;

    IF NEW.org_id <> parent_org_id THEN
        RAISE EXCEPTION 'human escalation question org_id % does not match parent org_id %',
            NEW.org_id, parent_org_id
            USING ERRCODE = '23514';
    END IF;

    RETURN NEW;
END
$$;

-- FUNCTION public.human_escalation_questions_touch_updated_at()
CREATE FUNCTION public.human_escalation_questions_touch_updated_at() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END
$$;

-- FUNCTION public.job_submission_receipts_touch_updated_at()
CREATE FUNCTION public.job_submission_receipts_touch_updated_at() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END
$$;

-- FUNCTION public.maintain_workspace_branch_repo_base_version()
CREATE FUNCTION public.maintain_workspace_branch_repo_base_version() RETURNS trigger
    LANGUAGE plpgsql
    AS $_$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.base_head_commit IS NULL
           AND NEW.head_commit ~ '^[0-9a-f]{40}$'
           AND NEW.head_commit <> repeat('0', 40) THEN
            NEW.base_head_commit := NEW.head_commit;
        END IF;
    ELSIF (
              OLD.head_commit IS NULL
              OR btrim(OLD.head_commit) = ''
              OR btrim(OLD.head_commit) IN ('HEAD', 'no-changes')
              OR btrim(OLD.head_commit) ~ '^0+$'
          )
          AND NEW.head_commit ~ '^[0-9a-f]{40}$'
          AND NEW.head_commit <> repeat('0', 40)
          AND (
              NEW.base_head_commit IS NULL
              OR btrim(NEW.base_head_commit) ~ '^0+$'
          ) THEN
        NEW.base_head_commit := NEW.head_commit;
    ELSIF NEW.base_branch IS DISTINCT FROM OLD.base_branch
          AND NEW.base_head_commit IS NOT DISTINCT FROM OLD.base_head_commit THEN
        NEW.base_head_commit := NULL;
    END IF;

    RETURN NEW;
END
$_$;

-- FUNCTION public.reject_approval_context_update()
CREATE FUNCTION public.reject_approval_context_update() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NEW.approval_context IS DISTINCT FROM OLD.approval_context THEN
        RAISE EXCEPTION 'jobs.approval_context is immutable'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'jobs_approval_context_immutable';
    END IF;
    RETURN NEW;
END;
$$;

-- FUNCTION public.repo_webhook_secret_for_current_org(uuid)
CREATE FUNCTION public.repo_webhook_secret_for_current_org(target_repo_id uuid) RETURNS bytea
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'authz', 'public'
    AS $$
    SELECT rs.webhook_secret_enc
    FROM repo_secrets rs
    WHERE rs.repo_id = target_repo_id
      AND rs.org_id = ANY(authz.current_org_ids())
    LIMIT 1
$$;

-- FUNCTION public.set_approval_request_insert_facts()
CREATE FUNCTION public.set_approval_request_insert_facts() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    NEW.created_at := transaction_timestamp();
    NEW.updated_at := transaction_timestamp();
    RETURN NEW;
END;
$$;

-- FUNCTION public.set_approvals_governance_created_at()
CREATE FUNCTION public.set_approvals_governance_created_at() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    NEW.created_at := now();
    RETURN NEW;
END;
$$;

-- FUNCTION public.set_org_id_from_session()
CREATE FUNCTION public.set_org_id_from_session() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'authz', 'public'
    AS $$
DECLARE
    session_orgs UUID[];
BEGIN
    IF NEW.org_id IS NOT NULL THEN
        -- Caller set org_id explicitly.
        -- If session context exists, validate; otherwise trust the caller
        -- (route-level auth has already validated org access).
        session_orgs := authz.current_org_ids();
        IF session_orgs IS NOT NULL AND array_length(session_orgs, 1) IS NOT NULL THEN
            IF NOT (NEW.org_id = ANY(session_orgs)) THEN
                RAISE EXCEPTION 'org_id % not in active session context', NEW.org_id
                    USING ERRCODE = '42501'; -- insufficient_privilege
            END IF;
        END IF;
        RETURN NEW;
    END IF;

    -- No org_id provided — infer from session context (strict).
    session_orgs := authz.current_org_ids();
    IF session_orgs IS NULL OR array_length(session_orgs, 1) IS NULL THEN
        RAISE EXCEPTION
            'No active session context — cannot infer org_id. '
            'Provide org_id explicitly or use ScopedDb.'
            USING ERRCODE = '42501'; -- insufficient_privilege
    END IF;

    NEW.org_id := session_orgs[1]; -- first org in session
    RETURN NEW;
END
$$;

-- FUNCTION public.stamp_task_async_decision_policy()
CREATE FUNCTION public.stamp_task_async_decision_policy() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public', 'pg_catalog'
    AS $$
DECLARE
    enabled BOOLEAN := false;
    parent_job_type TEXT;
BEGIN
    SELECT
        COALESCE(up.autonomous_async_tasks, false),
        j.job_type
    INTO enabled, parent_job_type
    FROM jobs j
    LEFT JOIN user_preferences up ON up.user_id = j.created_by
    WHERE j.id = NEW.job_id;

    IF NEW.task_type NOT IN ('chat', 'preview', 'golden_builder')
       AND parent_job_type <> 'approval_review'
       AND enabled THEN
        NEW.async_decision_policy := 'autonomous_v1';
    ELSE
        NEW.async_decision_policy := 'interactive_v1';
    END IF;

    RETURN NEW;
END;
$$;

-- FUNCTION public.sync_workspace_branch_repo_from_branch()
CREATE FUNCTION public.sync_workspace_branch_repo_from_branch() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    INSERT INTO workspace_branch_repos (
        org_id,
        workspace_branch_id,
        repo_id,
        repo_branch_name,
        base_branch,
        head_commit,
        tracking_mode,
        materialization_status,
        root_path,
        updated_at
    )
    VALUES (
        NEW.org_id,
        NEW.id,
        NEW.repo_id,
        NEW.branch_name,
        NEW.base_branch,
        NEW.head_commit,
        NEW.tracking_mode,
        NEW.materialization_status,
        '',
        now()
    )
    ON CONFLICT (workspace_branch_id, repo_id) DO UPDATE
    SET repo_branch_name = EXCLUDED.repo_branch_name,
        base_branch = EXCLUDED.base_branch,
        updated_at = now();

    RETURN NEW;
END
$$;

-- FUNCTION public.uuid_array_is_set(uuid[])
CREATE FUNCTION public.uuid_array_is_set(arr uuid[]) RETURNS boolean
    LANGUAGE sql IMMUTABLE
    AS $$
    SELECT COALESCE(array_position(arr, NULL) IS NULL, true)
       AND (SELECT COUNT(*) = COUNT(DISTINCT x) FROM unnest(arr) AS t(x));
$$;

-- FUNCTION public.validate_agent_approval_status_v3_run()
CREATE FUNCTION public.validate_agent_approval_status_v3_run() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    run approval_review_runs%ROWTYPE;
    details JSONB;
BEGIN
    details := NEW.agent_approval_run_details;
    IF NEW.actor_type <> 'agent'
       OR details -> 'schemaVersion' IS DISTINCT FROM '3'::JSONB THEN
        RETURN NEW;
    END IF;

    SELECT * INTO run
     FROM approval_review_runs
     WHERE org_id = NEW.org_id
       AND job_id = (details ->> 'agentJobId')::UUID;

    IF NOT FOUND
       OR run.state <> 'settled'
       OR run.approval_status_id <> NEW.id
       OR run.source_workspace_id <> NEW.source_workspace_id
       OR run.definition_id <> NEW.approval_definition_id
       OR run.definition_version <> NEW.approval_definition_version
       OR run.generated_result <> NEW.outcome
       OR run.source_heads IS DISTINCT FROM NEW.reviewed_git_heads
       OR details ->> 'agentJobId' <> run.job_id::TEXT
       OR details ->> 'resolutionId' <> run.resolution_id
       OR details ->> 'targetWorkspaceId' <> run.target_workspace_id::TEXT
       OR details -> 'targetHeads' IS DISTINCT FROM run.target_heads
       OR details -> 'assignedSourceHeads' IS DISTINCT FROM run.source_heads
       OR details -> 'publishedSourceHeads' IS DISTINCT FROM run.published_source_heads
       OR details -> 'generatedArtifact' ->> 'outputRepoId' <> run.output_repo_id::TEXT
       OR details -> 'generatedArtifact' ->> 'path' <> run.output_doc_path
       OR details -> 'generatedArtifact' ->> 'jobCommitSha' <> run.generated_job_commit_sha
       OR details -> 'generatedArtifact' ->> 'blobOid' <> run.generated_blob_oid
       OR (details -> 'generatedArtifact' ->> 'byteSize')::BIGINT <> run.generated_byte_size
       OR details -> 'generatedArtifact' ->> 'contentSha256' <> run.generated_content_sha256
       OR details -> 'generatedArtifact' ->> 'mediaType' <> run.generated_media_type
       OR details -> 'acceptedArtifact' ->> 'outputRepoId' <> run.output_repo_id::TEXT
       OR details -> 'acceptedArtifact' ->> 'path' <> run.output_doc_path
       OR details -> 'acceptedArtifact' ->> 'sourceCommitSha' <> run.artifact_commit_sha
       OR details -> 'acceptedArtifact' ->> 'blobOid' <> run.artifact_blob_oid
       OR (details -> 'acceptedArtifact' ->> 'byteSize')::BIGINT <> run.artifact_byte_size
       OR details -> 'acceptedArtifact' ->> 'contentSha256' <> run.artifact_content_sha256
       OR details -> 'acceptedArtifact' ->> 'mediaType' <> run.artifact_media_type
       OR details -> 'acceptedArtifact' ->> 'publicationOperationId'
            <> run.publication_operation_id::TEXT
    THEN
        RAISE EXCEPTION 'agent approval status v3 does not match its settled run'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'approval_statuses_v3_run_match';
    END IF;
    RETURN NEW;
END
$$;

-- FUNCTION public.validate_agent_approval_status_v4_run()
CREATE FUNCTION public.validate_agent_approval_status_v4_run() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    run approval_review_runs%ROWTYPE;
    details JSONB;
BEGIN
    details := NEW.agent_approval_run_details;
    IF NEW.actor_type <> 'agent'
       OR details -> 'schemaVersion' IS DISTINCT FROM '4'::JSONB THEN
        RETURN NEW;
    END IF;

    SELECT * INTO run
    FROM approval_review_runs
    WHERE org_id = NEW.org_id
      AND job_id = (details ->> 'agentJobId')::UUID;

    IF NOT FOUND
       OR run.state <> 'settled'
       OR run.approval_status_id <> NEW.id
       OR run.source_workspace_id <> NEW.source_workspace_id
       OR run.definition_id <> NEW.approval_definition_id
       OR run.definition_version <> NEW.approval_definition_version
       OR run.generated_result <> NEW.outcome
       OR run.source_heads IS DISTINCT FROM NEW.reviewed_git_heads
       OR run.published_source_heads IS NOT NULL
       OR run.artifact_commit_sha IS NOT NULL
       OR run.artifact_blob_oid IS NOT NULL
       OR run.artifact_byte_size IS NOT NULL
       OR run.artifact_content_sha256 IS NOT NULL
       OR run.artifact_media_type IS NOT NULL
       OR run.publication_operation_id IS NOT NULL
       OR run.published_at IS NOT NULL
       OR details ->> 'agentJobId' <> run.job_id::TEXT
       OR details ->> 'resolutionId' <> run.resolution_id
       OR details ->> 'targetWorkspaceId' <> run.target_workspace_id::TEXT
       OR details -> 'targetHeads' IS DISTINCT FROM run.target_heads
       OR details -> 'assignedSourceHeads' IS DISTINCT FROM run.source_heads
       OR details -> 'generatedArtifact' ->> 'outputRepoId' <> run.output_repo_id::TEXT
       OR details -> 'generatedArtifact' ->> 'path' <> run.output_doc_path
       OR details -> 'generatedArtifact' ->> 'jobCommitSha' <> run.generated_job_commit_sha
       OR details -> 'generatedArtifact' ->> 'blobOid' <> run.generated_blob_oid
       OR (details -> 'generatedArtifact' ->> 'byteSize')::BIGINT <> run.generated_byte_size
       OR details -> 'generatedArtifact' ->> 'contentSha256' <> run.generated_content_sha256
       OR details -> 'generatedArtifact' ->> 'mediaType' <> run.generated_media_type
       OR details -> 'draftProjection' ->> 'outputDocId' <> run.output_doc_id::TEXT
       OR details -> 'draftProjection' ->> 'outputRepoId' <> run.output_repo_id::TEXT
       OR details -> 'draftProjection' ->> 'path' <> run.output_doc_path
       OR (details -> 'draftProjection' ->> 'byteSize')::BIGINT <> run.generated_byte_size
       OR details -> 'draftProjection' ->> 'contentSha256' <> run.generated_content_sha256
       OR (details -> 'draftProjection' ->> 'projectedAt')::TIMESTAMPTZ
            IS DISTINCT FROM run.merged_to_draft_at
    THEN
        RAISE EXCEPTION 'agent approval status v4 does not match its settled run'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'approval_statuses_v4_run_match';
    END IF;
    RETURN NEW;
END
$$;

-- FUNCTION public.validate_workspace_branch_authority_scope()
CREATE FUNCTION public.validate_workspace_branch_authority_scope() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    owner_job RECORD;
    owner_task RECORD;
BEGIN
    IF NEW.authority_scope_kind = 'job' THEN
        SELECT job.org_id, job.workspace_id
          INTO owner_job
          FROM jobs job
         WHERE job.id = NEW.job_id;

        IF NOT FOUND
           OR owner_job.org_id <> NEW.org_id
           OR owner_job.workspace_id <> NEW.workspace_id THEN
            RAISE EXCEPTION
                'Workspace branch % is not an exact member of job %',
                NEW.id,
                NEW.job_id;
        END IF;
    ELSIF NEW.authority_scope_kind = 'task' THEN
        SELECT task.org_id, task.job_id, job.workspace_id
          INTO owner_task
          FROM tasks task
          JOIN jobs job ON job.id = task.job_id
         WHERE task.id = NEW.task_id;

        IF NOT FOUND
           OR owner_task.org_id <> NEW.org_id
           OR owner_task.job_id <> NEW.job_id
           OR owner_task.workspace_id <> NEW.workspace_id THEN
            RAISE EXCEPTION
                'Workspace branch % is not an exact member of task % in job %',
                NEW.id,
                NEW.task_id,
                NEW.job_id;
        END IF;
    END IF;

    RETURN NEW;
END
$$;

-- P1.4: removed validate_workspace_head_operation_membership_proposal(), validate_workspace_head_operation_source(), validate_workspace_head_operation_target() and validate_workspace_operation_make_remote_application(); all four existed only to validate rows of the deleted head-operation registry tables (the make-remote validator read workspace_head_operations on behalf of workspace_head_operation_make_remote_applications, itself deleted), so no non-registry branch survives.

-- FUNCTION public.wbr_fill_default_root_path()
CREATE FUNCTION public.wbr_fill_default_root_path() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    derived TEXT;
BEGIN
    IF NEW.root_path IS NULL OR NEW.root_path = '' THEN
        SELECT g5_default_repo_mount_root(r.slug, r.name)
        INTO derived
        FROM repos r
        WHERE r.id = NEW.repo_id;

        NEW.root_path := COALESCE(derived, '');
    END IF;

    RETURN NEW;
END
$$;

SET default_tablespace = '';

SET default_table_access_method = heap;

-- TABLE authz.session_ctx
CREATE TABLE authz.session_ctx (
    pid integer NOT NULL,
    org_ids uuid[] NOT NULL,
    user_id uuid NOT NULL,
    scopes text[] DEFAULT '{}'::text[] NOT NULL,
    session_id uuid,
    set_at timestamp with time zone DEFAULT now() NOT NULL
);

-- COMMENT authz.TABLE session_ctx
COMMENT ON TABLE authz.session_ctx IS 'Per-connection RLS context keyed by pg_backend_pid(). Written by set_ctx(), read by current_org_ids(), cleared by clear_ctx(). Rows older than 1h are considered stale and pruned.';

-- TABLE authz.user_sessions
CREATE TABLE authz.user_sessions (
    session_id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    org_ids uuid[] DEFAULT '{}'::uuid[] NOT NULL,
    scopes text[] DEFAULT '{}'::text[] NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    refreshed_at timestamp with time zone DEFAULT now() NOT NULL,
    org_hash bytea NOT NULL,
    scope_hash bytea NOT NULL
);

-- COMMENT authz.TABLE user_sessions
COMMENT ON TABLE authz.user_sessions IS 'Reusable scoped sessions for RLS scoping. 60-min TTL. One row per exact actor/org_ids/scopes context; multiple distinct contexts per actor are allowed.';

-- COMMENT authz.COLUMN user_sessions.org_hash
COMMENT ON COLUMN authz.user_sessions.org_hash IS 'SHA-256 over canonical org-id set using g5-authz-org-v1 length-prefixed UUID text encoding; uniqueness arbiter only.';

-- COMMENT authz.COLUMN user_sessions.scope_hash
COMMENT ON COLUMN authz.user_sessions.scope_hash IS 'SHA-256 over canonical scope set using g5-authz-scope-v1 length-prefixed UTF-8 encoding; uniqueness arbiter only.';

-- TABLE public.agent_approval_handoffs
CREATE TABLE public.agent_approval_handoffs (
    delivery_id uuid NOT NULL,
    org_id uuid NOT NULL,
    session_id uuid NOT NULL,
    task_id uuid NOT NULL,
    request_entry_id uuid NOT NULL,
    approval_id text NOT NULL,
    decision text NOT NULL,
    reason text,
    updated_input jsonb,
    updated_permissions jsonb,
    interrupt boolean,
    decision_fingerprint text NOT NULL,
    status text DEFAULT 'resolving'::text NOT NULL,
    error_code text,
    attempt_count integer DEFAULT 0 NOT NULL,
    last_attempt_at timestamp with time zone,
    applied_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT agent_approval_handoffs_attempt_count_check CHECK ((attempt_count >= 0)),
    CONSTRAINT agent_approval_handoffs_decision_check CHECK ((decision = ANY (ARRAY['approved'::text, 'rejected'::text]))),
    CONSTRAINT agent_approval_handoffs_status_check CHECK ((status = ANY (ARRAY['resolving'::text, 'applied'::text, 'failed'::text, 'cancelled'::text])))
);

ALTER TABLE ONLY public.agent_approval_handoffs FORCE ROW LEVEL SECURITY;

-- TABLE public.api_keys
CREATE TABLE public.api_keys (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    user_id uuid NOT NULL,
    name text NOT NULL,
    key_prefix text NOT NULL,
    key_hash text NOT NULL,
    scopes text[] DEFAULT '{}'::text[] NOT NULL,
    expires_at timestamp with time zone,
    last_used_at timestamp with time zone,
    last_used_ip inet,
    revoked_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

-- TABLE public.approval_configs
CREATE TABLE public.approval_configs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    seq bigint NOT NULL,
    org_id uuid NOT NULL,
    target_workspace_id uuid NOT NULL,
    approval_definition_id uuid NOT NULL,
    definition_version_at_write integer NOT NULL,
    retired boolean DEFAULT false NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

ALTER TABLE ONLY public.approval_configs FORCE ROW LEVEL SECURITY;

-- SEQUENCE public.approval_configs_seq_seq
ALTER TABLE public.approval_configs ALTER COLUMN seq ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME public.approval_configs_seq_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);

-- TABLE public.approval_definitions
CREATE TABLE public.approval_definitions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    version integer NOT NULL,
    org_id uuid NOT NULL,
    name text NOT NULL,
    description text,
    humans_list uuid[] DEFAULT '{}'::uuid[] NOT NULL,
    agent_type text,
    agent_prompt text,
    invalidates_previous boolean DEFAULT false NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    stage_planning boolean DEFAULT false NOT NULL,
    stage_finalizing boolean DEFAULT true NOT NULL,
    planning_fallback text DEFAULT 'carry_over'::text NOT NULL,
    agent_harness text,
    agent_model text,
    agent_effort text,
    CONSTRAINT approval_definitions_agent_launch_shape_check CHECK ((((agent_type IS NULL) AND (agent_harness IS NULL) AND (agent_model IS NULL) AND (agent_effort IS NULL)) OR ((agent_type = 'simple'::text) AND (agent_harness = ANY (ARRAY['claude_code'::text, 'codex'::text])) AND (length(btrim(agent_model)) > 0) AND (length(btrim(agent_effort)) > 0)))),
    CONSTRAINT approval_definitions_agent_type_check CHECK ((agent_type = 'simple'::text)),
    CONSTRAINT approval_definitions_description_check CHECK (((description IS NULL) OR (btrim(description) <> ''::text))),
    CONSTRAINT approval_definitions_humans_list_check CHECK (public.uuid_array_is_set(humans_list)),
    CONSTRAINT approval_definitions_name_check CHECK ((btrim(name) <> ''::text)),
    CONSTRAINT approval_definitions_planning_fallback_check CHECK ((planning_fallback = ANY (ARRAY['carry_over'::text, 'skip'::text]))),
    CONSTRAINT approval_definitions_version_check CHECK ((version >= 1)),
    CONSTRAINT chk_agent_prompt CHECK (((agent_type IS NULL) = (agent_prompt IS NULL))),
    CONSTRAINT chk_agent_prompt_nonempty CHECK (((agent_prompt IS NULL) OR (btrim(agent_prompt) <> ''::text))),
    CONSTRAINT chk_def_nonempty CHECK (((cardinality(humans_list) > 0) OR (agent_type IS NOT NULL)))
);

ALTER TABLE ONLY public.approval_definitions FORCE ROW LEVEL SECURITY;

-- TABLE public.approval_requests
CREATE TABLE public.approval_requests (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    seq bigint NOT NULL,
    org_id uuid NOT NULL,
    source_workspace_id uuid NOT NULL,
    approval_definition_id uuid NOT NULL,
    approval_definition_version integer NOT NULL,
    request_key text NOT NULL,
    actor_type text NOT NULL,
    actor_user_id uuid,
    state text DEFAULT 'pending'::text NOT NULL,
    settlement_fingerprint text,
    requested_by uuid,
    expires_at timestamp with time zone,
    closed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT chk_approval_request_actor CHECK ((((actor_type = 'human'::text) AND (actor_user_id IS NOT NULL)) OR ((actor_type = 'agent'::text) AND (actor_user_id IS NULL)))),
    CONSTRAINT chk_approval_request_expiry CHECK (((expires_at IS NULL) OR (expires_at > created_at))),
    CONSTRAINT chk_approval_request_lifecycle CHECK ((((state = 'pending'::text) AND (settlement_fingerprint IS NULL) AND (closed_at IS NULL)) OR ((state = 'settled'::text) AND (settlement_fingerprint IS NOT NULL) AND (closed_at IS NOT NULL)) OR ((state = ANY (ARRAY['cancelled'::text, 'expired'::text, 'superseded'::text])) AND (settlement_fingerprint IS NULL) AND (closed_at IS NOT NULL)))),
    CONSTRAINT chk_approval_requests_actor_type CHECK ((actor_type = ANY (ARRAY['human'::text, 'agent'::text]))),
    CONSTRAINT chk_approval_requests_definition_version CHECK ((approval_definition_version >= 1)),
    CONSTRAINT chk_approval_requests_request_key CHECK ((request_key ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT chk_approval_requests_settlement_fingerprint CHECK (((settlement_fingerprint IS NULL) OR (settlement_fingerprint ~ '^[0-9a-f]{64}$'::text))),
    CONSTRAINT chk_approval_requests_state CHECK ((state = ANY (ARRAY['pending'::text, 'settled'::text, 'cancelled'::text, 'expired'::text, 'superseded'::text])))
);

ALTER TABLE ONLY public.approval_requests FORCE ROW LEVEL SECURITY;

-- SEQUENCE public.approval_requests_seq_seq
ALTER TABLE public.approval_requests ALTER COLUMN seq ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME public.approval_requests_seq_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);

-- TABLE public.approval_review_runs
CREATE TABLE public.approval_review_runs (
    job_id uuid NOT NULL,
    org_id uuid NOT NULL,
    source_workspace_id uuid NOT NULL,
    approval_request_id uuid NOT NULL,
    definition_id uuid NOT NULL,
    definition_version integer NOT NULL,
    request_key text NOT NULL,
    state text DEFAULT 'pending_assignment'::text NOT NULL,
    resolution_id text,
    resolved_bootstrap_task_id uuid,
    resolved_attempt_id uuid,
    resolved_worker_id text,
    target_workspace_id uuid,
    source_heads jsonb,
    target_heads jsonb,
    output_doc_id uuid,
    output_repo_id uuid,
    output_doc_path text,
    resolved_at timestamp with time zone,
    generated_result text,
    generated_task_id uuid,
    generated_attempt_id uuid,
    generated_job_commit_sha text,
    generated_blob_oid text,
    generated_byte_size bigint,
    generated_content_sha256 text,
    generated_media_type text,
    generated_at timestamp with time zone,
    merged_to_draft_at timestamp with time zone,
    published_source_heads jsonb,
    artifact_commit_sha text,
    artifact_blob_oid text,
    artifact_byte_size bigint,
    artifact_content_sha256 text,
    artifact_media_type text,
    publication_operation_id uuid,
    published_at timestamp with time zone,
    approval_status_id uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT approval_review_runs_artifact_blob_oid_check CHECK (((artifact_blob_oid IS NULL) OR (artifact_blob_oid ~ '^([0-9a-f]{40}|[0-9a-f]{64})$'::text))),
    CONSTRAINT approval_review_runs_artifact_byte_size_check CHECK (((artifact_byte_size IS NULL) OR ((artifact_byte_size >= 1) AND (artifact_byte_size <= 2097152)))),
    CONSTRAINT approval_review_runs_artifact_commit_sha_check CHECK (((artifact_commit_sha IS NULL) OR (artifact_commit_sha ~ '^([0-9a-f]{40}|[0-9a-f]{64})$'::text))),
    CONSTRAINT approval_review_runs_artifact_content_sha256_check CHECK (((artifact_content_sha256 IS NULL) OR (artifact_content_sha256 ~ '^[0-9a-f]{64}$'::text))),
    CONSTRAINT approval_review_runs_artifact_media_type_check CHECK (((artifact_media_type IS NULL) OR (artifact_media_type = 'text/markdown; charset=utf-8'::text))),
    CONSTRAINT approval_review_runs_assignment_group_check CHECK ((((resolution_id IS NULL) AND (resolved_bootstrap_task_id IS NULL) AND (resolved_attempt_id IS NULL) AND (resolved_worker_id IS NULL) AND (target_workspace_id IS NULL) AND (source_heads IS NULL) AND (target_heads IS NULL) AND (output_doc_id IS NULL) AND (output_repo_id IS NULL) AND (output_doc_path IS NULL) AND (resolved_at IS NULL)) OR ((resolution_id IS NOT NULL) AND (resolved_bootstrap_task_id IS NOT NULL) AND (((resolved_attempt_id IS NULL) AND (resolved_worker_id IS NULL)) OR ((resolved_attempt_id IS NOT NULL) AND (resolved_worker_id IS NOT NULL))) AND (target_workspace_id IS NOT NULL) AND (source_heads IS NOT NULL) AND (target_heads IS NOT NULL) AND (output_repo_id IS NOT NULL) AND (output_doc_path IS NOT NULL) AND (resolved_at IS NOT NULL)))),
    CONSTRAINT approval_review_runs_check CHECK (((output_doc_path IS NULL) OR (output_doc_path = (((('docs/'::text || (source_workspace_id)::text) || '/reviews/agent-approval-'::text) || (job_id)::text) || '.md'::text)))),
    CONSTRAINT approval_review_runs_definition_version_check CHECK ((definition_version > 0)),
    CONSTRAINT approval_review_runs_generated_blob_oid_check CHECK (((generated_blob_oid IS NULL) OR (generated_blob_oid ~ '^([0-9a-f]{40}|[0-9a-f]{64})$'::text))),
    CONSTRAINT approval_review_runs_generated_byte_size_check CHECK (((generated_byte_size IS NULL) OR ((generated_byte_size >= 1) AND (generated_byte_size <= 2097152)))),
    CONSTRAINT approval_review_runs_generated_content_sha256_check CHECK (((generated_content_sha256 IS NULL) OR (generated_content_sha256 ~ '^[0-9a-f]{64}$'::text))),
    CONSTRAINT approval_review_runs_generated_group_check CHECK ((((generated_result IS NULL) AND (generated_task_id IS NULL) AND (generated_attempt_id IS NULL) AND (generated_job_commit_sha IS NULL) AND (generated_blob_oid IS NULL) AND (generated_byte_size IS NULL) AND (generated_content_sha256 IS NULL) AND (generated_media_type IS NULL) AND (generated_at IS NULL)) OR ((generated_result IS NOT NULL) AND (generated_task_id IS NOT NULL) AND (generated_attempt_id IS NOT NULL) AND (generated_job_commit_sha IS NOT NULL) AND (generated_blob_oid IS NOT NULL) AND (generated_byte_size IS NOT NULL) AND (generated_content_sha256 IS NOT NULL) AND (generated_media_type = 'text/markdown; charset=utf-8'::text) AND (generated_at IS NOT NULL)))),
    CONSTRAINT approval_review_runs_generated_job_commit_sha_check CHECK (((generated_job_commit_sha IS NULL) OR (generated_job_commit_sha ~ '^([0-9a-f]{40}|[0-9a-f]{64})$'::text))),
    CONSTRAINT approval_review_runs_generated_media_type_check CHECK (((generated_media_type IS NULL) OR (generated_media_type = 'text/markdown; charset=utf-8'::text))),
    CONSTRAINT approval_review_runs_generated_result_check CHECK (((generated_result IS NULL) OR (generated_result = ANY (ARRAY['approved'::text, 'rejected'::text])))),
    CONSTRAINT approval_review_runs_publication_group_check CHECK ((((published_source_heads IS NULL) AND (artifact_commit_sha IS NULL) AND (artifact_blob_oid IS NULL) AND (artifact_byte_size IS NULL) AND (artifact_content_sha256 IS NULL) AND (artifact_media_type IS NULL) AND (publication_operation_id IS NULL) AND (published_at IS NULL)) OR ((published_source_heads IS NOT NULL) AND (artifact_commit_sha IS NOT NULL) AND (artifact_blob_oid IS NOT NULL) AND (artifact_byte_size IS NOT NULL) AND (artifact_content_sha256 IS NOT NULL) AND (artifact_media_type = 'text/markdown; charset=utf-8'::text) AND (publication_operation_id IS NOT NULL) AND (published_at IS NOT NULL)))),
    CONSTRAINT approval_review_runs_published_source_heads_check CHECK (((published_source_heads IS NULL) OR public.approval_review_head_vector_v2_is_valid(published_source_heads))),
    CONSTRAINT approval_review_runs_request_key_check CHECK ((request_key ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT approval_review_runs_resolution_id_check CHECK (((resolution_id IS NULL) OR (resolution_id ~ '^apr_[0-9a-f]{64}$'::text))),
    CONSTRAINT approval_review_runs_source_heads_check CHECK (((source_heads IS NULL) OR public.approval_review_head_vector_v2_is_valid(source_heads))),
    CONSTRAINT approval_review_runs_state_check CHECK ((state = ANY (ARRAY['pending_assignment'::text, 'assigned'::text, 'generated'::text, 'merged_to_draft'::text, 'settled'::text, 'failed'::text, 'cancelled'::text]))),
    CONSTRAINT approval_review_runs_state_shape_check CHECK ((((state = 'pending_assignment'::text) AND (resolution_id IS NULL) AND (generated_result IS NULL) AND (merged_to_draft_at IS NULL) AND (published_source_heads IS NULL) AND (approval_status_id IS NULL)) OR ((state = 'assigned'::text) AND (resolution_id IS NOT NULL) AND (generated_result IS NULL) AND (merged_to_draft_at IS NULL) AND (published_source_heads IS NULL) AND (approval_status_id IS NULL)) OR ((state = 'generated'::text) AND (resolution_id IS NOT NULL) AND (generated_result IS NOT NULL) AND (merged_to_draft_at IS NULL) AND (published_source_heads IS NULL) AND (approval_status_id IS NULL)) OR ((state = 'merged_to_draft'::text) AND (resolution_id IS NOT NULL) AND (generated_result IS NOT NULL) AND (merged_to_draft_at IS NOT NULL) AND (published_source_heads IS NULL) AND (approval_status_id IS NULL)) OR ((state = 'settled'::text) AND (resolution_id IS NOT NULL) AND (generated_result IS NOT NULL) AND (merged_to_draft_at IS NOT NULL) AND (approval_status_id IS NOT NULL) AND (((published_source_heads IS NOT NULL) AND (generated_blob_oid = artifact_blob_oid) AND (generated_byte_size = artifact_byte_size) AND (generated_content_sha256 = artifact_content_sha256) AND (generated_media_type = artifact_media_type)) OR ((published_source_heads IS NULL) AND (artifact_commit_sha IS NULL) AND (artifact_blob_oid IS NULL) AND (artifact_byte_size IS NULL) AND (artifact_content_sha256 IS NULL) AND (artifact_media_type IS NULL) AND (publication_operation_id IS NULL) AND (published_at IS NULL)))) OR ((state = ANY (ARRAY['failed'::text, 'cancelled'::text])) AND (published_source_heads IS NULL) AND (approval_status_id IS NULL)))),
    CONSTRAINT approval_review_runs_target_heads_check CHECK (((target_heads IS NULL) OR public.approval_review_head_vector_v2_is_valid(target_heads)))
);

ALTER TABLE ONLY public.approval_review_runs FORCE ROW LEVEL SECURITY;

-- TABLE public.approval_statuses
CREATE TABLE public.approval_statuses (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    seq bigint NOT NULL,
    org_id uuid NOT NULL,
    source_workspace_id uuid NOT NULL,
    approval_definition_id uuid NOT NULL,
    approval_definition_version integer NOT NULL,
    outcome text NOT NULL,
    comment text,
    actor_type text NOT NULL,
    actor_user_id uuid,
    actor_task_id uuid,
    reviewed_git_heads jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    agent_job_id uuid,
    agent_approval_run_details jsonb,
    stage text DEFAULT 'finalizing'::text NOT NULL,
    CONSTRAINT approval_statuses_actor_type_check CHECK ((actor_type = ANY (ARRAY['human'::text, 'agent'::text]))),
    CONSTRAINT approval_statuses_agent_comment_size_check CHECK (((actor_type <> 'agent'::text) OR (comment IS NULL) OR (octet_length(comment) <= 4096))),
    CONSTRAINT approval_statuses_agent_evidence_check CHECK (((actor_type <> 'agent'::text) OR ((outcome = ANY (ARRAY['approved'::text, 'rejected'::text])) AND ((((agent_approval_run_details -> 'schemaVersion'::text) = '1'::jsonb) AND public.approval_review_head_vector_v1_is_valid(reviewed_git_heads) AND (jsonb_typeof((agent_approval_run_details -> 'artifact'::text)) = 'object'::text)) OR (((agent_approval_run_details -> 'schemaVersion'::text) = '2'::jsonb) AND public.approval_review_head_vector_v1_is_canonical(reviewed_git_heads) AND (jsonb_typeof((agent_approval_run_details -> 'artifact'::text)) = 'object'::text)) OR (((agent_approval_run_details -> 'schemaVersion'::text) = '3'::jsonb) AND public.approval_review_head_vector_v2_is_valid(reviewed_git_heads) AND (jsonb_typeof((agent_approval_run_details -> 'generatedArtifact'::text)) = 'object'::text) AND (jsonb_typeof((agent_approval_run_details -> 'acceptedArtifact'::text)) = 'object'::text) AND (jsonb_typeof((agent_approval_run_details -> 'publishedSourceHeads'::text)) = 'array'::text)) OR (((agent_approval_run_details -> 'schemaVersion'::text) = '4'::jsonb) AND public.approval_review_head_vector_v2_is_valid(reviewed_git_heads) AND (jsonb_typeof((agent_approval_run_details -> 'generatedArtifact'::text)) = 'object'::text) AND (jsonb_typeof((agent_approval_run_details -> 'draftProjection'::text)) = 'object'::text))) AND ((agent_approval_run_details ->> 'validationDisposition'::text) = 'valid'::text) AND ((agent_approval_run_details -> 'failure'::text) = 'null'::jsonb)) OR ((outcome = 'deferred'::text) AND (public.approval_review_head_vector_v1_is_valid(reviewed_git_heads) OR public.approval_review_head_vector_v2_is_valid(reviewed_git_heads)) AND (jsonb_typeof((agent_approval_run_details -> 'failure'::text)) = 'object'::text)))),
    CONSTRAINT approval_statuses_agent_run_details_shape_check CHECK (((agent_approval_run_details IS NULL) OR public.agent_approval_run_details_is_valid(agent_approval_run_details) OR public.agent_approval_run_details_v3_is_valid(agent_approval_run_details) OR public.agent_approval_run_details_v4_is_valid(agent_approval_run_details))),
    CONSTRAINT approval_statuses_comment_check CHECK (((comment IS NULL) OR (btrim(comment) <> ''::text))),
    CONSTRAINT approval_statuses_outcome_check CHECK ((outcome = ANY (ARRAY['approved'::text, 'rejected'::text, 'deferred'::text]))),
    CONSTRAINT approval_statuses_reviewed_git_heads_check CHECK ((jsonb_typeof(reviewed_git_heads) = 'array'::text)),
    CONSTRAINT approval_statuses_stage_check CHECK ((stage = ANY (ARRAY['planning'::text, 'finalizing'::text]))),
    CONSTRAINT chk_actor_shape CHECK ((((actor_type = 'human'::text) AND (actor_user_id IS NOT NULL) AND (actor_task_id IS NULL) AND (agent_job_id IS NULL) AND (agent_approval_run_details IS NULL)) OR ((actor_type = 'agent'::text) AND (actor_user_id IS NULL) AND (actor_task_id IS NULL) AND (agent_approval_run_details IS NOT NULL) AND ((((agent_approval_run_details -> 'schemaVersion'::text) = '1'::jsonb) AND (agent_job_id IS NOT NULL)) OR (((agent_approval_run_details -> 'schemaVersion'::text) = ANY (ARRAY['2'::jsonb, '3'::jsonb, '4'::jsonb])) AND (agent_job_id IS NULL))))))
);

ALTER TABLE ONLY public.approval_statuses FORCE ROW LEVEL SECURITY;

-- SEQUENCE public.approval_statuses_seq_seq
ALTER TABLE public.approval_statuses ALTER COLUMN seq ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME public.approval_statuses_seq_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);

-- TABLE public.branch_integration_queue
CREATE TABLE public.branch_integration_queue (
    workspace_branch_repo_id uuid NOT NULL,
    org_id uuid NOT NULL,
    requested_at timestamp with time zone DEFAULT now() NOT NULL,
    trigger_kind text NOT NULL,
    attempts integer DEFAULT 0 NOT NULL,
    last_attempt_at timestamp with time zone,
    last_error text,
    locked_by uuid,
    locked_until timestamp with time zone,
    -- LEAN-TODO(P1.5): remote-tracking merge handoff ordering still reads this operation.
    advance_id uuid,
    CONSTRAINT branch_integration_queue_trigger_kind_check CHECK ((trigger_kind = ANY (ARRAY['commit'::text, 'manual'::text, 'workspace_open'::text, 'background_inbound'::text, 'push'::text, 'approved_rebase_publish'::text])))
);

ALTER TABLE ONLY public.branch_integration_queue FORCE ROW LEVEL SECURITY;

-- TABLE public.branch_integration_remote_pair_leases
CREATE TABLE public.branch_integration_remote_pair_leases (
    repo_id uuid NOT NULL,
    remote_branch text NOT NULL,
    org_id uuid NOT NULL,
    owner_token uuid NOT NULL,
    lease_generation bigint NOT NULL,
    lease_expires_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT branch_integration_remote_pair_leases_lease_generation_check CHECK ((lease_generation > 0)),
    CONSTRAINT branch_integration_remote_pair_leases_remote_branch_check CHECK ((remote_branch <> ''::text))
);

ALTER TABLE ONLY public.branch_integration_remote_pair_leases FORCE ROW LEVEL SECURITY;

-- TABLE public.branch_sync_runs
CREATE TABLE public.branch_sync_runs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    workspace_branch_repo_id uuid NOT NULL,
    acting_user_id uuid NOT NULL,
    kind text NOT NULL,
    trigger_kind text NOT NULL,
    status text NOT NULL,
    old_workspace_tip text,
    new_workspace_tip text,
    old_mirror_tip text,
    new_mirror_tip text,
    new_baseline text,
    advisory_sha text,
    error_detail text,
    merge_run_id uuid,
    started_at timestamp with time zone DEFAULT now() NOT NULL,
    completed_at timestamp with time zone,
    CONSTRAINT branch_sync_runs_kind_check CHECK ((kind = ANY (ARRAY['integration'::text, 'advisory'::text]))),
    CONSTRAINT branch_sync_runs_status_check CHECK ((status = ANY (ARRAY['noop'::text, 'imported'::text, 'pushed'::text, 'diverged'::text, 'dirty_skipped'::text, 'advisory_updated'::text, 'auth_required'::text, 'push_failed'::text, 'fetch_failed'::text]))),
    CONSTRAINT branch_sync_runs_trigger_kind_check CHECK ((trigger_kind = ANY (ARRAY['commit'::text, 'manual'::text, 'workspace_open'::text, 'ping'::text, 'webhook'::text, 'background_inbound'::text, 'push'::text, 'approved_rebase_publish'::text, 'base_auto_pull'::text])))
);

ALTER TABLE ONLY public.branch_sync_runs FORCE ROW LEVEL SECURITY;

-- TABLE public.builder_task_specs
CREATE TABLE public.builder_task_specs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    task_id uuid,
    family text NOT NULL,
    golden_key text NOT NULL,
    status text DEFAULT 'queued'::text NOT NULL,
    failure_reason text,
    run_nonce uuid DEFAULT gen_random_uuid() NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT builder_task_specs_failure_reason_check CHECK ((failure_reason = ANY (ARRAY['build_failed'::text, 'hygiene_failed'::text, 'receipt_invalid'::text, 'nonce_mismatch'::text, 'seal_failed'::text, 'timed_out'::text, 'worker_lost'::text, 'capacity_denied'::text, 'cancelled'::text]))),
    CONSTRAINT builder_task_specs_failure_reason_presence CHECK (((status = 'failed'::text) = (failure_reason IS NOT NULL))),
    CONSTRAINT builder_task_specs_family_check CHECK ((btrim(family) <> ''::text)),
    CONSTRAINT builder_task_specs_golden_key_check CHECK ((golden_key ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT builder_task_specs_status_check CHECK ((status = ANY (ARRAY['queued'::text, 'dispatched'::text, 'building'::text, 'sealing'::text, 'succeeded'::text, 'failed'::text, 'cancelled'::text])))
);

-- TABLE public.chat_attachments
CREATE TABLE public.chat_attachments (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    session_id uuid NOT NULL,
    message_id uuid,
    created_by uuid,
    file_name text NOT NULL,
    content_type text,
    size_bytes bigint NOT NULL,
    sha256 text NOT NULL,
    storage_key text NOT NULL,
    provider_mount_path text NOT NULL,
    scan_status text DEFAULT 'pending'::text NOT NULL,
    content_bytes bytea NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT chat_attachments_content_bytes_bounded CHECK ((octet_length(content_bytes) <= 5242880)),
    CONSTRAINT chat_attachments_content_type_length CHECK (((content_type IS NULL) OR (char_length(content_type) <= 255))),
    CONSTRAINT chat_attachments_file_name_length CHECK ((char_length(file_name) <= 255)),
    CONSTRAINT chat_attachments_file_name_not_empty CHECK ((btrim(file_name) <> ''::text)),
    CONSTRAINT chat_attachments_scan_status_check CHECK ((scan_status = ANY (ARRAY['pending'::text, 'clean'::text, 'blocked'::text, 'failed'::text]))),
    CONSTRAINT chat_attachments_size_bounded CHECK (((size_bytes >= 0) AND (size_bytes <= 5242880)))
);

ALTER TABLE ONLY public.chat_attachments FORCE ROW LEVEL SECURITY;

-- TABLE public.chat_message_context_resolutions
CREATE TABLE public.chat_message_context_resolutions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    target_session_id uuid NOT NULL,
    target_user_message_id uuid NOT NULL,
    target_assistant_message_id uuid NOT NULL,
    context_link_id uuid NOT NULL,
    source_session_id uuid NOT NULL,
    mode text NOT NULL,
    from_message_index_exclusive bigint,
    through_message_index bigint,
    source_digest text NOT NULL,
    resolved_messages jsonb NOT NULL,
    resolution_kind text NOT NULL,
    message_count integer NOT NULL,
    byte_count integer NOT NULL,
    truncated boolean NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT chat_message_context_resolutions_byte_count_check CHECK ((byte_count >= 0)),
    CONSTRAINT chat_message_context_resolutions_message_count_check CHECK ((message_count >= 0)),
    CONSTRAINT chat_message_context_resolutions_mode_check CHECK ((mode = ANY (ARRAY['snapshot'::text, 'live'::text]))),
    CONSTRAINT chat_message_context_resolutions_resolution_kind_check CHECK ((resolution_kind = ANY (ARRAY['baseline'::text, 'delta'::text, 'unchanged'::text, 'full_refresh'::text, 'snapshot'::text, 'removal_notice'::text])))
);

ALTER TABLE ONLY public.chat_message_context_resolutions FORCE ROW LEVEL SECURITY;

-- TABLE public.chat_messages
CREATE TABLE public.chat_messages (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    session_id uuid NOT NULL,
    role text NOT NULL,
    content text NOT NULL,
    artifacts jsonb,
    citations jsonb DEFAULT '[]'::jsonb NOT NULL,
    input_tokens integer,
    output_tokens integer,
    status text DEFAULT 'complete'::text NOT NULL,
    message_index integer NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    org_id uuid NOT NULL,
    CONSTRAINT chat_messages_role_check CHECK ((role = ANY (ARRAY['user'::text, 'assistant'::text, 'system'::text, 'tool'::text]))),
    CONSTRAINT chat_messages_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'streaming'::text, 'complete'::text, 'error'::text, 'stopped'::text])))
);

ALTER TABLE ONLY public.chat_messages FORCE ROW LEVEL SECURITY;

-- TABLE public.chat_session_context_links
CREATE TABLE public.chat_session_context_links (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    target_session_id uuid NOT NULL,
    source_session_id uuid NOT NULL,
    mode text NOT NULL,
    status text DEFAULT 'active'::text NOT NULL,
    snapshot_through_message_index bigint,
    snapshot_payload jsonb,
    snapshot_digest text,
    last_resolved_message_index bigint,
    last_resolved_digest text,
    last_resolved_at timestamp with time zone,
    created_by uuid NOT NULL,
    removed_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    removed_at timestamp with time zone,
    include_agent_activity boolean DEFAULT false NOT NULL,
    CONSTRAINT chat_session_context_links_mode_check CHECK ((mode = ANY (ARRAY['snapshot'::text, 'live'::text]))),
    CONSTRAINT chat_session_context_links_mode_payload_consistency CHECK ((((mode = 'live'::text) AND (snapshot_payload IS NULL) AND (snapshot_through_message_index IS NULL) AND (snapshot_digest IS NULL)) OR ((mode = 'snapshot'::text) AND (status = 'active'::text) AND (snapshot_payload IS NOT NULL) AND (snapshot_through_message_index IS NOT NULL) AND (snapshot_digest IS NOT NULL)) OR ((mode = 'snapshot'::text) AND (status = 'removed'::text)))),
    CONSTRAINT chat_session_context_links_no_self_reference CHECK ((source_session_id <> target_session_id)),
    CONSTRAINT chat_session_context_links_status_check CHECK ((status = ANY (ARRAY['active'::text, 'removed'::text])))
);

ALTER TABLE ONLY public.chat_session_context_links FORCE ROW LEVEL SECURITY;

-- TABLE public.chat_sessions
CREATE TABLE public.chat_sessions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid NOT NULL,
    session_type text DEFAULT 'chat'::text NOT NULL,
    task_id uuid,
    scope_type text DEFAULT 'workspace'::text NOT NULL,
    scope_ref text,
    model text,
    title text,
    summary text,
    tags text[] DEFAULT '{}'::text[] NOT NULL,
    linked_commits text[] DEFAULT '{}'::text[] NOT NULL,
    linked_nodes text[] DEFAULT '{}'::text[] NOT NULL,
    total_input_tokens bigint DEFAULT 0 NOT NULL,
    total_output_tokens bigint DEFAULT 0 NOT NULL,
    total_cost_usd numeric(10,4) DEFAULT 0 NOT NULL,
    status text DEFAULT 'active'::text NOT NULL,
    message_count integer DEFAULT 0 NOT NULL,
    forked_from_id uuid,
    forked_at_index integer,
    branch text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    org_id uuid NOT NULL,
    -- P3.5 begin
    agent_provider text DEFAULT 'claude_code'::text NOT NULL,
    -- P3.5 end
    required_worker_id text,
    provider_state jsonb DEFAULT '{}'::jsonb NOT NULL,
    branch_mode text DEFAULT 'workspace_head'::text NOT NULL,
    created_by uuid NOT NULL,
    attach_preview_task_id uuid,
    -- P3.5 begin
    CONSTRAINT chat_sessions_agent_provider_check CHECK ((agent_provider = ANY (ARRAY['claude_code'::text, 'codex'::text]))),
    -- P3.5 end
    CONSTRAINT chat_sessions_branch_mode_check CHECK ((branch_mode = ANY (ARRAY['workspace_head'::text, 'job_branch'::text]))),
    CONSTRAINT chat_sessions_scope_type_check CHECK ((scope_type = ANY (ARRAY['workspace'::text, 'node'::text, 'task'::text]))),
    CONSTRAINT chat_sessions_session_type_check CHECK ((session_type = ANY (ARRAY['chat'::text, 'task'::text]))),
    CONSTRAINT chat_sessions_status_check CHECK ((status = ANY (ARRAY['active'::text, 'completed'::text, 'archived'::text])))
);

ALTER TABLE ONLY public.chat_sessions FORCE ROW LEVEL SECURITY;

-- TABLE public.chat_sticky_runtimes
CREATE TABLE public.chat_sticky_runtimes (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    chat_session_id uuid NOT NULL,
    task_id uuid NOT NULL,
    worker_id text NOT NULL,
    host_id uuid,
    microvm_id text,
    provider text NOT NULL,
    status text NOT NULL,
    idle_since timestamp with time zone,
    expires_at timestamp with time zone,
    last_liveness_at timestamp with time zone DEFAULT now() NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    pending_question_since timestamp with time zone,
    pending_question_expires_at timestamp with time zone,
    lost_reason text,
    busy_pending_count integer,
    busy_reported_at timestamp with time zone,
    teardown_generation bigint DEFAULT 0 NOT NULL,
    teardown_operation_id uuid,
    teardown_requested_at timestamp with time zone,
    teardown_deadline_at timestamp with time zone,
    teardown_repository_scopes jsonb,
    teardown_disposition text,
    teardown_outcome text,
    teardown_evidence jsonb,
    teardown_acknowledged_at timestamp with time zone,
    warm_resume_claimed_at timestamp with time zone,
    worker_session_generation bigint,
    teardown_scope_provenance jsonb,
    teardown_workspace_released_at timestamp with time zone,
    worker_workspace_root_lease_v1 boolean DEFAULT false NOT NULL,
    teardown_reconciliation_kind text,
    teardown_retired_worker_session_generation bigint,
    teardown_reconciliation_receipt jsonb,
    teardown_delivery_started_at timestamp with time zone,
    host_discard_fence_receipt jsonb,
    CONSTRAINT chat_sticky_runtimes_host_discard_fence_receipt_object CHECK (((host_discard_fence_receipt IS NULL) OR (jsonb_typeof(host_discard_fence_receipt) = 'object'::text))),
    CONSTRAINT chat_sticky_runtimes_provider_check CHECK ((provider = ANY (ARRAY['claude_code'::text, 'codex'::text]))),
    CONSTRAINT chat_sticky_runtimes_status_check CHECK ((status = ANY (ARRAY['active'::text, 'reaping'::text, 'expired'::text, 'lost'::text]))),
    CONSTRAINT chat_sticky_runtimes_teardown_scope_provenance_check CHECK (((teardown_scope_provenance IS NULL) OR (jsonb_typeof(teardown_scope_provenance) = 'array'::text))),
    CONSTRAINT chk_chat_sticky_runtime_worker_generation_positive CHECK (((worker_session_generation IS NULL) OR (worker_session_generation > 0))),
    CONSTRAINT chk_chat_sticky_runtimes_lost_reason CHECK (((lost_reason IS NULL) OR (lost_reason = ANY (ARRAY['worker_terminated'::text, 'failed_redeposit'::text, 'stranded_recovery'::text, 'stale_liveness'::text, 'expired_claimed_redelivery'::text])))),
    CONSTRAINT chk_chat_sticky_teardown_ack_pair CHECK ((((teardown_disposition IS NULL) AND (teardown_outcome IS NULL) AND (teardown_evidence IS NULL) AND (teardown_acknowledged_at IS NULL)) OR ((teardown_disposition IS NOT NULL) AND (teardown_outcome IS NOT NULL) AND (teardown_evidence IS NOT NULL) AND (teardown_acknowledged_at IS NOT NULL) AND (teardown_operation_id IS NOT NULL)))),
    CONSTRAINT chk_chat_sticky_teardown_deadline CHECK (((teardown_operation_id IS NULL) OR (teardown_deadline_at = (teardown_requested_at + '00:00:30'::interval)))),
    CONSTRAINT chk_chat_sticky_teardown_delivery_started CHECK (((teardown_delivery_started_at IS NULL) OR ((teardown_operation_id IS NOT NULL) AND (teardown_requested_at IS NOT NULL) AND (teardown_delivery_started_at = teardown_requested_at)))),
    CONSTRAINT chk_chat_sticky_teardown_disposition CHECK (((teardown_disposition IS NULL) OR (teardown_disposition = ANY (ARRAY['durablyFlushed'::text, 'preservedForRecovery'::text])))),
    CONSTRAINT chk_chat_sticky_teardown_operation_pair CHECK ((((teardown_operation_id IS NULL) AND (teardown_requested_at IS NULL) AND (teardown_deadline_at IS NULL) AND (teardown_repository_scopes IS NULL)) OR ((teardown_operation_id IS NOT NULL) AND (teardown_requested_at IS NOT NULL) AND (teardown_deadline_at IS NOT NULL) AND (teardown_repository_scopes IS NOT NULL) AND (jsonb_typeof(teardown_repository_scopes) = 'array'::text)))),
    CONSTRAINT chk_chat_sticky_teardown_outcome CHECK (((teardown_outcome IS NULL) OR (teardown_outcome = ANY (ARRAY['tornDown'::text, 'alreadyAbsent'::text, 'forceEvicted'::text])))),
    CONSTRAINT chk_chat_sticky_teardown_positive_generation CHECK (((teardown_operation_id IS NULL) OR (teardown_generation > 0))),
    CONSTRAINT chk_chat_sticky_teardown_workspace_release CHECK (((teardown_workspace_released_at IS NULL) OR ((teardown_operation_id IS NOT NULL) AND ((teardown_acknowledged_at IS NOT NULL) OR (teardown_reconciliation_receipt IS NOT NULL))))),
    CONSTRAINT chk_sticky_runtime_teardown_reconciliation_kind CHECK ((((teardown_reconciliation_kind IS NULL) AND (teardown_retired_worker_session_generation IS NULL) AND (teardown_reconciliation_receipt IS NULL)) OR ((teardown_reconciliation_kind = ANY (ARRAY['registration_retirement'::text, 'task_workspace_recovery'::text])) AND (teardown_retired_worker_session_generation > 0) AND (teardown_operation_id IS NOT NULL)))),
    CONSTRAINT chk_sticky_runtime_teardown_reconciliation_receipt CHECK (((teardown_reconciliation_receipt IS NULL) OR ((jsonb_typeof(teardown_reconciliation_receipt) = 'object'::text) AND (teardown_workspace_released_at IS NOT NULL))))
);

ALTER TABLE ONLY public.chat_sticky_runtimes FORCE ROW LEVEL SECURITY;

-- COMMENT public.COLUMN chat_sticky_runtimes.teardown_scope_provenance
COMMENT ON COLUMN public.chat_sticky_runtimes.teardown_scope_provenance IS 'API-private per-scope teardown provenance ({workspaceBranchRepoId, crdtSessionId, headCommit, operationKind}), identity-aligned with teardown_repository_scopes; NULL = legacy capture, fails closed';

-- COMMENT public.COLUMN chat_sticky_runtimes.worker_workspace_root_lease_v1
COMMENT ON COLUMN public.chat_sticky_runtimes.worker_workspace_root_lease_v1 IS 'The worker session that created this runtime positively advertised the process-lifetime workspace-root lease contract';

-- COMMENT public.COLUMN chat_sticky_runtimes.teardown_reconciliation_kind
COMMENT ON COLUMN public.chat_sticky_runtimes.teardown_reconciliation_kind IS 'Cleanup-only reconciliation class for a typed teardown operation; registration_retirement and task_workspace_recovery are separate from ordinary teardown ACK evidence';

-- COMMENT public.COLUMN chat_sticky_runtimes.teardown_retired_worker_session_generation
COMMENT ON COLUMN public.chat_sticky_runtimes.teardown_retired_worker_session_generation IS 'Explicit positive worker-session generation retired by this registration-reconciliation marker';

-- COMMENT public.COLUMN chat_sticky_runtimes.teardown_reconciliation_receipt
COMMENT ON COLUMN public.chat_sticky_runtimes.teardown_reconciliation_receipt IS 'Exact positive workspace-custody/quarantine receipt accepted for the current reconciliation operation; NULL until release';

-- COMMENT public.COLUMN chat_sticky_runtimes.teardown_delivery_started_at
COMMENT ON COLUMN public.chat_sticky_runtimes.teardown_delivery_started_at IS 'One-shot API-private receipt for first authorized teardown instruction listing; NULL means the bounded delivery window has not yet been armed';

-- COMMENT public.COLUMN chat_sticky_runtimes.host_discard_fence_receipt
COMMENT ON COLUMN public.chat_sticky_runtimes.host_discard_fence_receipt IS 'Immutable grant-time host composition receipt; NULL is legacy/direct-worker evidence.';

-- TABLE public.cli_device_authorizations
CREATE TABLE public.cli_device_authorizations (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    device_code_hash text NOT NULL,
    user_code text NOT NULL,
    workspace_id uuid,
    org_id uuid NOT NULL,
    device_name text,
    status text DEFAULT 'pending'::text NOT NULL,
    user_id uuid,
    expires_at timestamp with time zone NOT NULL,
    approved_at timestamp with time zone,
    consumed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT cli_device_authorizations_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'approved'::text, 'denied'::text, 'consumed'::text, 'expired'::text])))
);

-- TABLE public.cloud_orphan_sightings
CREATE TABLE public.cloud_orphan_sightings (
    instance_name text NOT NULL,
    zone text NOT NULL,
    host_id uuid,
    first_seen timestamp with time zone DEFAULT now() NOT NULL,
    last_seen timestamp with time zone DEFAULT now() NOT NULL
);

-- COMMENT public.TABLE cloud_orphan_sightings
COMMENT ON TABLE public.cloud_orphan_sightings IS 'Stale-host detector Pass 3 ledger ([D65], #1464): first/last sighting of a RUNNING g5-host-* instance whose worker_hosts row is terminated or absent. Reap requires first_seen older than G5_CLOUD_ORPHAN_GRACE_SECS; a revived row clears the sighting. System-owned; no tenant rows; no RLS.';

-- TABLE public.comment_index
CREATE TABLE public.comment_index (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid NOT NULL,
    author_id uuid NOT NULL,
    scope text NOT NULL,
    node_id uuid,
    file_path text,
    line_number integer,
    commit_sha text,
    commit_range_base text,
    commit_range_head text,
    content text NOT NULL,
    resolved boolean DEFAULT false NOT NULL,
    resolved_at timestamp with time zone,
    origin_branch text NOT NULL,
    branch_deleted boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    org_id uuid NOT NULL,
    repo_id uuid,
    CONSTRAINT comment_index_node_has_no_repo CHECK (((scope <> 'node'::text) OR (repo_id IS NULL))),
    CONSTRAINT comment_index_scope_check CHECK ((scope = ANY (ARRAY['line'::text, 'node'::text])))
);

ALTER TABLE ONLY public.comment_index FORCE ROW LEVEL SECURITY;

-- TABLE public.comment_mentions
CREATE TABLE public.comment_mentions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid NOT NULL,
    comment_id uuid,
    reply_id uuid,
    mentioned_user_id uuid NOT NULL,
    mentioned_by_user_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

-- TABLE public.comment_replies
CREATE TABLE public.comment_replies (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    comment_id uuid NOT NULL,
    author_id uuid NOT NULL,
    content text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    org_id uuid NOT NULL
);

ALTER TABLE ONLY public.comment_replies FORCE ROW LEVEL SECURITY;

-- TABLE public.doc_folders
CREATE TABLE public.doc_folders (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    workspace_branch_id uuid NOT NULL,
    workspace_branch_repo_id uuid NOT NULL,
    path text NOT NULL,
    created_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT doc_folders_path_not_empty CHECK ((btrim(path) <> ''::text))
);

ALTER TABLE ONLY public.doc_folders FORCE ROW LEVEL SECURITY;

-- TABLE public.doc_node_links
CREATE TABLE public.doc_node_links (
    doc_id uuid NOT NULL,
    node_id uuid NOT NULL,
    org_id uuid NOT NULL
);

ALTER TABLE ONLY public.doc_node_links FORCE ROW LEVEL SECURITY;

-- TABLE public.docs
CREATE TABLE public.docs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid NOT NULL,
    name text NOT NULL,
    doc_type text DEFAULT 'design'::text NOT NULL,
    file_path text,
    last_uplifted_at timestamp with time zone,
    last_downlifted_at timestamp with time zone,
    content_updated_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    org_id uuid NOT NULL,
    content text,
    content_hash text,
    last_uplifted_commit_sha text,
    last_downlifted_commit_sha text,
    persona text,
    instruction text,
    scope_mode text DEFAULT 'root'::text,
    scope_node_id uuid,
    is_future boolean DEFAULT false,
    review_iterations smallint DEFAULT 2,
    custom_persona_label text,
    source_files jsonb,
    linked_doc_paths jsonb,
    mapping_content text,
    workspace_branch_id uuid,
    workspace_branch_repo_id uuid,
    document_source text,
    CONSTRAINT docs_document_source_check CHECK (((document_source IS NULL) OR (document_source = ANY (ARRAY['user'::text, 'auxiliary'::text])))),
    CONSTRAINT docs_persona_check CHECK (((persona IS NULL) OR (persona = ANY (ARRAY['pm'::text, 'arch'::text, 'security'::text, 'performance'::text, 'test'::text, 'ui'::text, 'custom'::text])))),
    CONSTRAINT docs_scope_mode_check CHECK (((scope_mode IS NULL) OR (scope_mode = ANY (ARRAY['root'::text, 'node'::text]))))
);

ALTER TABLE ONLY public.docs FORCE ROW LEVEL SECURITY;

-- COMMENT public.COLUMN docs.linked_doc_paths
COMMENT ON COLUMN public.docs.linked_doc_paths IS 'JSON array of workspace-relative file paths to linked documents (views, designs, etc.)';

-- TABLE public.draft_participants
CREATE TABLE public.draft_participants (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    session_id uuid NOT NULL,
    user_id uuid NOT NULL,
    role text DEFAULT 'editor'::text NOT NULL,
    connected_at timestamp with time zone DEFAULT now() NOT NULL,
    cursor_data jsonb,
    org_id uuid NOT NULL,
    CONSTRAINT draft_participants_role_check CHECK ((role = ANY (ARRAY['editor'::text, 'reviewer'::text])))
);

ALTER TABLE ONLY public.draft_participants FORCE ROW LEVEL SECURITY;

-- TABLE public.draft_sessions
CREATE TABLE public.draft_sessions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_branch_id uuid NOT NULL,
    yjs_state bytea DEFAULT '\x'::bytea NOT NULL,
    compacted_at timestamp with time zone DEFAULT now() NOT NULL,
    compacted_seq bigint DEFAULT 0 NOT NULL,
    pending_updates integer DEFAULT 0 NOT NULL,
    diff_summary jsonb DEFAULT '{"files": [], "totalAdditions": 0, "totalDeletions": 0}'::jsonb NOT NULL,
    dirty_files text[] DEFAULT '{}'::text[] NOT NULL,
    dirty_count integer DEFAULT 0 NOT NULL,
    status text DEFAULT 'active'::text NOT NULL,
    last_activity_at timestamp with time zone DEFAULT now() NOT NULL,
    persisted_at timestamp with time zone DEFAULT now() NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    org_id uuid NOT NULL,
    head_commit text,
    workspace_branch_repo_id uuid NOT NULL,
    task_id uuid,
    pending_update_bytes bigint DEFAULT 0 NOT NULL,
    CONSTRAINT draft_sessions_pending_update_bytes_check CHECK ((pending_update_bytes >= 0)),
    CONSTRAINT draft_sessions_status_check CHECK ((status = ANY (ARRAY['active'::text, 'committing'::text, 'discarding'::text])))
);

ALTER TABLE ONLY public.draft_sessions FORCE ROW LEVEL SECURITY;

-- TABLE public.draft_updates
CREATE TABLE public.draft_updates (
    seq bigint NOT NULL,
    session_id uuid NOT NULL,
    update_data bytea NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    org_id uuid NOT NULL
);

ALTER TABLE ONLY public.draft_updates FORCE ROW LEVEL SECURITY;

-- SEQUENCE public.draft_updates_seq_seq
CREATE SEQUENCE public.draft_updates_seq_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

-- SEQUENCE OWNED BY public.draft_updates_seq_seq
ALTER SEQUENCE public.draft_updates_seq_seq OWNED BY public.draft_updates.seq;

-- TABLE public.hotlist_events
CREATE TABLE public.hotlist_events (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    hotlist_id uuid NOT NULL,
    actor_id uuid,
    kind text NOT NULL,
    payload jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT hotlist_events_kind_check CHECK ((kind = ANY (ARRAY['created'::text, 'renamed'::text, 'archived'::text, 'unarchived'::text, 'issue_added'::text, 'issue_removed'::text, 'position_changed'::text, 'owner_changed'::text])))
);

-- TABLE public.hotlist_memberships
CREATE TABLE public.hotlist_memberships (
    hotlist_id uuid NOT NULL,
    issue_id uuid NOT NULL,
    org_id uuid NOT NULL,
    added_by uuid NOT NULL,
    added_at timestamp with time zone DEFAULT now() NOT NULL,
    "position" text,
    CONSTRAINT hotlist_memberships_position_check CHECK ((("position" IS NULL) OR (char_length("position") <= 128)))
);

-- TABLE public.hotlist_slug_sequence
CREATE TABLE public.hotlist_slug_sequence (
    org_id uuid NOT NULL,
    prefix text DEFAULT 'H'::text NOT NULL,
    next_value bigint DEFAULT 1 NOT NULL
);

-- TABLE public.hotlists
CREATE TABLE public.hotlists (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    slug text NOT NULL,
    name text NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    owner_id uuid NOT NULL,
    archived_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT hotlists_name_check CHECK (((char_length(name) <= 200) AND (char_length(name) > 0)))
);

-- TABLE public.human_escalation_questions
CREATE TABLE public.human_escalation_questions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    escalation_id uuid NOT NULL,
    ordinal integer NOT NULL,
    question_key text NOT NULL,
    title text NOT NULL,
    context text NOT NULL,
    uncertainty text NOT NULL,
    options_to_unblock jsonb NOT NULL,
    uncertainty_severity text NOT NULL,
    recommended_option text,
    supporting_evidence jsonb,
    status text DEFAULT 'open'::text NOT NULL,
    response_text text,
    selected_option text,
    rationale text,
    decision_source text,
    resolved_by_user_id uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    resolved_at timestamp with time zone,
    CONSTRAINT human_escalation_questions_context_check CHECK ((btrim(context) <> ''::text)),
    CONSTRAINT human_escalation_questions_decision_source_check CHECK ((decision_source = ANY (ARRAY['human'::text, 'supervisor_agent'::text, 'experience_match'::text]))),
    CONSTRAINT human_escalation_questions_ordinal_check CHECK ((ordinal > 0)),
    CONSTRAINT human_escalation_questions_question_key_check CHECK ((btrim(question_key) <> ''::text)),
    CONSTRAINT human_escalation_questions_status_check CHECK ((status = ANY (ARRAY['open'::text, 'resolved'::text, 'cancelled'::text]))),
    CONSTRAINT human_escalation_questions_title_check CHECK ((btrim(title) <> ''::text)),
    CONSTRAINT human_escalation_questions_uncertainty_check CHECK ((btrim(uncertainty) <> ''::text)),
    CONSTRAINT human_escalation_questions_uncertainty_severity_check CHECK ((uncertainty_severity = ANY (ARRAY['low'::text, 'medium'::text, 'high'::text, 'critical'::text])))
);

ALTER TABLE ONLY public.human_escalation_questions FORCE ROW LEVEL SECURITY;

-- TABLE public.human_escalations
CREATE TABLE public.human_escalations (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid NOT NULL,
    job_id uuid NOT NULL,
    task_id uuid NOT NULL,
    scope text NOT NULL,
    status text DEFAULT 'open'::text NOT NULL,
    context text NOT NULL,
    uncertainty text NOT NULL,
    options_to_unblock jsonb NOT NULL,
    uncertainty_severity text NOT NULL,
    recommended_option text,
    supporting_evidence jsonb,
    created_by_session_id uuid,
    resolved_by_user_id uuid,
    resolution_summary text,
    decision_source text,
    routing_decision text DEFAULT 'to_human'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    resolved_at timestamp with time zone,
    resolution_confidence double precision,
    resolution_rationale text,
    org_id uuid NOT NULL,
    CONSTRAINT human_escalations_decision_source_check CHECK ((decision_source = ANY (ARRAY['human'::text, 'supervisor_agent'::text, 'experience_match'::text]))),
    CONSTRAINT human_escalations_routing_decision_check CHECK ((routing_decision = ANY (ARRAY['to_human'::text, 'to_supervisor'::text]))),
    CONSTRAINT human_escalations_scope_check CHECK ((scope = ANY (ARRAY['task'::text, 'job'::text]))),
    CONSTRAINT human_escalations_status_check CHECK ((status = ANY (ARRAY['open'::text, 'in_review'::text, 'resolved'::text, 'cancelled'::text]))),
    CONSTRAINT human_escalations_uncertainty_severity_check CHECK ((uncertainty_severity = ANY (ARRAY['low'::text, 'medium'::text, 'high'::text, 'critical'::text])))
);

ALTER TABLE ONLY public.human_escalations FORCE ROW LEVEL SECURITY;

-- TABLE public.intent_record
CREATE TABLE public.intent_record (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_branch_id uuid NOT NULL,
    org_id uuid NOT NULL,
    task_id uuid NOT NULL,
    file_type text NOT NULL,
    file_name text,
    intent_record_number text,
    code_block_name text,
    chat_session_id uuid,
    chat_session_name text,
    chat_context text,
    user_content text,
    is_human boolean DEFAULT false NOT NULL,
    priority smallint,
    certainty smallint,
    created_by text,
    last_updated_by text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT intent_record_certainty_check CHECK (((certainty IS NULL) OR ((certainty >= 0) AND (certainty <= 100)))),
    CONSTRAINT intent_record_file_type_check CHECK ((file_type = ANY (ARRAY['view'::text, 'ontology'::text, 'code'::text, 'chat'::text]))),
    CONSTRAINT intent_record_priority_check CHECK (((priority IS NULL) OR ((priority >= 0) AND (priority <= 100))))
);

-- TABLE public.issue_attachments
CREATE TABLE public.issue_attachments (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    issue_id uuid NOT NULL,
    comment_id uuid,
    uploader_id uuid NOT NULL,
    file_name text NOT NULL,
    content_type text NOT NULL,
    byte_size bigint NOT NULL,
    sha256 text NOT NULL,
    storage_key text NOT NULL,
    image_width integer,
    image_height integer,
    scan_status text DEFAULT 'pending'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    deleted_at timestamp with time zone,
    CONSTRAINT issue_attachments_byte_size_check CHECK ((byte_size > 0)),
    CONSTRAINT issue_attachments_content_type_check CHECK (((length(content_type) >= 1) AND (length(content_type) <= 255))),
    CONSTRAINT issue_attachments_file_name_check CHECK (((length(file_name) >= 1) AND (length(file_name) <= 255))),
    CONSTRAINT issue_attachments_image_height_check CHECK (((image_height IS NULL) OR (image_height > 0))),
    CONSTRAINT issue_attachments_image_width_check CHECK (((image_width IS NULL) OR (image_width > 0))),
    CONSTRAINT issue_attachments_scan_status_check CHECK ((scan_status = ANY (ARRAY['pending'::text, 'clean'::text, 'blocked'::text, 'failed'::text]))),
    CONSTRAINT issue_attachments_sha256_check CHECK ((sha256 ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT issue_attachments_storage_key_check CHECK (((length(storage_key) >= 1) AND (length(storage_key) <= 1024)))
);

-- TABLE public.issue_comments
CREATE TABLE public.issue_comments (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    issue_id uuid NOT NULL,
    author_id uuid NOT NULL,
    body text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT issue_comments_body_check CHECK ((char_length(body) > 0))
);

-- TABLE public.issue_components
CREATE TABLE public.issue_components (
    issue_id uuid NOT NULL,
    ontology_node_id uuid NOT NULL,
    relevance real DEFAULT 0.6 NOT NULL,
    auto_inferred boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    inference_reason text,
    CONSTRAINT issue_components_relevance_check CHECK (((relevance >= (0.0)::double precision) AND (relevance <= (1.0)::double precision)))
);

-- TABLE public.issue_events
CREATE TABLE public.issue_events (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    issue_id uuid NOT NULL,
    actor_id uuid,
    kind text NOT NULL,
    payload jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT issue_events_kind_check CHECK ((kind = ANY (ARRAY['created'::text, 'state_changed'::text, 'assigned'::text, 'priority_changed'::text, 'severity_changed'::text, 'linked'::text, 'unlinked'::text, 'commented'::text, 'component_added'::text, 'component_removed'::text, 'attachment_added'::text, 'attachment_removed'::text, 'synthetic_anchor_failed'::text, 'grounding_failed'::text, 'discovery_scheduling_failed'::text, 'merged'::text, 'reopened'::text, 'hotlist_added'::text, 'hotlist_removed'::text, 'synced_in'::text, 'synced_out'::text])))
);

-- TABLE public.issue_inference_jobs
CREATE TABLE public.issue_inference_jobs (
    issue_id uuid NOT NULL,
    org_id uuid NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    attempts integer DEFAULT 0 NOT NULL,
    next_run_at timestamp with time zone DEFAULT now() NOT NULL,
    locked_at timestamp with time zone,
    last_error text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    scheduled_job_id uuid,
    scheduled_task_id uuid,
    CONSTRAINT issue_inference_jobs_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'processing'::text, 'completed'::text, 'failed'::text])))
);

-- TABLE public.issue_job_links
CREATE TABLE public.issue_job_links (
    issue_id uuid NOT NULL,
    job_id uuid NOT NULL,
    role text NOT NULL,
    CONSTRAINT issue_job_links_role_check CHECK ((role = ANY (ARRAY['investigates'::text, 'addresses'::text, 'closes'::text])))
);

-- TABLE public.issue_labels
CREATE TABLE public.issue_labels (
    issue_id uuid NOT NULL,
    label text NOT NULL,
    CONSTRAINT issue_labels_label_check CHECK (((char_length(label) <= 64) AND (char_length(label) > 0)))
);

-- TABLE public.issue_links
CREATE TABLE public.issue_links (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    src_issue_id uuid NOT NULL,
    relation text NOT NULL,
    dst_issue_id uuid NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT issue_links_no_self CHECK ((src_issue_id <> dst_issue_id)),
    CONSTRAINT issue_links_relation_check CHECK ((relation = ANY (ARRAY['blocks'::text, 'relates_to'::text, 'duplicates'::text, 'caused_by'::text, 'fixed_by'::text, 'triggered_by'::text])))
);

-- TABLE public.issue_slug_sequence
CREATE TABLE public.issue_slug_sequence (
    org_id uuid NOT NULL,
    prefix text DEFAULT 'G5'::text NOT NULL,
    next_value bigint DEFAULT 1 NOT NULL
);

-- TABLE public.issues
CREATE TABLE public.issues (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    slug text NOT NULL,
    kind text NOT NULL,
    title text NOT NULL,
    body text DEFAULT ''::text NOT NULL,
    status text DEFAULT 'triage'::text NOT NULL,
    priority text DEFAULT 'p3'::text NOT NULL,
    severity text,
    reporter_id uuid NOT NULL,
    assignee_id uuid,
    workspace_id uuid,
    repo_id uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    closed_at timestamp with time zone,
    CONSTRAINT issues_kind_check CHECK ((kind = ANY (ARRAY['bug'::text, 'feature_request'::text, 'chore'::text, 'docs'::text, 'spike'::text]))),
    CONSTRAINT issues_priority_check CHECK ((priority = ANY (ARRAY['p0'::text, 'p1'::text, 'p2'::text, 'p3'::text, 'p4'::text]))),
    CONSTRAINT issues_severity_bug_xor CHECK (((kind = 'bug'::text) = (severity IS NOT NULL))),
    CONSTRAINT issues_severity_check CHECK (((severity IS NULL) OR (severity = ANY (ARRAY['s1'::text, 's2'::text, 's3'::text, 's4'::text])))),
    CONSTRAINT issues_status_check CHECK ((status = ANY (ARRAY['triage'::text, 'open'::text, 'in_progress'::text, 'in_review'::text, 'resolved'::text, 'closed'::text, 'wont_fix'::text, 'duplicate'::text]))),
    CONSTRAINT issues_title_check CHECK (((char_length(title) <= 200) AND (char_length(title) > 0)))
);

-- TABLE public.job_merge_artifacts
CREATE TABLE public.job_merge_artifacts (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    job_merge_run_id uuid NOT NULL,
    artifact_type text NOT NULL,
    path text,
    status text DEFAULT 'pending'::text NOT NULL,
    metadata jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT job_merge_artifacts_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'written'::text, 'failed'::text, 'superseded'::text])))
);

ALTER TABLE ONLY public.job_merge_artifacts FORCE ROW LEVEL SECURITY;

-- TABLE public.job_merge_runs
CREATE TABLE public.job_merge_runs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    source_job_id uuid,
    merge_job_id uuid,
    source_branch_name text NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    phase text DEFAULT 'job_rebase_preflight'::text NOT NULL,
    blocking_reason text,
    source_heads jsonb DEFAULT '{}'::jsonb NOT NULL,
    target_heads jsonb DEFAULT '{}'::jsonb NOT NULL,
    pre_start_source_heads jsonb DEFAULT '{}'::jsonb NOT NULL,
    target_checkpoints jsonb DEFAULT '{}'::jsonb NOT NULL,
    confirmed_by uuid,
    started_at timestamp with time zone,
    completed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    source_kind text DEFAULT 'job'::text NOT NULL,
    source_workspace_id uuid,
    target_workspace_id uuid,
    workspace_branch_repo_id uuid,
    advances_target boolean DEFAULT true NOT NULL,
    completion_advance_id uuid,
    CONSTRAINT job_merge_runs_source_kind_check CHECK ((source_kind = ANY (ARRAY['job'::text, 'workspace'::text, 'remote_tracking'::text]))),
    CONSTRAINT job_merge_runs_source_shape_check CHECK ((((source_kind = 'job'::text) AND (source_job_id IS NOT NULL) AND (source_workspace_id IS NOT NULL) AND (target_workspace_id IS NOT NULL) AND (workspace_branch_repo_id IS NULL)) OR ((source_kind = 'workspace'::text) AND (source_job_id IS NULL) AND (source_workspace_id IS NOT NULL) AND (target_workspace_id IS NOT NULL) AND (source_workspace_id <> target_workspace_id) AND (workspace_branch_repo_id IS NULL)) OR ((source_kind = 'remote_tracking'::text) AND (source_job_id IS NULL) AND (workspace_branch_repo_id IS NOT NULL)))),
    CONSTRAINT job_merge_runs_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'running'::text, 'waiting_for_human'::text, 'rolling_back'::text, 'completed'::text, 'failed'::text, 'cancelled'::text])))
);

ALTER TABLE ONLY public.job_merge_runs FORCE ROW LEVEL SECURITY;

-- TABLE public.job_preview_sessions
CREATE TABLE public.job_preview_sessions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    job_id uuid NOT NULL,
    task_id uuid,
    mode text NOT NULL,
    status text NOT NULL,
    guest_port integer,
    host_port integer,
    endpoint_url text,
    expires_at timestamp with time zone,
    started_at timestamp with time zone,
    stopped_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    host_id uuid,
    profile text,
    attach_supported boolean DEFAULT false NOT NULL,
    boot_generation uuid,
    CONSTRAINT job_preview_sessions_mode_check CHECK ((mode = ANY (ARRAY['unmerged'::text, 'merged'::text])))
);

ALTER TABLE ONLY public.job_preview_sessions FORCE ROW LEVEL SECURITY;

-- COMMENT public.COLUMN job_preview_sessions.boot_generation
COMMENT ON COLUMN public.job_preview_sessions.boot_generation IS 'Server-issued generation of the newest dispatched boot, minted at assignment build and echoed by the host; observation reports apply only on exact equality (data_model/database_schema D21).';

-- TABLE public.job_submission_receipts
CREATE TABLE public.job_submission_receipts (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    job_id uuid NOT NULL,
    idempotency_key text NOT NULL,
    request_hash text NOT NULL,
    source_hash text NOT NULL,
    parser_version text NOT NULL,
    task_id_map jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT job_submission_receipts_idempotency_key_check CHECK ((length(btrim(idempotency_key)) > 0)),
    CONSTRAINT job_submission_receipts_parser_version_check CHECK ((length(btrim(parser_version)) > 0)),
    CONSTRAINT job_submission_receipts_request_hash_check CHECK ((request_hash ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT job_submission_receipts_source_hash_check CHECK ((source_hash ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT job_submission_receipts_task_id_map_object_check CHECK ((jsonb_typeof(task_id_map) = 'object'::text))
);

ALTER TABLE ONLY public.job_submission_receipts FORCE ROW LEVEL SECURITY;

-- TABLE public.jobs
CREATE TABLE public.jobs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid NOT NULL,
    name text NOT NULL,
    job_type text NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    start_node uuid,
    force boolean DEFAULT false NOT NULL,
    depends_on_job uuid,
    max_concurrency integer DEFAULT 4 NOT NULL,
    priority integer DEFAULT 0 NOT NULL,
    total_tasks integer DEFAULT 0 NOT NULL,
    completed_tasks integer DEFAULT 0 NOT NULL,
    failed_tasks integer DEFAULT 0 NOT NULL,
    input_tokens bigint DEFAULT 0 NOT NULL,
    output_tokens bigint DEFAULT 0 NOT NULL,
    cost_usd numeric(10,4) DEFAULT 0 NOT NULL,
    created_by uuid,
    started_at timestamp with time zone,
    completed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    org_id uuid NOT NULL,
    branch_name text,
    head_commit text,
    base_branch text,
    tasks_since_checkpoint integer DEFAULT 0 NOT NULL,
    checkpoint_count integer DEFAULT 0 NOT NULL,
    review_every_k integer,
    active_checkpoint_id uuid,
    final_review_done boolean DEFAULT false NOT NULL,
    mode text,
    target_doc_id uuid,
    pause_reason text,
    start_commit text,
    sync_context jsonb,
    ended_by uuid,
    agent_launch jsonb,
    approval_context jsonb,
    orchestration_context jsonb DEFAULT '{}'::jsonb NOT NULL,
    golden_builder_context jsonb,
    -- P3.5 begin
    CONSTRAINT jobs_sync_provider_check CHECK (
        (sync_context ->> 'agentProvider') IN ('claude_code', 'codex')
        AND (sync_context ->> 'agent_provider') IN ('claude_code', 'codex')
    ),
    CONSTRAINT jobs_launch_provider_check CHECK (
        (agent_launch #>> '{resolved,provider,value}') IN ('claude_code', 'codex')
        AND (agent_launch #>> '{preferences,provider,value}') IN ('claude_code', 'codex')
    ),
    -- P3.5 end
    CONSTRAINT jobs_approval_context_shape_check CHECK ((((job_type = 'approval_review'::text) AND (approval_context IS NOT NULL) AND (public.approval_review_context_is_valid(approval_context, workspace_id, id) OR public.approval_review_context_v3_is_valid(approval_context))) OR ((job_type <> 'approval_review'::text) AND (approval_context IS NULL)))),
    CONSTRAINT jobs_golden_builder_context_object_check CHECK (((golden_builder_context IS NULL) OR (jsonb_typeof(golden_builder_context) = 'object'::text))),
    CONSTRAINT jobs_orchestration_context_object_check CHECK ((jsonb_typeof(orchestration_context) = 'object'::text)),
    CONSTRAINT jobs_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'bootstrapping'::text, 'running'::text, 'paused'::text, 'completed'::text, 'blocked'::text, 'failed'::text, 'cancelled'::text, 'rolled_back'::text, 'merged'::text])))
);

ALTER TABLE ONLY public.jobs FORCE ROW LEVEL SECURITY;

-- COMMENT public.COLUMN jobs.branch_name
COMMENT ON COLUMN public.jobs.branch_name IS 'Git branch forked for this job, e.g. job/<job-id>';

-- COMMENT public.COLUMN jobs.head_commit
COMMENT ON COLUMN public.jobs.head_commit IS 'Legacy single-repo job branch cursor. Not an authoritative workspace-wide commit pointer for multi-repo jobs; use workspace_branch_repos.head_commit / repo-local job refs for current repo state.';

-- COMMENT public.COLUMN jobs.base_branch
COMMENT ON COLUMN public.jobs.base_branch IS 'Workspace branch from which this job was forked, for later merge-back';

-- COMMENT public.COLUMN jobs.start_commit
COMMENT ON COLUMN public.jobs.start_commit IS 'Immutable SHA where the job branch started; preserved for audit even as head_commit advances';

-- COMMENT public.COLUMN jobs.agent_launch
COMMENT ON COLUMN public.jobs.agent_launch IS 'Versioned persisted agent launch preferences and immutable resolved execution contract';

-- TABLE public.merge_evaluations
CREATE TABLE public.merge_evaluations (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    seq bigint NOT NULL,
    org_id uuid NOT NULL,
    merge_run_id uuid NOT NULL,
    source_workspace_id uuid NOT NULL,
    target_workspace_id uuid NOT NULL,
    outcome text NOT NULL,
    schema_version integer NOT NULL,
    evaluation jsonb NOT NULL,
    evaluated_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT merge_evaluations_evaluation_check CHECK ((jsonb_typeof(evaluation) = 'object'::text)),
    CONSTRAINT merge_evaluations_outcome_check CHECK ((outcome = ANY (ARRAY['passed'::text, 'failed'::text]))),
    CONSTRAINT merge_evaluations_schema_version_check CHECK ((schema_version >= 1))
);

ALTER TABLE ONLY public.merge_evaluations FORCE ROW LEVEL SECURITY;

-- SEQUENCE public.merge_evaluations_seq_seq
ALTER TABLE public.merge_evaluations ALTER COLUMN seq ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME public.merge_evaluations_seq_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);

-- TABLE public.node_dependencies
CREATE TABLE public.node_dependencies (
    from_node_id uuid NOT NULL,
    to_node_id uuid NOT NULL,
    dep_type text DEFAULT 'direct'::text NOT NULL,
    org_id uuid NOT NULL,
    CONSTRAINT node_dependencies_dep_type_check CHECK ((dep_type = ANY (ARRAY['direct'::text, 'inherited'::text])))
);

ALTER TABLE ONLY public.node_dependencies FORCE ROW LEVEL SECURITY;

-- TABLE public.notifications
CREATE TABLE public.notifications (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid NOT NULL,
    recipient_user_id uuid NOT NULL,
    actor_user_id uuid,
    category text DEFAULT 'info'::text NOT NULL,
    severity text DEFAULT 'info'::text NOT NULL,
    title text NOT NULL,
    message text DEFAULT ''::text NOT NULL,
    task_id uuid,
    job_id uuid,
    comment_id uuid,
    reply_id uuid,
    actions jsonb DEFAULT '[]'::jsonb NOT NULL,
    read_at timestamp with time zone,
    expires_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT notifications_severity_check CHECK ((severity = ANY (ARRAY['info'::text, 'warning'::text, 'error'::text])))
);

-- TABLE public.oauth_scopes
CREATE TABLE public.oauth_scopes (
    scope text NOT NULL,
    description text NOT NULL,
    category text NOT NULL
);

-- TABLE public.oauth_states
CREATE TABLE public.oauth_states (
    state text NOT NULL,
    provider text NOT NULL,
    user_id uuid,
    expires_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

-- TABLE public.ontology_grounding
CREATE TABLE public.ontology_grounding (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_branch_id uuid NOT NULL,
    org_id uuid NOT NULL,
    task_id uuid NOT NULL,
    source_intent_record_id uuid NOT NULL,
    target_intent_record_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    created_by text,
    CONSTRAINT chk_no_self_reference CHECK ((source_intent_record_id <> target_intent_record_id))
);

-- TABLE public.ontology_nodes
CREATE TABLE public.ontology_nodes (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_branch_id uuid NOT NULL,
    parent_id uuid,
    name text NOT NULL,
    node_type text NOT NULL,
    file_path text,
    content_hash text,
    sort_order integer DEFAULT 0 NOT NULL,
    sync_status text DEFAULT 'synced'::text NOT NULL,
    ahead_of_base integer DEFAULT 0 NOT NULL,
    behind_base integer DEFAULT 0 NOT NULL,
    last_uplifted_at timestamp with time zone,
    last_downlifted_at timestamp with time zone,
    content_updated_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    org_id uuid NOT NULL,
    component_id text,
    src_files text[] DEFAULT '{}'::text[] NOT NULL,
    last_uplifted_commit_sha text,
    last_downlifted_commit_sha text,
    workspace_branch_repo_id uuid,
    CONSTRAINT ontology_nodes_node_type_check CHECK ((node_type = ANY (ARRAY['node'::text, 'document'::text, 'code'::text, 'view'::text]))),
    CONSTRAINT ontology_nodes_sync_status_check CHECK ((sync_status = ANY (ARRAY['synced'::text, 'modified'::text, 'added'::text, 'deleted'::text, 'conflict'::text])))
);

ALTER TABLE ONLY public.ontology_nodes FORCE ROW LEVEL SECURITY;

-- TABLE public.org_configs
CREATE TABLE public.org_configs (
    org_id uuid NOT NULL,
    settings_json jsonb DEFAULT '{}'::jsonb NOT NULL,
    secrets_enc bytea,
    secret_keys text[] DEFAULT '{}'::text[] NOT NULL,
    updated_by uuid,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT org_configs_settings_json_check CHECK ((jsonb_typeof(settings_json) = 'object'::text))
);

ALTER TABLE ONLY public.org_configs FORCE ROW LEVEL SECURITY;

-- TABLE public.org_default_scopes
CREATE TABLE public.org_default_scopes (
    org_id uuid NOT NULL,
    scope text NOT NULL
);

-- TABLE public.org_invitations
CREATE TABLE public.org_invitations (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    email text NOT NULL,
    role text DEFAULT 'member'::text NOT NULL,
    invited_by uuid NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    accepted_by uuid,
    expires_at timestamp with time zone DEFAULT (now() + '7 days'::interval) NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT org_invitations_role_check CHECK ((role = ANY (ARRAY['admin'::text, 'member'::text, 'viewer'::text]))),
    CONSTRAINT org_invitations_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'accepted'::text, 'declined'::text, 'expired'::text])))
);

-- TABLE public.org_memberships
CREATE TABLE public.org_memberships (
    org_id uuid NOT NULL,
    user_id uuid NOT NULL,
    role text DEFAULT 'member'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT org_memberships_role_check CHECK ((role = ANY (ARRAY['owner'::text, 'admin'::text, 'member'::text, 'viewer'::text])))
);

-- TABLE public.orgs
CREATE TABLE public.orgs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    name text NOT NULL,
    slug text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    default_base_image_ref text,
    default_base_image_track text,
    CONSTRAINT orgs_default_base_image_ref_digest_pinned CHECK (((default_base_image_ref IS NULL) OR (default_base_image_ref ~ '^[a-z0-9._-]+(:[0-9]+)?(/[a-z0-9._-]+)+@sha256:[0-9a-f]{64}$'::text))),
    CONSTRAINT orgs_default_base_image_track_name CHECK (((default_base_image_track IS NULL) OR (default_base_image_track ~ '^[a-z0-9][a-z0-9._-]*$'::text)))
);

-- COMMENT public.COLUMN orgs.default_base_image_ref
COMMENT ON COLUMN public.orgs.default_base_image_ref IS 'Digest-pinned default microVM base image for newly provisioned worker hosts (worker_hosts ontology D50). Written only by per-release pin migrations; NULL = the buildpack-deps:noble-scm startup-script default.';

-- COMMENT public.COLUMN orgs.default_base_image_track
COMMENT ON COLUMN public.orgs.default_base_image_track IS 'Customer image name whose promoted digest the org default tracks (worker_hosts ontology D52). NULL = no tracking; default_base_image_ref is then a manual pin (or NULL for the buildpack-deps default).';

-- TABLE public.platform_issue_delivery_tombstone
CREATE TABLE public.platform_issue_delivery_tombstone (
    submission_id uuid NOT NULL,
    outcome text NOT NULL,
    attempts integer NOT NULL,
    last_error text,
    github_issue_number bigint,
    github_issue_url text,
    created_at timestamp with time zone NOT NULL,
    finalized_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT platform_issue_delivery_tombstone_outcome_check CHECK ((outcome = ANY (ARRAY['completed'::text, 'terminal_failed'::text])))
);

-- TABLE public.platform_issue_outbox
CREATE TABLE public.platform_issue_outbox (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    attempts integer DEFAULT 0 NOT NULL,
    next_run_at timestamp with time zone DEFAULT now() NOT NULL,
    locked_at timestamp with time zone,
    last_error text,
    github_create_started_at timestamp with time zone,
    reporter_user_id uuid NOT NULL,
    title text NOT NULL,
    body text NOT NULL,
    kind text DEFAULT 'bug'::text NOT NULL,
    severity text NOT NULL,
    labels text[] DEFAULT '{}'::text[] NOT NULL,
    context jsonb DEFAULT '{}'::jsonb NOT NULL,
    screenshot_bytes bytea,
    screenshot_mime text,
    github_issue_number bigint,
    github_issue_url text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    reporter_display_name text,
    report_base_url text,
    CONSTRAINT platform_issue_outbox_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'processing'::text, 'failed'::text, 'reconciling'::text])))
);

-- P1.4: removed the eight pr1884_authority_cutover_* tables (branch_dispositions, canonical_draft_retirements, job_archives, policy_overrides, runs, stale_draft_normalizations, task_archives, workspace_receipts) with their CHECK constraints and COMMENT ON TABLE statements.

-- TABLE public.provider_accounts
CREATE TABLE public.provider_accounts (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    scope text NOT NULL,
    workspace_id uuid,
    user_id uuid,
    name text NOT NULL,
    kind text NOT NULL,
    base_url text,
    secret_enc bytea,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT provider_accounts_base_url_check CHECK (((base_url IS NULL) OR (base_url ~ '^https://'::text))),
    CONSTRAINT provider_accounts_kind_check CHECK ((kind = ANY (ARRAY['claude_subscription'::text, 'anthropic_api_key'::text, 'anthropic_compatible_bearer'::text, 'openrouter_api_key'::text, 'openai_api_key'::text, 'chatgpt_subscription'::text]))),
    CONSTRAINT provider_accounts_name_check CHECK (((btrim(name) <> ''::text) AND (length(name) <= 80))),
    CONSTRAINT provider_accounts_scope_check CHECK ((scope = ANY (ARRAY['org'::text, 'workspace'::text, 'user'::text]))),
    CONSTRAINT provider_accounts_scope_shape CHECK ((((scope = 'org'::text) AND (workspace_id IS NULL) AND (user_id IS NULL)) OR ((scope = 'workspace'::text) AND (workspace_id IS NOT NULL) AND (user_id IS NULL)) OR ((scope = 'user'::text) AND (workspace_id IS NULL) AND (user_id IS NOT NULL))))
);

ALTER TABLE ONLY public.provider_accounts FORCE ROW LEVEL SECURITY;

-- TABLE public.provider_quota_snapshots
CREATE TABLE public.provider_quota_snapshots (
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    provider text NOT NULL,
    auth_mode text NOT NULL,
    five_hour_utilization real,
    five_hour_reset_at timestamp with time zone,
    seven_day_utilization real,
    seven_day_reset_at timestamp with time zone,
    representative_claim text,
    unified_status text,
    captured_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    account_id uuid DEFAULT '00000000-0000-0000-0000-000000000000'::uuid NOT NULL,
    CONSTRAINT provider_quota_snapshots_five_hour_utilization_check CHECK (((five_hour_utilization IS NULL) OR ((five_hour_utilization >= (0)::double precision) AND (five_hour_utilization <= (1)::double precision)))),
    CONSTRAINT provider_quota_snapshots_seven_day_utilization_check CHECK (((seven_day_utilization IS NULL) OR ((seven_day_utilization >= (0)::double precision) AND (seven_day_utilization <= (1)::double precision))))
);

-- TABLE public.refresh_tokens
CREATE TABLE public.refresh_tokens (
    token text NOT NULL,
    user_id uuid NOT NULL,
    org_id uuid,
    expires_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

-- TABLE public.repo_manifests
CREATE TABLE public.repo_manifests (
    worker_id text NOT NULL,
    repo_id uuid NOT NULL,
    org_id uuid NOT NULL,
    local_head text NOT NULL,
    upstream_head text,
    last_fetched_at timestamp with time zone,
    storage_path text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

ALTER TABLE ONLY public.repo_manifests FORCE ROW LEVEL SECURITY;

-- TABLE public.repo_secrets
CREATE TABLE public.repo_secrets (
    repo_id uuid NOT NULL,
    org_id uuid NOT NULL,
    webhook_secret_enc bytea,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

ALTER TABLE ONLY public.repo_secrets FORCE ROW LEVEL SECURITY;

-- TABLE public.repos
CREATE TABLE public.repos (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    name text NOT NULL,
    slug text NOT NULL,
    git_url text NOT NULL,
    default_branch text DEFAULT 'main'::text NOT NULL,
    -- P1.2 begin
    protected_branches text[] NOT NULL,
    -- P1.2 end
    provider text,
    provider_repo_id text,
    provider_full_name text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

ALTER TABLE ONLY public.repos FORCE ROW LEVEL SECURITY;

-- P1.2 begin
-- PostgreSQL column defaults cannot reference another column. Fill the omitted
-- array before NOT NULL is checked, including repos with a non-main default.
CREATE FUNCTION public.default_repo_protected_branches() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
BEGIN
    IF NEW.protected_branches IS NULL THEN
        NEW.protected_branches := ARRAY[NEW.default_branch];
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER repos_default_protected_branches BEFORE INSERT ON public.repos
    FOR EACH ROW EXECUTE FUNCTION public.default_repo_protected_branches();
-- P1.2 end


-- TABLE public.runtime_attribution_events
CREATE TABLE public.runtime_attribution_events (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    task_id uuid NOT NULL,
    runtime_id uuid NOT NULL,
    local_session_id text NOT NULL,
    event_id text NOT NULL,
    event_type text NOT NULL,
    schema_version integer NOT NULL,
    epoch bigint,
    frame_seq bigint,
    attribution_seq bigint,
    event_ts_ms bigint,
    ring_append_seq bigint,
    payload jsonb NOT NULL,
    received_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT runtime_attribution_events_event_id_check CHECK (((char_length(event_id) >= 1) AND (char_length(event_id) <= 256))),
    CONSTRAINT runtime_attribution_events_event_type_check CHECK ((event_type = ANY (ARRAY['boundary'::text, 'wake_finalization'::text, 'input_accepted'::text, 'ring_stats'::text, 'assistant_persisted'::text, 'wake_dispatch'::text, 'wake_recovery_outcome'::text]))),
    CONSTRAINT runtime_attribution_events_local_session_id_check CHECK (((char_length(local_session_id) >= 1) AND (char_length(local_session_id) <= 128))),
    CONSTRAINT runtime_attribution_events_schema_version_check CHECK ((schema_version > 0))
);

ALTER TABLE ONLY public.runtime_attribution_events FORCE ROW LEVEL SECURITY;

-- TABLE public.search_documents
CREATE TABLE public.search_documents (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_branch_id uuid NOT NULL,
    file_path text NOT NULL,
    node_id uuid,
    doc_type text DEFAULT 'file'::text NOT NULL,
    title text,
    content_hash text NOT NULL,
    chunk_index integer DEFAULT 0 NOT NULL,
    content_snippet text NOT NULL,
    fts tsvector NOT NULL,
    embedding public.vector(1536),
    indexed_at timestamp with time zone DEFAULT now() NOT NULL,
    org_id uuid NOT NULL,
    CONSTRAINT search_documents_doc_type_check CHECK ((doc_type = ANY (ARRAY['spec'::text, 'doc'::text, 'code'::text, 'file'::text])))
);

ALTER TABLE ONLY public.search_documents FORCE ROW LEVEL SECURITY;

-- TABLE public.search_reindex_queue
CREATE TABLE public.search_reindex_queue (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid NOT NULL,
    branch_id uuid NOT NULL,
    commit_sha text NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    attempts integer DEFAULT 0 NOT NULL,
    max_attempts integer DEFAULT 3 NOT NULL,
    error_message text,
    started_at timestamp with time zone,
    completed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    org_id uuid NOT NULL,
    CONSTRAINT search_reindex_queue_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'processing'::text, 'completed'::text, 'failed'::text])))
);

-- TABLE public.self_hosted_licenses
CREATE TABLE public.self_hosted_licenses (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    license_key_hash text NOT NULL,
    license_key_prefix text NOT NULL,
    customer_id text NOT NULL,
    customer_name text NOT NULL,
    contact_email text NOT NULL,
    product text DEFAULT 'g5-self-hosted'::text NOT NULL,
    version text DEFAULT 'latest'::text NOT NULL,
    status text DEFAULT 'active'::text NOT NULL,
    expires_at timestamp with time zone,
    registry_location text NOT NULL,
    registry_project text NOT NULL,
    registry_repository text NOT NULL,
    registry_puller_service_account text NOT NULL,
    created_by text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    last_issued_at timestamp with time zone,
    CONSTRAINT self_hosted_licenses_status_check CHECK ((status = ANY (ARRAY['active'::text, 'revoked'::text])))
);

-- TABLE public.session_build_telemetry
CREATE TABLE public.session_build_telemetry (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    task_id uuid,
    session_id text,
    task_type text,
    tool text NOT NULL,
    phase text DEFAULT ''::text NOT NULL,
    duration_ms bigint NOT NULL,
    exit_code integer NOT NULL,
    ts_unix_ms bigint NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT session_build_telemetry_duration_ms_check CHECK ((duration_ms >= 0)),
    CONSTRAINT session_build_telemetry_tool_check CHECK ((btrim(tool) <> ''::text)),
    CONSTRAINT session_build_telemetry_ts_unix_ms_check CHECK ((ts_unix_ms >= 0))
);

-- TABLE public.task_checkpoints
CREATE TABLE public.task_checkpoints (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    task_id uuid NOT NULL,
    commit_sha text NOT NULL,
    message text NOT NULL,
    files_changed text[] DEFAULT '{}'::text[] NOT NULL,
    stats jsonb DEFAULT '{"additions": 0, "deletions": 0}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    org_id uuid NOT NULL
);

ALTER TABLE ONLY public.task_checkpoints FORCE ROW LEVEL SECURITY;

-- TABLE public.task_commit_frontier_cuts
CREATE TABLE public.task_commit_frontier_cuts (
    org_id uuid NOT NULL,
    task_id uuid NOT NULL,
    request_id uuid NOT NULL,
    workspace_branch_repo_id uuid NOT NULL,
    session_id uuid NOT NULL,
    expected_head_commit text NOT NULL,
    materialized_head_commit text NOT NULL,
    state_vector bytea NOT NULL,
    yjs_state bytea NOT NULL,
    manifest jsonb NOT NULL,
    aggregate_sha256 bytea NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT task_commit_frontier_cuts_aggregate_sha256_check CHECK ((octet_length(aggregate_sha256) = 32)),
    CONSTRAINT task_commit_frontier_cuts_expected_head_commit_check CHECK ((btrim(expected_head_commit) <> ''::text)),
    CONSTRAINT task_commit_frontier_cuts_manifest_check CHECK ((jsonb_typeof(manifest) = 'array'::text)),
    CONSTRAINT task_commit_frontier_cuts_materialized_head_commit_check CHECK ((btrim(materialized_head_commit) <> ''::text)),
    CONSTRAINT task_commit_frontier_cuts_state_vector_check CHECK (((octet_length(state_vector) >= 1) AND (octet_length(state_vector) <= 65536))),
    CONSTRAINT task_commit_frontier_cuts_yjs_state_check CHECK (((octet_length(yjs_state) >= 1) AND (octet_length(yjs_state) <= 67108864)))
);

ALTER TABLE ONLY public.task_commit_frontier_cuts FORCE ROW LEVEL SECURITY;

-- TABLE public.task_commit_frontier_receipts
CREATE TABLE public.task_commit_frontier_receipts (
    org_id uuid NOT NULL,
    receipt_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    task_id uuid,
    worker_id text NOT NULL,
    worker_session_generation bigint NOT NULL,
    workspace_branch_repo_id uuid NOT NULL,
    session_id uuid NOT NULL,
    state_vector bytea NOT NULL,
    yjs_state bytea NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone DEFAULT (now() + '00:10:00'::interval) NOT NULL,
    CONSTRAINT chk_task_commit_frontier_receipt_expiry CHECK ((expires_at > created_at)),
    CONSTRAINT task_commit_frontier_receipts_state_vector_check CHECK (((octet_length(state_vector) >= 1) AND (octet_length(state_vector) <= 65536))),
    CONSTRAINT task_commit_frontier_receipts_worker_id_check CHECK ((btrim(worker_id) <> ''::text)),
    CONSTRAINT task_commit_frontier_receipts_yjs_state_check CHECK (((octet_length(yjs_state) >= 1) AND (octet_length(yjs_state) <= 67108864)))
);

ALTER TABLE ONLY public.task_commit_frontier_receipts FORCE ROW LEVEL SECURITY;

-- TABLE public.task_commit_requests
CREATE TABLE public.task_commit_requests (
    org_id uuid NOT NULL,
    task_id uuid NOT NULL,
    request_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    worker_id text NOT NULL,
    payload_sha256 bytea NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    terminal_result jsonb,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT chk_task_commit_requests_terminal_shape CHECK ((((status = 'pending'::text) AND (terminal_result IS NULL)) OR ((status = ANY (ARRAY['succeeded'::text, 'failed'::text])) AND (terminal_result IS NOT NULL)))),
    CONSTRAINT task_commit_requests_payload_sha256_check CHECK ((octet_length(payload_sha256) = 32)),
    CONSTRAINT task_commit_requests_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'succeeded'::text, 'failed'::text]))),
    CONSTRAINT task_commit_requests_worker_id_check CHECK ((btrim(worker_id) <> ''::text))
);

ALTER TABLE ONLY public.task_commit_requests FORCE ROW LEVEL SECURITY;

-- TABLE public.task_completion_constraints
CREATE TABLE public.task_completion_constraints (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    task_id uuid NOT NULL,
    action text NOT NULL,
    target_kind text NOT NULL,
    target_ref text NOT NULL,
    source_scope text DEFAULT 'task'::text NOT NULL,
    source_trigger_id text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT task_completion_constraints_action_check CHECK ((action = ANY (ARRAY['create'::text, 'edit'::text, 'delete'::text]))),
    CONSTRAINT task_completion_constraints_check CHECK ((((source_scope = 'task'::text) AND (source_trigger_id IS NULL)) OR ((source_scope = 'trigger_template'::text) AND (source_trigger_id IS NOT NULL)))),
    CONSTRAINT task_completion_constraints_source_scope_check CHECK ((source_scope = ANY (ARRAY['task'::text, 'trigger_template'::text]))),
    CONSTRAINT task_completion_constraints_target_kind_check CHECK ((target_kind = ANY (ARRAY['node'::text, 'doc'::text, 'file'::text])))
);

ALTER TABLE ONLY public.task_completion_constraints FORCE ROW LEVEL SECURITY;

-- TABLE public.task_execution_attempts
CREATE TABLE public.task_execution_attempts (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    task_id uuid NOT NULL,
    attempt_number integer NOT NULL,
    worker_id text,
    provider text,
    provider_session_id text,
    continuation_summary text,
    disposition text,
    reason text,
    retry_policy text,
    started_at timestamp with time zone DEFAULT now() NOT NULL,
    ended_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    close_evidence jsonb,
    CONSTRAINT task_execution_attempts_attempt_number_check CHECK ((attempt_number > 0)),
    CONSTRAINT task_execution_attempts_close_evidence_object CHECK (((close_evidence IS NULL) OR (jsonb_typeof(close_evidence) = 'object'::text))),
    CONSTRAINT task_execution_attempts_close_shape_check CHECK ((((ended_at IS NULL) AND (disposition IS NULL) AND (reason IS NULL) AND (retry_policy IS NULL)) OR ((ended_at IS NOT NULL) AND (disposition IS NOT NULL) AND (reason IS NOT NULL) AND (retry_policy IS NOT NULL)))),
    CONSTRAINT task_execution_attempts_continuation_summary_length_check CHECK (((continuation_summary IS NULL) OR (char_length(continuation_summary) <= 8192))),
    CONSTRAINT task_execution_attempts_disposition_check CHECK (((disposition IS NULL) OR (disposition = ANY (ARRAY['completed'::text, 'interrupted'::text, 'blocked'::text, 'failed'::text, 'cancelled'::text])))),
    -- P3.5 begin
    CONSTRAINT task_execution_attempts_provider_check CHECK (provider IN ('claude_code', 'codex')),
    -- P3.5 end
    CONSTRAINT task_execution_attempts_provider_session_length_check CHECK (((provider_session_id IS NULL) OR (char_length(provider_session_id) <= 512))),
    CONSTRAINT task_execution_attempts_reason_check CHECK (((reason IS NULL) OR (reason = ANY (ARRAY['completed'::text, 'budget_exhausted'::text, 'provider_transient'::text, 'provider_rate_limited'::text, 'provider_auth'::text, 'provider_timeout'::text, 'worker_lost'::text, 'lease_expired'::text, 'human_input_required'::text, 'dependency_blocked'::text, 'manual_pause'::text, 'task_defect'::text, 'state_corruption'::text, 'cancelled'::text, 'legacy_unclassified'::text])))),
    CONSTRAINT task_execution_attempts_retry_policy_check CHECK (((retry_policy IS NULL) OR (retry_policy = ANY (ARRAY['automatic'::text, 'manual'::text, 'none'::text])))),
    CONSTRAINT task_execution_attempts_time_order_check CHECK (((ended_at IS NULL) OR (ended_at >= started_at)))
);

ALTER TABLE ONLY public.task_execution_attempts FORCE ROW LEVEL SECURITY;

-- COMMENT public.COLUMN task_execution_attempts.close_evidence
COMMENT ON COLUMN public.task_execution_attempts.close_evidence IS 'Optional ExecutionLifecycle close evidence (cause/source/observed generations). Additive only; issue #2402.';

-- TABLE public.task_question_windows
CREATE TABLE public.task_question_windows (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    task_id uuid NOT NULL,
    session_id uuid NOT NULL,
    approval_id text NOT NULL,
    assistant_turn_id uuid NOT NULL,
    request_timeline_id uuid NOT NULL,
    policy_version text NOT NULL,
    state text NOT NULL,
    armed_at timestamp with time zone NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    settled_at timestamp with time zone,
    settlement_source text,
    platform_receipt_id uuid,
    receipt_consumed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT task_question_windows_check CHECK ((((state = 'open'::text) AND (settled_at IS NULL) AND (settlement_source IS NULL)) OR ((state <> 'open'::text) AND (settled_at IS NOT NULL) AND (settlement_source IS NOT NULL)))),
    CONSTRAINT task_question_windows_check1 CHECK (((platform_receipt_id IS NULL) OR (state = 'idle_timeout'::text))),
    CONSTRAINT task_question_windows_check2 CHECK (((receipt_consumed_at IS NULL) OR (platform_receipt_id IS NOT NULL))),
    CONSTRAINT task_question_windows_policy_version_check CHECK ((policy_version = 'question_v1'::text)),
    CONSTRAINT task_question_windows_state_check CHECK ((state = ANY (ARRAY['open'::text, 'answered'::text, 'idle_timeout'::text, 'cancelled'::text])))
);

ALTER TABLE ONLY public.task_question_windows FORCE ROW LEVEL SECURITY;

-- TABLE public.tasks
CREATE TABLE public.tasks (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    job_id uuid NOT NULL,
    node_id uuid,
    name text NOT NULL,
    task_type text DEFAULT 'regular'::text NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    branch_name text,
    base_commit text,
    workspace_ref text,
    worker_id text,
    assigned_at timestamp with time zone,
    retry_count integer DEFAULT 0 NOT NULL,
    max_retries integer DEFAULT 2 NOT NULL,
    depends_on uuid[] DEFAULT '{}'::uuid[] NOT NULL,
    input_tokens bigint DEFAULT 0 NOT NULL,
    output_tokens bigint DEFAULT 0 NOT NULL,
    cost_usd numeric(10,4) DEFAULT 0 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    component_id text,
    src_files text[] DEFAULT '{}'::text[] NOT NULL,
    node_level text,
    language text,
    parent_component_id text,
    child_component_ids text[] DEFAULT '{}'::text[] NOT NULL,
    org_id uuid NOT NULL,
    pruned boolean DEFAULT false NOT NULL,
    is_checkpoint boolean DEFAULT false NOT NULL,
    lease_expires_at timestamp with time zone,
    lease_renewals integer DEFAULT 0 NOT NULL,
    max_duration_secs integer,
    task_observations jsonb DEFAULT '{}'::jsonb NOT NULL,
    agent_auth_enc bytea,
    agent_auth_worker_id text,
    agent_auth_submitted_by uuid,
    agent_auth_updated_at timestamp with time zone,
    agent_execution_receipt jsonb,
    async_decision_policy text DEFAULT 'interactive_v1'::text NOT NULL,
    active_execution_attempt_id uuid,
    advisory_predecessor_job_id uuid,
    -- P3.5 begin: preserve legacy non-JSON workspace references.
    CONSTRAINT tasks_provider_check CHECK (CASE WHEN workspace_ref IS JSON OBJECT THEN
        (workspace_ref::jsonb ->> 'agentProvider') IN ('claude_code', 'codex')
        AND (workspace_ref::jsonb ->> 'agent_provider') IN ('claude_code', 'codex')
        ELSE true END),
    CONSTRAINT tasks_execution_provider_check CHECK (
        (agent_execution_receipt ->> 'provider') IN ('claude_code', 'codex')
    ),
    -- P3.5 end
    CONSTRAINT tasks_async_decision_policy_check CHECK ((async_decision_policy = ANY (ARRAY['interactive_v1'::text, 'autonomous_v1'::text]))),
    CONSTRAINT tasks_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'running'::text, 'paused'::text, 'completed'::text, 'failed'::text, 'blocked'::text, 'cancelled'::text, 'rolled_back'::text, 'waiting'::text, 'claimed'::text, 'recovering'::text, 'recovery_blocked'::text])))
);

ALTER TABLE ONLY public.tasks FORCE ROW LEVEL SECURITY;

-- COMMENT public.COLUMN tasks.agent_auth_enc
COMMENT ON COLUMN public.tasks.agent_auth_enc IS 'Encrypted task-bound Codex auth.json; never returned by public APIs';

-- COMMENT public.COLUMN tasks.agent_auth_worker_id
COMMENT ON COLUMN public.tasks.agent_auth_worker_id IS 'Worker selected before interactive subscription authentication';

-- COMMENT public.COLUMN tasks.agent_execution_receipt
COMMENT ON COLUMN public.tasks.agent_execution_receipt IS 'Worker-reported exact execution receipt validated against jobs.agent_launch before provider start';

-- TABLE public.timeline_entries
CREATE TABLE public.timeline_entries (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    task_id uuid NOT NULL,
    entry_type text NOT NULL,
    content text NOT NULL,
    metadata jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    org_id uuid NOT NULL,
    CONSTRAINT timeline_entries_entry_type_check CHECK ((entry_type = ANY (ARRAY['thought'::text, 'tool_call'::text, 'tool_result'::text, 'output'::text, 'error'::text, 'user_message'::text, 'agent_message'::text, 'checkpoint'::text, 'skill_invocation'::text, 'subagent_invocation'::text, 'host_microvm_started'::text, 'host_microvm_exited'::text, 'host_microvm_ready'::text, 'preview_attach_started'::text, 'preview_attach_exited'::text, 'sync_plan_receipt'::text])))
);

ALTER TABLE ONLY public.timeline_entries FORCE ROW LEVEL SECURITY;

-- TABLE public.usage_records
CREATE TABLE public.usage_records (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    task_id uuid,
    session_id uuid,
    job_id uuid,
    provider text NOT NULL,
    model text NOT NULL,
    input_tokens bigint DEFAULT 0 NOT NULL,
    output_tokens bigint DEFAULT 0 NOT NULL,
    total_tokens bigint DEFAULT 0 NOT NULL,
    cache_creation_tokens bigint DEFAULT 0 NOT NULL,
    cache_read_tokens bigint DEFAULT 0 NOT NULL,
    cost_usd numeric(10,6) DEFAULT 0 NOT NULL,
    request_type text DEFAULT 'chat'::text NOT NULL,
    finish_reason text,
    duration_ms integer,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT usage_records_request_type_check CHECK ((request_type = ANY (ARRAY['chat'::text, 'completion'::text, 'summarization'::text, 'embedding'::text])))
);

-- TABLE public.user_configs
CREATE TABLE public.user_configs (
    org_id uuid NOT NULL,
    user_id uuid NOT NULL,
    settings_json jsonb DEFAULT '{}'::jsonb NOT NULL,
    secrets_enc bytea,
    secret_keys text[] DEFAULT '{}'::text[] NOT NULL,
    updated_by uuid,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT user_configs_settings_json_check CHECK ((jsonb_typeof(settings_json) = 'object'::text))
);

ALTER TABLE ONLY public.user_configs FORCE ROW LEVEL SECURITY;

-- TABLE public.user_preferences
CREATE TABLE public.user_preferences (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    supervisor_delegation_threshold text DEFAULT 'off'::text NOT NULL,
    escalation_notification_level text DEFAULT 'all'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    autonomous_async_tasks boolean DEFAULT false NOT NULL,
    CONSTRAINT user_preferences_escalation_notification_level_check CHECK ((escalation_notification_level = ANY (ARRAY['all'::text, 'high_critical'::text, 'critical_only'::text, 'none'::text]))),
    CONSTRAINT user_preferences_supervisor_delegation_threshold_check CHECK ((supervisor_delegation_threshold = ANY (ARRAY['off'::text, 'low'::text, 'medium'::text, 'high'::text])))
);

-- TABLE public.user_scm_connections
CREATE TABLE public.user_scm_connections (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    provider text NOT NULL,
    provider_user_id text NOT NULL,
    provider_username text,
    access_token_enc bytea NOT NULL,
    refresh_token_enc bytea,
    token_expires_at timestamp with time zone,
    scopes text[] DEFAULT '{}'::text[] NOT NULL,
    installation_id bigint,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    auth_method text DEFAULT 'oauth'::text NOT NULL,
    CONSTRAINT user_scm_connections_auth_method_check CHECK ((auth_method = ANY (ARRAY['oauth'::text, 'pat'::text]))),
    CONSTRAINT user_scm_connections_provider_check CHECK ((provider = ANY (ARRAY['github'::text, 'gitlab'::text, 'bitbucket'::text])))
);

-- TABLE public.user_scope_grants
CREATE TABLE public.user_scope_grants (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    user_id uuid NOT NULL,
    scope text NOT NULL,
    granted_by uuid,
    granted_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone
);

-- TABLE public.users
CREATE TABLE public.users (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    email text NOT NULL,
    display_name text NOT NULL,
    avatar_url text,
    oauth_provider text,
    oauth_id text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

-- TABLE public.waitlist_signups
CREATE TABLE public.waitlist_signups (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    email text NOT NULL,
    source text DEFAULT 'landing'::text NOT NULL,
    ip_address text,
    user_agent text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

-- TABLE public.worker_hosts
CREATE TABLE public.worker_hosts (
    host_id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    vpc_id text,
    cloud_instance_id text,
    instance_type text DEFAULT 'c3-standard-4'::text NOT NULL,
    region text NOT NULL,
    libkrun_version text,
    libkrunfw_version text,
    kernel_version text,
    cache_state jsonb DEFAULT '{}'::jsonb NOT NULL,
    attestation_state jsonb DEFAULT '{}'::jsonb NOT NULL,
    status text DEFAULT 'provisioning'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_heartbeat timestamp with time zone,
    active_microvm_count integer,
    cache_disk_used_bytes bigint,
    cache_disk_total_bytes bigint,
    provision_error_code text,
    orphan_check_misses integer DEFAULT 0 NOT NULL,
    base_image_ref text,
    base_image_digest text,
    runtime_schema text,
    runtime_labels jsonb DEFAULT '{}'::jsonb NOT NULL,
    runtime_labels_refreshed_at timestamp with time zone,
    worker_profile text DEFAULT 'fat-default'::text NOT NULL,
    rotation_replaced_by uuid,
    last_active_at timestamp with time zone,
    concurrent_jobs integer,
    boot_image_id text,
    bringup_heartbeat_at timestamp with time zone DEFAULT now(),
    cloud_gone_since timestamp with time zone,
    provision_retry_count integer DEFAULT 0 NOT NULL,
    provision_next_retry_at timestamp with time zone,
    base_image_source_kind text,
    base_image_source_value text,
    CONSTRAINT worker_hosts_base_image_digest_format CHECK (((base_image_digest IS NULL) OR (base_image_digest ~ '^sha256:[0-9a-f]{64}$'::text))),
    CONSTRAINT worker_hosts_base_image_source_shape CHECK ((((base_image_source_kind IS NULL) AND (base_image_source_value IS NULL)) OR ((base_image_source_kind = ANY (ARRAY['track'::text, 'pin'::text])) AND (base_image_source_value IS NOT NULL) AND (btrim(base_image_source_value) <> ''::text)))),
    CONSTRAINT worker_hosts_concurrent_jobs_min CHECK (((concurrent_jobs IS NULL) OR (concurrent_jobs >= 1))),
    CONSTRAINT worker_hosts_provision_error_code_check CHECK (((provision_error_code IS NULL) OR (provision_error_code = ANY (ARRAY['zone_capacity_exhausted'::text, 'abandoned_bringup'::text, 'never_registered'::text, 'base_image_below_glibc_floor'::text])))),
    CONSTRAINT worker_hosts_status_check CHECK ((status = ANY (ARRAY['provisioning'::text, 'ready'::text, 'draining'::text, 'terminated'::text]))),
    CONSTRAINT worker_hosts_worker_profile_allowed CHECK ((worker_profile = ANY (ARRAY['fat-default'::text, 'slim'::text])))
);

ALTER TABLE ONLY public.worker_hosts FORCE ROW LEVEL SECURITY;

-- COMMENT public.COLUMN worker_hosts.provision_error_code
COMMENT ON COLUMN public.worker_hosts.provision_error_code IS 'Cloud bring-up failure classification while status=provisioning. NULL when bring-up has not failed (or has not been attempted). The known-codes set is enforced by CHECK; extend the CHECK and the UI badge map in lockstep.';

-- COMMENT public.COLUMN worker_hosts.orphan_check_misses
COMMENT ON COLUMN public.worker_hosts.orphan_check_misses IS 'Reconciler orphan-pass counter: number of consecutive ticks the GCE instance for this row (looked up by deterministic name g5-host-{host_id}) was reported missing. Reaches the terminate threshold ([D39]); reset is implicit (the row leaves the orphan SELECT predicate when cloud_instance_id is populated or status moves out of provisioning).';

-- COMMENT public.COLUMN worker_hosts.base_image_ref
COMMENT ON COLUMN public.worker_hosts.base_image_ref IS 'Operator-supplied digest-pinned base image ref (<registry>/<image>@sha256:<64hex>); NULL means the startup script uses the ubuntu:24.04 default. PR1 does not resolve tags or fetch OCI labels.';

-- COMMENT public.COLUMN worker_hosts.base_image_digest
COMMENT ON COLUMN public.worker_hosts.base_image_digest IS 'sha256:<64 lowercase hex> extracted from base_image_ref; CHECK-enforced shape.';

-- COMMENT public.COLUMN worker_hosts.runtime_schema
COMMENT ON COLUMN public.worker_hosts.runtime_schema IS 'Forward-compat runtime schema id; PR2 populates from OCI label extraction. NULL in PR1.';

-- COMMENT public.COLUMN worker_hosts.runtime_labels
COMMENT ON COLUMN public.worker_hosts.runtime_labels IS 'Forward-compat OCI/runtime labels (PR2). Defaults to {}; not populated by PR1.';

-- COMMENT public.COLUMN worker_hosts.runtime_labels_refreshed_at
COMMENT ON COLUMN public.worker_hosts.runtime_labels_refreshed_at IS 'Forward-compat: when runtime_labels was last refreshed (PR2). NULL in PR1.';

-- COMMENT public.COLUMN worker_hosts.worker_profile
COMMENT ON COLUMN public.worker_hosts.worker_profile IS 'Worker profile selection; CHECK-enforced codeset {fat-default,slim}. Extend the CHECK and WORKER_PROFILE_ALLOWLIST in lockstep.';

-- COMMENT public.COLUMN worker_hosts.rotation_replaced_by
COMMENT ON COLUMN public.worker_hosts.rotation_replaced_by IS 'host_id of the replacement provisioned by the idle-rotation loop ([D47]); NULL unless this row was rotated out.';

-- COMMENT public.COLUMN worker_hosts.last_active_at
COMMENT ON COLUMN public.worker_hosts.last_active_at IS 'Last heartbeat that reported active_microvm_count > 0 ([D30]/[D47]); never cleared, only advanced.';

-- COMMENT public.COLUMN worker_hosts.concurrent_jobs
COMMENT ON COLUMN public.worker_hosts.concurrent_jobs IS 'Operator-pinned concurrentJobs budget from the provision request ([D54]); NULL tracks the current default and is re-resolved on rotation.';

-- COMMENT public.COLUMN worker_hosts.boot_image_id
COMMENT ON COLUMN public.worker_hosts.boot_image_id IS 'Immutable outer GCE boot image this host was pinned to at provision (migration 168, cloud_provider [D56], issue #1424). Version-stamped image name (e.g. g5-worker-v20260723-1046-9749e00); the boot-image-staleness comparison key for idle rotation. NULL for rows provisioned before this migration or when boot-image resolution fell back to the raw family sourceImage. Not backfilled — legacy rows converge through rotation.';

-- COMMENT public.COLUMN worker_hosts.bringup_heartbeat_at
COMMENT ON COLUMN public.worker_hosts.bringup_heartbeat_at IS 'Bring-up liveness lease ([D60]): born now() with the row (column DEFAULT), renewed every 30s by bring_up_cloud_resources while the detached task runs. The orphan pass treats a stamp fresher than 90s as "bring-up in flight" and will neither list nor attach the row. NULL = pre-171 legacy row (60s-grace semantics apply).';

-- COMMENT public.COLUMN worker_hosts.cloud_gone_since
COMMENT ON COLUMN public.worker_hosts.cloud_gone_since IS 'Wall-clock start of the current confirmed-gone window ([D61]): stamped when the linked cloud instance is observed Terminated/404 AND the by-name re-search across the region''s UP zones finds nothing alive; cleared on any alive observation or linkage repoint. terminate_gone_worker_host requires this to be older than GONE_CONFIRMATION_SECS — replica-count independent.';

-- COMMENT public.COLUMN worker_hosts.provision_retry_count
COMMENT ON COLUMN public.worker_hosts.provision_retry_count IS 'Zone-stockout retry attempts consumed ([D66], #1880). Incremented by the retry pass''s claim UPDATE; frozen when the budget exhausts.';

-- COMMENT public.COLUMN worker_hosts.provision_next_retry_at
COMMENT ON COLUMN public.worker_hosts.provision_next_retry_at IS 'Next scheduled stockout retry ([D66], #1880). Stamped with backoff by bring-up when the exhausted-everywhere stockout is classified and retry is enabled; NULL = not scheduled (never failed, currently claimed, or budget exhausted). Cleared atomically by a successful cloud-ref attach.';

-- TABLE public.worker_tokens
CREATE TABLE public.worker_tokens (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    scope text NOT NULL,
    workspace_id uuid,
    user_id uuid,
    key_hash text NOT NULL,
    key_prefix text NOT NULL,
    name text,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone,
    revoked_at timestamp with time zone,
    CONSTRAINT worker_tokens_scope_check CHECK ((scope = ANY (ARRAY['org'::text, 'workspace'::text, 'user'::text]))),
    CONSTRAINT worker_tokens_user_scope_check CHECK (((scope <> 'user'::text) OR (user_id IS NOT NULL))),
    CONSTRAINT worker_tokens_workspace_scope_check CHECK (((scope <> 'workspace'::text) OR (workspace_id IS NOT NULL)))
);

-- TABLE public.workers
CREATE TABLE public.workers (
    id text NOT NULL,
    role text DEFAULT 'worker'::text NOT NULL,
    status text DEFAULT 'provisioning'::text NOT NULL,
    owner_user_id uuid,
    org_id uuid,
    memory_mb integer,
    cpu_cores integer,
    task_id uuid,
    workspace_id uuid,
    branch text,
    base_commit text,
    last_heartbeat timestamp with time zone,
    last_task_completed_at timestamp with time zone,
    last_node_id uuid,
    started_at timestamp with time zone DEFAULT now() NOT NULL,
    terminated_at timestamp with time zone,
    capabilities jsonb DEFAULT '[]'::jsonb NOT NULL,
    max_concurrent_tasks integer DEFAULT 1 NOT NULL,
    display_name text,
    host_id uuid,
    preferred boolean DEFAULT false NOT NULL,
    last_poll_at timestamp with time zone,
    external_host_id uuid,
    worker_session_generation bigint DEFAULT 1 NOT NULL,
    host_discard_fence_challenge jsonb,
    host_discard_fence_receipt jsonb,
    CONSTRAINT chk_workers_preferred_requires_org CHECK (((NOT preferred) OR (org_id IS NOT NULL))),
    CONSTRAINT chk_workers_session_generation_positive CHECK ((worker_session_generation > 0)),
    CONSTRAINT workers_host_discard_fence_challenge_object CHECK (((host_discard_fence_challenge IS NULL) OR (jsonb_typeof(host_discard_fence_challenge) = 'object'::text))),
    CONSTRAINT workers_host_discard_fence_receipt_object CHECK (((host_discard_fence_receipt IS NULL) OR (jsonb_typeof(host_discard_fence_receipt) = 'object'::text))),
    CONSTRAINT workers_role_check CHECK ((role = ANY (ARRAY['editor'::text, 'worker'::text]))),
    CONSTRAINT workers_status_check CHECK ((status = ANY (ARRAY['provisioning'::text, 'idle'::text, 'assigned'::text, 'executing'::text, 'terminated'::text])))
);

-- COMMENT public.COLUMN workers.host_discard_fence_challenge
COMMENT ON COLUMN public.workers.host_discard_fence_challenge IS 'Short-lived schema-1 server challenge for host discard-fence activation; never authority by itself.';

-- COMMENT public.COLUMN workers.host_discard_fence_receipt
COMMENT ON COLUMN public.workers.host_discard_fence_receipt IS 'Current schema-1 server-normalized host composition receipt, generation-bound to this worker row.';

-- P1.4 begin
-- LEAN-TODO(P4.8): advance ids are minted by the landing helper; no registry row backs them.
-- P1.4 end
-- TABLE public.workspace_advance_consumer_inputs
CREATE TABLE public.workspace_advance_consumer_inputs (
    operation_id uuid NOT NULL,
    org_id uuid NOT NULL,
    consumer_kind text NOT NULL,
    payload jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT workspace_advance_consumer_inputs_consumer_kind_check CHECK ((btrim(consumer_kind) <> ''::text))
);

-- P1.4 begin
-- LEAN-TODO(P4.8): advance-consumer bookkeeping keyed by advance id; job-merge (P4.8) and workspace-PR (P6.4) consumers retire it.
-- P1.4 end
ALTER TABLE ONLY public.workspace_advance_consumer_inputs FORCE ROW LEVEL SECURITY;

-- P1.4 begin
-- LEAN-TODO(P4.8): advance-consumer bookkeeping keyed by advance id; job-merge (P4.8) and workspace-PR (P6.4) consumers retire it.
-- P1.4 end
-- TABLE public.workspace_advance_consumer_receipts
CREATE TABLE public.workspace_advance_consumer_receipts (
    operation_id uuid NOT NULL,
    org_id uuid NOT NULL,
    consumer_kind text NOT NULL,
    completed_at timestamp with time zone DEFAULT now() NOT NULL,
    result jsonb,
    CONSTRAINT chk_workspace_advance_consumer_kind CHECK ((btrim(consumer_kind) <> ''::text))
);

-- P1.4 begin
-- LEAN-TODO(P4.8): advance-consumer bookkeeping keyed by advance id; job-merge (P4.8) and workspace-PR (P6.4) consumers retire it.
-- P1.4 end
ALTER TABLE ONLY public.workspace_advance_consumer_receipts FORCE ROW LEVEL SECURITY;

-- TABLE public.workspace_branch_repos
CREATE TABLE public.workspace_branch_repos (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    workspace_branch_id uuid NOT NULL,
    repo_id uuid NOT NULL,
    repo_branch_name text NOT NULL,
    base_branch text DEFAULT 'main'::text NOT NULL,
    head_commit text NOT NULL,
    tracking_mode text DEFAULT 'pinned'::text NOT NULL,
    materialization_status text DEFAULT 'materialized'::text NOT NULL,
    root_path text DEFAULT ''::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    remote_branch text,
    baseline_commit text,
    sync_paused_reason text,
    last_sync_at timestamp with time zone,
    remote_tip_advisory_sha text,
    remote_tip_advisory_observed_at timestamp with time zone,
    auto_pull_enabled boolean DEFAULT false NOT NULL,
    diff_base_commit text,
    base_head_commit text,
    CONSTRAINT chk_workspace_branch_repos_base_head_commit_exact CHECK (((base_head_commit IS NULL) OR ((base_head_commit ~ '^[0-9a-f]{40}$'::text) AND (base_head_commit <> repeat('0'::text, 40))))),
    CONSTRAINT workspace_branch_repos_materialization_status_check CHECK ((materialization_status = ANY (ARRAY['materialized'::text, 'unborn'::text, 'missing_ref'::text, 'error'::text]))),
    CONSTRAINT workspace_branch_repos_remote_tracking_shape_check CHECK (((tracking_mode <> 'remote_tracking'::text) OR ((remote_branch IS NOT NULL) AND (baseline_commit IS NOT NULL)))),
    CONSTRAINT workspace_branch_repos_tracking_mode_check CHECK ((tracking_mode = ANY (ARRAY['tracking'::text, 'pinned'::text, 'remote_tracking'::text])))
);

ALTER TABLE ONLY public.workspace_branch_repos FORCE ROW LEVEL SECURITY;

-- P1.4 begin
-- LEAN-TODO(P4.8): authority scope columns identify job/task branch rows until job and task branches are deleted.
-- P1.4 end
-- TABLE public.workspace_branches
CREATE TABLE public.workspace_branches (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid NOT NULL,
    repo_id uuid NOT NULL,
    branch_name text NOT NULL,
    base_branch text DEFAULT 'main'::text NOT NULL,
    head_commit text NOT NULL,
    tracking_mode text DEFAULT 'pinned'::text NOT NULL,
    org_id uuid NOT NULL,
    cloned_from_branch_id uuid,
    materialization_status text DEFAULT 'materialized'::text NOT NULL,
    authority_scope_kind text,
    job_id uuid,
    task_id uuid,
    authority_scope_id uuid GENERATED ALWAYS AS (
CASE authority_scope_kind
    WHEN 'workspace'::text THEN workspace_id
    WHEN 'job'::text THEN job_id
    WHEN 'task'::text THEN task_id
    ELSE NULL::uuid
END) STORED,
    CONSTRAINT chk_workspace_branches_authority_scope CHECK ((((authority_scope_kind IS NULL) AND (job_id IS NULL) AND (task_id IS NULL)) OR ((authority_scope_kind = 'workspace'::text) AND (job_id IS NULL) AND (task_id IS NULL)) OR ((authority_scope_kind = 'job'::text) AND (job_id IS NOT NULL) AND (task_id IS NULL)) OR ((authority_scope_kind = 'task'::text) AND (job_id IS NOT NULL) AND (task_id IS NOT NULL)))),
    CONSTRAINT workspace_branches_authority_scope_kind_check CHECK ((authority_scope_kind = ANY (ARRAY['workspace'::text, 'job'::text, 'task'::text]))),
    CONSTRAINT workspace_branches_materialization_status_check CHECK ((materialization_status = ANY (ARRAY['materialized'::text, 'unborn'::text, 'missing_ref'::text, 'error'::text]))),
    CONSTRAINT workspace_branches_tracking_mode_check CHECK ((tracking_mode = ANY (ARRAY['tracking'::text, 'pinned'::text])))
);

ALTER TABLE ONLY public.workspace_branches FORCE ROW LEVEL SECURITY;

-- COMMENT public.COLUMN workspace_branches.authority_scope_kind
COMMENT ON COLUMN public.workspace_branches.authority_scope_kind IS 'Typed authority owner: workspace, job, or task; NULL rows are explicitly non-current operation staging or historical tombstones';

-- COMMENT public.COLUMN workspace_branches.job_id
COMMENT ON COLUMN public.workspace_branches.job_id IS 'Direct job owner for job members and parent job for task members';

-- COMMENT public.COLUMN workspace_branches.task_id
COMMENT ON COLUMN public.workspace_branches.task_id IS 'Direct task owner for task authority members';

-- COMMENT public.COLUMN workspace_branches.authority_scope_id
COMMENT ON COLUMN public.workspace_branches.authority_scope_id IS 'Generated typed scope id used by the common authority resolver';

-- TABLE public.workspace_configs
CREATE TABLE public.workspace_configs (
    workspace_id uuid NOT NULL,
    org_id uuid NOT NULL,
    settings_json jsonb DEFAULT '{}'::jsonb NOT NULL,
    secrets_enc bytea,
    secret_keys text[] DEFAULT '{}'::text[] NOT NULL,
    updated_by uuid,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT workspace_configs_settings_json_check CHECK ((jsonb_typeof(settings_json) = 'object'::text))
);

ALTER TABLE ONLY public.workspace_configs FORCE ROW LEVEL SECURITY;

-- TABLE public.workspace_dependencies
CREATE TABLE public.workspace_dependencies (
    workspace_id uuid NOT NULL,
    depends_on uuid NOT NULL,
    org_id uuid NOT NULL,
    CONSTRAINT workspace_dependencies_check CHECK ((workspace_id <> depends_on))
);

ALTER TABLE ONLY public.workspace_dependencies FORCE ROW LEVEL SECURITY;

-- P1.4: removed the head-operation registry tables workspace_head_operation_make_remote_applications, workspace_head_operation_membership_proposals, workspace_head_operation_repos, workspace_head_operation_source_repos and workspace_head_operations with their CHECK constraints, FORCE ROW LEVEL SECURITY statements and COMMENT ON COLUMN statements.

-- TABLE public.workspace_members
CREATE TABLE public.workspace_members (
    workspace_id uuid NOT NULL,
    user_id uuid NOT NULL,
    role text DEFAULT 'editor'::text NOT NULL,
    invited_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    org_id uuid NOT NULL,
    CONSTRAINT workspace_members_role_check CHECK ((role = ANY (ARRAY['admin'::text, 'editor'::text, 'viewer'::text])))
);

ALTER TABLE ONLY public.workspace_members FORCE ROW LEVEL SECURITY;

-- TABLE public.workspace_permission_overrides
CREATE TABLE public.workspace_permission_overrides (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid NOT NULL,
    user_id uuid NOT NULL,
    scope text NOT NULL,
    granted_by uuid,
    granted_at timestamp with time zone DEFAULT now() NOT NULL
);

-- TABLE public.workspace_prebuild_state
CREATE TABLE public.workspace_prebuild_state (
    workspace_id uuid NOT NULL,
    org_id uuid NOT NULL,
    families_seen jsonb DEFAULT '[]'::jsonb NOT NULL,
    last_cargo_at timestamp with time zone,
    last_npm_at timestamp with time zone,
    last_docker_at timestamp with time zone,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT workspace_prebuild_state_families_seen_check CHECK ((jsonb_typeof(families_seen) = 'array'::text))
);

-- TABLE public.workspace_pull_request_events
CREATE TABLE public.workspace_pull_request_events (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    pull_request_id uuid NOT NULL,
    event_type text NOT NULL,
    actor_id uuid,
    source_heads jsonb,
    target_heads jsonb,
    preflight jsonb,
    message text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT workspace_pull_request_events_event_type_check CHECK ((event_type = ANY (ARRAY['created'::text, 'refreshed'::text, 'source_rebased'::text, 'merge_blocked'::text, 'merged'::text, 'closed'::text, 'provider_created'::text, 'provider_updated'::text, 'provider_action_required'::text, 'provider_failed'::text, 'provider_merged'::text])))
);

ALTER TABLE ONLY public.workspace_pull_request_events FORCE ROW LEVEL SECURITY;

-- TABLE public.workspace_pull_request_provider_repos
CREATE TABLE public.workspace_pull_request_provider_repos (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    pull_request_id uuid NOT NULL,
    repo_id uuid NOT NULL,
    provider text NOT NULL,
    provider_repo_full_name text NOT NULL,
    source_remote_branch text NOT NULL,
    target_remote_branch text NOT NULL,
    provider_pr_number bigint,
    provider_pr_node_id text,
    provider_pr_url text,
    provider_status text DEFAULT 'pending_create'::text NOT NULL,
    source_head_sha text,
    target_head_sha text,
    provider_head_sha text,
    provider_base_sha text,
    last_error text,
    last_synced_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT workspace_pull_request_provider_repos_provider_check CHECK ((provider = 'github'::text)),
    CONSTRAINT workspace_pull_request_provider_repos_provider_pr_number_check CHECK (((provider_pr_number IS NULL) OR (provider_pr_number > 0))),
    CONSTRAINT workspace_pull_request_provider_repos_provider_status_check CHECK ((provider_status = ANY (ARRAY['pending_create'::text, 'open'::text, 'action_required'::text, 'failed'::text, 'merged'::text])))
);

ALTER TABLE ONLY public.workspace_pull_request_provider_repos FORCE ROW LEVEL SECURITY;

-- TABLE public.workspace_pull_requests
CREATE TABLE public.workspace_pull_requests (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    source_workspace_id uuid NOT NULL,
    target_workspace_id uuid NOT NULL,
    status text DEFAULT 'open'::text NOT NULL,
    local_merge_status text NOT NULL,
    provider_status text DEFAULT 'local_only'::text NOT NULL,
    title text NOT NULL,
    source_heads jsonb DEFAULT '[]'::jsonb NOT NULL,
    target_heads jsonb DEFAULT '[]'::jsonb NOT NULL,
    preflight jsonb DEFAULT '{}'::jsonb NOT NULL,
    last_preflight_fingerprint text NOT NULL,
    created_by uuid,
    merged_by uuid,
    merged_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    description_override text,
    CONSTRAINT workspace_pull_requests_check CHECK ((source_workspace_id <> target_workspace_id)),
    CONSTRAINT workspace_pull_requests_local_merge_status_check CHECK ((local_merge_status = ANY (ARRAY['ready'::text, 'conflict'::text, 'stale'::text, 'blocked'::text, 'merged'::text, 'closed'::text]))),
    CONSTRAINT workspace_pull_requests_provider_status_check CHECK ((provider_status = ANY (ARRAY['local_only'::text, 'pending'::text, 'open'::text, 'action_required'::text, 'failed'::text, 'partial'::text, 'merged'::text]))),
    CONSTRAINT workspace_pull_requests_status_check CHECK ((status = ANY (ARRAY['open'::text, 'merged'::text, 'closed'::text])))
);

ALTER TABLE ONLY public.workspace_pull_requests FORCE ROW LEVEL SECURITY;

-- TABLE public.workspace_settings
CREATE TABLE public.workspace_settings (
    workspace_id uuid NOT NULL,
    max_concurrent_tasks integer DEFAULT 4 NOT NULL,
    max_spend_usd numeric(10,2),
    spend_period text DEFAULT 'monthly'::text NOT NULL,
    spend_period_start timestamp with time zone,
    max_input_tokens bigint,
    max_output_tokens bigint,
    max_cloud_workers integer DEFAULT 8 NOT NULL,
    allow_cloud_workers boolean DEFAULT true NOT NULL,
    worker_idle_timeout_min integer DEFAULT 5 NOT NULL,
    tasks_per_worker integer DEFAULT 1 NOT NULL,
    worker_scaling_interval_sec integer DEFAULT 5 NOT NULL,
    embedding_model text DEFAULT 'text-embedding-3-small'::text NOT NULL,
    embedding_dimensions integer DEFAULT 1536 NOT NULL,
    embedding_provider text DEFAULT 'openai'::text NOT NULL,
    updated_by uuid,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    org_id uuid NOT NULL,
    chat_sticky_idle_timeout_min integer DEFAULT 60 NOT NULL,
    CONSTRAINT chk_workspace_settings_chat_sticky_idle_timeout_min CHECK (((chat_sticky_idle_timeout_min >= 0) AND (chat_sticky_idle_timeout_min <= 240))),
    CONSTRAINT workspace_settings_embedding_provider_check CHECK ((embedding_provider = ANY (ARRAY['openai'::text, 'local'::text, 'none'::text]))),
    CONSTRAINT workspace_settings_max_concurrent_tasks_check CHECK ((max_concurrent_tasks >= 1)),
    CONSTRAINT workspace_settings_spend_period_check CHECK ((spend_period = ANY (ARRAY['hourly'::text, 'daily'::text, 'weekly'::text, 'monthly'::text, 'total'::text])))
);

ALTER TABLE ONLY public.workspace_settings FORCE ROW LEVEL SECURITY;

-- TABLE public.workspaces
CREATE TABLE public.workspaces (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    org_id uuid NOT NULL,
    name text NOT NULL,
    is_base boolean DEFAULT false NOT NULL,
    status text DEFAULT 'idea'::text NOT NULL,
    description text,
    cloned_from_id uuid,
    docs_repo_id uuid,
    docs_branch_name text,
    docs_tracking_mode text DEFAULT 'pinned'::text NOT NULL,
    visibility text DEFAULT 'org'::text NOT NULL,
    created_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    push_mode text DEFAULT 'manual'::text NOT NULL,
    base_auto_pull_enabled boolean DEFAULT false NOT NULL,
    base_auto_pull_actor_user_id uuid,
    deleted_at timestamp with time zone,
    deleted_by uuid,
    mutation_policy text DEFAULT 'read_write'::text NOT NULL,
    mutation_policy_transition text,
    mutation_policy_updated_by uuid,
    mutation_policy_updated_at timestamp with time zone,
    stages integer DEFAULT 1 NOT NULL,
    merge_target_workspace_id uuid,
    -- P1.4: removed authority_advance_seq (bigint DEFAULT 0 NOT NULL) and CHECK workspaces_authority_advance_seq_check
    CONSTRAINT workspaces_base_auto_pull_actor_required_check CHECK (((NOT base_auto_pull_enabled) OR (base_auto_pull_actor_user_id IS NOT NULL))),
    CONSTRAINT workspaces_docs_tracking_mode_check CHECK ((docs_tracking_mode = ANY (ARRAY['tracking'::text, 'pinned'::text]))),
    CONSTRAINT workspaces_mutation_policy_check CHECK ((mutation_policy = ANY (ARRAY['read_write'::text, 'read_only'::text]))),
    CONSTRAINT workspaces_mutation_policy_transition_check CHECK ((mutation_policy_transition = 'freezing'::text)),
    CONSTRAINT workspaces_push_mode_check CHECK ((push_mode = ANY (ARRAY['manual'::text, 'auto'::text]))),
    CONSTRAINT workspaces_stages_check CHECK ((stages = ANY (ARRAY[1, 2]))),
    CONSTRAINT workspaces_status_check CHECK ((status = ANY (ARRAY['idea'::text, 'active'::text, 'blocked'::text, 'review'::text, 'integrated'::text]))),
    CONSTRAINT workspaces_visibility_check CHECK ((visibility = ANY (ARRAY['org'::text, 'invite_only'::text])))
);

ALTER TABLE ONLY public.workspaces FORCE ROW LEVEL SECURITY;

-- DEFAULT public.draft_updates seq
ALTER TABLE ONLY public.draft_updates ALTER COLUMN seq SET DEFAULT nextval('public.draft_updates_seq_seq'::regclass);

-- CONSTRAINT authz.session_ctx session_ctx_pkey
ALTER TABLE ONLY authz.session_ctx
    ADD CONSTRAINT session_ctx_pkey PRIMARY KEY (pid);

-- CONSTRAINT authz.user_sessions user_sessions_pkey
ALTER TABLE ONLY authz.user_sessions
    ADD CONSTRAINT user_sessions_pkey PRIMARY KEY (session_id);

-- CONSTRAINT public.agent_approval_handoffs agent_approval_handoffs_pkey
ALTER TABLE ONLY public.agent_approval_handoffs
    ADD CONSTRAINT agent_approval_handoffs_pkey PRIMARY KEY (delivery_id);

-- CONSTRAINT public.agent_approval_handoffs agent_approval_handoffs_session_id_approval_id_key
ALTER TABLE ONLY public.agent_approval_handoffs
    ADD CONSTRAINT agent_approval_handoffs_session_id_approval_id_key UNIQUE (session_id, approval_id);

-- CONSTRAINT public.api_keys api_keys_pkey
ALTER TABLE ONLY public.api_keys
    ADD CONSTRAINT api_keys_pkey PRIMARY KEY (id);

-- CONSTRAINT public.approval_configs approval_configs_pkey
ALTER TABLE ONLY public.approval_configs
    ADD CONSTRAINT approval_configs_pkey PRIMARY KEY (id);

-- CONSTRAINT public.approval_configs approval_configs_seq_key
ALTER TABLE ONLY public.approval_configs
    ADD CONSTRAINT approval_configs_seq_key UNIQUE (seq);

-- CONSTRAINT public.approval_definitions approval_definitions_org_id_id_version_key
ALTER TABLE ONLY public.approval_definitions
    ADD CONSTRAINT approval_definitions_org_id_id_version_key UNIQUE (org_id, id, version);

-- CONSTRAINT public.approval_definitions approval_definitions_pkey
ALTER TABLE ONLY public.approval_definitions
    ADD CONSTRAINT approval_definitions_pkey PRIMARY KEY (id, version);

-- CONSTRAINT public.approval_requests approval_requests_pkey
ALTER TABLE ONLY public.approval_requests
    ADD CONSTRAINT approval_requests_pkey PRIMARY KEY (id);

-- CONSTRAINT public.approval_requests approval_requests_seq_key
ALTER TABLE ONLY public.approval_requests
    ADD CONSTRAINT approval_requests_seq_key UNIQUE (seq);

-- CONSTRAINT public.approval_review_runs approval_review_runs_pkey
ALTER TABLE ONLY public.approval_review_runs
    ADD CONSTRAINT approval_review_runs_pkey PRIMARY KEY (job_id);

-- CONSTRAINT public.approval_statuses approval_statuses_pkey
ALTER TABLE ONLY public.approval_statuses
    ADD CONSTRAINT approval_statuses_pkey PRIMARY KEY (id);

-- CONSTRAINT public.approval_statuses approval_statuses_seq_key
ALTER TABLE ONLY public.approval_statuses
    ADD CONSTRAINT approval_statuses_seq_key UNIQUE (seq);

-- CONSTRAINT public.branch_integration_queue branch_integration_queue_pkey
ALTER TABLE ONLY public.branch_integration_queue
    ADD CONSTRAINT branch_integration_queue_pkey PRIMARY KEY (workspace_branch_repo_id);

-- CONSTRAINT public.branch_integration_remote_pair_leases branch_integration_remote_pair_leases_pkey
ALTER TABLE ONLY public.branch_integration_remote_pair_leases
    ADD CONSTRAINT branch_integration_remote_pair_leases_pkey PRIMARY KEY (repo_id, remote_branch);

-- CONSTRAINT public.branch_sync_runs branch_sync_runs_pkey
ALTER TABLE ONLY public.branch_sync_runs
    ADD CONSTRAINT branch_sync_runs_pkey PRIMARY KEY (id);

-- CONSTRAINT public.builder_task_specs builder_task_specs_pkey
ALTER TABLE ONLY public.builder_task_specs
    ADD CONSTRAINT builder_task_specs_pkey PRIMARY KEY (id);

-- CONSTRAINT public.chat_attachments chat_attachments_pkey
ALTER TABLE ONLY public.chat_attachments
    ADD CONSTRAINT chat_attachments_pkey PRIMARY KEY (id);

-- CONSTRAINT public.chat_message_context_resolutions chat_message_context_resolutions_pkey
ALTER TABLE ONLY public.chat_message_context_resolutions
    ADD CONSTRAINT chat_message_context_resolutions_pkey PRIMARY KEY (id);

-- CONSTRAINT public.chat_messages chat_messages_pkey
ALTER TABLE ONLY public.chat_messages
    ADD CONSTRAINT chat_messages_pkey PRIMARY KEY (id);

-- CONSTRAINT public.chat_session_context_links chat_session_context_links_pkey
ALTER TABLE ONLY public.chat_session_context_links
    ADD CONSTRAINT chat_session_context_links_pkey PRIMARY KEY (id);

-- CONSTRAINT public.chat_sessions chat_sessions_pkey
ALTER TABLE ONLY public.chat_sessions
    ADD CONSTRAINT chat_sessions_pkey PRIMARY KEY (id);

-- CONSTRAINT public.chat_sticky_runtimes chat_sticky_runtimes_pkey
ALTER TABLE ONLY public.chat_sticky_runtimes
    ADD CONSTRAINT chat_sticky_runtimes_pkey PRIMARY KEY (id);

-- CONSTRAINT public.cli_device_authorizations cli_device_authorizations_device_code_hash_key
ALTER TABLE ONLY public.cli_device_authorizations
    ADD CONSTRAINT cli_device_authorizations_device_code_hash_key UNIQUE (device_code_hash);

-- CONSTRAINT public.cli_device_authorizations cli_device_authorizations_pkey
ALTER TABLE ONLY public.cli_device_authorizations
    ADD CONSTRAINT cli_device_authorizations_pkey PRIMARY KEY (id);

-- CONSTRAINT public.cli_device_authorizations cli_device_authorizations_user_code_key
ALTER TABLE ONLY public.cli_device_authorizations
    ADD CONSTRAINT cli_device_authorizations_user_code_key UNIQUE (user_code);

-- CONSTRAINT public.cloud_orphan_sightings cloud_orphan_sightings_pkey
ALTER TABLE ONLY public.cloud_orphan_sightings
    ADD CONSTRAINT cloud_orphan_sightings_pkey PRIMARY KEY (instance_name, zone);

-- CONSTRAINT public.comment_index comment_index_pkey
ALTER TABLE ONLY public.comment_index
    ADD CONSTRAINT comment_index_pkey PRIMARY KEY (id);

-- CONSTRAINT public.comment_mentions comment_mentions_pkey
ALTER TABLE ONLY public.comment_mentions
    ADD CONSTRAINT comment_mentions_pkey PRIMARY KEY (id);

-- CONSTRAINT public.comment_replies comment_replies_pkey
ALTER TABLE ONLY public.comment_replies
    ADD CONSTRAINT comment_replies_pkey PRIMARY KEY (id);

-- CONSTRAINT public.doc_folders doc_folders_pkey
ALTER TABLE ONLY public.doc_folders
    ADD CONSTRAINT doc_folders_pkey PRIMARY KEY (id);

-- CONSTRAINT public.doc_folders doc_folders_workspace_id_workspace_branch_repo_id_path_key
ALTER TABLE ONLY public.doc_folders
    ADD CONSTRAINT doc_folders_workspace_id_workspace_branch_repo_id_path_key UNIQUE (workspace_id, workspace_branch_repo_id, path);

-- CONSTRAINT public.doc_node_links doc_node_links_pkey
ALTER TABLE ONLY public.doc_node_links
    ADD CONSTRAINT doc_node_links_pkey PRIMARY KEY (doc_id, node_id);

-- CONSTRAINT public.docs docs_pkey
ALTER TABLE ONLY public.docs
    ADD CONSTRAINT docs_pkey PRIMARY KEY (id);

-- CONSTRAINT public.draft_participants draft_participants_pkey
ALTER TABLE ONLY public.draft_participants
    ADD CONSTRAINT draft_participants_pkey PRIMARY KEY (id);

-- CONSTRAINT public.draft_sessions draft_sessions_pkey
ALTER TABLE ONLY public.draft_sessions
    ADD CONSTRAINT draft_sessions_pkey PRIMARY KEY (id);

-- CONSTRAINT public.draft_updates draft_updates_pkey
ALTER TABLE ONLY public.draft_updates
    ADD CONSTRAINT draft_updates_pkey PRIMARY KEY (seq);

-- CONSTRAINT public.hotlist_events hotlist_events_pkey
ALTER TABLE ONLY public.hotlist_events
    ADD CONSTRAINT hotlist_events_pkey PRIMARY KEY (id);

-- CONSTRAINT public.hotlist_memberships hotlist_memberships_pkey
ALTER TABLE ONLY public.hotlist_memberships
    ADD CONSTRAINT hotlist_memberships_pkey PRIMARY KEY (hotlist_id, issue_id);

-- CONSTRAINT public.hotlist_slug_sequence hotlist_slug_sequence_pkey
ALTER TABLE ONLY public.hotlist_slug_sequence
    ADD CONSTRAINT hotlist_slug_sequence_pkey PRIMARY KEY (org_id);

-- CONSTRAINT public.hotlists hotlists_id_org_uniq
ALTER TABLE ONLY public.hotlists
    ADD CONSTRAINT hotlists_id_org_uniq UNIQUE (id, org_id);

-- CONSTRAINT public.hotlists hotlists_pkey
ALTER TABLE ONLY public.hotlists
    ADD CONSTRAINT hotlists_pkey PRIMARY KEY (id);

-- CONSTRAINT public.hotlists hotlists_slug_unique_per_org
ALTER TABLE ONLY public.hotlists
    ADD CONSTRAINT hotlists_slug_unique_per_org UNIQUE (org_id, slug);

-- CONSTRAINT public.human_escalation_questions human_escalation_questions_escalation_id_ordinal_key
ALTER TABLE ONLY public.human_escalation_questions
    ADD CONSTRAINT human_escalation_questions_escalation_id_ordinal_key UNIQUE (escalation_id, ordinal);

-- CONSTRAINT public.human_escalation_questions human_escalation_questions_escalation_id_question_key_key
ALTER TABLE ONLY public.human_escalation_questions
    ADD CONSTRAINT human_escalation_questions_escalation_id_question_key_key UNIQUE (escalation_id, question_key);

-- CONSTRAINT public.human_escalation_questions human_escalation_questions_pkey
ALTER TABLE ONLY public.human_escalation_questions
    ADD CONSTRAINT human_escalation_questions_pkey PRIMARY KEY (id);

-- CONSTRAINT public.human_escalations human_escalations_pkey
ALTER TABLE ONLY public.human_escalations
    ADD CONSTRAINT human_escalations_pkey PRIMARY KEY (id);

-- CONSTRAINT public.intent_record intent_record_pkey
ALTER TABLE ONLY public.intent_record
    ADD CONSTRAINT intent_record_pkey PRIMARY KEY (id);

-- CONSTRAINT public.issue_attachments issue_attachments_pkey
ALTER TABLE ONLY public.issue_attachments
    ADD CONSTRAINT issue_attachments_pkey PRIMARY KEY (id);

-- CONSTRAINT public.issue_attachments issue_attachments_storage_key_key
ALTER TABLE ONLY public.issue_attachments
    ADD CONSTRAINT issue_attachments_storage_key_key UNIQUE (storage_key);

-- CONSTRAINT public.issue_comments issue_comments_pkey
ALTER TABLE ONLY public.issue_comments
    ADD CONSTRAINT issue_comments_pkey PRIMARY KEY (id);

-- CONSTRAINT public.issue_components issue_components_pkey
ALTER TABLE ONLY public.issue_components
    ADD CONSTRAINT issue_components_pkey PRIMARY KEY (issue_id, ontology_node_id);

-- CONSTRAINT public.issue_events issue_events_pkey
ALTER TABLE ONLY public.issue_events
    ADD CONSTRAINT issue_events_pkey PRIMARY KEY (id);

-- CONSTRAINT public.issue_inference_jobs issue_inference_jobs_pkey
ALTER TABLE ONLY public.issue_inference_jobs
    ADD CONSTRAINT issue_inference_jobs_pkey PRIMARY KEY (issue_id);

-- CONSTRAINT public.issue_job_links issue_job_links_pkey
ALTER TABLE ONLY public.issue_job_links
    ADD CONSTRAINT issue_job_links_pkey PRIMARY KEY (issue_id, job_id);

-- CONSTRAINT public.issue_labels issue_labels_pkey
ALTER TABLE ONLY public.issue_labels
    ADD CONSTRAINT issue_labels_pkey PRIMARY KEY (issue_id, label);

-- CONSTRAINT public.issue_links issue_links_pkey
ALTER TABLE ONLY public.issue_links
    ADD CONSTRAINT issue_links_pkey PRIMARY KEY (id);

-- CONSTRAINT public.issue_slug_sequence issue_slug_sequence_pkey
ALTER TABLE ONLY public.issue_slug_sequence
    ADD CONSTRAINT issue_slug_sequence_pkey PRIMARY KEY (org_id);

-- CONSTRAINT public.issues issues_id_org_uniq
ALTER TABLE ONLY public.issues
    ADD CONSTRAINT issues_id_org_uniq UNIQUE (id, org_id);

-- CONSTRAINT public.issues issues_pkey
ALTER TABLE ONLY public.issues
    ADD CONSTRAINT issues_pkey PRIMARY KEY (id);

-- CONSTRAINT public.issues issues_slug_unique_per_org
ALTER TABLE ONLY public.issues
    ADD CONSTRAINT issues_slug_unique_per_org UNIQUE (org_id, slug);

-- CONSTRAINT public.job_merge_artifacts job_merge_artifacts_pkey
ALTER TABLE ONLY public.job_merge_artifacts
    ADD CONSTRAINT job_merge_artifacts_pkey PRIMARY KEY (id);

-- CONSTRAINT public.job_merge_runs job_merge_runs_pkey
ALTER TABLE ONLY public.job_merge_runs
    ADD CONSTRAINT job_merge_runs_pkey PRIMARY KEY (id);

-- CONSTRAINT public.job_preview_sessions job_preview_sessions_pkey
ALTER TABLE ONLY public.job_preview_sessions
    ADD CONSTRAINT job_preview_sessions_pkey PRIMARY KEY (id);

-- CONSTRAINT public.job_submission_receipts job_submission_receipts_job_id_key
ALTER TABLE ONLY public.job_submission_receipts
    ADD CONSTRAINT job_submission_receipts_job_id_key UNIQUE (job_id);

-- CONSTRAINT public.job_submission_receipts job_submission_receipts_pkey
ALTER TABLE ONLY public.job_submission_receipts
    ADD CONSTRAINT job_submission_receipts_pkey PRIMARY KEY (id);

-- CONSTRAINT public.job_submission_receipts job_submission_receipts_workspace_key_unique
ALTER TABLE ONLY public.job_submission_receipts
    ADD CONSTRAINT job_submission_receipts_workspace_key_unique UNIQUE (workspace_id, idempotency_key);

-- CONSTRAINT public.jobs jobs_id_org_workspace_unique
ALTER TABLE ONLY public.jobs
    ADD CONSTRAINT jobs_id_org_workspace_unique UNIQUE (id, org_id, workspace_id);

-- CHECK CONSTRAINT public.jobs jobs_job_type_check
ALTER TABLE public.jobs
    ADD CONSTRAINT jobs_job_type_check CHECK ((job_type = ANY (ARRAY['compile'::text, 'uplift'::text, 'downlift'::text, 'test'::text, 'review'::text, 'approval_review'::text, 'chat'::text, 'sync'::text, 'merge'::text, 'view_create'::text, 'preview'::text, 'explainer_generate'::text, 'golden_builder'::text, 'custom'::text]))) NOT VALID;

-- CONSTRAINT public.jobs jobs_pkey
ALTER TABLE ONLY public.jobs
    ADD CONSTRAINT jobs_pkey PRIMARY KEY (id);

-- CONSTRAINT public.merge_evaluations merge_evaluations_pkey
ALTER TABLE ONLY public.merge_evaluations
    ADD CONSTRAINT merge_evaluations_pkey PRIMARY KEY (id);

-- CONSTRAINT public.merge_evaluations merge_evaluations_seq_key
ALTER TABLE ONLY public.merge_evaluations
    ADD CONSTRAINT merge_evaluations_seq_key UNIQUE (seq);

-- CONSTRAINT public.node_dependencies node_dependencies_pkey
ALTER TABLE ONLY public.node_dependencies
    ADD CONSTRAINT node_dependencies_pkey PRIMARY KEY (from_node_id, to_node_id);

-- CONSTRAINT public.notifications notifications_pkey
ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_pkey PRIMARY KEY (id);

-- CONSTRAINT public.oauth_scopes oauth_scopes_pkey
ALTER TABLE ONLY public.oauth_scopes
    ADD CONSTRAINT oauth_scopes_pkey PRIMARY KEY (scope);

-- CONSTRAINT public.oauth_states oauth_states_pkey
ALTER TABLE ONLY public.oauth_states
    ADD CONSTRAINT oauth_states_pkey PRIMARY KEY (state);

-- CONSTRAINT public.ontology_grounding ontology_grounding_pkey
ALTER TABLE ONLY public.ontology_grounding
    ADD CONSTRAINT ontology_grounding_pkey PRIMARY KEY (id);

-- CONSTRAINT public.ontology_nodes ontology_nodes_pkey
ALTER TABLE ONLY public.ontology_nodes
    ADD CONSTRAINT ontology_nodes_pkey PRIMARY KEY (id);

-- CONSTRAINT public.ontology_nodes ontology_nodes_workspace_branch_id_parent_id_name_key
ALTER TABLE ONLY public.ontology_nodes
    ADD CONSTRAINT ontology_nodes_workspace_branch_id_parent_id_name_key UNIQUE (workspace_branch_id, parent_id, name);

-- CONSTRAINT public.org_configs org_configs_pkey
ALTER TABLE ONLY public.org_configs
    ADD CONSTRAINT org_configs_pkey PRIMARY KEY (org_id);

-- CONSTRAINT public.org_default_scopes org_default_scopes_pkey
ALTER TABLE ONLY public.org_default_scopes
    ADD CONSTRAINT org_default_scopes_pkey PRIMARY KEY (org_id, scope);

-- CONSTRAINT public.org_invitations org_invitations_org_id_email_status_key
ALTER TABLE ONLY public.org_invitations
    ADD CONSTRAINT org_invitations_org_id_email_status_key UNIQUE (org_id, email, status);

-- CONSTRAINT public.org_invitations org_invitations_pkey
ALTER TABLE ONLY public.org_invitations
    ADD CONSTRAINT org_invitations_pkey PRIMARY KEY (id);

-- CONSTRAINT public.org_memberships org_memberships_pkey
ALTER TABLE ONLY public.org_memberships
    ADD CONSTRAINT org_memberships_pkey PRIMARY KEY (org_id, user_id);

-- CONSTRAINT public.orgs orgs_pkey
ALTER TABLE ONLY public.orgs
    ADD CONSTRAINT orgs_pkey PRIMARY KEY (id);

-- CONSTRAINT public.orgs orgs_slug_key
ALTER TABLE ONLY public.orgs
    ADD CONSTRAINT orgs_slug_key UNIQUE (slug);

-- CONSTRAINT public.platform_issue_delivery_tombstone platform_issue_delivery_tombstone_pkey
ALTER TABLE ONLY public.platform_issue_delivery_tombstone
    ADD CONSTRAINT platform_issue_delivery_tombstone_pkey PRIMARY KEY (submission_id);

-- CONSTRAINT public.platform_issue_outbox platform_issue_outbox_pkey
ALTER TABLE ONLY public.platform_issue_outbox
    ADD CONSTRAINT platform_issue_outbox_pkey PRIMARY KEY (id);

-- P1.4: removed the pr1884_authority_cutover_* primary keys and unique constraints (pr1884_authority_cutover_branch_dispositions_pkey, pr1884_authority_cutover_branch_run_id_archived_branch_name_key, pr1884_authority_cutover_canonical_draft_retirements_pkey, pr1884_authority_cutover_job_archives_pkey, pr1884_authority_cutover_policy_overrides_pkey, pr1884_authority_cutover_runs_environment_database_name_can_key, pr1884_authority_cutover_runs_pkey, pr1884_authority_cutover_stale_draft_normalizations_pkey, pr1884_authority_cutover_task_archives_pkey, pr1884_authority_cutover_workspace_receipts_pkey).

-- CONSTRAINT public.provider_accounts provider_accounts_pkey
ALTER TABLE ONLY public.provider_accounts
    ADD CONSTRAINT provider_accounts_pkey PRIMARY KEY (id);

-- CONSTRAINT public.provider_quota_snapshots provider_quota_snapshots_pkey
ALTER TABLE ONLY public.provider_quota_snapshots
    ADD CONSTRAINT provider_quota_snapshots_pkey PRIMARY KEY (org_id, workspace_id, provider, auth_mode, account_id);

-- CONSTRAINT public.refresh_tokens refresh_tokens_pkey
ALTER TABLE ONLY public.refresh_tokens
    ADD CONSTRAINT refresh_tokens_pkey PRIMARY KEY (token);

-- CONSTRAINT public.repo_manifests repo_manifests_pkey
ALTER TABLE ONLY public.repo_manifests
    ADD CONSTRAINT repo_manifests_pkey PRIMARY KEY (worker_id, repo_id);

-- CONSTRAINT public.repo_secrets repo_secrets_pkey
ALTER TABLE ONLY public.repo_secrets
    ADD CONSTRAINT repo_secrets_pkey PRIMARY KEY (repo_id);

-- CONSTRAINT public.repos repos_org_id_slug_key
ALTER TABLE ONLY public.repos
    ADD CONSTRAINT repos_org_id_slug_key UNIQUE (org_id, slug);

-- CONSTRAINT public.repos repos_pkey
ALTER TABLE ONLY public.repos
    ADD CONSTRAINT repos_pkey PRIMARY KEY (id);

-- CONSTRAINT public.runtime_attribution_events runtime_attribution_events_pkey
ALTER TABLE ONLY public.runtime_attribution_events
    ADD CONSTRAINT runtime_attribution_events_pkey PRIMARY KEY (id);

-- CONSTRAINT public.search_documents search_documents_pkey
ALTER TABLE ONLY public.search_documents
    ADD CONSTRAINT search_documents_pkey PRIMARY KEY (id);

-- CONSTRAINT public.search_documents search_documents_workspace_branch_id_file_path_chunk_index_key
ALTER TABLE ONLY public.search_documents
    ADD CONSTRAINT search_documents_workspace_branch_id_file_path_chunk_index_key UNIQUE (workspace_branch_id, file_path, chunk_index);

-- CONSTRAINT public.search_reindex_queue search_reindex_queue_pkey
ALTER TABLE ONLY public.search_reindex_queue
    ADD CONSTRAINT search_reindex_queue_pkey PRIMARY KEY (id);

-- CONSTRAINT public.self_hosted_licenses self_hosted_licenses_license_key_hash_key
ALTER TABLE ONLY public.self_hosted_licenses
    ADD CONSTRAINT self_hosted_licenses_license_key_hash_key UNIQUE (license_key_hash);

-- CONSTRAINT public.self_hosted_licenses self_hosted_licenses_pkey
ALTER TABLE ONLY public.self_hosted_licenses
    ADD CONSTRAINT self_hosted_licenses_pkey PRIMARY KEY (id);

-- CONSTRAINT public.session_build_telemetry session_build_telemetry_pkey
ALTER TABLE ONLY public.session_build_telemetry
    ADD CONSTRAINT session_build_telemetry_pkey PRIMARY KEY (id);

-- CONSTRAINT public.task_checkpoints task_checkpoints_pkey
ALTER TABLE ONLY public.task_checkpoints
    ADD CONSTRAINT task_checkpoints_pkey PRIMARY KEY (id);

-- CONSTRAINT public.task_commit_frontier_cuts task_commit_frontier_cuts_pkey
ALTER TABLE ONLY public.task_commit_frontier_cuts
    ADD CONSTRAINT task_commit_frontier_cuts_pkey PRIMARY KEY (org_id, task_id, request_id, workspace_branch_repo_id);

-- CONSTRAINT public.task_commit_frontier_receipts task_commit_frontier_receipts_pkey
ALTER TABLE ONLY public.task_commit_frontier_receipts
    ADD CONSTRAINT task_commit_frontier_receipts_pkey PRIMARY KEY (org_id, receipt_id);

-- CONSTRAINT public.task_commit_requests task_commit_requests_pkey
ALTER TABLE ONLY public.task_commit_requests
    ADD CONSTRAINT task_commit_requests_pkey PRIMARY KEY (org_id, task_id, request_id);

-- CONSTRAINT public.task_completion_constraints task_completion_constraints_pkey
ALTER TABLE ONLY public.task_completion_constraints
    ADD CONSTRAINT task_completion_constraints_pkey PRIMARY KEY (id);

-- CONSTRAINT public.task_execution_attempts task_execution_attempts_pkey
ALTER TABLE ONLY public.task_execution_attempts
    ADD CONSTRAINT task_execution_attempts_pkey PRIMARY KEY (id);

-- CONSTRAINT public.task_question_windows task_question_windows_pkey
ALTER TABLE ONLY public.task_question_windows
    ADD CONSTRAINT task_question_windows_pkey PRIMARY KEY (id);

-- CONSTRAINT public.tasks tasks_pkey
ALTER TABLE ONLY public.tasks
    ADD CONSTRAINT tasks_pkey PRIMARY KEY (id);

-- CHECK CONSTRAINT public.tasks tasks_task_type_check
ALTER TABLE public.tasks
    ADD CONSTRAINT tasks_task_type_check CHECK ((task_type = ANY (ARRAY['bootstrap'::text, 'regular'::text, 'chat'::text, 'sync'::text, 'merge'::text, 'preview'::text, 'explainer_generate'::text, 'golden_builder'::text]))) NOT VALID;

-- CONSTRAINT public.timeline_entries timeline_entries_pkey
ALTER TABLE ONLY public.timeline_entries
    ADD CONSTRAINT timeline_entries_pkey PRIMARY KEY (id);

-- CONSTRAINT public.approval_review_runs uq_approval_review_runs_org_path
ALTER TABLE ONLY public.approval_review_runs
    ADD CONSTRAINT uq_approval_review_runs_org_path UNIQUE (org_id, source_workspace_id, output_doc_path);

-- CONSTRAINT public.approval_review_runs uq_approval_review_runs_org_publication
ALTER TABLE ONLY public.approval_review_runs
    ADD CONSTRAINT uq_approval_review_runs_org_publication UNIQUE (org_id, publication_operation_id);

-- CONSTRAINT public.approval_review_runs uq_approval_review_runs_org_resolution
ALTER TABLE ONLY public.approval_review_runs
    ADD CONSTRAINT uq_approval_review_runs_org_resolution UNIQUE (org_id, resolution_id);

-- CONSTRAINT public.approval_statuses uq_approval_statuses_org_id_id
ALTER TABLE ONLY public.approval_statuses
    ADD CONSTRAINT uq_approval_statuses_org_id_id UNIQUE (org_id, id);

-- CONSTRAINT public.chat_message_context_resolutions uq_chat_message_context_resolutions_turn_link
ALTER TABLE ONLY public.chat_message_context_resolutions
    ADD CONSTRAINT uq_chat_message_context_resolutions_turn_link UNIQUE (target_assistant_message_id, context_link_id);

-- CONSTRAINT public.job_merge_runs uq_job_merge_runs_org_id_id
ALTER TABLE ONLY public.job_merge_runs
    ADD CONSTRAINT uq_job_merge_runs_org_id_id UNIQUE (org_id, id);

-- CONSTRAINT public.jobs uq_jobs_org_id_id
ALTER TABLE ONLY public.jobs
    ADD CONSTRAINT uq_jobs_org_id_id UNIQUE (org_id, id);

-- CONSTRAINT public.comment_mentions uq_mention_per_content
ALTER TABLE ONLY public.comment_mentions
    ADD CONSTRAINT uq_mention_per_content UNIQUE NULLS NOT DISTINCT (comment_id, reply_id, mentioned_user_id);

-- CONSTRAINT public.merge_evaluations uq_merge_evaluations_run
ALTER TABLE ONLY public.merge_evaluations
    ADD CONSTRAINT uq_merge_evaluations_run UNIQUE (org_id, merge_run_id);

-- CONSTRAINT public.provider_accounts uq_provider_accounts_org_id_id
ALTER TABLE ONLY public.provider_accounts
    ADD CONSTRAINT uq_provider_accounts_org_id_id UNIQUE (org_id, id);

-- CONSTRAINT public.runtime_attribution_events uq_runtime_attribution_events_runtime_event
ALTER TABLE ONLY public.runtime_attribution_events
    ADD CONSTRAINT uq_runtime_attribution_events_runtime_event UNIQUE (runtime_id, event_id);

-- CONSTRAINT public.task_execution_attempts uq_task_execution_attempts_org_task_id
ALTER TABLE ONLY public.task_execution_attempts
    ADD CONSTRAINT uq_task_execution_attempts_org_task_id UNIQUE (org_id, task_id, id);

-- CONSTRAINT public.task_execution_attempts uq_task_execution_attempts_task_number
ALTER TABLE ONLY public.task_execution_attempts
    ADD CONSTRAINT uq_task_execution_attempts_task_number UNIQUE (task_id, attempt_number);

-- CONSTRAINT public.tasks uq_tasks_org_id_id
ALTER TABLE ONLY public.tasks
    ADD CONSTRAINT uq_tasks_org_id_id UNIQUE (org_id, id);

-- CONSTRAINT public.waitlist_signups uq_waitlist_email
ALTER TABLE ONLY public.waitlist_signups
    ADD CONSTRAINT uq_waitlist_email UNIQUE (email);

-- CONSTRAINT public.workers uq_workers_org_id_id
ALTER TABLE ONLY public.workers
    ADD CONSTRAINT uq_workers_org_id_id UNIQUE (org_id, id);

-- CONSTRAINT public.workspace_branches uq_workspace_branches_id_repo
ALTER TABLE ONLY public.workspace_branches
    ADD CONSTRAINT uq_workspace_branches_id_repo UNIQUE (id, repo_id);

-- CONSTRAINT public.workspace_branches uq_workspace_branches_repo_branch
ALTER TABLE ONLY public.workspace_branches
    ADD CONSTRAINT uq_workspace_branches_repo_branch UNIQUE (repo_id, branch_name);

-- CONSTRAINT public.workspace_branches uq_workspace_branches_ws_repo_branch
ALTER TABLE ONLY public.workspace_branches
    ADD CONSTRAINT uq_workspace_branches_ws_repo_branch UNIQUE (workspace_id, repo_id, branch_name);

-- P1.4: removed uq_workspace_head_operations_org_id_id

-- CONSTRAINT public.workspace_pull_requests uq_workspace_pull_requests_org_id_id
ALTER TABLE ONLY public.workspace_pull_requests
    ADD CONSTRAINT uq_workspace_pull_requests_org_id_id UNIQUE (org_id, id);

-- CONSTRAINT public.workspaces uq_workspaces_org_id_id
ALTER TABLE ONLY public.workspaces
    ADD CONSTRAINT uq_workspaces_org_id_id UNIQUE (org_id, id);

-- CONSTRAINT public.usage_records usage_records_pkey
ALTER TABLE ONLY public.usage_records
    ADD CONSTRAINT usage_records_pkey PRIMARY KEY (id);

-- CONSTRAINT public.user_configs user_configs_pkey
ALTER TABLE ONLY public.user_configs
    ADD CONSTRAINT user_configs_pkey PRIMARY KEY (org_id, user_id);

-- CONSTRAINT public.user_preferences user_preferences_pkey
ALTER TABLE ONLY public.user_preferences
    ADD CONSTRAINT user_preferences_pkey PRIMARY KEY (id);

-- CONSTRAINT public.user_preferences user_preferences_user_id_key
ALTER TABLE ONLY public.user_preferences
    ADD CONSTRAINT user_preferences_user_id_key UNIQUE (user_id);

-- CONSTRAINT public.user_scm_connections user_scm_connections_pkey
ALTER TABLE ONLY public.user_scm_connections
    ADD CONSTRAINT user_scm_connections_pkey PRIMARY KEY (id);

-- CONSTRAINT public.user_scm_connections user_scm_connections_user_id_provider_key
ALTER TABLE ONLY public.user_scm_connections
    ADD CONSTRAINT user_scm_connections_user_id_provider_key UNIQUE (user_id, provider);

-- CONSTRAINT public.user_scope_grants user_scope_grants_org_id_user_id_scope_key
ALTER TABLE ONLY public.user_scope_grants
    ADD CONSTRAINT user_scope_grants_org_id_user_id_scope_key UNIQUE (org_id, user_id, scope);

-- CONSTRAINT public.user_scope_grants user_scope_grants_pkey
ALTER TABLE ONLY public.user_scope_grants
    ADD CONSTRAINT user_scope_grants_pkey PRIMARY KEY (id);

-- CONSTRAINT public.users users_email_key
ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_email_key UNIQUE (email);

-- CONSTRAINT public.users users_oauth_provider_oauth_id_key
ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_oauth_provider_oauth_id_key UNIQUE (oauth_provider, oauth_id);

-- CONSTRAINT public.users users_pkey
ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);

-- CONSTRAINT public.waitlist_signups waitlist_signups_pkey
ALTER TABLE ONLY public.waitlist_signups
    ADD CONSTRAINT waitlist_signups_pkey PRIMARY KEY (id);

-- CONSTRAINT public.worker_hosts worker_hosts_pkey
ALTER TABLE ONLY public.worker_hosts
    ADD CONSTRAINT worker_hosts_pkey PRIMARY KEY (host_id);

-- CONSTRAINT public.worker_tokens worker_tokens_key_hash_key
ALTER TABLE ONLY public.worker_tokens
    ADD CONSTRAINT worker_tokens_key_hash_key UNIQUE (key_hash);

-- CONSTRAINT public.worker_tokens worker_tokens_pkey
ALTER TABLE ONLY public.worker_tokens
    ADD CONSTRAINT worker_tokens_pkey PRIMARY KEY (id);

-- CONSTRAINT public.workers workers_pkey
ALTER TABLE ONLY public.workers
    ADD CONSTRAINT workers_pkey PRIMARY KEY (id);

-- P1.4 begin
-- LEAN-TODO(P4.8): advance-consumer bookkeeping keyed by advance id; job-merge (P4.8) and workspace-PR (P6.4) consumers retire it.
-- P1.4 end
-- CONSTRAINT public.workspace_advance_consumer_receipts workspace_advance_adapter_receipts_pkey
ALTER TABLE ONLY public.workspace_advance_consumer_receipts
    ADD CONSTRAINT workspace_advance_adapter_receipts_pkey PRIMARY KEY (operation_id, consumer_kind);

-- P1.4 begin
-- LEAN-TODO(P4.8): advance-consumer bookkeeping keyed by advance id; job-merge (P4.8) and workspace-PR (P6.4) consumers retire it.
-- P1.4 end
-- CONSTRAINT public.workspace_advance_consumer_inputs workspace_advance_consumer_inputs_pkey
ALTER TABLE ONLY public.workspace_advance_consumer_inputs
    ADD CONSTRAINT workspace_advance_consumer_inputs_pkey PRIMARY KEY (operation_id, consumer_kind);

-- CONSTRAINT public.workspace_branch_repos workspace_branch_repos_pkey
ALTER TABLE ONLY public.workspace_branch_repos
    ADD CONSTRAINT workspace_branch_repos_pkey PRIMARY KEY (id);

-- CONSTRAINT public.workspace_branch_repos workspace_branch_repos_workspace_branch_id_repo_id_key
ALTER TABLE ONLY public.workspace_branch_repos
    ADD CONSTRAINT workspace_branch_repos_workspace_branch_id_repo_id_key UNIQUE (workspace_branch_id, repo_id);

-- CONSTRAINT public.workspace_branches workspace_branches_pkey
ALTER TABLE ONLY public.workspace_branches
    ADD CONSTRAINT workspace_branches_pkey PRIMARY KEY (id);

-- CONSTRAINT public.workspace_configs workspace_configs_pkey
ALTER TABLE ONLY public.workspace_configs
    ADD CONSTRAINT workspace_configs_pkey PRIMARY KEY (workspace_id);

-- CONSTRAINT public.workspace_dependencies workspace_dependencies_pkey
ALTER TABLE ONLY public.workspace_dependencies
    ADD CONSTRAINT workspace_dependencies_pkey PRIMARY KEY (workspace_id, depends_on);

-- P1.4: removed the head-operation registry primary keys and unique constraints (workspace_head_operation_make_remote_applications_pkey, workspace_head_operation_membership__operation_id_repo_slug_key, workspace_head_operation_membership__operation_id_root_path_key, workspace_head_operation_membership_proposals_pkey, workspace_head_operation_membership_proposals_repo_id_key, workspace_head_operation_repos_operation_id_repo_id_key, workspace_head_operation_repos_pkey, workspace_head_operation_source_repos_operation_id_repo_id_key, workspace_head_operation_source_repos_pkey, workspace_head_operations_pkey, workspace_head_operations_workspace_id_mode_idempotency_key_key).

-- CONSTRAINT public.workspace_members workspace_members_pkey
ALTER TABLE ONLY public.workspace_members
    ADD CONSTRAINT workspace_members_pkey PRIMARY KEY (workspace_id, user_id);

-- CONSTRAINT public.workspace_permission_overrides workspace_permission_overrides_pkey
ALTER TABLE ONLY public.workspace_permission_overrides
    ADD CONSTRAINT workspace_permission_overrides_pkey PRIMARY KEY (id);

-- CONSTRAINT public.workspace_permission_overrides workspace_permission_overrides_workspace_id_user_id_scope_key
ALTER TABLE ONLY public.workspace_permission_overrides
    ADD CONSTRAINT workspace_permission_overrides_workspace_id_user_id_scope_key UNIQUE (workspace_id, user_id, scope);

-- CONSTRAINT public.workspace_prebuild_state workspace_prebuild_state_pkey
ALTER TABLE ONLY public.workspace_prebuild_state
    ADD CONSTRAINT workspace_prebuild_state_pkey PRIMARY KEY (workspace_id);

-- CONSTRAINT public.workspace_pull_request_events workspace_pull_request_events_pkey
ALTER TABLE ONLY public.workspace_pull_request_events
    ADD CONSTRAINT workspace_pull_request_events_pkey PRIMARY KEY (id);

-- CONSTRAINT public.workspace_pull_request_provider_repos workspace_pull_request_provid_org_id_pull_request_id_repo_i_key
ALTER TABLE ONLY public.workspace_pull_request_provider_repos
    ADD CONSTRAINT workspace_pull_request_provid_org_id_pull_request_id_repo_i_key UNIQUE (org_id, pull_request_id, repo_id, provider);

-- CONSTRAINT public.workspace_pull_request_provider_repos workspace_pull_request_provider_repos_pkey
ALTER TABLE ONLY public.workspace_pull_request_provider_repos
    ADD CONSTRAINT workspace_pull_request_provider_repos_pkey PRIMARY KEY (id);

-- CONSTRAINT public.workspace_pull_requests workspace_pull_requests_pkey
ALTER TABLE ONLY public.workspace_pull_requests
    ADD CONSTRAINT workspace_pull_requests_pkey PRIMARY KEY (id);

-- CONSTRAINT public.workspace_settings workspace_settings_pkey
ALTER TABLE ONLY public.workspace_settings
    ADD CONSTRAINT workspace_settings_pkey PRIMARY KEY (workspace_id);

-- CONSTRAINT public.workspaces workspaces_pkey
ALTER TABLE ONLY public.workspaces
    ADD CONSTRAINT workspaces_pkey PRIMARY KEY (id);

-- INDEX authz.idx_user_sessions_expires
CREATE INDEX idx_user_sessions_expires ON authz.user_sessions USING btree (expires_at);

-- INDEX authz.idx_user_sessions_user
CREATE INDEX idx_user_sessions_user ON authz.user_sessions USING btree (user_id);

-- INDEX authz.uq_user_sessions_exact_context
CREATE UNIQUE INDEX uq_user_sessions_exact_context ON authz.user_sessions USING btree (user_id, org_hash, scope_hash);

-- COMMENT authz.INDEX uq_user_sessions_exact_context
COMMENT ON INDEX authz.uq_user_sessions_exact_context IS 'One reusable authz session row per exact actor + stored SHA-256 hash of canonical org-id set + stored SHA-256 hash of canonical scope set.';

-- INDEX public.approval_review_runs_active_request_unique
CREATE UNIQUE INDEX approval_review_runs_active_request_unique ON public.approval_review_runs USING btree (org_id, request_key) WHERE (state = ANY (ARRAY['pending_assignment'::text, 'assigned'::text, 'generated'::text, 'merged_to_draft'::text]));

-- INDEX public.approval_review_runs_status_unique
CREATE UNIQUE INDEX approval_review_runs_status_unique ON public.approval_review_runs USING btree (approval_status_id) WHERE (approval_status_id IS NOT NULL);

-- INDEX public.docs_explainer_repo_path_unique
CREATE UNIQUE INDEX docs_explainer_repo_path_unique ON public.docs USING btree (workspace_branch_repo_id, file_path) WHERE ((doc_type = 'explainer'::text) AND (file_path IS NOT NULL) AND (workspace_branch_repo_id IS NOT NULL));

-- INDEX public.docs_review_repo_path_unique
CREATE UNIQUE INDEX docs_review_repo_path_unique ON public.docs USING btree (workspace_branch_repo_id, file_path) WHERE ((doc_type = 'review'::text) AND (file_path ~ '^docs/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/reviews/agent-approval-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.md$'::text) AND (workspace_branch_repo_id IS NOT NULL));

-- INDEX public.hotlist_events_hotlist_idx
CREATE INDEX hotlist_events_hotlist_idx ON public.hotlist_events USING btree (hotlist_id, created_at);

-- INDEX public.hotlist_memberships_issue_idx
CREATE INDEX hotlist_memberships_issue_idx ON public.hotlist_memberships USING btree (issue_id);

-- INDEX public.hotlist_memberships_ordered_idx
CREATE INDEX hotlist_memberships_ordered_idx ON public.hotlist_memberships USING btree (hotlist_id, "position", added_at);

-- INDEX public.hotlists_org_active_idx
CREATE INDEX hotlists_org_active_idx ON public.hotlists USING btree (org_id) WHERE (archived_at IS NULL);

-- INDEX public.idx_agent_approval_handoffs_resolving_session
CREATE INDEX idx_agent_approval_handoffs_resolving_session ON public.agent_approval_handoffs USING btree (session_id, created_at) WHERE (status = 'resolving'::text);

-- INDEX public.idx_api_keys_key_hash
CREATE UNIQUE INDEX idx_api_keys_key_hash ON public.api_keys USING btree (key_hash);

-- INDEX public.idx_api_keys_org_id
CREATE INDEX idx_api_keys_org_id ON public.api_keys USING btree (org_id);

-- INDEX public.idx_api_keys_org_prefix
CREATE UNIQUE INDEX idx_api_keys_org_prefix ON public.api_keys USING btree (org_id, key_prefix);

-- INDEX public.idx_approval_configs_target
CREATE INDEX idx_approval_configs_target ON public.approval_configs USING btree (org_id, target_workspace_id, approval_definition_id, seq DESC);

-- INDEX public.idx_approval_definitions_org
CREATE INDEX idx_approval_definitions_org ON public.approval_definitions USING btree (org_id, id, version DESC);

-- INDEX public.idx_approval_requests_human_inbox
CREATE INDEX idx_approval_requests_human_inbox ON public.approval_requests USING btree (org_id, actor_user_id, created_at, id) WHERE ((actor_type = 'human'::text) AND (state = 'pending'::text));

-- INDEX public.idx_approval_requests_source_history
CREATE INDEX idx_approval_requests_source_history ON public.approval_requests USING btree (org_id, source_workspace_id, approval_definition_id, seq DESC);

-- INDEX public.idx_approval_review_runs_request_history
CREATE INDEX idx_approval_review_runs_request_history ON public.approval_review_runs USING btree (org_id, source_workspace_id, approval_request_id, created_at, job_id);

-- INDEX public.idx_approval_statuses_eval
CREATE INDEX idx_approval_statuses_eval ON public.approval_statuses USING btree (org_id, source_workspace_id, approval_definition_id, actor_type, actor_user_id, seq DESC) INCLUDE (outcome, approval_definition_version);

-- INDEX public.idx_biq_approved_rebase_publish_order
CREATE INDEX idx_biq_approved_rebase_publish_order ON public.branch_integration_queue USING btree (requested_at, workspace_branch_repo_id) WHERE (trigger_kind = 'approved_rebase_publish'::text);

-- LEAN-TODO(P1.5): remote-tracking handoff ordering.
-- INDEX public.idx_branch_integration_queue_advance
CREATE INDEX idx_branch_integration_queue_advance ON public.branch_integration_queue USING btree (advance_id) WHERE (advance_id IS NOT NULL);

-- INDEX public.idx_branch_integration_remote_pair_leases_expiry
CREATE INDEX idx_branch_integration_remote_pair_leases_expiry ON public.branch_integration_remote_pair_leases USING btree (lease_expires_at);

-- INDEX public.idx_branch_sync_runs_acting_user
CREATE INDEX idx_branch_sync_runs_acting_user ON public.branch_sync_runs USING btree (acting_user_id, started_at DESC);

-- INDEX public.idx_branch_sync_runs_repo_recent
CREATE INDEX idx_branch_sync_runs_repo_recent ON public.branch_sync_runs USING btree (workspace_branch_repo_id, started_at DESC);

-- INDEX public.idx_builder_task_specs_active_coalesce
CREATE UNIQUE INDEX idx_builder_task_specs_active_coalesce ON public.builder_task_specs USING btree (workspace_id, golden_key) WHERE (status = ANY (ARRAY['queued'::text, 'dispatched'::text, 'building'::text, 'sealing'::text]));

-- INDEX public.idx_builder_task_specs_org
CREATE INDEX idx_builder_task_specs_org ON public.builder_task_specs USING btree (org_id);

-- INDEX public.idx_builder_task_specs_workspace_status
CREATE INDEX idx_builder_task_specs_workspace_status ON public.builder_task_specs USING btree (workspace_id, status);

-- INDEX public.idx_chat_attachments_message
CREATE INDEX idx_chat_attachments_message ON public.chat_attachments USING btree (message_id) WHERE (message_id IS NOT NULL);

-- INDEX public.idx_chat_attachments_session
CREATE INDEX idx_chat_attachments_session ON public.chat_attachments USING btree (session_id, created_at DESC);

-- INDEX public.idx_chat_message_context_resolutions_turn
CREATE INDEX idx_chat_message_context_resolutions_turn ON public.chat_message_context_resolutions USING btree (org_id, target_session_id, target_assistant_message_id);

-- INDEX public.idx_chat_session_context_links_source
CREATE INDEX idx_chat_session_context_links_source ON public.chat_session_context_links USING btree (org_id, source_session_id);

-- INDEX public.idx_chat_session_context_links_target
CREATE INDEX idx_chat_session_context_links_target ON public.chat_session_context_links USING btree (org_id, target_session_id, id);

-- INDEX public.idx_chat_sessions_attach_preview_task
CREATE INDEX idx_chat_sessions_attach_preview_task ON public.chat_sessions USING btree (attach_preview_task_id) WHERE (attach_preview_task_id IS NOT NULL);

-- INDEX public.idx_chat_sessions_required_worker_id
CREATE INDEX idx_chat_sessions_required_worker_id ON public.chat_sessions USING btree (required_worker_id) WHERE (required_worker_id IS NOT NULL);

-- INDEX public.idx_chat_sticky_runtimes_active_task
CREATE INDEX idx_chat_sticky_runtimes_active_task ON public.chat_sticky_runtimes USING btree (task_id) WHERE (status = 'active'::text);

-- INDEX public.idx_chat_sticky_runtimes_pending_question_expiry
CREATE INDEX idx_chat_sticky_runtimes_pending_question_expiry ON public.chat_sticky_runtimes USING btree (pending_question_expires_at) WHERE ((status = 'active'::text) AND (pending_question_expires_at IS NOT NULL));

-- INDEX public.idx_chat_sticky_runtimes_unreleased_teardown_task
CREATE INDEX idx_chat_sticky_runtimes_unreleased_teardown_task ON public.chat_sticky_runtimes USING btree (task_id) WHERE ((status = 'reaping'::text) OR ((teardown_operation_id IS NOT NULL) AND (teardown_workspace_released_at IS NULL)));

-- INDEX public.idx_chat_sticky_runtimes_worker_live
CREATE INDEX idx_chat_sticky_runtimes_worker_live ON public.chat_sticky_runtimes USING btree (worker_id) WHERE (status = ANY (ARRAY['active'::text, 'reaping'::text]));

-- INDEX public.idx_chat_sticky_runtimes_worker_lost
CREATE INDEX idx_chat_sticky_runtimes_worker_lost ON public.chat_sticky_runtimes USING btree (worker_id) WHERE (status = 'lost'::text);

-- INDEX public.idx_chat_sticky_runtimes_worker_reaping_teardown
CREATE INDEX idx_chat_sticky_runtimes_worker_reaping_teardown ON public.chat_sticky_runtimes USING btree (worker_id, teardown_deadline_at) WHERE ((status = 'reaping'::text) AND (teardown_operation_id IS NOT NULL) AND (teardown_acknowledged_at IS NULL));

-- INDEX public.idx_checkpoint_task
CREATE INDEX idx_checkpoint_task ON public.task_checkpoints USING btree (task_id, created_at DESC);

-- INDEX public.idx_ci_org
CREATE INDEX idx_ci_org ON public.comment_index USING btree (org_id);

-- INDEX public.idx_cli_device_authorizations_expires
CREATE INDEX idx_cli_device_authorizations_expires ON public.cli_device_authorizations USING btree (expires_at);

-- INDEX public.idx_cli_device_authorizations_user_code
CREATE INDEX idx_cli_device_authorizations_user_code ON public.cli_device_authorizations USING btree (user_code);

-- INDEX public.idx_cli_device_authorizations_workspace
CREATE INDEX idx_cli_device_authorizations_workspace ON public.cli_device_authorizations USING btree (workspace_id);

-- INDEX public.idx_cm_org
CREATE INDEX idx_cm_org ON public.chat_messages USING btree (org_id);

-- INDEX public.idx_comment_branch
CREATE INDEX idx_comment_branch ON public.comment_index USING btree (workspace_id, origin_branch);

-- INDEX public.idx_comment_file
CREATE INDEX idx_comment_file ON public.comment_index USING btree (workspace_id, file_path) WHERE (scope = 'line'::text);

-- INDEX public.idx_comment_gc
CREATE INDEX idx_comment_gc ON public.comment_index USING btree (branch_deleted, updated_at) WHERE (branch_deleted = true);

-- INDEX public.idx_comment_node
CREATE INDEX idx_comment_node ON public.comment_index USING btree (node_id) WHERE (scope = 'node'::text);

-- INDEX public.idx_comment_repo_file
CREATE INDEX idx_comment_repo_file ON public.comment_index USING btree (workspace_id, repo_id, file_path) WHERE ((scope = 'line'::text) AND (repo_id IS NOT NULL));

-- INDEX public.idx_cr_org
CREATE INDEX idx_cr_org ON public.comment_replies USING btree (org_id);

-- INDEX public.idx_cs_org
CREATE INDEX idx_cs_org ON public.chat_sessions USING btree (org_id);

-- INDEX public.idx_dnl_org
CREATE INDEX idx_dnl_org ON public.doc_node_links USING btree (org_id);

-- INDEX public.idx_doc_folders_branch_repo_path
CREATE INDEX idx_doc_folders_branch_repo_path ON public.doc_folders USING btree (workspace_branch_repo_id, path);

-- INDEX public.idx_doc_folders_workspace
CREATE INDEX idx_doc_folders_workspace ON public.doc_folders USING btree (workspace_id);

-- INDEX public.idx_doc_links_node
CREATE INDEX idx_doc_links_node ON public.doc_node_links USING btree (node_id);

-- INDEX public.idx_docs_org
CREATE INDEX idx_docs_org ON public.docs USING btree (org_id);

-- INDEX public.idx_docs_workspace_branch_repo
CREATE INDEX idx_docs_workspace_branch_repo ON public.docs USING btree (workspace_branch_repo_id);

-- INDEX public.idx_docs_workspace_branch_repo_path
CREATE INDEX idx_docs_workspace_branch_repo_path ON public.docs USING btree (workspace_branch_repo_id, file_path) WHERE (file_path IS NOT NULL);

-- INDEX public.idx_docs_workspace_view_types
CREATE INDEX idx_docs_workspace_view_types ON public.docs USING btree (workspace_id, doc_type) WHERE (doc_type = ANY (ARRAY['pm_view'::text, 'arch_view'::text, 'security_view'::text, 'performance_view'::text, 'test_view'::text, 'ui_view'::text, 'customized_view'::text]));

-- INDEX public.idx_dp_org
CREATE INDEX idx_dp_org ON public.draft_participants USING btree (org_id);

-- INDEX public.idx_dp_session
CREATE INDEX idx_dp_session ON public.draft_participants USING btree (session_id);

-- INDEX public.idx_draft_sessions_active_branch
CREATE INDEX idx_draft_sessions_active_branch ON public.draft_sessions USING btree (org_id, workspace_branch_repo_id) WHERE ((task_id IS NULL) AND (status = ANY (ARRAY['active'::text, 'committing'::text])));

-- INDEX public.idx_draft_sessions_active_task
CREATE INDEX idx_draft_sessions_active_task ON public.draft_sessions USING btree (org_id, task_id, workspace_branch_repo_id) WHERE ((task_id IS NOT NULL) AND (status = ANY (ARRAY['active'::text, 'committing'::text])));

-- INDEX public.idx_draft_sessions_pending_crdt_compaction
CREATE INDEX idx_draft_sessions_pending_crdt_compaction ON public.draft_sessions USING btree (pending_update_bytes DESC, pending_updates DESC) WHERE ((status = 'active'::text) AND (pending_updates > 0));

-- INDEX public.idx_draft_updates_session
CREATE INDEX idx_draft_updates_session ON public.draft_updates USING btree (session_id, seq);

-- INDEX public.idx_ds_org
CREATE INDEX idx_ds_org ON public.draft_sessions USING btree (org_id);

-- INDEX public.idx_du_org
CREATE INDEX idx_du_org ON public.draft_updates USING btree (org_id);

-- INDEX public.idx_grounding_edge_unique
CREATE UNIQUE INDEX idx_grounding_edge_unique ON public.ontology_grounding USING btree (workspace_branch_id, source_intent_record_id, target_intent_record_id);

-- INDEX public.idx_grounding_source
CREATE INDEX idx_grounding_source ON public.ontology_grounding USING btree (source_intent_record_id);

-- INDEX public.idx_grounding_target
CREATE INDEX idx_grounding_target ON public.ontology_grounding USING btree (target_intent_record_id);

-- INDEX public.idx_grounding_task_id
CREATE INDEX idx_grounding_task_id ON public.ontology_grounding USING btree (task_id);

-- INDEX public.idx_human_escalation_questions_escalation
CREATE INDEX idx_human_escalation_questions_escalation ON public.human_escalation_questions USING btree (org_id, escalation_id, ordinal);

-- INDEX public.idx_human_escalation_questions_status
CREATE INDEX idx_human_escalation_questions_status ON public.human_escalation_questions USING btree (org_id, status);

-- INDEX public.idx_human_escalations_job_status
CREATE INDEX idx_human_escalations_job_status ON public.human_escalations USING btree (job_id, status);

-- INDEX public.idx_human_escalations_one_active_per_task
CREATE UNIQUE INDEX idx_human_escalations_one_active_per_task ON public.human_escalations USING btree (task_id) WHERE (status = ANY (ARRAY['open'::text, 'in_review'::text]));

-- INDEX public.idx_human_escalations_org
CREATE INDEX idx_human_escalations_org ON public.human_escalations USING btree (org_id);

-- INDEX public.idx_human_escalations_task_status
CREATE INDEX idx_human_escalations_task_status ON public.human_escalations USING btree (task_id, status);

-- INDEX public.idx_human_escalations_workspace_status
CREATE INDEX idx_human_escalations_workspace_status ON public.human_escalations USING btree (workspace_id, status);

-- INDEX public.idx_intent_record_chat_session
CREATE INDEX idx_intent_record_chat_session ON public.intent_record USING btree (chat_session_id) WHERE (file_type = 'chat'::text);

-- INDEX public.idx_intent_record_code_unique
CREATE UNIQUE INDEX idx_intent_record_code_unique ON public.intent_record USING btree (workspace_branch_id, file_type, file_name, code_block_name) WHERE (file_type = 'code'::text);

-- INDEX public.idx_intent_record_file_lookup
CREATE INDEX idx_intent_record_file_lookup ON public.intent_record USING btree (file_type, file_name);

-- INDEX public.idx_intent_record_task_id
CREATE INDEX idx_intent_record_task_id ON public.intent_record USING btree (task_id);

-- INDEX public.idx_intent_record_view_ontology_unique
CREATE UNIQUE INDEX idx_intent_record_view_ontology_unique ON public.intent_record USING btree (workspace_branch_id, file_type, file_name, intent_record_number) WHERE (file_type = ANY (ARRAY['view'::text, 'ontology'::text]));

-- INDEX public.idx_invitation_email
CREATE INDEX idx_invitation_email ON public.org_invitations USING btree (email) WHERE (status = 'pending'::text);

-- INDEX public.idx_invitation_gc
CREATE INDEX idx_invitation_gc ON public.org_invitations USING btree (expires_at) WHERE (status = 'pending'::text);

-- INDEX public.idx_invitation_org
CREATE INDEX idx_invitation_org ON public.org_invitations USING btree (org_id, status);

-- INDEX public.idx_issue_attachments_issue_active
CREATE INDEX idx_issue_attachments_issue_active ON public.issue_attachments USING btree (issue_id, created_at) WHERE (deleted_at IS NULL);

-- INDEX public.idx_issue_attachments_org_created
CREATE INDEX idx_issue_attachments_org_created ON public.issue_attachments USING btree (org_id, created_at DESC);

-- INDEX public.idx_issue_attachments_scan_pending
CREATE INDEX idx_issue_attachments_scan_pending ON public.issue_attachments USING btree (scan_status, created_at) WHERE ((deleted_at IS NULL) AND (scan_status = 'pending'::text));

-- INDEX public.idx_job_branch
CREATE INDEX idx_job_branch ON public.jobs USING btree (workspace_id, branch_name) WHERE (branch_name IS NOT NULL);

-- INDEX public.idx_job_merge_artifacts_run
CREATE INDEX idx_job_merge_artifacts_run ON public.job_merge_artifacts USING btree (job_merge_run_id);

-- INDEX public.idx_job_merge_runs_source_workspace
CREATE INDEX idx_job_merge_runs_source_workspace ON public.job_merge_runs USING btree (source_workspace_id, created_at DESC);

-- INDEX public.idx_job_merge_runs_target_advancing_workspace
CREATE INDEX idx_job_merge_runs_target_advancing_workspace ON public.job_merge_runs USING btree (target_workspace_id, advances_target, created_at DESC);

-- INDEX public.idx_job_merge_runs_target_workspace
CREATE INDEX idx_job_merge_runs_target_workspace ON public.job_merge_runs USING btree (target_workspace_id, created_at DESC);

-- INDEX public.idx_job_merge_runs_workspace
CREATE INDEX idx_job_merge_runs_workspace ON public.job_merge_runs USING btree (workspace_id, created_at DESC);

-- INDEX public.idx_job_merge_runs_workspace_branch_repo
CREATE INDEX idx_job_merge_runs_workspace_branch_repo ON public.job_merge_runs USING btree (workspace_branch_repo_id, created_at DESC);

-- INDEX public.idx_job_predecessor
CREATE INDEX idx_job_predecessor ON public.jobs USING btree (depends_on_job) WHERE (status = 'pending'::text);

-- INDEX public.idx_job_preview_sessions_job
CREATE INDEX idx_job_preview_sessions_job ON public.job_preview_sessions USING btree (job_id);

-- INDEX public.idx_job_scheduling
CREATE INDEX idx_job_scheduling ON public.jobs USING btree (status, priority DESC, created_at) WHERE (status = ANY (ARRAY['pending'::text, 'bootstrapping'::text, 'running'::text]));

-- INDEX public.idx_job_workspace
CREATE INDEX idx_job_workspace ON public.jobs USING btree (workspace_id, status);

-- INDEX public.idx_jobs_approval_request
CREATE INDEX idx_jobs_approval_request ON public.jobs USING btree (org_id, (((approval_context ->> 'approvalRequestId'::text))::uuid)) WHERE (job_type = 'approval_review'::text);

-- INDEX public.idx_jobs_mode
CREATE INDEX idx_jobs_mode ON public.jobs USING btree (mode) WHERE (mode IS NOT NULL);

-- INDEX public.idx_jobs_org
CREATE INDEX idx_jobs_org ON public.jobs USING btree (org_id);

-- INDEX public.idx_mentions_comment
CREATE INDEX idx_mentions_comment ON public.comment_mentions USING btree (comment_id) WHERE (comment_id IS NOT NULL);

-- INDEX public.idx_mentions_user
CREATE INDEX idx_mentions_user ON public.comment_mentions USING btree (mentioned_user_id, created_at DESC);

-- INDEX public.idx_merge_evaluations_source
CREATE INDEX idx_merge_evaluations_source ON public.merge_evaluations USING btree (org_id, source_workspace_id, seq DESC);

-- INDEX public.idx_merge_evaluations_target
CREATE INDEX idx_merge_evaluations_target ON public.merge_evaluations USING btree (org_id, target_workspace_id, seq DESC);

-- INDEX public.idx_msg_fts
CREATE INDEX idx_msg_fts ON public.chat_messages USING gin (to_tsvector('english'::regconfig, content));

-- INDEX public.idx_ndep_org
CREATE INDEX idx_ndep_org ON public.node_dependencies USING btree (org_id);

-- INDEX public.idx_node_dep_from
CREATE INDEX idx_node_dep_from ON public.node_dependencies USING btree (from_node_id);

-- INDEX public.idx_node_dep_to
CREATE INDEX idx_node_dep_to ON public.node_dependencies USING btree (to_node_id);

-- INDEX public.idx_notifications_expiry
CREATE INDEX idx_notifications_expiry ON public.notifications USING btree (expires_at) WHERE ((expires_at IS NOT NULL) AND (read_at IS NULL));

-- INDEX public.idx_notifications_recipient
CREATE INDEX idx_notifications_recipient ON public.notifications USING btree (recipient_user_id, created_at DESC);

-- INDEX public.idx_notifications_workspace
CREATE INDEX idx_notifications_workspace ON public.notifications USING btree (workspace_id, recipient_user_id, created_at DESC);

-- INDEX public.idx_oauth_states_expires
CREATE INDEX idx_oauth_states_expires ON public.oauth_states USING btree (expires_at);

-- INDEX public.idx_on_org
CREATE INDEX idx_on_org ON public.ontology_nodes USING btree (org_id);

-- INDEX public.idx_on_workspace_branch_repo
CREATE INDEX idx_on_workspace_branch_repo ON public.ontology_nodes USING btree (workspace_branch_repo_id);

-- INDEX public.idx_onto_parent
CREATE INDEX idx_onto_parent ON public.ontology_nodes USING btree (workspace_branch_id, parent_id);

-- INDEX public.idx_onto_root_unique
CREATE UNIQUE INDEX idx_onto_root_unique ON public.ontology_nodes USING btree (workspace_branch_id, name) WHERE (parent_id IS NULL);

-- INDEX public.idx_onto_workspace_branch
CREATE INDEX idx_onto_workspace_branch ON public.ontology_nodes USING btree (workspace_branch_id);

-- INDEX public.idx_ontology_nodes_component_id_unique
CREATE UNIQUE INDEX idx_ontology_nodes_component_id_unique ON public.ontology_nodes USING btree (workspace_branch_id, component_id) WHERE (component_id IS NOT NULL);

-- INDEX public.idx_ppo_workspace
CREATE INDEX idx_ppo_workspace ON public.workspace_permission_overrides USING btree (workspace_id);

-- INDEX public.idx_refresh_tokens_expires
CREATE INDEX idx_refresh_tokens_expires ON public.refresh_tokens USING btree (expires_at);

-- INDEX public.idx_refresh_tokens_user
CREATE INDEX idx_refresh_tokens_user ON public.refresh_tokens USING btree (user_id);

-- INDEX public.idx_reindex_queue_cleanup
CREATE INDEX idx_reindex_queue_cleanup ON public.search_reindex_queue USING btree (status, completed_at) WHERE (status = ANY (ARRAY['completed'::text, 'failed'::text]));

-- INDEX public.idx_reindex_queue_org_status
CREATE INDEX idx_reindex_queue_org_status ON public.search_reindex_queue USING btree (org_id, status, created_at);

-- INDEX public.idx_reindex_queue_status
CREATE INDEX idx_reindex_queue_status ON public.search_reindex_queue USING btree (status, created_at) WHERE (status = 'pending'::text);

-- INDEX public.idx_remote_tracking_by_remote_branch
CREATE INDEX idx_remote_tracking_by_remote_branch ON public.workspace_branch_repos USING btree (org_id, repo_id, remote_branch) WHERE (tracking_mode = 'remote_tracking'::text);

-- INDEX public.idx_reply_comment
CREATE INDEX idx_reply_comment ON public.comment_replies USING btree (comment_id, created_at);

-- INDEX public.idx_repo_manifests_by_repo
CREATE INDEX idx_repo_manifests_by_repo ON public.repo_manifests USING btree (repo_id);

-- INDEX public.idx_repo_manifests_org_id
CREATE INDEX idx_repo_manifests_org_id ON public.repo_manifests USING btree (org_id);

-- INDEX public.idx_repo_manifests_stale
CREATE INDEX idx_repo_manifests_stale ON public.repo_manifests USING btree (worker_id) WHERE (upstream_head IS DISTINCT FROM local_head);

-- INDEX public.idx_repos_org_id_id
CREATE UNIQUE INDEX idx_repos_org_id_id ON public.repos USING btree (org_id, id);

-- INDEX public.idx_repos_org_provider_repo_id
CREATE UNIQUE INDEX idx_repos_org_provider_repo_id ON public.repos USING btree (org_id, provider, provider_repo_id) WHERE ((provider IS NOT NULL) AND (provider_repo_id IS NOT NULL));

-- INDEX public.idx_runtime_attribution_events_org_received
CREATE INDEX idx_runtime_attribution_events_org_received ON public.runtime_attribution_events USING btree (org_id, received_at);

-- INDEX public.idx_runtime_attribution_events_received
CREATE INDEX idx_runtime_attribution_events_received ON public.runtime_attribution_events USING btree (received_at);

-- INDEX public.idx_runtime_attribution_events_task_received
CREATE INDEX idx_runtime_attribution_events_task_received ON public.runtime_attribution_events USING btree (task_id, received_at);

-- INDEX public.idx_scm_conn_provider
CREATE INDEX idx_scm_conn_provider ON public.user_scm_connections USING btree (provider, provider_user_id);

-- INDEX public.idx_sd_org
CREATE INDEX idx_sd_org ON public.search_documents USING btree (org_id);

-- INDEX public.idx_search_branch
CREATE INDEX idx_search_branch ON public.search_documents USING btree (workspace_branch_id);

-- INDEX public.idx_search_embedding
CREATE INDEX idx_search_embedding ON public.search_documents USING hnsw (embedding public.vector_cosine_ops);

-- INDEX public.idx_search_fts
CREATE INDEX idx_search_fts ON public.search_documents USING gin (fts);

-- INDEX public.idx_self_hosted_licenses_customer_id
CREATE INDEX idx_self_hosted_licenses_customer_id ON public.self_hosted_licenses USING btree (customer_id);

-- INDEX public.idx_self_hosted_licenses_product_version
CREATE INDEX idx_self_hosted_licenses_product_version ON public.self_hosted_licenses USING btree (product, version);

-- INDEX public.idx_self_hosted_licenses_status
CREATE INDEX idx_self_hosted_licenses_status ON public.self_hosted_licenses USING btree (status);

-- INDEX public.idx_session_build_telemetry_org
CREATE INDEX idx_session_build_telemetry_org ON public.session_build_telemetry USING btree (org_id);

-- INDEX public.idx_session_build_telemetry_workspace_tool
CREATE INDEX idx_session_build_telemetry_workspace_tool ON public.session_build_telemetry USING btree (workspace_id, tool);

-- INDEX public.idx_session_search
CREATE INDEX idx_session_search ON public.chat_sessions USING gin (to_tsvector('english'::regconfig, ((COALESCE(title, ''::text) || ' '::text) || COALESCE(summary, ''::text))));

-- INDEX public.idx_session_task
CREATE INDEX idx_session_task ON public.chat_sessions USING btree (task_id) WHERE (task_id IS NOT NULL);

-- INDEX public.idx_session_workspace
CREATE INDEX idx_session_workspace ON public.chat_sessions USING btree (workspace_id, created_by, status, updated_at DESC);

-- INDEX public.idx_sticky_runtime_registration_retirement_reconciliation
CREATE INDEX idx_sticky_runtime_registration_retirement_reconciliation ON public.chat_sticky_runtimes USING btree (worker_id, host_id, id) WHERE ((teardown_reconciliation_kind = 'registration_retirement'::text) AND (teardown_reconciliation_receipt IS NULL) AND (teardown_workspace_released_at IS NULL));

-- INDEX public.idx_sticky_runtime_task_workspace_recovery
CREATE INDEX idx_sticky_runtime_task_workspace_recovery ON public.chat_sticky_runtimes USING btree (worker_id, host_id, id) WHERE ((teardown_reconciliation_kind = 'task_workspace_recovery'::text) AND (teardown_reconciliation_receipt IS NULL) AND (teardown_workspace_released_at IS NULL));

-- INDEX public.idx_task_commit_frontier_receipts_expiry
CREATE INDEX idx_task_commit_frontier_receipts_expiry ON public.task_commit_frontier_receipts USING btree (org_id, expires_at);

-- INDEX public.idx_task_commit_requests_pending
CREATE INDEX idx_task_commit_requests_pending ON public.task_commit_requests USING btree (org_id, updated_at) WHERE (status = 'pending'::text);

-- INDEX public.idx_task_completion_constraints_task
CREATE INDEX idx_task_completion_constraints_task ON public.task_completion_constraints USING btree (task_id);

-- INDEX public.idx_task_execution_attempts_org_started
CREATE INDEX idx_task_execution_attempts_org_started ON public.task_execution_attempts USING btree (org_id, started_at DESC, id DESC);

-- INDEX public.idx_task_execution_attempts_task_history
CREATE INDEX idx_task_execution_attempts_task_history ON public.task_execution_attempts USING btree (task_id, attempt_number DESC);

-- INDEX public.idx_task_job
CREATE INDEX idx_task_job ON public.tasks USING btree (job_id, status);

-- INDEX public.idx_task_pending
CREATE INDEX idx_task_pending ON public.tasks USING btree (job_id, status) WHERE (status = 'pending'::text);

-- INDEX public.idx_task_question_windows_expiry
CREATE INDEX idx_task_question_windows_expiry ON public.task_question_windows USING btree (expires_at, id) WHERE (state = 'open'::text);

-- INDEX public.idx_tasks_advisory_predecessor_job_id
CREATE INDEX idx_tasks_advisory_predecessor_job_id ON public.tasks USING btree (advisory_predecessor_job_id) WHERE (advisory_predecessor_job_id IS NOT NULL);

-- INDEX public.idx_tasks_checkpoint
CREATE INDEX idx_tasks_checkpoint ON public.tasks USING btree (job_id) WHERE (is_checkpoint = true);

-- INDEX public.idx_tasks_expired_claimed_leases
CREATE INDEX idx_tasks_expired_claimed_leases ON public.tasks USING btree (lease_expires_at) WHERE ((status = 'claimed'::text) AND (lease_expires_at IS NOT NULL));

-- INDEX public.idx_tasks_lease_expiry
CREATE INDEX idx_tasks_lease_expiry ON public.tasks USING btree (lease_expires_at) WHERE ((status = 'running'::text) AND (lease_expires_at IS NOT NULL));

-- INDEX public.idx_tasks_org
CREATE INDEX idx_tasks_org ON public.tasks USING btree (org_id);

-- INDEX public.idx_tc_org
CREATE INDEX idx_tc_org ON public.task_checkpoints USING btree (org_id);

-- INDEX public.idx_te_org
CREATE INDEX idx_te_org ON public.timeline_entries USING btree (org_id);

-- INDEX public.idx_timeline_task
CREATE INDEX idx_timeline_task ON public.timeline_entries USING btree (task_id, created_at DESC);

-- INDEX public.idx_usage_org_time
CREATE INDEX idx_usage_org_time ON public.usage_records USING btree (org_id, created_at DESC);

-- INDEX public.idx_usage_provider_model
CREATE INDEX idx_usage_provider_model ON public.usage_records USING btree (provider, model, created_at DESC);

-- INDEX public.idx_usage_session
CREATE INDEX idx_usage_session ON public.usage_records USING btree (session_id) WHERE (session_id IS NOT NULL);

-- INDEX public.idx_usage_task
CREATE INDEX idx_usage_task ON public.usage_records USING btree (task_id) WHERE (task_id IS NOT NULL);

-- INDEX public.idx_usage_workspace_time
CREATE INDEX idx_usage_workspace_time ON public.usage_records USING btree (workspace_id, created_at DESC);

-- INDEX public.idx_user_configs_user_id
CREATE INDEX idx_user_configs_user_id ON public.user_configs USING btree (user_id);

-- INDEX public.idx_usg_user
CREATE INDEX idx_usg_user ON public.user_scope_grants USING btree (user_id, org_id);

-- INDEX public.idx_waitlist_created
CREATE INDEX idx_waitlist_created ON public.waitlist_signups USING btree (created_at DESC);

-- INDEX public.idx_wb_org
CREATE INDEX idx_wb_org ON public.workspace_branches USING btree (org_id);

-- INDEX public.idx_wbr_materialized
CREATE INDEX idx_wbr_materialized ON public.workspace_branch_repos USING btree (workspace_branch_id, repo_id, repo_branch_name) WHERE (materialization_status = 'materialized'::text);

-- INDEX public.idx_wbr_org
CREATE INDEX idx_wbr_org ON public.workspace_branch_repos USING btree (org_id);

-- INDEX public.idx_wbr_overlay
CREATE INDEX idx_wbr_overlay ON public.workspace_branch_repos USING btree (workspace_branch_id);

-- INDEX public.idx_wbr_repo
CREATE INDEX idx_wbr_repo ON public.workspace_branch_repos USING btree (repo_id);

-- INDEX public.idx_wdep_org
CREATE INDEX idx_wdep_org ON public.workspace_dependencies USING btree (org_id);

-- INDEX public.idx_wmem_org
CREATE INDEX idx_wmem_org ON public.workspace_members USING btree (org_id);

-- INDEX public.idx_worker_hosts_last_heartbeat
CREATE INDEX idx_worker_hosts_last_heartbeat ON public.worker_hosts USING btree (last_heartbeat);

-- INDEX public.idx_worker_hosts_org_status
CREATE INDEX idx_worker_hosts_org_status ON public.worker_hosts USING btree (org_id, status);

-- INDEX public.idx_worker_hosts_orphan_reconciler
CREATE INDEX idx_worker_hosts_orphan_reconciler ON public.worker_hosts USING btree (created_at) WHERE ((status = 'provisioning'::text) AND (cloud_instance_id IS NULL) AND (provision_error_code IS NULL));

-- INDEX public.idx_worker_hosts_provision_retry
CREATE INDEX idx_worker_hosts_provision_retry ON public.worker_hosts USING btree (provision_next_retry_at) WHERE ((status = 'provisioning'::text) AND (cloud_instance_id IS NULL) AND (provision_error_code IS NOT NULL));

-- INDEX public.idx_worker_tokens_key_hash
CREATE UNIQUE INDEX idx_worker_tokens_key_hash ON public.worker_tokens USING btree (key_hash);

-- INDEX public.idx_worker_tokens_org_id
CREATE INDEX idx_worker_tokens_org_id ON public.worker_tokens USING btree (org_id);

-- INDEX public.idx_workers_display_name
CREATE INDEX idx_workers_display_name ON public.workers USING btree (display_name) WHERE (display_name IS NOT NULL);

-- INDEX public.idx_workers_external_host_id
CREATE INDEX idx_workers_external_host_id ON public.workers USING btree (external_host_id) WHERE (external_host_id IS NOT NULL);

-- INDEX public.idx_workers_host_id
CREATE INDEX idx_workers_host_id ON public.workers USING btree (host_id) WHERE (host_id IS NOT NULL);

-- INDEX public.idx_workers_one_preferred_per_org
CREATE UNIQUE INDEX idx_workers_one_preferred_per_org ON public.workers USING btree (org_id) WHERE (preferred AND (org_id IS NOT NULL));

-- P1.4 begin
-- LEAN-TODO(P4.8): advance-consumer bookkeeping keyed by advance id; job-merge (P4.8) and workspace-PR (P6.4) consumers retire it.
-- P1.4 end
-- INDEX public.idx_workspace_advance_consumer_inputs_pending
CREATE INDEX idx_workspace_advance_consumer_inputs_pending ON public.workspace_advance_consumer_inputs USING btree (org_id, consumer_kind, created_at);

-- INDEX public.idx_workspace_branches_authority_scope
CREATE INDEX idx_workspace_branches_authority_scope ON public.workspace_branches USING btree (workspace_id, authority_scope_kind, authority_scope_id) WHERE (authority_scope_kind IS NOT NULL);

-- INDEX public.idx_workspace_branches_cloned_from_branch_id
CREATE INDEX idx_workspace_branches_cloned_from_branch_id ON public.workspace_branches USING btree (cloned_from_branch_id);

-- INDEX public.idx_workspace_branches_materialized
CREATE INDEX idx_workspace_branches_materialized ON public.workspace_branches USING btree (workspace_id, repo_id, branch_name) WHERE (materialization_status = 'materialized'::text);

-- INDEX public.idx_workspace_configs_org_id
CREATE INDEX idx_workspace_configs_org_id ON public.workspace_configs USING btree (org_id);

-- P1.4: removed the head-operation registry indexes (idx_workspace_head_operation_membership_proposals_cleanup, idx_workspace_head_operation_membership_proposals_staging, idx_workspace_head_operation_producer, idx_workspace_head_operation_repos_projection, idx_workspace_head_operation_source, idx_workspace_head_operations_cleanup, idx_workspace_head_operations_live_lease, idx_workspace_head_operations_projection, idx_workspace_head_operations_superseded_by, idx_workspace_head_operations_target, idx_workspace_operation_make_remote_repo).

-- INDEX public.idx_workspace_pr_events_pr
CREATE INDEX idx_workspace_pr_events_pr ON public.workspace_pull_request_events USING btree (pull_request_id, created_at DESC);

-- INDEX public.idx_workspace_pr_source_status
CREATE INDEX idx_workspace_pr_source_status ON public.workspace_pull_requests USING btree (source_workspace_id, status, updated_at DESC);

-- INDEX public.idx_workspace_pr_target_status
CREATE INDEX idx_workspace_pr_target_status ON public.workspace_pull_requests USING btree (target_workspace_id, status, updated_at DESC);

-- INDEX public.idx_workspace_prebuild_state_org
CREATE INDEX idx_workspace_prebuild_state_org ON public.workspace_prebuild_state USING btree (org_id);

-- INDEX public.idx_workspaces_org_live
CREATE INDEX idx_workspaces_org_live ON public.workspaces USING btree (org_id, updated_at DESC) WHERE (deleted_at IS NULL);

-- INDEX public.idx_workspaces_org_updated
CREATE INDEX idx_workspaces_org_updated ON public.workspaces USING btree (org_id, updated_at DESC);

-- INDEX public.idx_wpr_provider_repos_pr
CREATE INDEX idx_wpr_provider_repos_pr ON public.workspace_pull_request_provider_repos USING btree (org_id, pull_request_id, updated_at DESC);

-- INDEX public.idx_wpr_provider_repos_repo
CREATE INDEX idx_wpr_provider_repos_repo ON public.workspace_pull_request_provider_repos USING btree (org_id, repo_id, provider_status);

-- INDEX public.idx_ws_dep_reverse
CREATE INDEX idx_ws_dep_reverse ON public.workspace_dependencies USING btree (depends_on);

-- INDEX public.idx_wset_org
CREATE INDEX idx_wset_org ON public.workspace_settings USING btree (org_id);

-- INDEX public.issue_comments_issue_idx
CREATE INDEX issue_comments_issue_idx ON public.issue_comments USING btree (issue_id, created_at);

-- INDEX public.issue_components_node_idx
CREATE INDEX issue_components_node_idx ON public.issue_components USING btree (ontology_node_id);

-- INDEX public.issue_events_issue_idx
CREATE INDEX issue_events_issue_idx ON public.issue_events USING btree (issue_id, created_at);

-- INDEX public.issue_inference_jobs_due_idx
CREATE INDEX issue_inference_jobs_due_idx ON public.issue_inference_jobs USING btree (status, next_run_at, created_at) WHERE (status = ANY (ARRAY['pending'::text, 'failed'::text]));

-- INDEX public.issue_inference_jobs_scheduled_task_idx
CREATE INDEX issue_inference_jobs_scheduled_task_idx ON public.issue_inference_jobs USING btree (scheduled_task_id) WHERE (scheduled_task_id IS NOT NULL);

-- INDEX public.issue_job_links_job_idx
CREATE INDEX issue_job_links_job_idx ON public.issue_job_links USING btree (job_id);

-- INDEX public.issue_labels_label_idx
CREATE INDEX issue_labels_label_idx ON public.issue_labels USING btree (label);

-- INDEX public.issue_links_dst_idx
CREATE INDEX issue_links_dst_idx ON public.issue_links USING btree (dst_issue_id, relation);

-- INDEX public.issue_links_src_idx
CREATE INDEX issue_links_src_idx ON public.issue_links USING btree (src_issue_id, relation);

-- INDEX public.issues_org_assignee_status
CREATE INDEX issues_org_assignee_status ON public.issues USING btree (org_id, assignee_id, status);

-- INDEX public.issues_org_created_at_idx
CREATE INDEX issues_org_created_at_idx ON public.issues USING btree (org_id, created_at DESC);

-- INDEX public.issues_org_status_idx
CREATE INDEX issues_org_status_idx ON public.issues USING btree (org_id, status);

-- INDEX public.job_submission_receipts_org_workspace_idx
CREATE INDEX job_submission_receipts_org_workspace_idx ON public.job_submission_receipts USING btree (org_id, workspace_id);

-- INDEX public.platform_issue_outbox_due_idx
CREATE INDEX platform_issue_outbox_due_idx ON public.platform_issue_outbox USING btree (status, next_run_at, created_at) WHERE (status = ANY (ARRAY['pending'::text, 'failed'::text, 'reconciling'::text]));

-- INDEX public.platform_issue_outbox_stale_processing_idx
CREATE INDEX platform_issue_outbox_stale_processing_idx ON public.platform_issue_outbox USING btree (locked_at) WHERE (status = 'processing'::text);

-- INDEX public.provider_accounts_org_idx
CREATE INDEX provider_accounts_org_idx ON public.provider_accounts USING btree (org_id);

-- INDEX public.provider_accounts_org_user_idx
CREATE INDEX provider_accounts_org_user_idx ON public.provider_accounts USING btree (org_id, user_id);

-- INDEX public.provider_accounts_org_workspace_idx
CREATE INDEX provider_accounts_org_workspace_idx ON public.provider_accounts USING btree (org_id, workspace_id);

-- INDEX public.provider_accounts_scope_name_key
CREATE UNIQUE INDEX provider_accounts_scope_name_key ON public.provider_accounts USING btree (org_id, scope, COALESCE(workspace_id, '00000000-0000-0000-0000-000000000000'::uuid), COALESCE(user_id, '00000000-0000-0000-0000-000000000000'::uuid), lower(name));

-- INDEX public.timeline_entries_explainer_exhausted_unique
CREATE UNIQUE INDEX timeline_entries_explainer_exhausted_unique ON public.timeline_entries USING btree (((metadata ->> 'sourceJobId'::text))) WHERE (((metadata ->> 'kind'::text) = 'explainer_auto_merge'::text) AND ((metadata ->> 'outcome'::text) = 'exhausted'::text));

-- INDEX public.uq_active_job_merge_per_source_job
CREATE UNIQUE INDEX uq_active_job_merge_per_source_job ON public.job_merge_runs USING btree (source_job_id) WHERE ((source_job_id IS NOT NULL) AND (status = ANY (ARRAY['pending'::text, 'running'::text, 'waiting_for_human'::text, 'rolling_back'::text])));

-- INDEX public.uq_active_job_merge_per_target_workspace
CREATE UNIQUE INDEX uq_active_job_merge_per_target_workspace ON public.job_merge_runs USING btree (target_workspace_id) WHERE ((target_workspace_id IS NOT NULL) AND (advances_target = true) AND (status = ANY (ARRAY['pending'::text, 'running'::text, 'waiting_for_human'::text, 'rolling_back'::text])));

-- INDEX public.uq_active_remote_tracking_merge_per_repo
CREATE UNIQUE INDEX uq_active_remote_tracking_merge_per_repo ON public.job_merge_runs USING btree (workspace_branch_repo_id) WHERE ((source_kind = 'remote_tracking'::text) AND (workspace_branch_repo_id IS NOT NULL) AND (status = ANY (ARRAY['pending'::text, 'running'::text, 'waiting_for_human'::text, 'rolling_back'::text])));

-- INDEX public.uq_active_workspace_merge_per_source_workspace
CREATE UNIQUE INDEX uq_active_workspace_merge_per_source_workspace ON public.job_merge_runs USING btree (source_workspace_id) WHERE ((source_kind = 'workspace'::text) AND (source_workspace_id IS NOT NULL) AND (status = ANY (ARRAY['pending'::text, 'running'::text, 'waiting_for_human'::text, 'rolling_back'::text])));

-- INDEX public.uq_approval_requests_active_key
CREATE UNIQUE INDEX uq_approval_requests_active_key ON public.approval_requests USING btree (org_id, source_workspace_id, request_key) WHERE (state = 'pending'::text);

-- INDEX public.uq_approval_statuses_agent_job
CREATE UNIQUE INDEX uq_approval_statuses_agent_job ON public.approval_statuses USING btree (org_id, agent_job_id) WHERE (agent_job_id IS NOT NULL);

-- INDEX public.uq_approval_statuses_agent_job_details
CREATE UNIQUE INDEX uq_approval_statuses_agent_job_details ON public.approval_statuses USING btree (org_id, (((agent_approval_run_details ->> 'agentJobId'::text))::uuid)) WHERE (actor_type = 'agent'::text);

-- INDEX public.uq_chat_attachments_storage_key
CREATE UNIQUE INDEX uq_chat_attachments_storage_key ON public.chat_attachments USING btree (storage_key);

-- INDEX public.uq_chat_messages_session_message_index
CREATE UNIQUE INDEX uq_chat_messages_session_message_index ON public.chat_messages USING btree (session_id, message_index);

-- INDEX public.uq_chat_session_context_links_active_pair
CREATE UNIQUE INDEX uq_chat_session_context_links_active_pair ON public.chat_session_context_links USING btree (target_session_id, source_session_id) WHERE (status = 'active'::text);

-- INDEX public.uq_chat_sticky_runtimes_live_session
CREATE UNIQUE INDEX uq_chat_sticky_runtimes_live_session ON public.chat_sticky_runtimes USING btree (chat_session_id) WHERE (status = ANY (ARRAY['active'::text, 'reaping'::text]));

-- INDEX public.uq_draft_sessions_branch_repo
CREATE UNIQUE INDEX uq_draft_sessions_branch_repo ON public.draft_sessions USING btree (workspace_branch_repo_id) WHERE (task_id IS NULL);

-- INDEX public.uq_draft_sessions_task_repo
CREATE UNIQUE INDEX uq_draft_sessions_task_repo ON public.draft_sessions USING btree (workspace_branch_repo_id, task_id) WHERE (task_id IS NOT NULL);

-- INDEX public.uq_job_merge_runs_completion_advance
CREATE UNIQUE INDEX uq_job_merge_runs_completion_advance ON public.job_merge_runs USING btree (completion_advance_id) WHERE (completion_advance_id IS NOT NULL);

-- INDEX public.uq_jobs_active_approval_review_request
CREATE UNIQUE INDEX uq_jobs_active_approval_review_request ON public.jobs USING btree (org_id, workspace_id, ((approval_context ->> 'requestKey'::text))) WHERE ((job_type = 'approval_review'::text) AND (status = ANY (ARRAY['pending'::text, 'bootstrapping'::text, 'running'::text, 'paused'::text])));

-- INDEX public.uq_open_workspace_pr_source_target
CREATE UNIQUE INDEX uq_open_workspace_pr_source_target ON public.workspace_pull_requests USING btree (source_workspace_id, target_workspace_id) WHERE (status = 'open'::text);


-- INDEX public.uq_task_execution_attempts_one_open_per_task
CREATE UNIQUE INDEX uq_task_execution_attempts_one_open_per_task ON public.task_execution_attempts USING btree (task_id) WHERE (ended_at IS NULL);

-- INDEX public.uq_task_question_windows_approval
CREATE UNIQUE INDEX uq_task_question_windows_approval ON public.task_question_windows USING btree (org_id, approval_id);

-- INDEX public.uq_task_question_windows_one_open_per_task
CREATE UNIQUE INDEX uq_task_question_windows_one_open_per_task ON public.task_question_windows USING btree (task_id) WHERE (state = 'open'::text);

-- INDEX public.uq_task_question_windows_platform_receipt
CREATE UNIQUE INDEX uq_task_question_windows_platform_receipt ON public.task_question_windows USING btree (platform_receipt_id) WHERE (platform_receipt_id IS NOT NULL);

-- INDEX public.uq_workers_live_external_host_id
CREATE UNIQUE INDEX uq_workers_live_external_host_id ON public.workers USING btree (external_host_id) WHERE ((external_host_id IS NOT NULL) AND (status <> 'terminated'::text));

-- INDEX public.uq_workspace_branches_authority_scope_repo
CREATE UNIQUE INDEX uq_workspace_branches_authority_scope_repo ON public.workspace_branches USING btree (authority_scope_kind, authority_scope_id, repo_id) WHERE (authority_scope_kind IS NOT NULL);

-- P1.4: removed the head-operation registry unique indexes (uq_workspace_head_operation_membership_proposal_docs, uq_workspace_head_operations_advance_seq).

-- TRIGGER public.repo_manifests set_org_id_repo_manifests
CREATE TRIGGER set_org_id_repo_manifests BEFORE INSERT ON public.repo_manifests FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.tasks tasks_stamp_async_decision_policy
CREATE TRIGGER tasks_stamp_async_decision_policy BEFORE INSERT ON public.tasks FOR EACH ROW EXECUTE FUNCTION public.stamp_task_async_decision_policy();

-- TRIGGER public.approval_configs trg_approval_configs_created_at
CREATE TRIGGER trg_approval_configs_created_at BEFORE INSERT ON public.approval_configs FOR EACH ROW EXECUTE FUNCTION public.set_approvals_governance_created_at();

-- TRIGGER public.approval_configs trg_approval_configs_set_org
CREATE TRIGGER trg_approval_configs_set_org BEFORE INSERT ON public.approval_configs FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.approval_definitions trg_approval_definitions_created_at
CREATE TRIGGER trg_approval_definitions_created_at BEFORE INSERT ON public.approval_definitions FOR EACH ROW EXECUTE FUNCTION public.set_approvals_governance_created_at();

-- TRIGGER public.approval_definitions trg_approval_definitions_set_org
CREATE TRIGGER trg_approval_definitions_set_org BEFORE INSERT ON public.approval_definitions FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.approval_requests trg_approval_requests_guard_update
CREATE TRIGGER trg_approval_requests_guard_update BEFORE UPDATE ON public.approval_requests FOR EACH ROW EXECUTE FUNCTION public.guard_approval_request_update();

-- TRIGGER public.approval_requests trg_approval_requests_insert_facts
CREATE TRIGGER trg_approval_requests_insert_facts BEFORE INSERT ON public.approval_requests FOR EACH ROW EXECUTE FUNCTION public.set_approval_request_insert_facts();

-- TRIGGER public.approval_requests trg_approval_requests_set_org
CREATE TRIGGER trg_approval_requests_set_org BEFORE INSERT ON public.approval_requests FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.approval_review_runs trg_approval_review_runs_set_org
CREATE TRIGGER trg_approval_review_runs_set_org BEFORE INSERT ON public.approval_review_runs FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.approval_review_runs trg_approval_review_runs_transition_guard
CREATE TRIGGER trg_approval_review_runs_transition_guard BEFORE INSERT OR UPDATE ON public.approval_review_runs FOR EACH ROW EXECUTE FUNCTION public.approval_review_runs_transition_guard();

-- TRIGGER public.approval_review_runs trg_approval_review_runs_verdict_immutable
CREATE TRIGGER trg_approval_review_runs_verdict_immutable BEFORE UPDATE ON public.approval_review_runs FOR EACH ROW EXECUTE FUNCTION public.approval_review_runs_verdict_immutable_guard();

-- TRIGGER public.approval_statuses trg_approval_statuses_created_at
CREATE TRIGGER trg_approval_statuses_created_at BEFORE INSERT ON public.approval_statuses FOR EACH ROW EXECUTE FUNCTION public.set_approvals_governance_created_at();

-- TRIGGER public.approval_statuses trg_approval_statuses_set_org
CREATE TRIGGER trg_approval_statuses_set_org BEFORE INSERT ON public.approval_statuses FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.approval_statuses trg_approval_statuses_v3_run_match
CREATE TRIGGER trg_approval_statuses_v3_run_match AFTER INSERT OR UPDATE ON public.approval_statuses FOR EACH ROW EXECUTE FUNCTION public.validate_agent_approval_status_v3_run();

-- TRIGGER public.approval_statuses trg_approval_statuses_v4_run_match
CREATE TRIGGER trg_approval_statuses_v4_run_match AFTER INSERT OR UPDATE ON public.approval_statuses FOR EACH ROW EXECUTE FUNCTION public.validate_agent_approval_status_v4_run();

-- TRIGGER public.branch_integration_queue trg_biq_set_org
CREATE TRIGGER trg_biq_set_org BEFORE INSERT ON public.branch_integration_queue FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.branch_integration_remote_pair_leases trg_branch_integration_remote_pair_leases_set_org
CREATE TRIGGER trg_branch_integration_remote_pair_leases_set_org BEFORE INSERT ON public.branch_integration_remote_pair_leases FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.branch_sync_runs trg_bsr_set_org
CREATE TRIGGER trg_bsr_set_org BEFORE INSERT ON public.branch_sync_runs FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.comment_index trg_ci_set_org
CREATE TRIGGER trg_ci_set_org BEFORE INSERT ON public.comment_index FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.tasks trg_clear_terminal_task_agent_auth
CREATE TRIGGER trg_clear_terminal_task_agent_auth BEFORE UPDATE OF status ON public.tasks FOR EACH ROW WHEN ((old.status IS DISTINCT FROM new.status)) EXECUTE FUNCTION public.clear_terminal_task_agent_auth();

-- TRIGGER public.chat_messages trg_cm_set_org
CREATE TRIGGER trg_cm_set_org BEFORE INSERT ON public.chat_messages FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.comment_replies trg_cr_set_org
CREATE TRIGGER trg_cr_set_org BEFORE INSERT ON public.comment_replies FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.chat_sessions trg_cs_set_org
CREATE TRIGGER trg_cs_set_org BEFORE INSERT ON public.chat_sessions FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.chat_sticky_runtimes trg_csr_set_org
CREATE TRIGGER trg_csr_set_org BEFORE INSERT ON public.chat_sticky_runtimes FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.doc_node_links trg_dnl_set_org
CREATE TRIGGER trg_dnl_set_org BEFORE INSERT ON public.doc_node_links FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.docs trg_docs_set_org
CREATE TRIGGER trg_docs_set_org BEFORE INSERT ON public.docs FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.draft_participants trg_dp_set_org
CREATE TRIGGER trg_dp_set_org BEFORE INSERT ON public.draft_participants FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.draft_sessions trg_ds_set_org
CREATE TRIGGER trg_ds_set_org BEFORE INSERT ON public.draft_sessions FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.draft_updates trg_du_set_org
CREATE TRIGGER trg_du_set_org BEFORE INSERT ON public.draft_updates FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.human_escalation_questions trg_heq_parent_org_guard
CREATE TRIGGER trg_heq_parent_org_guard BEFORE INSERT OR UPDATE OF org_id, escalation_id ON public.human_escalation_questions FOR EACH ROW EXECUTE FUNCTION public.human_escalation_questions_parent_org_guard();

-- TRIGGER public.human_escalation_questions trg_heq_set_org
CREATE TRIGGER trg_heq_set_org BEFORE INSERT ON public.human_escalation_questions FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.human_escalation_questions trg_heq_touch_updated_at
CREATE TRIGGER trg_heq_touch_updated_at BEFORE UPDATE ON public.human_escalation_questions FOR EACH ROW EXECUTE FUNCTION public.human_escalation_questions_touch_updated_at();

-- TRIGGER public.human_escalations trg_hesc_set_org
CREATE TRIGGER trg_hesc_set_org BEFORE INSERT ON public.human_escalations FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.job_merge_artifacts trg_jma_set_org
CREATE TRIGGER trg_jma_set_org BEFORE INSERT ON public.job_merge_artifacts FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.job_merge_runs trg_jmr_set_org
CREATE TRIGGER trg_jmr_set_org BEFORE INSERT ON public.job_merge_runs FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.job_submission_receipts trg_job_submission_receipts_touch_updated_at
CREATE TRIGGER trg_job_submission_receipts_touch_updated_at BEFORE UPDATE ON public.job_submission_receipts FOR EACH ROW EXECUTE FUNCTION public.job_submission_receipts_touch_updated_at();

-- TRIGGER public.jobs trg_jobs_approval_context_immutable
CREATE TRIGGER trg_jobs_approval_context_immutable BEFORE UPDATE OF approval_context ON public.jobs FOR EACH ROW EXECUTE FUNCTION public.reject_approval_context_update();

-- TRIGGER public.jobs trg_jobs_set_org
CREATE TRIGGER trg_jobs_set_org BEFORE INSERT ON public.jobs FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.job_preview_sessions trg_jps_set_org
CREATE TRIGGER trg_jps_set_org BEFORE INSERT ON public.job_preview_sessions FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.merge_evaluations trg_merge_evaluations_created_at
CREATE TRIGGER trg_merge_evaluations_created_at BEFORE INSERT ON public.merge_evaluations FOR EACH ROW EXECUTE FUNCTION public.set_approvals_governance_created_at();

-- TRIGGER public.merge_evaluations trg_merge_evaluations_set_org
CREATE TRIGGER trg_merge_evaluations_set_org BEFORE INSERT ON public.merge_evaluations FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.node_dependencies trg_ndep_set_org
CREATE TRIGGER trg_ndep_set_org BEFORE INSERT ON public.node_dependencies FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.ontology_nodes trg_on_set_org
CREATE TRIGGER trg_on_set_org BEFORE INSERT ON public.ontology_nodes FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.org_configs trg_org_configs_set_org
CREATE TRIGGER trg_org_configs_set_org BEFORE INSERT ON public.org_configs FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.provider_accounts trg_provider_accounts_set_org
CREATE TRIGGER trg_provider_accounts_set_org BEFORE INSERT ON public.provider_accounts FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.repo_secrets trg_repo_secrets_set_org
CREATE TRIGGER trg_repo_secrets_set_org BEFORE INSERT ON public.repo_secrets FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.repos trg_repos_set_org
CREATE TRIGGER trg_repos_set_org BEFORE INSERT ON public.repos FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.runtime_attribution_events trg_runtime_attribution_events_set_org
CREATE TRIGGER trg_runtime_attribution_events_set_org BEFORE INSERT ON public.runtime_attribution_events FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.search_documents trg_sd_set_org
CREATE TRIGGER trg_sd_set_org BEFORE INSERT ON public.search_documents FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.task_commit_frontier_cuts trg_task_commit_frontier_cuts_set_org
CREATE TRIGGER trg_task_commit_frontier_cuts_set_org BEFORE INSERT ON public.task_commit_frontier_cuts FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.task_commit_frontier_receipts trg_task_commit_frontier_receipts_set_org
CREATE TRIGGER trg_task_commit_frontier_receipts_set_org BEFORE INSERT ON public.task_commit_frontier_receipts FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.task_commit_requests trg_task_commit_requests_set_org
CREATE TRIGGER trg_task_commit_requests_set_org BEFORE INSERT ON public.task_commit_requests FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.tasks trg_tasks_guard_sticky_execution_admission
CREATE TRIGGER trg_tasks_guard_sticky_execution_admission BEFORE UPDATE OF status, worker_id, assigned_at, lease_expires_at, active_execution_attempt_id ON public.tasks FOR EACH ROW WHEN ((((new.status = 'claimed'::text) AND (old.status <> 'claimed'::text)) OR ((new.status = 'running'::text) AND (old.status <> 'running'::text)) OR ((old.worker_id IS NULL) AND (new.worker_id IS NOT NULL)) OR ((old.assigned_at IS NULL) AND (new.assigned_at IS NOT NULL)) OR ((old.lease_expires_at IS NULL) AND (new.lease_expires_at IS NOT NULL)) OR ((old.active_execution_attempt_id IS NULL) AND (new.active_execution_attempt_id IS NOT NULL)))) EXECUTE FUNCTION authz.guard_sticky_task_execution_admission();

-- TRIGGER public.tasks trg_tasks_set_org
CREATE TRIGGER trg_tasks_set_org BEFORE INSERT ON public.tasks FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.task_checkpoints trg_tc_set_org
CREATE TRIGGER trg_tc_set_org BEFORE INSERT ON public.task_checkpoints FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.timeline_entries trg_te_set_org
CREATE TRIGGER trg_te_set_org BEFORE INSERT ON public.timeline_entries FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.user_configs trg_user_configs_set_org
CREATE TRIGGER trg_user_configs_set_org BEFORE INSERT ON public.user_configs FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.workspace_branches trg_validate_workspace_branch_authority_scope
CREATE TRIGGER trg_validate_workspace_branch_authority_scope BEFORE INSERT OR UPDATE OF org_id, workspace_id, authority_scope_kind, job_id, task_id ON public.workspace_branches FOR EACH ROW EXECUTE FUNCTION public.validate_workspace_branch_authority_scope();

-- TRIGGER public.workspace_branches trg_wb_set_org
CREATE TRIGGER trg_wb_set_org BEFORE INSERT ON public.workspace_branches FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.workspace_branches trg_wb_sync_repo_entry
CREATE TRIGGER trg_wb_sync_repo_entry AFTER INSERT OR UPDATE OF repo_id, branch_name, base_branch ON public.workspace_branches FOR EACH ROW EXECUTE FUNCTION public.sync_workspace_branch_repo_from_branch();

-- TRIGGER public.workspace_branch_repos trg_wbr_default_root_path
CREATE TRIGGER trg_wbr_default_root_path BEFORE INSERT ON public.workspace_branch_repos FOR EACH ROW EXECUTE FUNCTION public.wbr_fill_default_root_path();

-- TRIGGER public.workspace_branch_repos trg_wbr_set_org
CREATE TRIGGER trg_wbr_set_org BEFORE INSERT ON public.workspace_branch_repos FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.workspace_dependencies trg_wdep_set_org
CREATE TRIGGER trg_wdep_set_org BEFORE INSERT ON public.workspace_dependencies FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.workspace_members trg_wmem_set_org
CREATE TRIGGER trg_wmem_set_org BEFORE INSERT ON public.workspace_members FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.workers trg_workers_guard_insert_dead_host
CREATE TRIGGER trg_workers_guard_insert_dead_host BEFORE INSERT ON public.workers FOR EACH ROW WHEN (((new.status IS DISTINCT FROM 'terminated'::text) AND ((new.host_id IS NOT NULL) OR (new.external_host_id IS NOT NULL)))) EXECUTE FUNCTION authz.guard_worker_insert_dead_host();

-- TRIGGER public.workers trg_workers_guard_revival_dead_host
CREATE TRIGGER trg_workers_guard_revival_dead_host BEFORE UPDATE OF status ON public.workers FOR EACH ROW WHEN (((old.status = 'terminated'::text) AND (new.status IS DISTINCT FROM 'terminated'::text))) EXECUTE FUNCTION authz.guard_worker_revival_dead_host();

-- P1.4 begin
-- LEAN-TODO(P4.8): advance-consumer bookkeeping keyed by advance id; job-merge (P4.8) and workspace-PR (P6.4) consumers retire it.
-- P1.4 end
-- TRIGGER public.workspace_advance_consumer_receipts trg_workspace_advance_adapter_receipts_set_org
CREATE TRIGGER trg_workspace_advance_adapter_receipts_set_org BEFORE INSERT ON public.workspace_advance_consumer_receipts FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- P1.4 begin
-- LEAN-TODO(P4.8): advance-consumer bookkeeping keyed by advance id; job-merge (P4.8) and workspace-PR (P6.4) consumers retire it.
-- P1.4 end
-- TRIGGER public.workspace_advance_consumer_inputs trg_workspace_advance_consumer_inputs_set_org
CREATE TRIGGER trg_workspace_advance_consumer_inputs_set_org BEFORE INSERT ON public.workspace_advance_consumer_inputs FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.workspace_branch_repos trg_workspace_branch_repo_base_version
CREATE TRIGGER trg_workspace_branch_repo_base_version BEFORE INSERT OR UPDATE OF head_commit, base_branch, base_head_commit ON public.workspace_branch_repos FOR EACH ROW EXECUTE FUNCTION public.maintain_workspace_branch_repo_base_version();

-- TRIGGER public.workspace_configs trg_workspace_configs_set_org
CREATE TRIGGER trg_workspace_configs_set_org BEFORE INSERT ON public.workspace_configs FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- P1.4: removed the head-operation registry triggers (trg_workspace_head_operation_membership_proposals_validate, trg_workspace_head_operation_repos_set_org, trg_workspace_head_operation_source_repos_set_org, trg_workspace_head_operations_set_org, trg_workspace_head_operations_validate_source, trg_workspace_head_operations_validate_target, trg_workspace_operation_make_remote_applications_validate).

-- TRIGGER public.workspace_pull_request_provider_repos trg_wpr_provider_set_org
CREATE TRIGGER trg_wpr_provider_set_org BEFORE INSERT ON public.workspace_pull_request_provider_repos FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.workspace_pull_requests trg_wpr_set_org
CREATE TRIGGER trg_wpr_set_org BEFORE INSERT ON public.workspace_pull_requests FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.workspace_pull_request_events trg_wpre_set_org
CREATE TRIGGER trg_wpre_set_org BEFORE INSERT ON public.workspace_pull_request_events FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- TRIGGER public.workspace_settings trg_wset_set_org
CREATE TRIGGER trg_wset_set_org BEFORE INSERT ON public.workspace_settings FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

-- FK CONSTRAINT public.agent_approval_handoffs agent_approval_handoffs_org_id_fkey
ALTER TABLE ONLY public.agent_approval_handoffs
    ADD CONSTRAINT agent_approval_handoffs_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.agent_approval_handoffs agent_approval_handoffs_request_entry_id_fkey
ALTER TABLE ONLY public.agent_approval_handoffs
    ADD CONSTRAINT agent_approval_handoffs_request_entry_id_fkey FOREIGN KEY (request_entry_id) REFERENCES public.timeline_entries(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.agent_approval_handoffs agent_approval_handoffs_session_id_fkey
ALTER TABLE ONLY public.agent_approval_handoffs
    ADD CONSTRAINT agent_approval_handoffs_session_id_fkey FOREIGN KEY (session_id) REFERENCES public.chat_sessions(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.agent_approval_handoffs agent_approval_handoffs_task_id_fkey
ALTER TABLE ONLY public.agent_approval_handoffs
    ADD CONSTRAINT agent_approval_handoffs_task_id_fkey FOREIGN KEY (task_id) REFERENCES public.tasks(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.api_keys api_keys_org_id_fkey
ALTER TABLE ONLY public.api_keys
    ADD CONSTRAINT api_keys_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.api_keys api_keys_user_id_fkey
ALTER TABLE ONLY public.api_keys
    ADD CONSTRAINT api_keys_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);

-- FK CONSTRAINT public.approval_configs approval_configs_created_by_fkey
ALTER TABLE ONLY public.approval_configs
    ADD CONSTRAINT approval_configs_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.approval_configs approval_configs_org_id_approval_definition_id_definition__fkey
ALTER TABLE ONLY public.approval_configs
    ADD CONSTRAINT approval_configs_org_id_approval_definition_id_definition__fkey FOREIGN KEY (org_id, approval_definition_id, definition_version_at_write) REFERENCES public.approval_definitions(org_id, id, version);

-- FK CONSTRAINT public.approval_configs approval_configs_org_id_fkey
ALTER TABLE ONLY public.approval_configs
    ADD CONSTRAINT approval_configs_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.approval_configs approval_configs_org_id_target_workspace_id_fkey
ALTER TABLE ONLY public.approval_configs
    ADD CONSTRAINT approval_configs_org_id_target_workspace_id_fkey FOREIGN KEY (org_id, target_workspace_id) REFERENCES public.workspaces(org_id, id) ON DELETE RESTRICT;

-- FK CONSTRAINT public.approval_definitions approval_definitions_created_by_fkey
ALTER TABLE ONLY public.approval_definitions
    ADD CONSTRAINT approval_definitions_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.approval_definitions approval_definitions_org_id_fkey
ALTER TABLE ONLY public.approval_definitions
    ADD CONSTRAINT approval_definitions_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.approval_review_runs approval_review_runs_output_doc_id_fkey
ALTER TABLE ONLY public.approval_review_runs
    ADD CONSTRAINT approval_review_runs_output_doc_id_fkey FOREIGN KEY (output_doc_id) REFERENCES public.docs(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.approval_statuses approval_statuses_actor_user_id_fkey
ALTER TABLE ONLY public.approval_statuses
    ADD CONSTRAINT approval_statuses_actor_user_id_fkey FOREIGN KEY (actor_user_id) REFERENCES public.users(id);

-- FK CONSTRAINT public.approval_statuses approval_statuses_org_id_approval_definition_id_approval_d_fkey
ALTER TABLE ONLY public.approval_statuses
    ADD CONSTRAINT approval_statuses_org_id_approval_definition_id_approval_d_fkey FOREIGN KEY (org_id, approval_definition_id, approval_definition_version) REFERENCES public.approval_definitions(org_id, id, version);

-- FK CONSTRAINT public.approval_statuses approval_statuses_org_id_fkey
ALTER TABLE ONLY public.approval_statuses
    ADD CONSTRAINT approval_statuses_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.approval_statuses approval_statuses_org_id_source_workspace_id_fkey
ALTER TABLE ONLY public.approval_statuses
    ADD CONSTRAINT approval_statuses_org_id_source_workspace_id_fkey FOREIGN KEY (org_id, source_workspace_id) REFERENCES public.workspaces(org_id, id) ON DELETE RESTRICT;

-- P1.4 begin: removed branch_integration_queue_advance_id_fkey; advance_id stays a plain uuid (idx_branch_integration_queue_advance kept).
-- LEAN-TODO(P1.5): remote-tracking handoff ordering still reads advance_id; no registry row backs it.
-- P1.4 end

-- FK CONSTRAINT public.branch_integration_queue branch_integration_queue_org_id_fkey
ALTER TABLE ONLY public.branch_integration_queue
    ADD CONSTRAINT branch_integration_queue_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.branch_integration_queue branch_integration_queue_workspace_branch_repo_id_fkey
ALTER TABLE ONLY public.branch_integration_queue
    ADD CONSTRAINT branch_integration_queue_workspace_branch_repo_id_fkey FOREIGN KEY (workspace_branch_repo_id) REFERENCES public.workspace_branch_repos(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.branch_integration_remote_pair_leases branch_integration_remote_pair_leases_org_id_fkey
ALTER TABLE ONLY public.branch_integration_remote_pair_leases
    ADD CONSTRAINT branch_integration_remote_pair_leases_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.branch_integration_remote_pair_leases branch_integration_remote_pair_leases_repo_org_fk
ALTER TABLE ONLY public.branch_integration_remote_pair_leases
    ADD CONSTRAINT branch_integration_remote_pair_leases_repo_org_fk FOREIGN KEY (org_id, repo_id) REFERENCES public.repos(org_id, id) ON DELETE CASCADE;

-- FK CONSTRAINT public.branch_sync_runs branch_sync_runs_acting_user_id_fkey
ALTER TABLE ONLY public.branch_sync_runs
    ADD CONSTRAINT branch_sync_runs_acting_user_id_fkey FOREIGN KEY (acting_user_id) REFERENCES public.users(id);

-- FK CONSTRAINT public.branch_sync_runs branch_sync_runs_merge_run_id_fkey
ALTER TABLE ONLY public.branch_sync_runs
    ADD CONSTRAINT branch_sync_runs_merge_run_id_fkey FOREIGN KEY (merge_run_id) REFERENCES public.job_merge_runs(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.branch_sync_runs branch_sync_runs_org_id_fkey
ALTER TABLE ONLY public.branch_sync_runs
    ADD CONSTRAINT branch_sync_runs_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.branch_sync_runs branch_sync_runs_workspace_branch_repo_id_fkey
ALTER TABLE ONLY public.branch_sync_runs
    ADD CONSTRAINT branch_sync_runs_workspace_branch_repo_id_fkey FOREIGN KEY (workspace_branch_repo_id) REFERENCES public.workspace_branch_repos(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.builder_task_specs builder_task_specs_workspace_id_fkey
ALTER TABLE ONLY public.builder_task_specs
    ADD CONSTRAINT builder_task_specs_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.chat_attachments chat_attachments_created_by_fkey
ALTER TABLE ONLY public.chat_attachments
    ADD CONSTRAINT chat_attachments_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.chat_attachments chat_attachments_message_id_fkey
ALTER TABLE ONLY public.chat_attachments
    ADD CONSTRAINT chat_attachments_message_id_fkey FOREIGN KEY (message_id) REFERENCES public.chat_messages(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.chat_attachments chat_attachments_org_id_fkey
ALTER TABLE ONLY public.chat_attachments
    ADD CONSTRAINT chat_attachments_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.chat_attachments chat_attachments_session_id_fkey
ALTER TABLE ONLY public.chat_attachments
    ADD CONSTRAINT chat_attachments_session_id_fkey FOREIGN KEY (session_id) REFERENCES public.chat_sessions(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.chat_messages chat_messages_session_id_fkey
ALTER TABLE ONLY public.chat_messages
    ADD CONSTRAINT chat_messages_session_id_fkey FOREIGN KEY (session_id) REFERENCES public.chat_sessions(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.chat_session_context_links chat_session_context_links_source_session_id_fkey
ALTER TABLE ONLY public.chat_session_context_links
    ADD CONSTRAINT chat_session_context_links_source_session_id_fkey FOREIGN KEY (source_session_id) REFERENCES public.chat_sessions(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.chat_session_context_links chat_session_context_links_target_session_id_fkey
ALTER TABLE ONLY public.chat_session_context_links
    ADD CONSTRAINT chat_session_context_links_target_session_id_fkey FOREIGN KEY (target_session_id) REFERENCES public.chat_sessions(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.chat_sessions chat_sessions_attach_preview_task_fk
ALTER TABLE ONLY public.chat_sessions
    ADD CONSTRAINT chat_sessions_attach_preview_task_fk FOREIGN KEY (org_id, attach_preview_task_id) REFERENCES public.tasks(org_id, id) ON DELETE SET NULL (attach_preview_task_id);

-- FK CONSTRAINT public.chat_sessions chat_sessions_created_by_fkey
ALTER TABLE ONLY public.chat_sessions
    ADD CONSTRAINT chat_sessions_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.chat_sessions chat_sessions_forked_from_id_fkey
ALTER TABLE ONLY public.chat_sessions
    ADD CONSTRAINT chat_sessions_forked_from_id_fkey FOREIGN KEY (forked_from_id) REFERENCES public.chat_sessions(id);

-- FK CONSTRAINT public.chat_sessions chat_sessions_task_id_fkey
ALTER TABLE ONLY public.chat_sessions
    ADD CONSTRAINT chat_sessions_task_id_fkey FOREIGN KEY (task_id) REFERENCES public.tasks(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.chat_sessions chat_sessions_workspace_id_fkey
ALTER TABLE ONLY public.chat_sessions
    ADD CONSTRAINT chat_sessions_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.chat_sticky_runtimes chat_sticky_runtimes_chat_session_id_fkey
ALTER TABLE ONLY public.chat_sticky_runtimes
    ADD CONSTRAINT chat_sticky_runtimes_chat_session_id_fkey FOREIGN KEY (chat_session_id) REFERENCES public.chat_sessions(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.chat_sticky_runtimes chat_sticky_runtimes_org_id_fkey
ALTER TABLE ONLY public.chat_sticky_runtimes
    ADD CONSTRAINT chat_sticky_runtimes_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.chat_sticky_runtimes chat_sticky_runtimes_workspace_id_fkey
ALTER TABLE ONLY public.chat_sticky_runtimes
    ADD CONSTRAINT chat_sticky_runtimes_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.cli_device_authorizations cli_device_authorizations_org_id_fkey
ALTER TABLE ONLY public.cli_device_authorizations
    ADD CONSTRAINT cli_device_authorizations_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.cli_device_authorizations cli_device_authorizations_user_id_fkey
ALTER TABLE ONLY public.cli_device_authorizations
    ADD CONSTRAINT cli_device_authorizations_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.cli_device_authorizations cli_device_authorizations_workspace_id_fkey
ALTER TABLE ONLY public.cli_device_authorizations
    ADD CONSTRAINT cli_device_authorizations_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.comment_index comment_index_author_id_fkey
ALTER TABLE ONLY public.comment_index
    ADD CONSTRAINT comment_index_author_id_fkey FOREIGN KEY (author_id) REFERENCES public.users(id);

-- FK CONSTRAINT public.comment_index comment_index_node_id_fkey
ALTER TABLE ONLY public.comment_index
    ADD CONSTRAINT comment_index_node_id_fkey FOREIGN KEY (node_id) REFERENCES public.ontology_nodes(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.comment_index comment_index_repo_id_fkey
ALTER TABLE ONLY public.comment_index
    ADD CONSTRAINT comment_index_repo_id_fkey FOREIGN KEY (repo_id) REFERENCES public.repos(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.comment_index comment_index_workspace_id_fkey
ALTER TABLE ONLY public.comment_index
    ADD CONSTRAINT comment_index_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.comment_mentions comment_mentions_comment_id_fkey
ALTER TABLE ONLY public.comment_mentions
    ADD CONSTRAINT comment_mentions_comment_id_fkey FOREIGN KEY (comment_id) REFERENCES public.comment_index(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.comment_mentions comment_mentions_mentioned_by_user_id_fkey
ALTER TABLE ONLY public.comment_mentions
    ADD CONSTRAINT comment_mentions_mentioned_by_user_id_fkey FOREIGN KEY (mentioned_by_user_id) REFERENCES public.users(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.comment_mentions comment_mentions_mentioned_user_id_fkey
ALTER TABLE ONLY public.comment_mentions
    ADD CONSTRAINT comment_mentions_mentioned_user_id_fkey FOREIGN KEY (mentioned_user_id) REFERENCES public.users(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.comment_mentions comment_mentions_reply_id_fkey
ALTER TABLE ONLY public.comment_mentions
    ADD CONSTRAINT comment_mentions_reply_id_fkey FOREIGN KEY (reply_id) REFERENCES public.comment_replies(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.comment_mentions comment_mentions_workspace_id_fkey
ALTER TABLE ONLY public.comment_mentions
    ADD CONSTRAINT comment_mentions_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.comment_replies comment_replies_author_id_fkey
ALTER TABLE ONLY public.comment_replies
    ADD CONSTRAINT comment_replies_author_id_fkey FOREIGN KEY (author_id) REFERENCES public.users(id);

-- FK CONSTRAINT public.comment_replies comment_replies_comment_id_fkey
ALTER TABLE ONLY public.comment_replies
    ADD CONSTRAINT comment_replies_comment_id_fkey FOREIGN KEY (comment_id) REFERENCES public.comment_index(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.doc_folders doc_folders_created_by_fkey
ALTER TABLE ONLY public.doc_folders
    ADD CONSTRAINT doc_folders_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.doc_folders doc_folders_org_id_fkey
ALTER TABLE ONLY public.doc_folders
    ADD CONSTRAINT doc_folders_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.doc_folders doc_folders_workspace_branch_id_fkey
ALTER TABLE ONLY public.doc_folders
    ADD CONSTRAINT doc_folders_workspace_branch_id_fkey FOREIGN KEY (workspace_branch_id) REFERENCES public.workspace_branches(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.doc_folders doc_folders_workspace_branch_repo_id_fkey
ALTER TABLE ONLY public.doc_folders
    ADD CONSTRAINT doc_folders_workspace_branch_repo_id_fkey FOREIGN KEY (workspace_branch_repo_id) REFERENCES public.workspace_branch_repos(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.doc_folders doc_folders_workspace_id_fkey
ALTER TABLE ONLY public.doc_folders
    ADD CONSTRAINT doc_folders_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.doc_node_links doc_node_links_doc_id_fkey
ALTER TABLE ONLY public.doc_node_links
    ADD CONSTRAINT doc_node_links_doc_id_fkey FOREIGN KEY (doc_id) REFERENCES public.docs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.doc_node_links doc_node_links_node_id_fkey
ALTER TABLE ONLY public.doc_node_links
    ADD CONSTRAINT doc_node_links_node_id_fkey FOREIGN KEY (node_id) REFERENCES public.ontology_nodes(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.docs docs_scope_node_id_fkey
ALTER TABLE ONLY public.docs
    ADD CONSTRAINT docs_scope_node_id_fkey FOREIGN KEY (scope_node_id) REFERENCES public.ontology_nodes(id);

-- FK CONSTRAINT public.docs docs_workspace_branch_id_fkey
ALTER TABLE ONLY public.docs
    ADD CONSTRAINT docs_workspace_branch_id_fkey FOREIGN KEY (workspace_branch_id) REFERENCES public.workspace_branches(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.docs docs_workspace_branch_repo_id_fkey
ALTER TABLE ONLY public.docs
    ADD CONSTRAINT docs_workspace_branch_repo_id_fkey FOREIGN KEY (workspace_branch_repo_id) REFERENCES public.workspace_branch_repos(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.docs docs_workspace_id_fkey
ALTER TABLE ONLY public.docs
    ADD CONSTRAINT docs_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.draft_participants draft_participants_session_id_fkey
ALTER TABLE ONLY public.draft_participants
    ADD CONSTRAINT draft_participants_session_id_fkey FOREIGN KEY (session_id) REFERENCES public.draft_sessions(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.draft_participants draft_participants_user_id_fkey
ALTER TABLE ONLY public.draft_participants
    ADD CONSTRAINT draft_participants_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);

-- FK CONSTRAINT public.draft_sessions draft_sessions_workspace_branch_id_fkey
ALTER TABLE ONLY public.draft_sessions
    ADD CONSTRAINT draft_sessions_workspace_branch_id_fkey FOREIGN KEY (workspace_branch_id) REFERENCES public.workspace_branches(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.draft_updates draft_updates_session_id_fkey
ALTER TABLE ONLY public.draft_updates
    ADD CONSTRAINT draft_updates_session_id_fkey FOREIGN KEY (session_id) REFERENCES public.draft_sessions(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.approval_requests fk_approval_requests_actor_user
ALTER TABLE ONLY public.approval_requests
    ADD CONSTRAINT fk_approval_requests_actor_user FOREIGN KEY (actor_user_id) REFERENCES public.users(id);

-- FK CONSTRAINT public.approval_requests fk_approval_requests_definition
ALTER TABLE ONLY public.approval_requests
    ADD CONSTRAINT fk_approval_requests_definition FOREIGN KEY (org_id, approval_definition_id, approval_definition_version) REFERENCES public.approval_definitions(org_id, id, version);

-- FK CONSTRAINT public.approval_requests fk_approval_requests_org
ALTER TABLE ONLY public.approval_requests
    ADD CONSTRAINT fk_approval_requests_org FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.approval_requests fk_approval_requests_requested_by
ALTER TABLE ONLY public.approval_requests
    ADD CONSTRAINT fk_approval_requests_requested_by FOREIGN KEY (requested_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.approval_requests fk_approval_requests_source_workspace
ALTER TABLE ONLY public.approval_requests
    ADD CONSTRAINT fk_approval_requests_source_workspace FOREIGN KEY (org_id, source_workspace_id) REFERENCES public.workspaces(org_id, id) ON DELETE RESTRICT;

-- FK CONSTRAINT public.approval_review_runs fk_approval_review_runs_definition
ALTER TABLE ONLY public.approval_review_runs
    ADD CONSTRAINT fk_approval_review_runs_definition FOREIGN KEY (org_id, definition_id, definition_version) REFERENCES public.approval_definitions(org_id, id, version) ON DELETE RESTRICT;

-- FK CONSTRAINT public.approval_review_runs fk_approval_review_runs_generated_attempt
ALTER TABLE ONLY public.approval_review_runs
    ADD CONSTRAINT fk_approval_review_runs_generated_attempt FOREIGN KEY (org_id, generated_task_id, generated_attempt_id) REFERENCES public.task_execution_attempts(org_id, task_id, id) ON DELETE RESTRICT;

-- FK CONSTRAINT public.approval_review_runs fk_approval_review_runs_generated_task
ALTER TABLE ONLY public.approval_review_runs
    ADD CONSTRAINT fk_approval_review_runs_generated_task FOREIGN KEY (org_id, generated_task_id) REFERENCES public.tasks(org_id, id) ON DELETE RESTRICT;

-- FK CONSTRAINT public.approval_review_runs fk_approval_review_runs_job
ALTER TABLE ONLY public.approval_review_runs
    ADD CONSTRAINT fk_approval_review_runs_job FOREIGN KEY (org_id, job_id) REFERENCES public.jobs(org_id, id) ON DELETE CASCADE;

-- FK CONSTRAINT public.approval_review_runs fk_approval_review_runs_output_repo
ALTER TABLE ONLY public.approval_review_runs
    ADD CONSTRAINT fk_approval_review_runs_output_repo FOREIGN KEY (org_id, output_repo_id) REFERENCES public.repos(org_id, id) ON DELETE RESTRICT;

-- P1.4 begin: removed fk_approval_review_runs_publication; publication_operation_id stays a plain uuid (uq_approval_review_runs_org_publication kept).
-- LEAN-TODO(P6.4): workspace pull requests still record the publication advance id; no registry row backs it.
-- P1.4 end

-- FK CONSTRAINT public.approval_review_runs fk_approval_review_runs_request
ALTER TABLE ONLY public.approval_review_runs
    ADD CONSTRAINT fk_approval_review_runs_request FOREIGN KEY (approval_request_id) REFERENCES public.approval_requests(id) ON DELETE RESTRICT;

-- FK CONSTRAINT public.approval_review_runs fk_approval_review_runs_resolved_attempt
ALTER TABLE ONLY public.approval_review_runs
    ADD CONSTRAINT fk_approval_review_runs_resolved_attempt FOREIGN KEY (org_id, resolved_bootstrap_task_id, resolved_attempt_id) REFERENCES public.task_execution_attempts(org_id, task_id, id) ON DELETE RESTRICT;

-- FK CONSTRAINT public.approval_review_runs fk_approval_review_runs_resolved_task
ALTER TABLE ONLY public.approval_review_runs
    ADD CONSTRAINT fk_approval_review_runs_resolved_task FOREIGN KEY (org_id, resolved_bootstrap_task_id) REFERENCES public.tasks(org_id, id) ON DELETE RESTRICT;

-- FK CONSTRAINT public.approval_review_runs fk_approval_review_runs_resolved_worker
ALTER TABLE ONLY public.approval_review_runs
    ADD CONSTRAINT fk_approval_review_runs_resolved_worker FOREIGN KEY (org_id, resolved_worker_id) REFERENCES public.workers(org_id, id) ON DELETE RESTRICT;

-- FK CONSTRAINT public.approval_review_runs fk_approval_review_runs_source_workspace
ALTER TABLE ONLY public.approval_review_runs
    ADD CONSTRAINT fk_approval_review_runs_source_workspace FOREIGN KEY (org_id, source_workspace_id) REFERENCES public.workspaces(org_id, id) ON DELETE RESTRICT;

-- FK CONSTRAINT public.approval_review_runs fk_approval_review_runs_status
ALTER TABLE ONLY public.approval_review_runs
    ADD CONSTRAINT fk_approval_review_runs_status FOREIGN KEY (org_id, approval_status_id) REFERENCES public.approval_statuses(org_id, id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED;

-- FK CONSTRAINT public.approval_review_runs fk_approval_review_runs_target_workspace
ALTER TABLE ONLY public.approval_review_runs
    ADD CONSTRAINT fk_approval_review_runs_target_workspace FOREIGN KEY (org_id, target_workspace_id) REFERENCES public.workspaces(org_id, id) ON DELETE RESTRICT;

-- FK CONSTRAINT public.comment_index fk_ci_org
ALTER TABLE ONLY public.comment_index
    ADD CONSTRAINT fk_ci_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.chat_messages fk_cm_org
ALTER TABLE ONLY public.chat_messages
    ADD CONSTRAINT fk_cm_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.chat_message_context_resolutions fk_cmcr_source_session
ALTER TABLE ONLY public.chat_message_context_resolutions
    ADD CONSTRAINT fk_cmcr_source_session FOREIGN KEY (source_session_id) REFERENCES public.chat_sessions(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.chat_message_context_resolutions fk_cmcr_target_session
ALTER TABLE ONLY public.chat_message_context_resolutions
    ADD CONSTRAINT fk_cmcr_target_session FOREIGN KEY (target_session_id) REFERENCES public.chat_sessions(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.comment_replies fk_cr_org
ALTER TABLE ONLY public.comment_replies
    ADD CONSTRAINT fk_cr_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.chat_sessions fk_cs_org
ALTER TABLE ONLY public.chat_sessions
    ADD CONSTRAINT fk_cs_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.doc_node_links fk_dnl_org
ALTER TABLE ONLY public.doc_node_links
    ADD CONSTRAINT fk_dnl_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.docs fk_docs_org
ALTER TABLE ONLY public.docs
    ADD CONSTRAINT fk_docs_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.draft_participants fk_dp_org
ALTER TABLE ONLY public.draft_participants
    ADD CONSTRAINT fk_dp_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.draft_sessions fk_draft_sessions_org_task
ALTER TABLE ONLY public.draft_sessions
    ADD CONSTRAINT fk_draft_sessions_org_task FOREIGN KEY (org_id, task_id) REFERENCES public.tasks(org_id, id) ON DELETE CASCADE;

-- FK CONSTRAINT public.draft_sessions fk_ds_org
ALTER TABLE ONLY public.draft_sessions
    ADD CONSTRAINT fk_ds_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.draft_sessions fk_ds_workspace_branch_repo
ALTER TABLE ONLY public.draft_sessions
    ADD CONSTRAINT fk_ds_workspace_branch_repo FOREIGN KEY (workspace_branch_repo_id) REFERENCES public.workspace_branch_repos(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.draft_updates fk_du_org
ALTER TABLE ONLY public.draft_updates
    ADD CONSTRAINT fk_du_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.human_escalations fk_human_escalations_org
ALTER TABLE ONLY public.human_escalations
    ADD CONSTRAINT fk_human_escalations_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.jobs fk_jobs_ended_by
ALTER TABLE ONLY public.jobs
    ADD CONSTRAINT fk_jobs_ended_by FOREIGN KEY (ended_by) REFERENCES public.users(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.jobs fk_jobs_org
ALTER TABLE ONLY public.jobs
    ADD CONSTRAINT fk_jobs_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.node_dependencies fk_ndep_org
ALTER TABLE ONLY public.node_dependencies
    ADD CONSTRAINT fk_ndep_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.ontology_nodes fk_on_org
ALTER TABLE ONLY public.ontology_nodes
    ADD CONSTRAINT fk_on_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.ontology_nodes fk_on_workspace_branch_repo
ALTER TABLE ONLY public.ontology_nodes
    ADD CONSTRAINT fk_on_workspace_branch_repo FOREIGN KEY (workspace_branch_repo_id) REFERENCES public.workspace_branch_repos(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.repo_manifests fk_repo_manifests_worker
ALTER TABLE ONLY public.repo_manifests
    ADD CONSTRAINT fk_repo_manifests_worker FOREIGN KEY (worker_id) REFERENCES public.workers(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.repo_manifests fk_repo_manifests_workspace
ALTER TABLE ONLY public.repo_manifests
    ADD CONSTRAINT fk_repo_manifests_workspace FOREIGN KEY (repo_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.runtime_attribution_events fk_runtime_attribution_events_task_org
ALTER TABLE ONLY public.runtime_attribution_events
    ADD CONSTRAINT fk_runtime_attribution_events_task_org FOREIGN KEY (org_id, task_id) REFERENCES public.tasks(org_id, id) ON DELETE CASCADE;

-- FK CONSTRAINT public.search_documents fk_sd_org
ALTER TABLE ONLY public.search_documents
    ADD CONSTRAINT fk_sd_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.task_commit_frontier_cuts fk_task_commit_frontier_cuts_request
ALTER TABLE ONLY public.task_commit_frontier_cuts
    ADD CONSTRAINT fk_task_commit_frontier_cuts_request FOREIGN KEY (org_id, task_id, request_id) REFERENCES public.task_commit_requests(org_id, task_id, request_id) ON DELETE CASCADE;

-- FK CONSTRAINT public.task_commit_requests fk_task_commit_requests_org_task
ALTER TABLE ONLY public.task_commit_requests
    ADD CONSTRAINT fk_task_commit_requests_org_task FOREIGN KEY (org_id, task_id) REFERENCES public.tasks(org_id, id) ON DELETE CASCADE;

-- FK CONSTRAINT public.task_execution_attempts fk_task_execution_attempts_org_task
ALTER TABLE ONLY public.task_execution_attempts
    ADD CONSTRAINT fk_task_execution_attempts_org_task FOREIGN KEY (org_id, task_id) REFERENCES public.tasks(org_id, id) ON DELETE CASCADE;

-- FK CONSTRAINT public.tasks fk_tasks_active_execution_attempt
ALTER TABLE ONLY public.tasks
    ADD CONSTRAINT fk_tasks_active_execution_attempt FOREIGN KEY (org_id, id, active_execution_attempt_id) REFERENCES public.task_execution_attempts(org_id, task_id, id) ON DELETE SET NULL (active_execution_attempt_id);

-- FK CONSTRAINT public.tasks fk_tasks_advisory_predecessor_job
ALTER TABLE ONLY public.tasks
    ADD CONSTRAINT fk_tasks_advisory_predecessor_job FOREIGN KEY (advisory_predecessor_job_id) REFERENCES public.jobs(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.tasks fk_tasks_org
ALTER TABLE ONLY public.tasks
    ADD CONSTRAINT fk_tasks_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.task_checkpoints fk_tc_org
ALTER TABLE ONLY public.task_checkpoints
    ADD CONSTRAINT fk_tc_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.timeline_entries fk_te_org
ALTER TABLE ONLY public.timeline_entries
    ADD CONSTRAINT fk_te_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.workspace_branches fk_wb_org
ALTER TABLE ONLY public.workspace_branches
    ADD CONSTRAINT fk_wb_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.workspace_dependencies fk_wdep_org
ALTER TABLE ONLY public.workspace_dependencies
    ADD CONSTRAINT fk_wdep_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.workspace_members fk_wmem_org
ALTER TABLE ONLY public.workspace_members
    ADD CONSTRAINT fk_wmem_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.workspace_branch_repos fk_workspace_branch_repos_branch_repo
ALTER TABLE ONLY public.workspace_branch_repos
    ADD CONSTRAINT fk_workspace_branch_repos_branch_repo FOREIGN KEY (workspace_branch_id, repo_id) REFERENCES public.workspace_branches(id, repo_id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_branches fk_workspace_branches_org_job
ALTER TABLE ONLY public.workspace_branches
    ADD CONSTRAINT fk_workspace_branches_org_job FOREIGN KEY (org_id, job_id) REFERENCES public.jobs(org_id, id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_branches fk_workspace_branches_org_task
ALTER TABLE ONLY public.workspace_branches
    ADD CONSTRAINT fk_workspace_branches_org_task FOREIGN KEY (org_id, task_id) REFERENCES public.tasks(org_id, id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_pull_request_provider_repos fk_wpr_provider_pull_request_same_org
ALTER TABLE ONLY public.workspace_pull_request_provider_repos
    ADD CONSTRAINT fk_wpr_provider_pull_request_same_org FOREIGN KEY (org_id, pull_request_id) REFERENCES public.workspace_pull_requests(org_id, id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_pull_request_provider_repos fk_wpr_provider_repo_same_org
ALTER TABLE ONLY public.workspace_pull_request_provider_repos
    ADD CONSTRAINT fk_wpr_provider_repo_same_org FOREIGN KEY (org_id, repo_id) REFERENCES public.repos(org_id, id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_pull_requests fk_wpr_source_workspace_same_org
ALTER TABLE ONLY public.workspace_pull_requests
    ADD CONSTRAINT fk_wpr_source_workspace_same_org FOREIGN KEY (org_id, source_workspace_id) REFERENCES public.workspaces(org_id, id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_pull_requests fk_wpr_target_workspace_same_org
ALTER TABLE ONLY public.workspace_pull_requests
    ADD CONSTRAINT fk_wpr_target_workspace_same_org FOREIGN KEY (org_id, target_workspace_id) REFERENCES public.workspaces(org_id, id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_pull_request_events fk_wpre_pull_request_same_org
ALTER TABLE ONLY public.workspace_pull_request_events
    ADD CONSTRAINT fk_wpre_pull_request_same_org FOREIGN KEY (org_id, pull_request_id) REFERENCES public.workspace_pull_requests(org_id, id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_settings fk_wset_org
ALTER TABLE ONLY public.workspace_settings
    ADD CONSTRAINT fk_wset_org FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.hotlist_events hotlist_events_actor_id_fkey
ALTER TABLE ONLY public.hotlist_events
    ADD CONSTRAINT hotlist_events_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES public.users(id);

-- FK CONSTRAINT public.hotlist_events hotlist_events_hotlist_id_fkey
ALTER TABLE ONLY public.hotlist_events
    ADD CONSTRAINT hotlist_events_hotlist_id_fkey FOREIGN KEY (hotlist_id) REFERENCES public.hotlists(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.hotlist_memberships hotlist_memberships_added_by_fkey
ALTER TABLE ONLY public.hotlist_memberships
    ADD CONSTRAINT hotlist_memberships_added_by_fkey FOREIGN KEY (added_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.hotlist_memberships hotlist_memberships_hotlist_id_org_id_fkey
ALTER TABLE ONLY public.hotlist_memberships
    ADD CONSTRAINT hotlist_memberships_hotlist_id_org_id_fkey FOREIGN KEY (hotlist_id, org_id) REFERENCES public.hotlists(id, org_id) ON DELETE CASCADE;

-- FK CONSTRAINT public.hotlist_memberships hotlist_memberships_issue_id_org_id_fkey
ALTER TABLE ONLY public.hotlist_memberships
    ADD CONSTRAINT hotlist_memberships_issue_id_org_id_fkey FOREIGN KEY (issue_id, org_id) REFERENCES public.issues(id, org_id) ON DELETE CASCADE;

-- FK CONSTRAINT public.hotlist_slug_sequence hotlist_slug_sequence_org_id_fkey
ALTER TABLE ONLY public.hotlist_slug_sequence
    ADD CONSTRAINT hotlist_slug_sequence_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.hotlists hotlists_org_id_fkey
ALTER TABLE ONLY public.hotlists
    ADD CONSTRAINT hotlists_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.hotlists hotlists_owner_id_fkey
ALTER TABLE ONLY public.hotlists
    ADD CONSTRAINT hotlists_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES public.users(id);

-- FK CONSTRAINT public.human_escalation_questions human_escalation_questions_escalation_id_fkey
ALTER TABLE ONLY public.human_escalation_questions
    ADD CONSTRAINT human_escalation_questions_escalation_id_fkey FOREIGN KEY (escalation_id) REFERENCES public.human_escalations(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.human_escalation_questions human_escalation_questions_org_id_fkey
ALTER TABLE ONLY public.human_escalation_questions
    ADD CONSTRAINT human_escalation_questions_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.human_escalation_questions human_escalation_questions_resolved_by_user_id_fkey
ALTER TABLE ONLY public.human_escalation_questions
    ADD CONSTRAINT human_escalation_questions_resolved_by_user_id_fkey FOREIGN KEY (resolved_by_user_id) REFERENCES public.users(id);

-- FK CONSTRAINT public.human_escalations human_escalations_job_id_fkey
ALTER TABLE ONLY public.human_escalations
    ADD CONSTRAINT human_escalations_job_id_fkey FOREIGN KEY (job_id) REFERENCES public.jobs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.human_escalations human_escalations_resolved_by_user_id_fkey
ALTER TABLE ONLY public.human_escalations
    ADD CONSTRAINT human_escalations_resolved_by_user_id_fkey FOREIGN KEY (resolved_by_user_id) REFERENCES public.users(id);

-- FK CONSTRAINT public.human_escalations human_escalations_task_id_fkey
ALTER TABLE ONLY public.human_escalations
    ADD CONSTRAINT human_escalations_task_id_fkey FOREIGN KEY (task_id) REFERENCES public.tasks(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.human_escalations human_escalations_workspace_id_fkey
ALTER TABLE ONLY public.human_escalations
    ADD CONSTRAINT human_escalations_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.intent_record intent_record_task_id_fkey
ALTER TABLE ONLY public.intent_record
    ADD CONSTRAINT intent_record_task_id_fkey FOREIGN KEY (task_id) REFERENCES public.tasks(id);

-- FK CONSTRAINT public.intent_record intent_record_workspace_branch_id_fkey
ALTER TABLE ONLY public.intent_record
    ADD CONSTRAINT intent_record_workspace_branch_id_fkey FOREIGN KEY (workspace_branch_id) REFERENCES public.workspace_branches(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.issue_attachments issue_attachments_comment_id_fkey
ALTER TABLE ONLY public.issue_attachments
    ADD CONSTRAINT issue_attachments_comment_id_fkey FOREIGN KEY (comment_id) REFERENCES public.issue_comments(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.issue_attachments issue_attachments_issue_id_fkey
ALTER TABLE ONLY public.issue_attachments
    ADD CONSTRAINT issue_attachments_issue_id_fkey FOREIGN KEY (issue_id) REFERENCES public.issues(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.issue_attachments issue_attachments_org_id_fkey
ALTER TABLE ONLY public.issue_attachments
    ADD CONSTRAINT issue_attachments_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.issue_attachments issue_attachments_uploader_id_fkey
ALTER TABLE ONLY public.issue_attachments
    ADD CONSTRAINT issue_attachments_uploader_id_fkey FOREIGN KEY (uploader_id) REFERENCES public.users(id);

-- FK CONSTRAINT public.issue_comments issue_comments_author_id_fkey
ALTER TABLE ONLY public.issue_comments
    ADD CONSTRAINT issue_comments_author_id_fkey FOREIGN KEY (author_id) REFERENCES public.users(id);

-- FK CONSTRAINT public.issue_comments issue_comments_issue_id_fkey
ALTER TABLE ONLY public.issue_comments
    ADD CONSTRAINT issue_comments_issue_id_fkey FOREIGN KEY (issue_id) REFERENCES public.issues(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.issue_components issue_components_issue_id_fkey
ALTER TABLE ONLY public.issue_components
    ADD CONSTRAINT issue_components_issue_id_fkey FOREIGN KEY (issue_id) REFERENCES public.issues(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.issue_components issue_components_ontology_node_id_fkey
ALTER TABLE ONLY public.issue_components
    ADD CONSTRAINT issue_components_ontology_node_id_fkey FOREIGN KEY (ontology_node_id) REFERENCES public.ontology_nodes(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.issue_events issue_events_actor_id_fkey
ALTER TABLE ONLY public.issue_events
    ADD CONSTRAINT issue_events_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES public.users(id);

-- FK CONSTRAINT public.issue_events issue_events_issue_id_fkey
ALTER TABLE ONLY public.issue_events
    ADD CONSTRAINT issue_events_issue_id_fkey FOREIGN KEY (issue_id) REFERENCES public.issues(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.issue_inference_jobs issue_inference_jobs_issue_id_fkey
ALTER TABLE ONLY public.issue_inference_jobs
    ADD CONSTRAINT issue_inference_jobs_issue_id_fkey FOREIGN KEY (issue_id) REFERENCES public.issues(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.issue_inference_jobs issue_inference_jobs_org_id_fkey
ALTER TABLE ONLY public.issue_inference_jobs
    ADD CONSTRAINT issue_inference_jobs_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.issue_inference_jobs issue_inference_jobs_scheduled_job_id_fkey
ALTER TABLE ONLY public.issue_inference_jobs
    ADD CONSTRAINT issue_inference_jobs_scheduled_job_id_fkey FOREIGN KEY (scheduled_job_id) REFERENCES public.jobs(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.issue_inference_jobs issue_inference_jobs_scheduled_task_id_fkey
ALTER TABLE ONLY public.issue_inference_jobs
    ADD CONSTRAINT issue_inference_jobs_scheduled_task_id_fkey FOREIGN KEY (scheduled_task_id) REFERENCES public.tasks(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.issue_job_links issue_job_links_issue_id_fkey
ALTER TABLE ONLY public.issue_job_links
    ADD CONSTRAINT issue_job_links_issue_id_fkey FOREIGN KEY (issue_id) REFERENCES public.issues(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.issue_job_links issue_job_links_job_id_fkey
ALTER TABLE ONLY public.issue_job_links
    ADD CONSTRAINT issue_job_links_job_id_fkey FOREIGN KEY (job_id) REFERENCES public.jobs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.issue_labels issue_labels_issue_id_fkey
ALTER TABLE ONLY public.issue_labels
    ADD CONSTRAINT issue_labels_issue_id_fkey FOREIGN KEY (issue_id) REFERENCES public.issues(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.issue_links issue_links_created_by_fkey
ALTER TABLE ONLY public.issue_links
    ADD CONSTRAINT issue_links_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.issue_links issue_links_dst_issue_id_fkey
ALTER TABLE ONLY public.issue_links
    ADD CONSTRAINT issue_links_dst_issue_id_fkey FOREIGN KEY (dst_issue_id) REFERENCES public.issues(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.issue_links issue_links_src_issue_id_fkey
ALTER TABLE ONLY public.issue_links
    ADD CONSTRAINT issue_links_src_issue_id_fkey FOREIGN KEY (src_issue_id) REFERENCES public.issues(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.issue_slug_sequence issue_slug_sequence_org_id_fkey
ALTER TABLE ONLY public.issue_slug_sequence
    ADD CONSTRAINT issue_slug_sequence_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.issues issues_assignee_id_fkey
ALTER TABLE ONLY public.issues
    ADD CONSTRAINT issues_assignee_id_fkey FOREIGN KEY (assignee_id) REFERENCES public.users(id);

-- FK CONSTRAINT public.issues issues_org_id_fkey
ALTER TABLE ONLY public.issues
    ADD CONSTRAINT issues_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.issues issues_repo_id_fkey
ALTER TABLE ONLY public.issues
    ADD CONSTRAINT issues_repo_id_fkey FOREIGN KEY (repo_id) REFERENCES public.repos(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.issues issues_reporter_id_fkey
ALTER TABLE ONLY public.issues
    ADD CONSTRAINT issues_reporter_id_fkey FOREIGN KEY (reporter_id) REFERENCES public.users(id);

-- FK CONSTRAINT public.issues issues_workspace_id_fkey
ALTER TABLE ONLY public.issues
    ADD CONSTRAINT issues_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.job_merge_artifacts job_merge_artifacts_job_merge_run_id_fkey
ALTER TABLE ONLY public.job_merge_artifacts
    ADD CONSTRAINT job_merge_artifacts_job_merge_run_id_fkey FOREIGN KEY (job_merge_run_id) REFERENCES public.job_merge_runs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.job_merge_artifacts job_merge_artifacts_org_id_fkey
ALTER TABLE ONLY public.job_merge_artifacts
    ADD CONSTRAINT job_merge_artifacts_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- P1.4 begin: removed job_merge_runs_completion_advance_id_fkey; completion_advance_id stays a plain uuid (uq_job_merge_runs_completion_advance kept).
-- LEAN-TODO(P4.8): job merge still records the completion advance id; no registry row backs it.
-- P1.4 end

-- FK CONSTRAINT public.job_merge_runs job_merge_runs_confirmed_by_fkey
ALTER TABLE ONLY public.job_merge_runs
    ADD CONSTRAINT job_merge_runs_confirmed_by_fkey FOREIGN KEY (confirmed_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.job_merge_runs job_merge_runs_merge_job_id_fkey
ALTER TABLE ONLY public.job_merge_runs
    ADD CONSTRAINT job_merge_runs_merge_job_id_fkey FOREIGN KEY (merge_job_id) REFERENCES public.jobs(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.job_merge_runs job_merge_runs_org_id_fkey
ALTER TABLE ONLY public.job_merge_runs
    ADD CONSTRAINT job_merge_runs_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.job_merge_runs job_merge_runs_source_job_id_fkey
ALTER TABLE ONLY public.job_merge_runs
    ADD CONSTRAINT job_merge_runs_source_job_id_fkey FOREIGN KEY (source_job_id) REFERENCES public.jobs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.job_merge_runs job_merge_runs_source_workspace_id_fkey
ALTER TABLE ONLY public.job_merge_runs
    ADD CONSTRAINT job_merge_runs_source_workspace_id_fkey FOREIGN KEY (source_workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.job_merge_runs job_merge_runs_target_workspace_id_fkey
ALTER TABLE ONLY public.job_merge_runs
    ADD CONSTRAINT job_merge_runs_target_workspace_id_fkey FOREIGN KEY (target_workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.job_merge_runs job_merge_runs_workspace_branch_repo_id_fkey
ALTER TABLE ONLY public.job_merge_runs
    ADD CONSTRAINT job_merge_runs_workspace_branch_repo_id_fkey FOREIGN KEY (workspace_branch_repo_id) REFERENCES public.workspace_branch_repos(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.job_merge_runs job_merge_runs_workspace_id_fkey
ALTER TABLE ONLY public.job_merge_runs
    ADD CONSTRAINT job_merge_runs_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.job_preview_sessions job_preview_sessions_job_id_fkey
ALTER TABLE ONLY public.job_preview_sessions
    ADD CONSTRAINT job_preview_sessions_job_id_fkey FOREIGN KEY (job_id) REFERENCES public.jobs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.job_preview_sessions job_preview_sessions_org_id_fkey
ALTER TABLE ONLY public.job_preview_sessions
    ADD CONSTRAINT job_preview_sessions_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.job_submission_receipts job_submission_receipts_job_authority_fk
ALTER TABLE ONLY public.job_submission_receipts
    ADD CONSTRAINT job_submission_receipts_job_authority_fk FOREIGN KEY (job_id, org_id, workspace_id) REFERENCES public.jobs(id, org_id, workspace_id) ON DELETE CASCADE;

-- FK CONSTRAINT public.job_submission_receipts job_submission_receipts_org_id_fkey
ALTER TABLE ONLY public.job_submission_receipts
    ADD CONSTRAINT job_submission_receipts_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.job_submission_receipts job_submission_receipts_workspace_fk
ALTER TABLE ONLY public.job_submission_receipts
    ADD CONSTRAINT job_submission_receipts_workspace_fk FOREIGN KEY (org_id, workspace_id) REFERENCES public.workspaces(org_id, id);

-- FK CONSTRAINT public.jobs jobs_created_by_fkey
ALTER TABLE ONLY public.jobs
    ADD CONSTRAINT jobs_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.jobs jobs_depends_on_job_fkey
ALTER TABLE ONLY public.jobs
    ADD CONSTRAINT jobs_depends_on_job_fkey FOREIGN KEY (depends_on_job) REFERENCES public.jobs(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.jobs jobs_start_node_fkey
ALTER TABLE ONLY public.jobs
    ADD CONSTRAINT jobs_start_node_fkey FOREIGN KEY (start_node) REFERENCES public.ontology_nodes(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.jobs jobs_target_doc_id_fkey
ALTER TABLE ONLY public.jobs
    ADD CONSTRAINT jobs_target_doc_id_fkey FOREIGN KEY (target_doc_id) REFERENCES public.docs(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.jobs jobs_workspace_id_fkey
ALTER TABLE ONLY public.jobs
    ADD CONSTRAINT jobs_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.merge_evaluations merge_evaluations_evaluated_by_fkey
ALTER TABLE ONLY public.merge_evaluations
    ADD CONSTRAINT merge_evaluations_evaluated_by_fkey FOREIGN KEY (evaluated_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.merge_evaluations merge_evaluations_org_id_fkey
ALTER TABLE ONLY public.merge_evaluations
    ADD CONSTRAINT merge_evaluations_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.merge_evaluations merge_evaluations_org_id_merge_run_id_fkey
ALTER TABLE ONLY public.merge_evaluations
    ADD CONSTRAINT merge_evaluations_org_id_merge_run_id_fkey FOREIGN KEY (org_id, merge_run_id) REFERENCES public.job_merge_runs(org_id, id) ON DELETE RESTRICT;

-- FK CONSTRAINT public.merge_evaluations merge_evaluations_org_id_source_workspace_id_fkey
ALTER TABLE ONLY public.merge_evaluations
    ADD CONSTRAINT merge_evaluations_org_id_source_workspace_id_fkey FOREIGN KEY (org_id, source_workspace_id) REFERENCES public.workspaces(org_id, id) ON DELETE RESTRICT;

-- FK CONSTRAINT public.merge_evaluations merge_evaluations_org_id_target_workspace_id_fkey
ALTER TABLE ONLY public.merge_evaluations
    ADD CONSTRAINT merge_evaluations_org_id_target_workspace_id_fkey FOREIGN KEY (org_id, target_workspace_id) REFERENCES public.workspaces(org_id, id) ON DELETE RESTRICT;

-- FK CONSTRAINT public.node_dependencies node_dependencies_from_node_id_fkey
ALTER TABLE ONLY public.node_dependencies
    ADD CONSTRAINT node_dependencies_from_node_id_fkey FOREIGN KEY (from_node_id) REFERENCES public.ontology_nodes(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.node_dependencies node_dependencies_to_node_id_fkey
ALTER TABLE ONLY public.node_dependencies
    ADD CONSTRAINT node_dependencies_to_node_id_fkey FOREIGN KEY (to_node_id) REFERENCES public.ontology_nodes(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.notifications notifications_actor_user_id_fkey
ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_actor_user_id_fkey FOREIGN KEY (actor_user_id) REFERENCES public.users(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.notifications notifications_comment_id_fkey
ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_comment_id_fkey FOREIGN KEY (comment_id) REFERENCES public.comment_index(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.notifications notifications_job_id_fkey
ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_job_id_fkey FOREIGN KEY (job_id) REFERENCES public.jobs(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.notifications notifications_recipient_user_id_fkey
ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_recipient_user_id_fkey FOREIGN KEY (recipient_user_id) REFERENCES public.users(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.notifications notifications_reply_id_fkey
ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_reply_id_fkey FOREIGN KEY (reply_id) REFERENCES public.comment_replies(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.notifications notifications_task_id_fkey
ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_task_id_fkey FOREIGN KEY (task_id) REFERENCES public.tasks(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.notifications notifications_workspace_id_fkey
ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.oauth_states oauth_states_user_id_fkey
ALTER TABLE ONLY public.oauth_states
    ADD CONSTRAINT oauth_states_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.ontology_grounding ontology_grounding_source_intent_record_id_fkey
ALTER TABLE ONLY public.ontology_grounding
    ADD CONSTRAINT ontology_grounding_source_intent_record_id_fkey FOREIGN KEY (source_intent_record_id) REFERENCES public.intent_record(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.ontology_grounding ontology_grounding_target_intent_record_id_fkey
ALTER TABLE ONLY public.ontology_grounding
    ADD CONSTRAINT ontology_grounding_target_intent_record_id_fkey FOREIGN KEY (target_intent_record_id) REFERENCES public.intent_record(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.ontology_grounding ontology_grounding_task_id_fkey
ALTER TABLE ONLY public.ontology_grounding
    ADD CONSTRAINT ontology_grounding_task_id_fkey FOREIGN KEY (task_id) REFERENCES public.tasks(id);

-- FK CONSTRAINT public.ontology_grounding ontology_grounding_workspace_branch_id_fkey
ALTER TABLE ONLY public.ontology_grounding
    ADD CONSTRAINT ontology_grounding_workspace_branch_id_fkey FOREIGN KEY (workspace_branch_id) REFERENCES public.workspace_branches(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.ontology_nodes ontology_nodes_parent_id_fkey
ALTER TABLE ONLY public.ontology_nodes
    ADD CONSTRAINT ontology_nodes_parent_id_fkey FOREIGN KEY (parent_id) REFERENCES public.ontology_nodes(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.ontology_nodes ontology_nodes_workspace_branch_id_fkey
ALTER TABLE ONLY public.ontology_nodes
    ADD CONSTRAINT ontology_nodes_workspace_branch_id_fkey FOREIGN KEY (workspace_branch_id) REFERENCES public.workspace_branches(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.org_configs org_configs_org_id_fkey
ALTER TABLE ONLY public.org_configs
    ADD CONSTRAINT org_configs_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.org_configs org_configs_updated_by_fkey
ALTER TABLE ONLY public.org_configs
    ADD CONSTRAINT org_configs_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.org_default_scopes org_default_scopes_org_id_fkey
ALTER TABLE ONLY public.org_default_scopes
    ADD CONSTRAINT org_default_scopes_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.org_default_scopes org_default_scopes_scope_fkey
ALTER TABLE ONLY public.org_default_scopes
    ADD CONSTRAINT org_default_scopes_scope_fkey FOREIGN KEY (scope) REFERENCES public.oauth_scopes(scope);

-- FK CONSTRAINT public.org_invitations org_invitations_accepted_by_fkey
ALTER TABLE ONLY public.org_invitations
    ADD CONSTRAINT org_invitations_accepted_by_fkey FOREIGN KEY (accepted_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.org_invitations org_invitations_invited_by_fkey
ALTER TABLE ONLY public.org_invitations
    ADD CONSTRAINT org_invitations_invited_by_fkey FOREIGN KEY (invited_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.org_invitations org_invitations_org_id_fkey
ALTER TABLE ONLY public.org_invitations
    ADD CONSTRAINT org_invitations_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.org_memberships org_memberships_org_id_fkey
ALTER TABLE ONLY public.org_memberships
    ADD CONSTRAINT org_memberships_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.org_memberships org_memberships_user_id_fkey
ALTER TABLE ONLY public.org_memberships
    ADD CONSTRAINT org_memberships_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

-- P1.4: removed the pr1884_authority_cutover_* run_id foreign keys (pr1884_authority_cutover_branch_dispositions_run_id_fkey, pr1884_authority_cutover_canonical_draft_retirement_run_id_fkey, pr1884_authority_cutover_job_archives_run_id_fkey, pr1884_authority_cutover_policy_overrides_run_id_fkey, pr1884_authority_cutover_stale_draft_normalizations_run_id_fkey, pr1884_authority_cutover_task_archives_run_id_fkey, pr1884_authority_cutover_workspace_receipts_run_id_fkey).

-- FK CONSTRAINT public.provider_accounts provider_accounts_created_by_fkey
ALTER TABLE ONLY public.provider_accounts
    ADD CONSTRAINT provider_accounts_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.provider_accounts provider_accounts_org_id_fkey
ALTER TABLE ONLY public.provider_accounts
    ADD CONSTRAINT provider_accounts_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.provider_accounts provider_accounts_user_id_fkey
ALTER TABLE ONLY public.provider_accounts
    ADD CONSTRAINT provider_accounts_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.provider_accounts provider_accounts_workspace_same_org
ALTER TABLE ONLY public.provider_accounts
    ADD CONSTRAINT provider_accounts_workspace_same_org FOREIGN KEY (org_id, workspace_id) REFERENCES public.workspaces(org_id, id) ON DELETE CASCADE;

-- FK CONSTRAINT public.provider_quota_snapshots provider_quota_snapshots_org_id_fkey
ALTER TABLE ONLY public.provider_quota_snapshots
    ADD CONSTRAINT provider_quota_snapshots_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.provider_quota_snapshots provider_quota_snapshots_workspace_id_fkey
ALTER TABLE ONLY public.provider_quota_snapshots
    ADD CONSTRAINT provider_quota_snapshots_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.refresh_tokens refresh_tokens_org_id_fkey
ALTER TABLE ONLY public.refresh_tokens
    ADD CONSTRAINT refresh_tokens_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.refresh_tokens refresh_tokens_user_id_fkey
ALTER TABLE ONLY public.refresh_tokens
    ADD CONSTRAINT refresh_tokens_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.repo_manifests repo_manifests_org_id_fkey
ALTER TABLE ONLY public.repo_manifests
    ADD CONSTRAINT repo_manifests_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.repo_secrets repo_secrets_repo_id_fkey
ALTER TABLE ONLY public.repo_secrets
    ADD CONSTRAINT repo_secrets_repo_id_fkey FOREIGN KEY (repo_id) REFERENCES public.repos(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.repo_secrets repo_secrets_repo_org_fk
ALTER TABLE ONLY public.repo_secrets
    ADD CONSTRAINT repo_secrets_repo_org_fk FOREIGN KEY (org_id, repo_id) REFERENCES public.repos(org_id, id) ON DELETE CASCADE;

-- FK CONSTRAINT public.repos repos_org_id_fkey
ALTER TABLE ONLY public.repos
    ADD CONSTRAINT repos_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.runtime_attribution_events runtime_attribution_events_org_id_fkey
ALTER TABLE ONLY public.runtime_attribution_events
    ADD CONSTRAINT runtime_attribution_events_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.runtime_attribution_events runtime_attribution_events_runtime_id_fkey
ALTER TABLE ONLY public.runtime_attribution_events
    ADD CONSTRAINT runtime_attribution_events_runtime_id_fkey FOREIGN KEY (runtime_id) REFERENCES public.chat_sticky_runtimes(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.search_documents search_documents_node_id_fkey
ALTER TABLE ONLY public.search_documents
    ADD CONSTRAINT search_documents_node_id_fkey FOREIGN KEY (node_id) REFERENCES public.ontology_nodes(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.search_documents search_documents_workspace_branch_id_fkey
ALTER TABLE ONLY public.search_documents
    ADD CONSTRAINT search_documents_workspace_branch_id_fkey FOREIGN KEY (workspace_branch_id) REFERENCES public.workspace_branches(id) ON DELETE CASCADE;


-- FK CONSTRAINT public.search_reindex_queue search_reindex_queue_branch_id_fkey
ALTER TABLE ONLY public.search_reindex_queue
    ADD CONSTRAINT search_reindex_queue_branch_id_fkey FOREIGN KEY (branch_id) REFERENCES public.workspace_branches(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.search_reindex_queue search_reindex_queue_org_id_fkey
ALTER TABLE ONLY public.search_reindex_queue
    ADD CONSTRAINT search_reindex_queue_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.search_reindex_queue search_reindex_queue_workspace_id_fkey
ALTER TABLE ONLY public.search_reindex_queue
    ADD CONSTRAINT search_reindex_queue_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.session_build_telemetry session_build_telemetry_workspace_id_fkey
ALTER TABLE ONLY public.session_build_telemetry
    ADD CONSTRAINT session_build_telemetry_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.task_checkpoints task_checkpoints_task_id_fkey
ALTER TABLE ONLY public.task_checkpoints
    ADD CONSTRAINT task_checkpoints_task_id_fkey FOREIGN KEY (task_id) REFERENCES public.tasks(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.task_commit_frontier_cuts task_commit_frontier_cuts_session_id_fkey
ALTER TABLE ONLY public.task_commit_frontier_cuts
    ADD CONSTRAINT task_commit_frontier_cuts_session_id_fkey FOREIGN KEY (session_id) REFERENCES public.draft_sessions(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.task_commit_frontier_cuts task_commit_frontier_cuts_workspace_branch_repo_id_fkey
ALTER TABLE ONLY public.task_commit_frontier_cuts
    ADD CONSTRAINT task_commit_frontier_cuts_workspace_branch_repo_id_fkey FOREIGN KEY (workspace_branch_repo_id) REFERENCES public.workspace_branch_repos(id);

-- FK CONSTRAINT public.task_commit_frontier_receipts task_commit_frontier_receipts_session_id_fkey
ALTER TABLE ONLY public.task_commit_frontier_receipts
    ADD CONSTRAINT task_commit_frontier_receipts_session_id_fkey FOREIGN KEY (session_id) REFERENCES public.draft_sessions(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.task_commit_frontier_receipts task_commit_frontier_receipts_workspace_branch_repo_id_fkey
ALTER TABLE ONLY public.task_commit_frontier_receipts
    ADD CONSTRAINT task_commit_frontier_receipts_workspace_branch_repo_id_fkey FOREIGN KEY (workspace_branch_repo_id) REFERENCES public.workspace_branch_repos(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.task_commit_frontier_receipts task_commit_frontier_receipts_workspace_id_fkey
ALTER TABLE ONLY public.task_commit_frontier_receipts
    ADD CONSTRAINT task_commit_frontier_receipts_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.task_commit_requests task_commit_requests_org_id_fkey
ALTER TABLE ONLY public.task_commit_requests
    ADD CONSTRAINT task_commit_requests_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.task_commit_requests task_commit_requests_workspace_id_fkey
ALTER TABLE ONLY public.task_commit_requests
    ADD CONSTRAINT task_commit_requests_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.task_completion_constraints task_completion_constraints_task_id_fkey
ALTER TABLE ONLY public.task_completion_constraints
    ADD CONSTRAINT task_completion_constraints_task_id_fkey FOREIGN KEY (task_id) REFERENCES public.tasks(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.task_question_windows task_question_windows_org_id_fkey
ALTER TABLE ONLY public.task_question_windows
    ADD CONSTRAINT task_question_windows_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.task_question_windows task_question_windows_request_timeline_id_fkey
ALTER TABLE ONLY public.task_question_windows
    ADD CONSTRAINT task_question_windows_request_timeline_id_fkey FOREIGN KEY (request_timeline_id) REFERENCES public.timeline_entries(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.task_question_windows task_question_windows_session_id_fkey
ALTER TABLE ONLY public.task_question_windows
    ADD CONSTRAINT task_question_windows_session_id_fkey FOREIGN KEY (session_id) REFERENCES public.chat_sessions(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.task_question_windows task_question_windows_task_id_fkey
ALTER TABLE ONLY public.task_question_windows
    ADD CONSTRAINT task_question_windows_task_id_fkey FOREIGN KEY (task_id) REFERENCES public.tasks(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.tasks tasks_agent_auth_submitted_by_fkey
ALTER TABLE ONLY public.tasks
    ADD CONSTRAINT tasks_agent_auth_submitted_by_fkey FOREIGN KEY (agent_auth_submitted_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.tasks tasks_job_id_fkey
ALTER TABLE ONLY public.tasks
    ADD CONSTRAINT tasks_job_id_fkey FOREIGN KEY (job_id) REFERENCES public.jobs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.tasks tasks_node_id_fkey
ALTER TABLE ONLY public.tasks
    ADD CONSTRAINT tasks_node_id_fkey FOREIGN KEY (node_id) REFERENCES public.ontology_nodes(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.timeline_entries timeline_entries_task_id_fkey
ALTER TABLE ONLY public.timeline_entries
    ADD CONSTRAINT timeline_entries_task_id_fkey FOREIGN KEY (task_id) REFERENCES public.tasks(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.usage_records usage_records_job_id_fkey
ALTER TABLE ONLY public.usage_records
    ADD CONSTRAINT usage_records_job_id_fkey FOREIGN KEY (job_id) REFERENCES public.jobs(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.usage_records usage_records_org_id_fkey
ALTER TABLE ONLY public.usage_records
    ADD CONSTRAINT usage_records_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.usage_records usage_records_session_id_fkey
ALTER TABLE ONLY public.usage_records
    ADD CONSTRAINT usage_records_session_id_fkey FOREIGN KEY (session_id) REFERENCES public.chat_sessions(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.usage_records usage_records_task_id_fkey
ALTER TABLE ONLY public.usage_records
    ADD CONSTRAINT usage_records_task_id_fkey FOREIGN KEY (task_id) REFERENCES public.tasks(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.usage_records usage_records_workspace_id_fkey
ALTER TABLE ONLY public.usage_records
    ADD CONSTRAINT usage_records_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.user_configs user_configs_org_id_fkey
ALTER TABLE ONLY public.user_configs
    ADD CONSTRAINT user_configs_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.user_configs user_configs_updated_by_fkey
ALTER TABLE ONLY public.user_configs
    ADD CONSTRAINT user_configs_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.user_configs user_configs_user_id_fkey
ALTER TABLE ONLY public.user_configs
    ADD CONSTRAINT user_configs_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.user_preferences user_preferences_user_id_fkey
ALTER TABLE ONLY public.user_preferences
    ADD CONSTRAINT user_preferences_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.user_scm_connections user_scm_connections_user_id_fkey
ALTER TABLE ONLY public.user_scm_connections
    ADD CONSTRAINT user_scm_connections_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.user_scope_grants user_scope_grants_granted_by_fkey
ALTER TABLE ONLY public.user_scope_grants
    ADD CONSTRAINT user_scope_grants_granted_by_fkey FOREIGN KEY (granted_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.user_scope_grants user_scope_grants_org_id_fkey
ALTER TABLE ONLY public.user_scope_grants
    ADD CONSTRAINT user_scope_grants_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.user_scope_grants user_scope_grants_scope_fkey
ALTER TABLE ONLY public.user_scope_grants
    ADD CONSTRAINT user_scope_grants_scope_fkey FOREIGN KEY (scope) REFERENCES public.oauth_scopes(scope);

-- FK CONSTRAINT public.user_scope_grants user_scope_grants_user_id_fkey
ALTER TABLE ONLY public.user_scope_grants
    ADD CONSTRAINT user_scope_grants_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.worker_hosts worker_hosts_org_id_fkey
ALTER TABLE ONLY public.worker_hosts
    ADD CONSTRAINT worker_hosts_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.worker_hosts worker_hosts_rotation_replaced_by_fkey
ALTER TABLE ONLY public.worker_hosts
    ADD CONSTRAINT worker_hosts_rotation_replaced_by_fkey FOREIGN KEY (rotation_replaced_by) REFERENCES public.worker_hosts(host_id);

-- FK CONSTRAINT public.worker_tokens worker_tokens_created_by_fkey
ALTER TABLE ONLY public.worker_tokens
    ADD CONSTRAINT worker_tokens_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.worker_tokens worker_tokens_org_id_fkey
ALTER TABLE ONLY public.worker_tokens
    ADD CONSTRAINT worker_tokens_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.worker_tokens worker_tokens_user_id_fkey
ALTER TABLE ONLY public.worker_tokens
    ADD CONSTRAINT worker_tokens_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.worker_tokens worker_tokens_workspace_id_fkey
ALTER TABLE ONLY public.worker_tokens
    ADD CONSTRAINT worker_tokens_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workers workers_host_id_fkey
ALTER TABLE ONLY public.workers
    ADD CONSTRAINT workers_host_id_fkey FOREIGN KEY (host_id) REFERENCES public.worker_hosts(host_id) ON DELETE SET NULL;

-- FK CONSTRAINT public.workers workers_last_node_id_fkey
ALTER TABLE ONLY public.workers
    ADD CONSTRAINT workers_last_node_id_fkey FOREIGN KEY (last_node_id) REFERENCES public.ontology_nodes(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.workers workers_org_id_fkey
ALTER TABLE ONLY public.workers
    ADD CONSTRAINT workers_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.workers workers_owner_user_id_fkey
ALTER TABLE ONLY public.workers
    ADD CONSTRAINT workers_owner_user_id_fkey FOREIGN KEY (owner_user_id) REFERENCES public.users(id);

-- FK CONSTRAINT public.workers workers_task_id_fkey
ALTER TABLE ONLY public.workers
    ADD CONSTRAINT workers_task_id_fkey FOREIGN KEY (task_id) REFERENCES public.tasks(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.workers workers_workspace_id_fkey
ALTER TABLE ONLY public.workers
    ADD CONSTRAINT workers_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE SET NULL;

-- P1.4 begin: removed workspace_advance_adapter_receipts_operation_id_fkey; operation_id stays a plain uuid.
-- LEAN-TODO(P4.8): advance-consumer bookkeeping keyed by advance id; job-merge (P4.8) and workspace-PR (P6.4) consumers retire it.
-- P1.4 end

-- P1.4 begin
-- LEAN-TODO(P4.8): advance-consumer bookkeeping keyed by advance id; job-merge (P4.8) and workspace-PR (P6.4) consumers retire it.
-- P1.4 end
-- FK CONSTRAINT public.workspace_advance_consumer_receipts workspace_advance_adapter_receipts_org_id_fkey
ALTER TABLE ONLY public.workspace_advance_consumer_receipts
    ADD CONSTRAINT workspace_advance_adapter_receipts_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- P1.4 begin: removed workspace_advance_consumer_inputs_operation_id_fkey; operation_id stays a plain uuid.
-- LEAN-TODO(P4.8): advance ids are minted by the landing helper; no registry row backs them.
-- P1.4 end

-- P1.4 begin
-- LEAN-TODO(P4.8): advance-consumer bookkeeping keyed by advance id; job-merge (P4.8) and workspace-PR (P6.4) consumers retire it.
-- P1.4 end
-- FK CONSTRAINT public.workspace_advance_consumer_inputs workspace_advance_consumer_inputs_org_id_fkey
ALTER TABLE ONLY public.workspace_advance_consumer_inputs
    ADD CONSTRAINT workspace_advance_consumer_inputs_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.workspace_branch_repos workspace_branch_repos_org_id_fkey
ALTER TABLE ONLY public.workspace_branch_repos
    ADD CONSTRAINT workspace_branch_repos_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.workspace_branch_repos workspace_branch_repos_repo_id_fkey
ALTER TABLE ONLY public.workspace_branch_repos
    ADD CONSTRAINT workspace_branch_repos_repo_id_fkey FOREIGN KEY (repo_id) REFERENCES public.repos(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_branch_repos workspace_branch_repos_workspace_branch_id_fkey
ALTER TABLE ONLY public.workspace_branch_repos
    ADD CONSTRAINT workspace_branch_repos_workspace_branch_id_fkey FOREIGN KEY (workspace_branch_id) REFERENCES public.workspace_branches(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_branches workspace_branches_cloned_from_branch_id_fkey
ALTER TABLE ONLY public.workspace_branches
    ADD CONSTRAINT workspace_branches_cloned_from_branch_id_fkey FOREIGN KEY (cloned_from_branch_id) REFERENCES public.workspace_branches(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.workspace_branches workspace_branches_repo_id_fkey
ALTER TABLE ONLY public.workspace_branches
    ADD CONSTRAINT workspace_branches_repo_id_fkey FOREIGN KEY (repo_id) REFERENCES public.repos(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_branches workspace_branches_workspace_id_fkey
ALTER TABLE ONLY public.workspace_branches
    ADD CONSTRAINT workspace_branches_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_configs workspace_configs_org_id_fkey
ALTER TABLE ONLY public.workspace_configs
    ADD CONSTRAINT workspace_configs_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_configs workspace_configs_updated_by_fkey
ALTER TABLE ONLY public.workspace_configs
    ADD CONSTRAINT workspace_configs_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.workspace_configs workspace_configs_workspace_id_fkey
ALTER TABLE ONLY public.workspace_configs
    ADD CONSTRAINT workspace_configs_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_dependencies workspace_dependencies_depends_on_fkey
ALTER TABLE ONLY public.workspace_dependencies
    ADD CONSTRAINT workspace_dependencies_depends_on_fkey FOREIGN KEY (depends_on) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_dependencies workspace_dependencies_workspace_id_fkey
ALTER TABLE ONLY public.workspace_dependencies
    ADD CONSTRAINT workspace_dependencies_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- P1.4: removed the head-operation registry foreign keys (workspace_head_operation_make_rem_workspace_branch_repo_id_fkey, workspace_head_operation_make_remote_a_workspace_branch_id_fkey, workspace_head_operation_make_remote_applic_acting_user_id_fkey, workspace_head_operation_make_remote_applicat_operation_id_fkey, workspace_head_operation_make_remote_applicat_workspace_id_fkey, workspace_head_operation_make_remote_applications_org_id_fkey, workspace_head_operation_make_remote_applications_repo_id_fkey, workspace_head_operation_membership_proposals_operation_id_fkey, workspace_head_operation_membership_proposals_org_id_fkey, workspace_head_operation_membership_proposals_workspace_id_fkey, workspace_head_operation_repos_operation_id_fkey, workspace_head_operation_repos_org_id_fkey, workspace_head_operation_repos_repo_id_fkey, workspace_head_operation_source_repos_operation_id_fkey, workspace_head_operation_source_repos_org_id_fkey, workspace_head_operation_source_repos_repo_id_fkey, workspace_head_operations_crdt_task_id_fkey, workspace_head_operations_org_id_fkey, workspace_head_operations_source_operation_id_fkey, workspace_head_operations_source_workspace_id_fkey, workspace_head_operations_superseded_by_advance_id_fkey, workspace_head_operations_workspace_id_fkey).

-- FK CONSTRAINT public.workspace_members workspace_members_invited_by_fkey
ALTER TABLE ONLY public.workspace_members
    ADD CONSTRAINT workspace_members_invited_by_fkey FOREIGN KEY (invited_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.workspace_members workspace_members_user_id_fkey
ALTER TABLE ONLY public.workspace_members
    ADD CONSTRAINT workspace_members_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_members workspace_members_workspace_id_fkey
ALTER TABLE ONLY public.workspace_members
    ADD CONSTRAINT workspace_members_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_permission_overrides workspace_permission_overrides_granted_by_fkey
ALTER TABLE ONLY public.workspace_permission_overrides
    ADD CONSTRAINT workspace_permission_overrides_granted_by_fkey FOREIGN KEY (granted_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.workspace_permission_overrides workspace_permission_overrides_scope_fkey
ALTER TABLE ONLY public.workspace_permission_overrides
    ADD CONSTRAINT workspace_permission_overrides_scope_fkey FOREIGN KEY (scope) REFERENCES public.oauth_scopes(scope);

-- FK CONSTRAINT public.workspace_permission_overrides workspace_permission_overrides_user_id_fkey
ALTER TABLE ONLY public.workspace_permission_overrides
    ADD CONSTRAINT workspace_permission_overrides_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_permission_overrides workspace_permission_overrides_workspace_id_fkey
ALTER TABLE ONLY public.workspace_permission_overrides
    ADD CONSTRAINT workspace_permission_overrides_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_prebuild_state workspace_prebuild_state_workspace_id_fkey
ALTER TABLE ONLY public.workspace_prebuild_state
    ADD CONSTRAINT workspace_prebuild_state_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_pull_request_events workspace_pull_request_events_actor_id_fkey
ALTER TABLE ONLY public.workspace_pull_request_events
    ADD CONSTRAINT workspace_pull_request_events_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES public.users(id);

-- FK CONSTRAINT public.workspace_pull_request_events workspace_pull_request_events_org_id_fkey
ALTER TABLE ONLY public.workspace_pull_request_events
    ADD CONSTRAINT workspace_pull_request_events_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.workspace_pull_request_events workspace_pull_request_events_pull_request_id_fkey
ALTER TABLE ONLY public.workspace_pull_request_events
    ADD CONSTRAINT workspace_pull_request_events_pull_request_id_fkey FOREIGN KEY (pull_request_id) REFERENCES public.workspace_pull_requests(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_pull_request_provider_repos workspace_pull_request_provider_repos_org_id_fkey
ALTER TABLE ONLY public.workspace_pull_request_provider_repos
    ADD CONSTRAINT workspace_pull_request_provider_repos_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.workspace_pull_requests workspace_pull_requests_created_by_fkey
ALTER TABLE ONLY public.workspace_pull_requests
    ADD CONSTRAINT workspace_pull_requests_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.workspace_pull_requests workspace_pull_requests_merged_by_fkey
ALTER TABLE ONLY public.workspace_pull_requests
    ADD CONSTRAINT workspace_pull_requests_merged_by_fkey FOREIGN KEY (merged_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.workspace_pull_requests workspace_pull_requests_org_id_fkey
ALTER TABLE ONLY public.workspace_pull_requests
    ADD CONSTRAINT workspace_pull_requests_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id);

-- FK CONSTRAINT public.workspace_pull_requests workspace_pull_requests_source_workspace_id_fkey
ALTER TABLE ONLY public.workspace_pull_requests
    ADD CONSTRAINT workspace_pull_requests_source_workspace_id_fkey FOREIGN KEY (source_workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_pull_requests workspace_pull_requests_target_workspace_id_fkey
ALTER TABLE ONLY public.workspace_pull_requests
    ADD CONSTRAINT workspace_pull_requests_target_workspace_id_fkey FOREIGN KEY (target_workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspace_settings workspace_settings_updated_by_fkey
ALTER TABLE ONLY public.workspace_settings
    ADD CONSTRAINT workspace_settings_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.workspace_settings workspace_settings_workspace_id_fkey
ALTER TABLE ONLY public.workspace_settings
    ADD CONSTRAINT workspace_settings_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES public.workspaces(id) ON DELETE CASCADE;

-- FK CONSTRAINT public.workspaces workspaces_base_auto_pull_actor_user_id_fkey
ALTER TABLE ONLY public.workspaces
    ADD CONSTRAINT workspaces_base_auto_pull_actor_user_id_fkey FOREIGN KEY (base_auto_pull_actor_user_id) REFERENCES public.users(id) ON DELETE RESTRICT;

-- FK CONSTRAINT public.workspaces workspaces_cloned_from_id_fkey
ALTER TABLE ONLY public.workspaces
    ADD CONSTRAINT workspaces_cloned_from_id_fkey FOREIGN KEY (cloned_from_id) REFERENCES public.workspaces(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.workspaces workspaces_created_by_fkey
ALTER TABLE ONLY public.workspaces
    ADD CONSTRAINT workspaces_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id);

-- FK CONSTRAINT public.workspaces workspaces_deleted_by_fkey
ALTER TABLE ONLY public.workspaces
    ADD CONSTRAINT workspaces_deleted_by_fkey FOREIGN KEY (deleted_by) REFERENCES public.users(id) ON DELETE SET NULL NOT VALID;

-- FK CONSTRAINT public.workspaces workspaces_docs_repo_id_fkey
ALTER TABLE ONLY public.workspaces
    ADD CONSTRAINT workspaces_docs_repo_id_fkey FOREIGN KEY (docs_repo_id) REFERENCES public.repos(id);

-- FK CONSTRAINT public.workspaces workspaces_merge_target_workspace_id_fkey
ALTER TABLE ONLY public.workspaces
    ADD CONSTRAINT workspaces_merge_target_workspace_id_fkey FOREIGN KEY (merge_target_workspace_id) REFERENCES public.workspaces(id) ON DELETE SET NULL;

-- FK CONSTRAINT public.workspaces workspaces_org_id_fkey
ALTER TABLE ONLY public.workspaces
    ADD CONSTRAINT workspaces_org_id_fkey FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE;

-- ROW SECURITY public.agent_approval_handoffs
ALTER TABLE public.agent_approval_handoffs ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.api_keys
ALTER TABLE public.api_keys ENABLE ROW LEVEL SECURITY;

-- POLICY public.api_keys api_keys_org_isolation
CREATE POLICY api_keys_org_isolation ON public.api_keys USING ((org_id = ANY (authz.current_org_ids())));

-- ROW SECURITY public.approval_configs
ALTER TABLE public.approval_configs ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.approval_definitions
ALTER TABLE public.approval_definitions ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.approval_requests
ALTER TABLE public.approval_requests ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.approval_review_runs
ALTER TABLE public.approval_review_runs ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.approval_statuses
ALTER TABLE public.approval_statuses ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.branch_integration_queue
ALTER TABLE public.branch_integration_queue ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.branch_integration_remote_pair_leases
ALTER TABLE public.branch_integration_remote_pair_leases ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.branch_sync_runs
ALTER TABLE public.branch_sync_runs ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.builder_task_specs
ALTER TABLE public.builder_task_specs ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.chat_attachments
ALTER TABLE public.chat_attachments ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.chat_message_context_resolutions
ALTER TABLE public.chat_message_context_resolutions ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.chat_messages
ALTER TABLE public.chat_messages ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.chat_session_context_links
ALTER TABLE public.chat_session_context_links ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.chat_sessions
ALTER TABLE public.chat_sessions ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.chat_sticky_runtimes
ALTER TABLE public.chat_sticky_runtimes ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.comment_index
ALTER TABLE public.comment_index ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.comment_replies
ALTER TABLE public.comment_replies ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.doc_folders
ALTER TABLE public.doc_folders ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.doc_node_links
ALTER TABLE public.doc_node_links ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.docs
ALTER TABLE public.docs ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.draft_participants
ALTER TABLE public.draft_participants ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.draft_sessions
ALTER TABLE public.draft_sessions ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.draft_updates
ALTER TABLE public.draft_updates ENABLE ROW LEVEL SECURITY;

-- POLICY public.ontology_grounding grounding_org_isolation
CREATE POLICY grounding_org_isolation ON public.ontology_grounding USING ((org_id = (current_setting('app.current_org_id'::text))::uuid));

-- ROW SECURITY public.human_escalation_questions
ALTER TABLE public.human_escalation_questions ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.human_escalations
ALTER TABLE public.human_escalations ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.intent_record
ALTER TABLE public.intent_record ENABLE ROW LEVEL SECURITY;

-- POLICY public.intent_record intent_record_org_isolation
CREATE POLICY intent_record_org_isolation ON public.intent_record USING ((org_id = (current_setting('app.current_org_id'::text))::uuid));

-- POLICY public.ontology_nodes issue_discovery_component_lookup
CREATE POLICY issue_discovery_component_lookup ON public.ontology_nodes FOR SELECT TO g5_app USING ((sync_status <> 'deleted'::text));

-- POLICY public.chat_messages issue_discovery_scheduler_chat_messages
CREATE POLICY issue_discovery_scheduler_chat_messages ON public.chat_messages TO g5_app USING ((session_id IN ( SELECT chat_sessions.id
   FROM public.chat_sessions
  WHERE ((chat_sessions.session_type = 'task'::text) AND (chat_sessions.title ~~ 'Issue ontology discovery:%'::text))))) WITH CHECK ((session_id IN ( SELECT chat_sessions.id
   FROM public.chat_sessions
  WHERE ((chat_sessions.session_type = 'task'::text) AND (chat_sessions.title ~~ 'Issue ontology discovery:%'::text)))));

-- POLICY public.chat_sessions issue_discovery_scheduler_chat_sessions
CREATE POLICY issue_discovery_scheduler_chat_sessions ON public.chat_sessions TO g5_app USING (((session_type = 'task'::text) AND (title ~~ 'Issue ontology discovery:%'::text))) WITH CHECK (((session_type = 'task'::text) AND (title ~~ 'Issue ontology discovery:%'::text)));

-- POLICY public.jobs issue_discovery_scheduler_jobs
CREATE POLICY issue_discovery_scheduler_jobs ON public.jobs TO g5_app USING (((job_type = 'chat'::text) AND (name ~~ 'Issue ontology discovery:%'::text))) WITH CHECK (((job_type = 'chat'::text) AND (name ~~ 'Issue ontology discovery:%'::text)));

-- POLICY public.tasks issue_discovery_scheduler_tasks
CREATE POLICY issue_discovery_scheduler_tasks ON public.tasks TO g5_app USING (((task_type = 'chat'::text) AND (workspace_ref ~~ '%issue_ontology_component_discovery%'::text))) WITH CHECK (((task_type = 'chat'::text) AND (workspace_ref ~~ '%issue_ontology_component_discovery%'::text)));

-- POLICY public.workspace_branches issue_discovery_workspace_branch_lookup
CREATE POLICY issue_discovery_workspace_branch_lookup ON public.workspace_branches FOR SELECT TO g5_app USING (true);

-- POLICY public.workspace_branch_repos issue_discovery_workspace_branch_repo_lookup
CREATE POLICY issue_discovery_workspace_branch_repo_lookup ON public.workspace_branch_repos FOR SELECT TO g5_app USING (true);

-- ROW SECURITY public.job_merge_artifacts
ALTER TABLE public.job_merge_artifacts ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.job_merge_runs
ALTER TABLE public.job_merge_runs ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.job_preview_sessions
ALTER TABLE public.job_preview_sessions ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.job_submission_receipts
ALTER TABLE public.job_submission_receipts ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.jobs
ALTER TABLE public.jobs ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.merge_evaluations
ALTER TABLE public.merge_evaluations ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.node_dependencies
ALTER TABLE public.node_dependencies ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.ontology_grounding
ALTER TABLE public.ontology_grounding ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.ontology_nodes
ALTER TABLE public.ontology_nodes ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.org_configs
ALTER TABLE public.org_configs ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.provider_accounts
ALTER TABLE public.provider_accounts ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.repo_manifests
ALTER TABLE public.repo_manifests ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.repo_secrets
ALTER TABLE public.repo_secrets ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.repos
ALTER TABLE public.repos ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.runtime_attribution_events
ALTER TABLE public.runtime_attribution_events ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.search_documents
ALTER TABLE public.search_documents ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.session_build_telemetry
ALTER TABLE public.session_build_telemetry ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.task_checkpoints
ALTER TABLE public.task_checkpoints ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.task_commit_frontier_cuts
ALTER TABLE public.task_commit_frontier_cuts ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.task_commit_frontier_receipts
ALTER TABLE public.task_commit_frontier_receipts ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.task_commit_requests
ALTER TABLE public.task_commit_requests ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.task_completion_constraints
ALTER TABLE public.task_completion_constraints ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.task_execution_attempts
ALTER TABLE public.task_execution_attempts ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.task_question_windows
ALTER TABLE public.task_question_windows ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.tasks
ALTER TABLE public.tasks ENABLE ROW LEVEL SECURITY;

-- POLICY public.agent_approval_handoffs tenant_isolation
CREATE POLICY tenant_isolation ON public.agent_approval_handoffs USING ((org_id = ANY (authz.current_org_ids()))) WITH CHECK ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.approval_configs tenant_isolation
CREATE POLICY tenant_isolation ON public.approval_configs USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.approval_definitions tenant_isolation
CREATE POLICY tenant_isolation ON public.approval_definitions USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.approval_requests tenant_isolation
CREATE POLICY tenant_isolation ON public.approval_requests USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.approval_review_runs tenant_isolation
CREATE POLICY tenant_isolation ON public.approval_review_runs USING ((org_id = ANY (authz.current_org_ids()))) WITH CHECK ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.approval_statuses tenant_isolation
CREATE POLICY tenant_isolation ON public.approval_statuses USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.branch_integration_queue tenant_isolation
CREATE POLICY tenant_isolation ON public.branch_integration_queue USING (((org_id = ANY (authz.current_org_ids())) OR (EXISTS ( SELECT 1
   FROM public.workspace_branch_repos wbr
  WHERE ((wbr.id = branch_integration_queue.workspace_branch_repo_id) AND (wbr.org_id = ANY (authz.current_org_ids()))))))) WITH CHECK (((org_id = ANY (authz.current_org_ids())) OR (EXISTS ( SELECT 1
   FROM public.workspace_branch_repos wbr
  WHERE ((wbr.id = branch_integration_queue.workspace_branch_repo_id) AND (wbr.org_id = ANY (authz.current_org_ids())))))));

-- POLICY public.branch_integration_remote_pair_leases tenant_isolation
CREATE POLICY tenant_isolation ON public.branch_integration_remote_pair_leases USING ((org_id = ANY (authz.current_org_ids()))) WITH CHECK ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.branch_sync_runs tenant_isolation
CREATE POLICY tenant_isolation ON public.branch_sync_runs USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.builder_task_specs tenant_isolation
CREATE POLICY tenant_isolation ON public.builder_task_specs USING ((org_id = ANY (authz.current_org_ids()))) WITH CHECK ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.chat_attachments tenant_isolation
CREATE POLICY tenant_isolation ON public.chat_attachments USING ((org_id = ANY (authz.current_org_ids()))) WITH CHECK ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.chat_message_context_resolutions tenant_isolation
CREATE POLICY tenant_isolation ON public.chat_message_context_resolutions USING ((org_id = ANY (authz.current_org_ids()))) WITH CHECK ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.chat_messages tenant_isolation
CREATE POLICY tenant_isolation ON public.chat_messages USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.chat_session_context_links tenant_isolation
CREATE POLICY tenant_isolation ON public.chat_session_context_links USING ((org_id = ANY (authz.current_org_ids()))) WITH CHECK ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.chat_sessions tenant_isolation
CREATE POLICY tenant_isolation ON public.chat_sessions USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.chat_sticky_runtimes tenant_isolation
CREATE POLICY tenant_isolation ON public.chat_sticky_runtimes USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.comment_index tenant_isolation
CREATE POLICY tenant_isolation ON public.comment_index USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.comment_replies tenant_isolation
CREATE POLICY tenant_isolation ON public.comment_replies USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.doc_folders tenant_isolation
CREATE POLICY tenant_isolation ON public.doc_folders USING ((org_id = ANY (authz.current_org_ids()))) WITH CHECK ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.doc_node_links tenant_isolation
CREATE POLICY tenant_isolation ON public.doc_node_links USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.docs tenant_isolation
CREATE POLICY tenant_isolation ON public.docs USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.draft_participants tenant_isolation
CREATE POLICY tenant_isolation ON public.draft_participants USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.draft_sessions tenant_isolation
CREATE POLICY tenant_isolation ON public.draft_sessions USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.draft_updates tenant_isolation
CREATE POLICY tenant_isolation ON public.draft_updates USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.human_escalation_questions tenant_isolation
CREATE POLICY tenant_isolation ON public.human_escalation_questions USING ((org_id = ANY (authz.current_org_ids()))) WITH CHECK ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.human_escalations tenant_isolation
CREATE POLICY tenant_isolation ON public.human_escalations USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.job_merge_artifacts tenant_isolation
CREATE POLICY tenant_isolation ON public.job_merge_artifacts USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.job_merge_runs tenant_isolation
CREATE POLICY tenant_isolation ON public.job_merge_runs USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.job_preview_sessions tenant_isolation
CREATE POLICY tenant_isolation ON public.job_preview_sessions USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.job_submission_receipts tenant_isolation
CREATE POLICY tenant_isolation ON public.job_submission_receipts USING (((org_id = ANY (authz.current_org_ids())) AND (EXISTS ( SELECT 1
   FROM public.workspaces w
  WHERE ((w.id = job_submission_receipts.workspace_id) AND (w.org_id = job_submission_receipts.org_id)))))) WITH CHECK (((org_id = ANY (authz.current_org_ids())) AND (EXISTS ( SELECT 1
   FROM public.workspaces w
  WHERE ((w.id = job_submission_receipts.workspace_id) AND (w.org_id = job_submission_receipts.org_id))))));

-- POLICY public.jobs tenant_isolation
CREATE POLICY tenant_isolation ON public.jobs USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.merge_evaluations tenant_isolation
CREATE POLICY tenant_isolation ON public.merge_evaluations USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.node_dependencies tenant_isolation
CREATE POLICY tenant_isolation ON public.node_dependencies USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.ontology_nodes tenant_isolation
CREATE POLICY tenant_isolation ON public.ontology_nodes USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.org_configs tenant_isolation
CREATE POLICY tenant_isolation ON public.org_configs USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.provider_accounts tenant_isolation
CREATE POLICY tenant_isolation ON public.provider_accounts USING ((org_id = ANY (authz.current_org_ids()))) WITH CHECK ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.repo_secrets tenant_isolation
CREATE POLICY tenant_isolation ON public.repo_secrets USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.repos tenant_isolation
CREATE POLICY tenant_isolation ON public.repos USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.runtime_attribution_events tenant_isolation
CREATE POLICY tenant_isolation ON public.runtime_attribution_events USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.search_documents tenant_isolation
CREATE POLICY tenant_isolation ON public.search_documents USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.session_build_telemetry tenant_isolation
CREATE POLICY tenant_isolation ON public.session_build_telemetry USING ((org_id = ANY (authz.current_org_ids()))) WITH CHECK ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.task_checkpoints tenant_isolation
CREATE POLICY tenant_isolation ON public.task_checkpoints USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.task_commit_frontier_cuts tenant_isolation
CREATE POLICY tenant_isolation ON public.task_commit_frontier_cuts USING ((org_id = ANY (authz.current_org_ids()))) WITH CHECK ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.task_commit_frontier_receipts tenant_isolation
CREATE POLICY tenant_isolation ON public.task_commit_frontier_receipts USING ((org_id = ANY (authz.current_org_ids()))) WITH CHECK ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.task_commit_requests tenant_isolation
CREATE POLICY tenant_isolation ON public.task_commit_requests USING ((org_id = ANY (authz.current_org_ids()))) WITH CHECK ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.task_completion_constraints tenant_isolation
CREATE POLICY tenant_isolation ON public.task_completion_constraints USING ((EXISTS ( SELECT 1
   FROM public.tasks t
  WHERE ((t.id = task_completion_constraints.task_id) AND (t.org_id = ANY (authz.current_org_ids())))))) WITH CHECK ((EXISTS ( SELECT 1
   FROM public.tasks t
  WHERE ((t.id = task_completion_constraints.task_id) AND (t.org_id = ANY (authz.current_org_ids()))))));

-- POLICY public.task_execution_attempts tenant_isolation
CREATE POLICY tenant_isolation ON public.task_execution_attempts USING ((org_id = ANY (authz.current_org_ids()))) WITH CHECK ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.task_question_windows tenant_isolation
CREATE POLICY tenant_isolation ON public.task_question_windows USING ((org_id = ANY (authz.current_org_ids()))) WITH CHECK ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.tasks tenant_isolation
CREATE POLICY tenant_isolation ON public.tasks USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.timeline_entries tenant_isolation
CREATE POLICY tenant_isolation ON public.timeline_entries USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.user_configs tenant_isolation
CREATE POLICY tenant_isolation ON public.user_configs USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.worker_hosts tenant_isolation
CREATE POLICY tenant_isolation ON public.worker_hosts USING ((org_id = ANY (authz.current_org_ids())));

-- P1.4 begin
-- LEAN-TODO(P4.8): advance-consumer bookkeeping keyed by advance id; job-merge (P4.8) and workspace-PR (P6.4) consumers retire it.
-- P1.4 end
-- POLICY public.workspace_advance_consumer_inputs tenant_isolation
CREATE POLICY tenant_isolation ON public.workspace_advance_consumer_inputs USING ((org_id = ANY (authz.current_org_ids()))) WITH CHECK ((org_id = ANY (authz.current_org_ids())));

-- P1.4 begin
-- LEAN-TODO(P4.8): advance-consumer bookkeeping keyed by advance id; job-merge (P4.8) and workspace-PR (P6.4) consumers retire it.
-- P1.4 end
-- POLICY public.workspace_advance_consumer_receipts tenant_isolation
CREATE POLICY tenant_isolation ON public.workspace_advance_consumer_receipts USING ((org_id = ANY (authz.current_org_ids()))) WITH CHECK ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.workspace_branch_repos tenant_isolation
CREATE POLICY tenant_isolation ON public.workspace_branch_repos USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.workspace_branches tenant_isolation
CREATE POLICY tenant_isolation ON public.workspace_branches USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.workspace_configs tenant_isolation
CREATE POLICY tenant_isolation ON public.workspace_configs USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.workspace_dependencies tenant_isolation
CREATE POLICY tenant_isolation ON public.workspace_dependencies USING ((org_id = ANY (authz.current_org_ids())));

-- P1.4: removed the tenant_isolation policies on workspace_head_operation_make_remote_applications, workspace_head_operation_membership_proposals, workspace_head_operation_repos, workspace_head_operation_source_repos and workspace_head_operations.

-- POLICY public.workspace_members tenant_isolation
CREATE POLICY tenant_isolation ON public.workspace_members USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.workspace_prebuild_state tenant_isolation
CREATE POLICY tenant_isolation ON public.workspace_prebuild_state USING ((org_id = ANY (authz.current_org_ids()))) WITH CHECK ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.workspace_pull_request_events tenant_isolation
CREATE POLICY tenant_isolation ON public.workspace_pull_request_events USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.workspace_pull_request_provider_repos tenant_isolation
CREATE POLICY tenant_isolation ON public.workspace_pull_request_provider_repos USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.workspace_pull_requests tenant_isolation
CREATE POLICY tenant_isolation ON public.workspace_pull_requests USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.workspace_settings tenant_isolation
CREATE POLICY tenant_isolation ON public.workspace_settings USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.workspaces tenant_isolation
CREATE POLICY tenant_isolation ON public.workspaces USING ((org_id = ANY (authz.current_org_ids())));

-- POLICY public.repo_manifests tenant_isolation_repo_manifests
CREATE POLICY tenant_isolation_repo_manifests ON public.repo_manifests USING ((org_id = ANY (authz.current_org_ids())));

-- ROW SECURITY public.timeline_entries
ALTER TABLE public.timeline_entries ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.user_configs
ALTER TABLE public.user_configs ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.worker_hosts
ALTER TABLE public.worker_hosts ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.worker_tokens
ALTER TABLE public.worker_tokens ENABLE ROW LEVEL SECURITY;

-- POLICY public.worker_tokens worker_tokens_org_isolation
CREATE POLICY worker_tokens_org_isolation ON public.worker_tokens USING ((org_id = ANY (authz.current_org_ids())));

-- P1.4 begin
-- LEAN-TODO(P4.8): advance-consumer bookkeeping keyed by advance id; job-merge (P4.8) and workspace-PR (P6.4) consumers retire it.
-- P1.4 end
-- ROW SECURITY public.workspace_advance_consumer_inputs
ALTER TABLE public.workspace_advance_consumer_inputs ENABLE ROW LEVEL SECURITY;

-- P1.4 begin
-- LEAN-TODO(P4.8): advance-consumer bookkeeping keyed by advance id; job-merge (P4.8) and workspace-PR (P6.4) consumers retire it.
-- P1.4 end
-- ROW SECURITY public.workspace_advance_consumer_receipts
ALTER TABLE public.workspace_advance_consumer_receipts ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.workspace_branch_repos
ALTER TABLE public.workspace_branch_repos ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.workspace_branches
ALTER TABLE public.workspace_branches ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.workspace_configs
ALTER TABLE public.workspace_configs ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.workspace_dependencies
ALTER TABLE public.workspace_dependencies ENABLE ROW LEVEL SECURITY;

-- P1.4: removed the ENABLE ROW LEVEL SECURITY statements for workspace_head_operation_make_remote_applications, workspace_head_operation_membership_proposals, workspace_head_operation_repos, workspace_head_operation_source_repos and workspace_head_operations.

-- ROW SECURITY public.workspace_members
ALTER TABLE public.workspace_members ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.workspace_prebuild_state
ALTER TABLE public.workspace_prebuild_state ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.workspace_pull_request_events
ALTER TABLE public.workspace_pull_request_events ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.workspace_pull_request_provider_repos
ALTER TABLE public.workspace_pull_request_provider_repos ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.workspace_pull_requests
ALTER TABLE public.workspace_pull_requests ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.workspace_settings
ALTER TABLE public.workspace_settings ENABLE ROW LEVEL SECURITY;

-- ROW SECURITY public.workspaces
ALTER TABLE public.workspaces ENABLE ROW LEVEL SECURITY;

-- ACL SCHEMA authz
-- authz_ctx_writer received USAGE, CREATE early (ownership scaffolding);
-- re-grant it here so the schema ACL keeps the historical entry order.
REVOKE ALL ON SCHEMA authz FROM authz_ctx_writer;
GRANT USAGE ON SCHEMA authz TO g5_tenant;
GRANT USAGE ON SCHEMA authz TO g5_app;
GRANT USAGE ON SCHEMA authz TO authz_writer;
GRANT ALL ON SCHEMA authz TO authz_ctx_writer;

-- ACL SCHEMA public
GRANT USAGE ON SCHEMA public TO g5_tenant;
GRANT USAGE ON SCHEMA public TO g5_app;
GRANT USAGE ON SCHEMA public TO authz_lookup_definer;

-- ACL authz.FUNCTION apply_retired_codex_auth_config_row(p_scope_kind text, p_org_id uuid, p_workspace_id uuid, p_user_id uuid, p_expected_settings jsonb, p_expected_secrets_enc bytea, p_expected_secret_keys text[], p_next_settings jsonb, p_next_secrets_enc bytea, p_next_secret_keys text[]) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.apply_retired_codex_auth_config_row(p_scope_kind text, p_org_id uuid, p_workspace_id uuid, p_user_id uuid, p_expected_settings jsonb, p_expected_secrets_enc bytea, p_expected_secret_keys text[], p_next_settings jsonb, p_next_secrets_enc bytea, p_next_secret_keys text[]) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.apply_retired_codex_auth_config_row(p_scope_kind text, p_org_id uuid, p_workspace_id uuid, p_user_id uuid, p_expected_settings jsonb, p_expected_secrets_enc bytea, p_expected_secret_keys text[], p_next_settings jsonb, p_next_secrets_enc bytea, p_next_secret_keys text[]) TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION attach_orphan_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.attach_orphan_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.attach_orphan_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text) TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION clear_ctx() (owner authz_ctx_writer)
SET ROLE authz_ctx_writer;
GRANT ALL ON FUNCTION authz.clear_ctx() TO g5_tenant;
GRANT ALL ON FUNCTION authz.clear_ctx() TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION current_org_ids() (owner authz_ctx_writer)
SET ROLE authz_ctx_writer;
GRANT ALL ON FUNCTION authz.current_org_ids() TO g5_tenant;
GRANT ALL ON FUNCTION authz.current_org_ids() TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION get_worker_host_cloud_linkage(p_host_id uuid) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.get_worker_host_cloud_linkage(p_host_id uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.get_worker_host_cloud_linkage(p_host_id uuid) TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION guard_sticky_task_execution_admission() (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.guard_sticky_task_execution_admission() FROM PUBLIC;
RESET ROLE;

-- ACL authz.FUNCTION guard_worker_insert_dead_host() (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.guard_worker_insert_dead_host() FROM PUBLIC;
RESET ROLE;

-- ACL authz.FUNCTION guard_worker_revival_dead_host() (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.guard_worker_revival_dead_host() FROM PUBLIC;
RESET ROLE;

-- ACL authz.FUNCTION list_orphan_bringup_worker_hosts() (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.list_orphan_bringup_worker_hosts() FROM PUBLIC;
GRANT ALL ON FUNCTION authz.list_orphan_bringup_worker_hosts() TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION list_provisioning_worker_hosts() (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.list_provisioning_worker_hosts() FROM PUBLIC;
GRANT ALL ON FUNCTION authz.list_provisioning_worker_hosts() TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION list_retired_codex_auth_config_rows() (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.list_retired_codex_auth_config_rows() FROM PUBLIC;
GRANT ALL ON FUNCTION authz.list_retired_codex_auth_config_rows() TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION list_worker_host_regions() (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.list_worker_host_regions() FROM PUBLIC;
GRANT ALL ON FUNCTION authz.list_worker_host_regions() TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION lookup_api_key_by_hash(p_key_hash text)
GRANT ALL ON FUNCTION authz.lookup_api_key_by_hash(p_key_hash text) TO g5_app;

-- ACL authz.FUNCTION lookup_worker_host_for_auth(p_host_id uuid) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.lookup_worker_host_for_auth(p_host_id uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.lookup_worker_host_for_auth(p_host_id uuid) TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION lookup_worker_token_by_hash(p_key_hash text)
GRANT ALL ON FUNCTION authz.lookup_worker_token_by_hash(p_key_hash text) TO g5_app;

-- ACL authz.FUNCTION mark_worker_host_cloud_gone(p_host_id uuid, p_expected_cloud_instance_id text) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.mark_worker_host_cloud_gone(p_host_id uuid, p_expected_cloud_instance_id text) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.mark_worker_host_cloud_gone(p_host_id uuid, p_expected_cloud_instance_id text) TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION prepare_prior_generation_sticky_runtime_retirement(p_worker_id text) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.prepare_prior_generation_sticky_runtime_retirement(p_worker_id text) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.prepare_prior_generation_sticky_runtime_retirement(p_worker_id text) TO g5_app;
GRANT ALL ON FUNCTION authz.prepare_prior_generation_sticky_runtime_retirement(p_worker_id text) TO g5_tenant;
RESET ROLE;

-- ACL authz.FUNCTION repoint_worker_host_cloud_instance(p_host_id uuid, p_expected_cloud_instance_id text, p_new_cloud_instance_id text) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.repoint_worker_host_cloud_instance(p_host_id uuid, p_expected_cloud_instance_id text, p_new_cloud_instance_id text) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.repoint_worker_host_cloud_instance(p_host_id uuid, p_expected_cloud_instance_id text, p_new_cloud_instance_id text) TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION reset_worker_host_cloud_gone(p_host_id uuid, p_expected_cloud_instance_id text) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.reset_worker_host_cloud_gone(p_host_id uuid, p_expected_cloud_instance_id text) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.reset_worker_host_cloud_gone(p_host_id uuid, p_expected_cloud_instance_id text) TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION reset_worker_host_orphan_check_miss(p_host_id uuid) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.reset_worker_host_orphan_check_miss(p_host_id uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.reset_worker_host_orphan_check_miss(p_host_id uuid) TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION retire_prelocked_prior_generation_sticky_runtimes(p_worker_id text, p_current_generation bigint, p_prepared_job_ids uuid[], p_prepared_task_ids uuid[]) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.retire_prelocked_prior_generation_sticky_runtimes(p_worker_id text, p_current_generation bigint, p_prepared_job_ids uuid[], p_prepared_task_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.retire_prelocked_prior_generation_sticky_runtimes(p_worker_id text, p_current_generation bigint, p_prepared_job_ids uuid[], p_prepared_task_ids uuid[]) TO g5_app;
GRANT ALL ON FUNCTION authz.retire_prelocked_prior_generation_sticky_runtimes(p_worker_id text, p_current_generation bigint, p_prepared_job_ids uuid[], p_prepared_task_ids uuid[]) TO g5_tenant;
RESET ROLE;

-- ACL authz.FUNCTION retire_prior_generation_sticky_runtimes(p_worker_id text, p_current_generation bigint) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.retire_prior_generation_sticky_runtimes(p_worker_id text, p_current_generation bigint) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.retire_prior_generation_sticky_runtimes(p_worker_id text, p_current_generation bigint) TO g5_app;
GRANT ALL ON FUNCTION authz.retire_prior_generation_sticky_runtimes(p_worker_id text, p_current_generation bigint) TO g5_tenant;
RESET ROLE;

-- ACL authz.FUNCTION schedule_worker_host_provision_retry(p_host_id uuid) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.schedule_worker_host_provision_retry(p_host_id uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.schedule_worker_host_provision_retry(p_host_id uuid) TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION set_ctx(p_session_id uuid) (owner authz_ctx_writer)
SET ROLE authz_ctx_writer;
GRANT ALL ON FUNCTION authz.set_ctx(p_session_id uuid) TO g5_tenant;
GRANT ALL ON FUNCTION authz.set_ctx(p_session_id uuid) TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION set_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.set_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.set_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text) TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION set_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text, p_boot_image_id text) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.set_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text, p_boot_image_id text) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.set_worker_host_cloud_refs(p_host_id uuid, p_vpc_id text, p_cloud_instance_id text, p_boot_image_id text) TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION set_worker_host_orphan_check_miss(p_host_id uuid) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.set_worker_host_orphan_check_miss(p_host_id uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.set_worker_host_orphan_check_miss(p_host_id uuid) TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION set_worker_host_provision_error_code(p_host_id uuid, p_error_code text) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.set_worker_host_provision_error_code(p_host_id uuid, p_error_code text) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.set_worker_host_provision_error_code(p_host_id uuid, p_error_code text) TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION terminate_abandoned_worker_host(p_host_id uuid) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.terminate_abandoned_worker_host(p_host_id uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.terminate_abandoned_worker_host(p_host_id uuid) TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION terminate_abandoned_worker_host(p_host_id uuid, p_miss_threshold integer) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.terminate_abandoned_worker_host(p_host_id uuid, p_miss_threshold integer) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.terminate_abandoned_worker_host(p_host_id uuid, p_miss_threshold integer) TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION terminate_gone_worker_host(p_host_id uuid, p_expected_cloud_instance_id text, p_confirm_secs integer) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.terminate_gone_worker_host(p_host_id uuid, p_expected_cloud_instance_id text, p_confirm_secs integer) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.terminate_gone_worker_host(p_host_id uuid, p_expected_cloud_instance_id text, p_confirm_secs integer) TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION touch_api_key_last_used(p_id uuid, p_last_used_ip text)
GRANT ALL ON FUNCTION authz.touch_api_key_last_used(p_id uuid, p_last_used_ip text) TO g5_app;

-- ACL authz.FUNCTION touch_worker_host_bringup(p_host_id uuid) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.touch_worker_host_bringup(p_host_id uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.touch_worker_host_bringup(p_host_id uuid) TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION transition_provisioning_worker_host(p_host_id uuid, p_to_status text) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.transition_provisioning_worker_host(p_host_id uuid, p_to_status text) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.transition_provisioning_worker_host(p_host_id uuid, p_to_status text) TO g5_app;
RESET ROLE;

-- ACL authz.FUNCTION worker_identity_bound_to_terminated_host(p_worker_id text, p_org_id uuid) (owner authz_lookup_definer)
SET ROLE authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.worker_identity_bound_to_terminated_host(p_worker_id text, p_org_id uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION authz.worker_identity_bound_to_terminated_host(p_worker_id text, p_org_id uuid) TO g5_app;
GRANT ALL ON FUNCTION authz.worker_identity_bound_to_terminated_host(p_worker_id text, p_org_id uuid) TO g5_tenant;
RESET ROLE;

-- ACL public.FUNCTION cli_device_workspace_org(p_workspace_id uuid)
GRANT ALL ON FUNCTION public.cli_device_workspace_org(p_workspace_id uuid) TO g5_app;

-- ACL public.FUNCTION repo_webhook_secret_for_current_org(target_repo_id uuid)
GRANT ALL ON FUNCTION public.repo_webhook_secret_for_current_org(target_repo_id uuid) TO g5_tenant;
GRANT ALL ON FUNCTION public.repo_webhook_secret_for_current_org(target_repo_id uuid) TO g5_app;

-- ACL authz.TABLE session_ctx
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE authz.session_ctx TO authz_ctx_writer;
GRANT SELECT,DELETE ON TABLE authz.session_ctx TO authz_writer;

-- ACL authz.TABLE user_sessions
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE authz.user_sessions TO authz_writer;
GRANT SELECT,UPDATE ON TABLE authz.user_sessions TO authz_ctx_writer;

-- ACL public.TABLE agent_approval_handoffs
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.agent_approval_handoffs TO g5_tenant;
GRANT SELECT,INSERT,UPDATE ON TABLE public.agent_approval_handoffs TO g5_app;

-- ACL public.TABLE api_keys
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.api_keys TO g5_tenant;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.api_keys TO g5_app;

-- ACL public.TABLE approval_configs
GRANT SELECT,INSERT ON TABLE public.approval_configs TO g5_tenant;
GRANT SELECT ON TABLE public.approval_configs TO g5_app;

-- ACL public.SEQUENCE approval_configs_seq_seq
GRANT USAGE ON SEQUENCE public.approval_configs_seq_seq TO g5_tenant;

-- ACL public.TABLE approval_definitions
GRANT SELECT,INSERT ON TABLE public.approval_definitions TO g5_tenant;
GRANT SELECT ON TABLE public.approval_definitions TO g5_app;

-- ACL public.TABLE approval_requests
GRANT SELECT,INSERT,DELETE ON TABLE public.approval_requests TO g5_tenant;
GRANT SELECT ON TABLE public.approval_requests TO g5_app;

-- ACL public.COLUMN approval_requests.state
GRANT UPDATE(state) ON TABLE public.approval_requests TO g5_tenant;

-- ACL public.COLUMN approval_requests.settlement_fingerprint
GRANT UPDATE(settlement_fingerprint) ON TABLE public.approval_requests TO g5_tenant;

-- ACL public.COLUMN approval_requests.closed_at
GRANT UPDATE(closed_at) ON TABLE public.approval_requests TO g5_tenant;

-- ACL public.COLUMN approval_requests.updated_at
GRANT UPDATE(updated_at) ON TABLE public.approval_requests TO g5_tenant;

-- ACL public.SEQUENCE approval_requests_seq_seq
GRANT USAGE ON SEQUENCE public.approval_requests_seq_seq TO g5_tenant;

-- ACL public.TABLE approval_review_runs
GRANT SELECT,INSERT,UPDATE ON TABLE public.approval_review_runs TO g5_tenant;
GRANT SELECT ON TABLE public.approval_review_runs TO g5_app;

-- ACL public.TABLE approval_statuses
GRANT SELECT,INSERT ON TABLE public.approval_statuses TO g5_tenant;
GRANT SELECT ON TABLE public.approval_statuses TO g5_app;

-- ACL public.SEQUENCE approval_statuses_seq_seq
GRANT USAGE ON SEQUENCE public.approval_statuses_seq_seq TO g5_tenant;

-- ACL public.TABLE branch_integration_queue
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.branch_integration_queue TO g5_tenant;

-- ACL public.TABLE branch_integration_remote_pair_leases
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.branch_integration_remote_pair_leases TO g5_tenant;
GRANT SELECT ON TABLE public.branch_integration_remote_pair_leases TO g5_app;

-- ACL public.TABLE branch_sync_runs
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.branch_sync_runs TO g5_tenant;

-- ACL public.TABLE builder_task_specs
GRANT SELECT,INSERT,UPDATE ON TABLE public.builder_task_specs TO g5_tenant;
GRANT SELECT,INSERT,UPDATE ON TABLE public.builder_task_specs TO g5_app;

-- ACL public.TABLE chat_attachments
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.chat_attachments TO g5_tenant;

-- ACL public.TABLE chat_message_context_resolutions
GRANT SELECT,INSERT ON TABLE public.chat_message_context_resolutions TO g5_tenant;
GRANT SELECT,INSERT ON TABLE public.chat_message_context_resolutions TO g5_app;

-- ACL public.TABLE chat_messages
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.chat_messages TO g5_tenant;
GRANT SELECT,INSERT,UPDATE ON TABLE public.chat_messages TO g5_app;

-- ACL public.TABLE chat_session_context_links
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.chat_session_context_links TO g5_tenant;
GRANT SELECT,INSERT,UPDATE ON TABLE public.chat_session_context_links TO g5_app;

-- ACL public.TABLE chat_sessions
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.chat_sessions TO g5_tenant;
GRANT SELECT,INSERT,UPDATE ON TABLE public.chat_sessions TO g5_app;

-- ACL public.TABLE chat_sticky_runtimes
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.chat_sticky_runtimes TO g5_tenant;
GRANT SELECT,UPDATE ON TABLE public.chat_sticky_runtimes TO g5_app;

-- ACL public.COLUMN chat_sticky_runtimes.id
GRANT SELECT(id) ON TABLE public.chat_sticky_runtimes TO authz_lookup_definer;

-- ACL public.COLUMN chat_sticky_runtimes.org_id
GRANT SELECT(org_id) ON TABLE public.chat_sticky_runtimes TO authz_lookup_definer;

-- ACL public.COLUMN chat_sticky_runtimes.task_id
GRANT SELECT(task_id) ON TABLE public.chat_sticky_runtimes TO authz_lookup_definer;

-- ACL public.COLUMN chat_sticky_runtimes.worker_id
GRANT SELECT(worker_id) ON TABLE public.chat_sticky_runtimes TO authz_lookup_definer;

-- ACL public.COLUMN chat_sticky_runtimes.host_id
GRANT SELECT(host_id) ON TABLE public.chat_sticky_runtimes TO authz_lookup_definer;

-- ACL public.COLUMN chat_sticky_runtimes.status
GRANT SELECT(status),UPDATE(status) ON TABLE public.chat_sticky_runtimes TO authz_lookup_definer;

-- ACL public.COLUMN chat_sticky_runtimes.updated_at
GRANT UPDATE(updated_at) ON TABLE public.chat_sticky_runtimes TO authz_lookup_definer;

-- ACL public.COLUMN chat_sticky_runtimes.teardown_generation
GRANT SELECT(teardown_generation),UPDATE(teardown_generation) ON TABLE public.chat_sticky_runtimes TO authz_lookup_definer;

-- ACL public.COLUMN chat_sticky_runtimes.teardown_operation_id
GRANT SELECT(teardown_operation_id),UPDATE(teardown_operation_id) ON TABLE public.chat_sticky_runtimes TO authz_lookup_definer;

-- ACL public.COLUMN chat_sticky_runtimes.teardown_requested_at
GRANT UPDATE(teardown_requested_at) ON TABLE public.chat_sticky_runtimes TO authz_lookup_definer;

-- ACL public.COLUMN chat_sticky_runtimes.teardown_deadline_at
GRANT UPDATE(teardown_deadline_at) ON TABLE public.chat_sticky_runtimes TO authz_lookup_definer;

-- ACL public.COLUMN chat_sticky_runtimes.teardown_repository_scopes
GRANT UPDATE(teardown_repository_scopes) ON TABLE public.chat_sticky_runtimes TO authz_lookup_definer;

-- ACL public.COLUMN chat_sticky_runtimes.worker_session_generation
GRANT SELECT(worker_session_generation) ON TABLE public.chat_sticky_runtimes TO authz_lookup_definer;

-- ACL public.COLUMN chat_sticky_runtimes.teardown_workspace_released_at
GRANT SELECT(teardown_workspace_released_at) ON TABLE public.chat_sticky_runtimes TO authz_lookup_definer;

-- ACL public.COLUMN chat_sticky_runtimes.worker_workspace_root_lease_v1
GRANT SELECT(worker_workspace_root_lease_v1) ON TABLE public.chat_sticky_runtimes TO authz_lookup_definer;

-- ACL public.COLUMN chat_sticky_runtimes.teardown_reconciliation_kind
GRANT SELECT(teardown_reconciliation_kind),UPDATE(teardown_reconciliation_kind) ON TABLE public.chat_sticky_runtimes TO authz_lookup_definer;

-- ACL public.COLUMN chat_sticky_runtimes.teardown_retired_worker_session_generation
GRANT UPDATE(teardown_retired_worker_session_generation) ON TABLE public.chat_sticky_runtimes TO authz_lookup_definer;

-- ACL public.COLUMN chat_sticky_runtimes.teardown_reconciliation_receipt
GRANT UPDATE(teardown_reconciliation_receipt) ON TABLE public.chat_sticky_runtimes TO authz_lookup_definer;

-- ACL public.TABLE cli_device_authorizations
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.cli_device_authorizations TO g5_app;

-- ACL public.TABLE cloud_orphan_sightings
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.cloud_orphan_sightings TO g5_app;

-- ACL public.TABLE comment_index
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.comment_index TO g5_tenant;

-- ACL public.TABLE comment_mentions
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.comment_mentions TO g5_tenant;

-- ACL public.TABLE comment_replies
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.comment_replies TO g5_tenant;

-- ACL public.TABLE doc_folders
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.doc_folders TO g5_tenant;

-- ACL public.TABLE doc_node_links
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.doc_node_links TO g5_tenant;

-- ACL public.TABLE docs
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.docs TO g5_tenant;

-- ACL public.TABLE draft_participants
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.draft_participants TO g5_tenant;

-- ACL public.TABLE draft_sessions
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.draft_sessions TO g5_tenant;

-- ACL public.TABLE draft_updates
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.draft_updates TO g5_tenant;

-- ACL public.SEQUENCE draft_updates_seq_seq
GRANT USAGE ON SEQUENCE public.draft_updates_seq_seq TO g5_tenant;
GRANT USAGE ON SEQUENCE public.draft_updates_seq_seq TO g5_app;

-- ACL public.TABLE hotlist_events
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.hotlist_events TO g5_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.hotlist_events TO g5_tenant;

-- ACL public.TABLE hotlist_memberships
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.hotlist_memberships TO g5_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.hotlist_memberships TO g5_tenant;

-- ACL public.TABLE hotlist_slug_sequence
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.hotlist_slug_sequence TO g5_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.hotlist_slug_sequence TO g5_tenant;

-- ACL public.TABLE hotlists
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.hotlists TO g5_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.hotlists TO g5_tenant;

-- ACL public.TABLE human_escalation_questions
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.human_escalation_questions TO g5_tenant;
GRANT SELECT ON TABLE public.human_escalation_questions TO g5_app;

-- ACL public.TABLE human_escalations
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.human_escalations TO g5_tenant;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.human_escalations TO g5_app;

-- ACL public.TABLE issue_attachments
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issue_attachments TO g5_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issue_attachments TO g5_tenant;

-- ACL public.TABLE issue_comments
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issue_comments TO g5_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issue_comments TO g5_tenant;

-- ACL public.TABLE issue_components
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issue_components TO g5_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issue_components TO g5_tenant;

-- ACL public.TABLE issue_events
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issue_events TO g5_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issue_events TO g5_tenant;

-- ACL public.TABLE issue_inference_jobs
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issue_inference_jobs TO g5_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issue_inference_jobs TO g5_tenant;

-- ACL public.TABLE issue_job_links
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issue_job_links TO g5_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issue_job_links TO g5_tenant;

-- ACL public.TABLE issue_labels
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issue_labels TO g5_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issue_labels TO g5_tenant;

-- ACL public.TABLE issue_links
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issue_links TO g5_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issue_links TO g5_tenant;

-- ACL public.TABLE issue_slug_sequence
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issue_slug_sequence TO g5_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issue_slug_sequence TO g5_tenant;

-- ACL public.TABLE issues
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issues TO g5_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.issues TO g5_tenant;

-- ACL public.TABLE job_merge_artifacts
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.job_merge_artifacts TO g5_tenant;

-- ACL public.TABLE job_merge_runs
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.job_merge_runs TO g5_tenant;

-- ACL public.TABLE job_preview_sessions
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.job_preview_sessions TO g5_tenant;
GRANT SELECT ON TABLE public.job_preview_sessions TO g5_app;

-- ACL public.TABLE job_submission_receipts
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.job_submission_receipts TO g5_tenant;
GRANT SELECT ON TABLE public.job_submission_receipts TO g5_app;

-- ACL public.TABLE jobs
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.jobs TO g5_tenant;
GRANT SELECT,INSERT,UPDATE ON TABLE public.jobs TO g5_app;

-- ACL public.COLUMN jobs.id
GRANT SELECT(id),UPDATE(id) ON TABLE public.jobs TO authz_lookup_definer;

-- ACL public.COLUMN jobs.workspace_id
GRANT SELECT(workspace_id) ON TABLE public.jobs TO authz_lookup_definer;

-- ACL public.COLUMN jobs.org_id
GRANT SELECT(org_id) ON TABLE public.jobs TO authz_lookup_definer;

-- ACL public.TABLE merge_evaluations
GRANT SELECT,INSERT ON TABLE public.merge_evaluations TO g5_tenant;
GRANT SELECT ON TABLE public.merge_evaluations TO g5_app;

-- ACL public.SEQUENCE merge_evaluations_seq_seq
GRANT USAGE ON SEQUENCE public.merge_evaluations_seq_seq TO g5_tenant;

-- ACL public.TABLE node_dependencies
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.node_dependencies TO g5_tenant;

-- ACL public.TABLE notifications
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.notifications TO g5_tenant;

-- ACL public.TABLE oauth_scopes
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.oauth_scopes TO g5_app;

-- ACL public.TABLE oauth_states
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.oauth_states TO g5_app;

-- ACL public.TABLE ontology_nodes
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.ontology_nodes TO g5_tenant;
GRANT SELECT ON TABLE public.ontology_nodes TO g5_app;

-- ACL public.TABLE org_configs
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.org_configs TO g5_tenant;

-- ACL public.COLUMN org_configs.org_id
GRANT SELECT(org_id) ON TABLE public.org_configs TO authz_lookup_definer;

-- ACL public.COLUMN org_configs.settings_json
GRANT SELECT(settings_json),UPDATE(settings_json) ON TABLE public.org_configs TO authz_lookup_definer;

-- ACL public.COLUMN org_configs.secrets_enc
GRANT SELECT(secrets_enc),UPDATE(secrets_enc) ON TABLE public.org_configs TO authz_lookup_definer;

-- ACL public.COLUMN org_configs.secret_keys
GRANT SELECT(secret_keys),UPDATE(secret_keys) ON TABLE public.org_configs TO authz_lookup_definer;

-- ACL public.COLUMN org_configs.updated_at
GRANT UPDATE(updated_at) ON TABLE public.org_configs TO authz_lookup_definer;

-- ACL public.TABLE org_default_scopes
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.org_default_scopes TO g5_app;

-- ACL public.TABLE org_invitations
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.org_invitations TO g5_app;

-- ACL public.TABLE org_memberships
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.org_memberships TO g5_app;

-- ACL public.TABLE orgs
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.orgs TO g5_app;

-- ACL public.TABLE platform_issue_delivery_tombstone
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.platform_issue_delivery_tombstone TO g5_app;

-- ACL public.TABLE platform_issue_outbox
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.platform_issue_outbox TO g5_app;

-- ACL public.TABLE provider_accounts
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.provider_accounts TO g5_tenant;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.provider_accounts TO g5_app;

-- ACL public.TABLE provider_quota_snapshots
GRANT SELECT,INSERT,UPDATE ON TABLE public.provider_quota_snapshots TO g5_app;
GRANT SELECT,INSERT,UPDATE ON TABLE public.provider_quota_snapshots TO g5_tenant;

-- ACL public.TABLE refresh_tokens
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.refresh_tokens TO g5_app;

-- ACL public.TABLE repo_manifests
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.repo_manifests TO g5_tenant;

-- ACL public.TABLE repos
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.repos TO g5_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.repos TO g5_tenant;

-- ACL public.TABLE runtime_attribution_events
GRANT SELECT,INSERT,DELETE ON TABLE public.runtime_attribution_events TO g5_tenant;
GRANT SELECT ON TABLE public.runtime_attribution_events TO g5_app;

-- ACL public.TABLE search_documents
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.search_documents TO g5_tenant;

-- ACL public.TABLE search_reindex_queue
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.search_reindex_queue TO g5_app;
GRANT INSERT ON TABLE public.search_reindex_queue TO g5_tenant;

-- ACL public.TABLE self_hosted_licenses
GRANT SELECT,INSERT,UPDATE ON TABLE public.self_hosted_licenses TO g5_app;

-- ACL public.TABLE session_build_telemetry
GRANT SELECT,INSERT ON TABLE public.session_build_telemetry TO g5_tenant;
GRANT SELECT,INSERT ON TABLE public.session_build_telemetry TO g5_app;

-- ACL public.TABLE task_checkpoints
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.task_checkpoints TO g5_tenant;

-- ACL public.TABLE task_commit_frontier_cuts
GRANT SELECT,INSERT ON TABLE public.task_commit_frontier_cuts TO g5_tenant;
GRANT SELECT ON TABLE public.task_commit_frontier_cuts TO g5_app;

-- ACL public.TABLE task_commit_frontier_receipts
GRANT SELECT,INSERT,DELETE ON TABLE public.task_commit_frontier_receipts TO g5_tenant;
GRANT SELECT,DELETE ON TABLE public.task_commit_frontier_receipts TO g5_app;

-- ACL public.TABLE task_commit_requests
GRANT SELECT,INSERT,UPDATE ON TABLE public.task_commit_requests TO g5_tenant;
GRANT SELECT ON TABLE public.task_commit_requests TO g5_app;

-- ACL public.TABLE task_completion_constraints
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.task_completion_constraints TO g5_tenant;
GRANT SELECT ON TABLE public.task_completion_constraints TO g5_app;

-- ACL public.TABLE task_execution_attempts
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.task_execution_attempts TO g5_tenant;
GRANT SELECT,INSERT,UPDATE ON TABLE public.task_execution_attempts TO g5_app;

-- ACL public.COLUMN task_execution_attempts.org_id
GRANT SELECT(org_id) ON TABLE public.task_execution_attempts TO authz_lookup_definer;

-- ACL public.COLUMN task_execution_attempts.task_id
GRANT SELECT(task_id) ON TABLE public.task_execution_attempts TO authz_lookup_definer;

-- ACL public.COLUMN task_execution_attempts.ended_at
GRANT SELECT(ended_at) ON TABLE public.task_execution_attempts TO authz_lookup_definer;

-- ACL public.TABLE task_question_windows
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.task_question_windows TO g5_tenant;
GRANT SELECT,INSERT,UPDATE ON TABLE public.task_question_windows TO g5_app;

-- ACL public.TABLE tasks
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.tasks TO g5_tenant;
GRANT SELECT,INSERT,UPDATE ON TABLE public.tasks TO g5_app;

-- ACL public.COLUMN tasks.id
GRANT SELECT(id) ON TABLE public.tasks TO authz_lookup_definer;

-- ACL public.COLUMN tasks.job_id
GRANT SELECT(job_id) ON TABLE public.tasks TO authz_lookup_definer;

-- ACL public.COLUMN tasks.status
GRANT SELECT(status),UPDATE(status) ON TABLE public.tasks TO authz_lookup_definer;

-- ACL public.COLUMN tasks.worker_id
GRANT SELECT(worker_id) ON TABLE public.tasks TO authz_lookup_definer;

-- ACL public.COLUMN tasks.assigned_at
GRANT SELECT(assigned_at) ON TABLE public.tasks TO authz_lookup_definer;

-- ACL public.COLUMN tasks.updated_at
GRANT UPDATE(updated_at) ON TABLE public.tasks TO authz_lookup_definer;

-- ACL public.COLUMN tasks.org_id
GRANT SELECT(org_id) ON TABLE public.tasks TO authz_lookup_definer;

-- ACL public.COLUMN tasks.lease_expires_at
GRANT SELECT(lease_expires_at) ON TABLE public.tasks TO authz_lookup_definer;

-- ACL public.COLUMN tasks.active_execution_attempt_id
GRANT SELECT(active_execution_attempt_id) ON TABLE public.tasks TO authz_lookup_definer;

-- ACL public.TABLE timeline_entries
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.timeline_entries TO g5_tenant;

-- ACL public.TABLE usage_records
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.usage_records TO g5_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.usage_records TO g5_tenant;

-- ACL public.TABLE user_configs
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.user_configs TO g5_tenant;

-- ACL public.COLUMN user_configs.org_id
GRANT SELECT(org_id) ON TABLE public.user_configs TO authz_lookup_definer;

-- ACL public.COLUMN user_configs.user_id
GRANT SELECT(user_id) ON TABLE public.user_configs TO authz_lookup_definer;

-- ACL public.COLUMN user_configs.settings_json
GRANT SELECT(settings_json),UPDATE(settings_json) ON TABLE public.user_configs TO authz_lookup_definer;

-- ACL public.COLUMN user_configs.secrets_enc
GRANT SELECT(secrets_enc),UPDATE(secrets_enc) ON TABLE public.user_configs TO authz_lookup_definer;

-- ACL public.COLUMN user_configs.secret_keys
GRANT SELECT(secret_keys),UPDATE(secret_keys) ON TABLE public.user_configs TO authz_lookup_definer;

-- ACL public.COLUMN user_configs.updated_at
GRANT UPDATE(updated_at) ON TABLE public.user_configs TO authz_lookup_definer;

-- ACL public.TABLE user_preferences
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.user_preferences TO g5_app;

-- ACL public.TABLE user_scm_connections
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.user_scm_connections TO g5_app;

-- ACL public.TABLE user_scope_grants
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.user_scope_grants TO g5_app;

-- ACL public.TABLE users
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.users TO g5_app;

-- ACL public.TABLE waitlist_signups
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.waitlist_signups TO g5_app;

-- ACL public.TABLE worker_hosts
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.worker_hosts TO g5_tenant;

-- ACL public.COLUMN worker_hosts.host_id
GRANT SELECT(host_id) ON TABLE public.worker_hosts TO authz_lookup_definer;

-- ACL public.COLUMN worker_hosts.org_id
GRANT SELECT(org_id) ON TABLE public.worker_hosts TO authz_lookup_definer;

-- ACL public.COLUMN worker_hosts.vpc_id
GRANT UPDATE(vpc_id) ON TABLE public.worker_hosts TO authz_lookup_definer;

-- ACL public.COLUMN worker_hosts.cloud_instance_id
GRANT SELECT(cloud_instance_id),UPDATE(cloud_instance_id) ON TABLE public.worker_hosts TO authz_lookup_definer;

-- ACL public.COLUMN worker_hosts.region
GRANT SELECT(region) ON TABLE public.worker_hosts TO authz_lookup_definer;

-- ACL public.COLUMN worker_hosts.status
GRANT SELECT(status),UPDATE(status) ON TABLE public.worker_hosts TO authz_lookup_definer;

-- ACL public.COLUMN worker_hosts.created_at
GRANT SELECT(created_at) ON TABLE public.worker_hosts TO authz_lookup_definer;

-- ACL public.COLUMN worker_hosts.last_heartbeat
GRANT SELECT(last_heartbeat),UPDATE(last_heartbeat) ON TABLE public.worker_hosts TO authz_lookup_definer;

-- ACL public.COLUMN worker_hosts.provision_error_code
GRANT SELECT(provision_error_code),UPDATE(provision_error_code) ON TABLE public.worker_hosts TO authz_lookup_definer;

-- ACL public.COLUMN worker_hosts.orphan_check_misses
GRANT SELECT(orphan_check_misses),UPDATE(orphan_check_misses) ON TABLE public.worker_hosts TO authz_lookup_definer;

-- ACL public.COLUMN worker_hosts.boot_image_id
GRANT UPDATE(boot_image_id) ON TABLE public.worker_hosts TO authz_lookup_definer;

-- ACL public.COLUMN worker_hosts.bringup_heartbeat_at
GRANT SELECT(bringup_heartbeat_at),UPDATE(bringup_heartbeat_at) ON TABLE public.worker_hosts TO authz_lookup_definer;

-- ACL public.COLUMN worker_hosts.cloud_gone_since
GRANT SELECT(cloud_gone_since),UPDATE(cloud_gone_since) ON TABLE public.worker_hosts TO authz_lookup_definer;

-- ACL public.COLUMN worker_hosts.provision_retry_count
GRANT SELECT(provision_retry_count) ON TABLE public.worker_hosts TO authz_lookup_definer;

-- ACL public.COLUMN worker_hosts.provision_next_retry_at
GRANT SELECT(provision_next_retry_at),UPDATE(provision_next_retry_at) ON TABLE public.worker_hosts TO authz_lookup_definer;

-- ACL public.TABLE worker_tokens
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.worker_tokens TO g5_tenant;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.worker_tokens TO g5_app;

-- ACL public.TABLE workers
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.workers TO g5_app;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.workers TO g5_tenant;

-- ACL public.COLUMN workers.id
GRANT SELECT(id) ON TABLE public.workers TO authz_lookup_definer;

-- ACL public.COLUMN workers.status
GRANT SELECT(status) ON TABLE public.workers TO authz_lookup_definer;

-- ACL public.COLUMN workers.org_id
GRANT SELECT(org_id) ON TABLE public.workers TO authz_lookup_definer;

-- ACL public.COLUMN workers.last_heartbeat
GRANT SELECT(last_heartbeat) ON TABLE public.workers TO authz_lookup_definer;

-- ACL public.COLUMN workers.host_id
GRANT SELECT(host_id) ON TABLE public.workers TO authz_lookup_definer;

-- ACL public.COLUMN workers.external_host_id
GRANT SELECT(external_host_id) ON TABLE public.workers TO authz_lookup_definer;

-- ACL public.COLUMN workers.worker_session_generation
GRANT SELECT(worker_session_generation) ON TABLE public.workers TO authz_lookup_definer;

-- P1.4 begin
-- LEAN-TODO(P4.8): advance-consumer bookkeeping keyed by advance id; job-merge (P4.8) and workspace-PR (P6.4) consumers retire it.
-- P1.4 end
-- ACL public.TABLE workspace_advance_consumer_inputs
GRANT SELECT,INSERT,UPDATE ON TABLE public.workspace_advance_consumer_inputs TO g5_tenant;
GRANT SELECT ON TABLE public.workspace_advance_consumer_inputs TO g5_app;

-- P1.4 begin
-- LEAN-TODO(P4.8): advance-consumer bookkeeping keyed by advance id; job-merge (P4.8) and workspace-PR (P6.4) consumers retire it.
-- P1.4 end
-- ACL public.TABLE workspace_advance_consumer_receipts
GRANT SELECT,INSERT,UPDATE ON TABLE public.workspace_advance_consumer_receipts TO g5_tenant;
GRANT SELECT ON TABLE public.workspace_advance_consumer_receipts TO g5_app;

-- ACL public.TABLE workspace_branch_repos
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.workspace_branch_repos TO g5_tenant;
GRANT SELECT,INSERT ON TABLE public.workspace_branch_repos TO g5_app;

-- ACL public.COLUMN workspace_branch_repos.org_id
GRANT UPDATE(org_id) ON TABLE public.workspace_branch_repos TO g5_app;

-- ACL public.COLUMN workspace_branch_repos.repo_branch_name
GRANT UPDATE(repo_branch_name) ON TABLE public.workspace_branch_repos TO g5_app;

-- ACL public.COLUMN workspace_branch_repos.base_branch
GRANT UPDATE(base_branch) ON TABLE public.workspace_branch_repos TO g5_app;

-- ACL public.COLUMN workspace_branch_repos.head_commit
GRANT UPDATE(head_commit) ON TABLE public.workspace_branch_repos TO g5_app;

-- ACL public.COLUMN workspace_branch_repos.tracking_mode
GRANT UPDATE(tracking_mode) ON TABLE public.workspace_branch_repos TO g5_app;

-- ACL public.COLUMN workspace_branch_repos.materialization_status
GRANT UPDATE(materialization_status) ON TABLE public.workspace_branch_repos TO g5_app;

-- ACL public.COLUMN workspace_branch_repos.updated_at
GRANT UPDATE(updated_at) ON TABLE public.workspace_branch_repos TO g5_app;

-- ACL public.TABLE workspace_branches
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.workspace_branches TO g5_tenant;
GRANT SELECT,INSERT ON TABLE public.workspace_branches TO g5_app;

-- ACL public.TABLE workspace_configs
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.workspace_configs TO g5_tenant;

-- ACL public.COLUMN workspace_configs.workspace_id
GRANT SELECT(workspace_id) ON TABLE public.workspace_configs TO authz_lookup_definer;

-- ACL public.COLUMN workspace_configs.settings_json
GRANT SELECT(settings_json),UPDATE(settings_json) ON TABLE public.workspace_configs TO authz_lookup_definer;

-- ACL public.COLUMN workspace_configs.secrets_enc
GRANT SELECT(secrets_enc),UPDATE(secrets_enc) ON TABLE public.workspace_configs TO authz_lookup_definer;

-- ACL public.COLUMN workspace_configs.secret_keys
GRANT SELECT(secret_keys),UPDATE(secret_keys) ON TABLE public.workspace_configs TO authz_lookup_definer;

-- ACL public.COLUMN workspace_configs.updated_at
GRANT UPDATE(updated_at) ON TABLE public.workspace_configs TO authz_lookup_definer;

-- ACL public.TABLE workspace_dependencies
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.workspace_dependencies TO g5_tenant;

-- P1.4: removed the g5_tenant/g5_app GRANTs on workspace_head_operation_make_remote_applications, workspace_head_operation_membership_proposals, workspace_head_operation_repos, workspace_head_operation_source_repos and workspace_head_operations.

-- ACL public.TABLE workspace_members
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.workspace_members TO g5_tenant;
GRANT SELECT ON TABLE public.workspace_members TO g5_app;

-- ACL public.TABLE workspace_permission_overrides
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.workspace_permission_overrides TO g5_app;

-- ACL public.TABLE workspace_prebuild_state
GRANT SELECT,INSERT,UPDATE ON TABLE public.workspace_prebuild_state TO g5_tenant;
GRANT SELECT,INSERT,UPDATE ON TABLE public.workspace_prebuild_state TO g5_app;

-- ACL public.TABLE workspace_pull_request_events
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.workspace_pull_request_events TO g5_tenant;

-- ACL public.TABLE workspace_pull_request_provider_repos
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.workspace_pull_request_provider_repos TO g5_tenant;

-- ACL public.TABLE workspace_pull_requests
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.workspace_pull_requests TO g5_tenant;

-- ACL public.TABLE workspace_settings
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.workspace_settings TO g5_tenant;
GRANT INSERT ON TABLE public.workspace_settings TO g5_app;

-- ACL public.TABLE workspaces
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.workspaces TO g5_tenant;
GRANT SELECT,INSERT ON TABLE public.workspaces TO g5_app;

-- -----------------------------------------------------------------------------
-- P1.3: route one repo hint without granting unscoped repository visibility.
-- Install before ownership scaffolding is revoked, including on a non-superuser apply.
CREATE FUNCTION authz.lookup_repo_org_for_projection(p_repo_id uuid) RETURNS TABLE(org_id uuid)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path = pg_catalog, public, pg_temp
    AS $$ SELECT r.org_id FROM public.repos r WHERE r.id = p_repo_id $$;
ALTER FUNCTION authz.lookup_repo_org_for_projection(uuid) OWNER TO authz_lookup_definer;
REVOKE ALL ON FUNCTION authz.lookup_repo_org_for_projection(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION authz.lookup_repo_org_for_projection(uuid) TO g5_app;
GRANT SELECT (id, org_id) ON public.repos TO authz_lookup_definer;

-- -----------------------------------------------------------------------------
-- Scaffolding cleanup: hand back exactly what this file had to grant itself
-- -----------------------------------------------------------------------------
-- Restore the migrator's authz_lookup_definer membership graph to the
-- snapshot taken before the role was created: rows this file added are
-- revoked, rows whose options changed are re-granted with their prior
-- options, pre-existing rows are left alone.
DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.baseline_authz_lookup_definer_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.baseline_had_authz_lookup_definer_set', true)
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
                EXECUTE format('REVOKE authz_lookup_definer FROM %I', SESSION_USER);
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

-- Whatever the membership graph looks like now, neither the migrator nor
-- authz_ctx_writer may walk away able to SET ROLE to the BYPASSRLS owner
-- (the CI guard scripts/ci-assert-migrator-revoke.sh checks this after a
-- non-superuser apply). Surgically drop only the SET capability so a
-- pre-existing membership keeps its other options.
DO $$
DECLARE
    target_role text;
BEGIN
    FOREACH target_role IN ARRAY ARRAY[SESSION_USER::text, 'authz_ctx_writer']
    LOOP
        IF EXISTS (
            SELECT 1
            FROM pg_auth_members m
            JOIN pg_roles r ON r.oid = m.roleid
            JOIN pg_roles g ON g.oid = m.member
            WHERE r.rolname = 'authz_lookup_definer'
              AND g.rolname = target_role
              AND m.set_option
        ) THEN
            EXECUTE format('REVOKE SET OPTION FOR authz_lookup_definer FROM %I', target_role);
        END IF;
    END LOOP;
END $$;

-- Temporary schema privileges for the BYPASSRLS owner.
DO $$
BEGIN
    IF current_setting('g5.baseline_authz_lookup_definer_had_authz_create', true)
        IS DISTINCT FROM 'true'
    THEN
        REVOKE CREATE ON SCHEMA authz FROM authz_lookup_definer;
    END IF;
    IF current_setting('g5.baseline_authz_lookup_definer_had_authz_usage', true)
        IS DISTINCT FROM 'true'
    THEN
        REVOKE USAGE ON SCHEMA authz FROM authz_lookup_definer;
    END IF;
END $$;

-- -----------------------------------------------------------------------------
-- Apply-time assertions (fail loudly rather than leave a silent regression)
-- -----------------------------------------------------------------------------
DO $$
DECLARE
    leftover text;
    misowned text;
BEGIN
    SELECT string_agg(g.rolname, ', ')
      INTO leftover
      FROM pg_auth_members m
      JOIN pg_roles r ON r.oid = m.roleid
      JOIN pg_roles g ON g.oid = m.member
     WHERE r.rolname = 'authz_lookup_definer'
       AND g.rolname IN (SESSION_USER, 'authz_ctx_writer')
       AND m.set_option;
    IF leftover IS NOT NULL THEN
        RAISE EXCEPTION
            'baseline must leave no SET-capable authz_lookup_definer membership for the migrator or authz_ctx_writer, found: %',
            leftover;
    END IF;

    IF NOT (SELECT rolbypassrls FROM pg_roles WHERE rolname = 'authz_lookup_definer') THEN
        RAISE EXCEPTION
            'authz_lookup_definer must have BYPASSRLS; the SECURITY DEFINER lookups it owns run against FORCE ROW LEVEL SECURITY tables';
    END IF;

    -- The host-authentication lookup is the canonical BYPASSRLS-owned
    -- SECURITY DEFINER function (the bootstrap secret lookups in schema authz
    -- are deliberately migrator-owned because their tables do not force RLS).
    SELECT r.rolname
      INTO misowned
      FROM pg_proc p
      JOIN pg_namespace n ON n.oid = p.pronamespace
      JOIN pg_roles r ON r.oid = p.proowner
     WHERE n.nspname = 'authz'
       AND p.proname = 'lookup_worker_host_for_auth';
    IF misowned IS DISTINCT FROM 'authz_lookup_definer' THEN
        RAISE EXCEPTION
            'authz.lookup_worker_host_for_auth must be owned by authz_lookup_definer, got %',
            misowned;
    END IF;
END $$;

RESET check_function_bodies;
RESET client_min_messages;

-- P1.3 begin: rebuildable ref projections. No operation/receipt dependency.
CREATE TABLE public.projection_state (
    projection text NOT NULL CHECK (projection IN ('search', 'docs', 'ontology', 'head_cache')),
    repo_id uuid NOT NULL REFERENCES public.repos(id) ON DELETE CASCADE,
    ref text NOT NULL,
    applied_oid text NOT NULL,
    applied_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (projection, repo_id, ref)
);
ALTER TABLE public.projection_state ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.projection_state FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.projection_state
    USING (repo_id IN (SELECT id FROM public.repos WHERE org_id = ANY(authz.current_org_ids())))
    WITH CHECK (repo_id IN (SELECT id FROM public.repos WHERE org_id = ANY(authz.current_org_ids())));
GRANT SELECT, INSERT, UPDATE, DELETE ON public.projection_state TO g5_tenant;
GRANT SELECT ON public.projection_state TO g5_app;
-- Convergence must inspect queue completion under its tenant capability.
-- Keep the existing system worker's cross-tenant claim/reaper access explicit.
ALTER TABLE public.search_reindex_queue ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.search_reindex_queue FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_projection_queue ON public.search_reindex_queue TO g5_tenant
    USING (org_id = ANY(authz.current_org_ids()))
    WITH CHECK (org_id = ANY(authz.current_org_ids()));
CREATE POLICY system_projection_queue ON public.search_reindex_queue TO g5_app
    USING (true) WITH CHECK (true);
GRANT SELECT ON public.search_reindex_queue TO g5_tenant;
-- A binding may commit after a pass that already checkpointed its ref. Its
-- transaction timestamp can predate that checkpoint, so invalidate atomically
-- with membership instead of treating created_at as projection evidence.
CREATE FUNCTION public.invalidate_bound_ref_projections() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'UPDATE' AND NEW.repo_id IS NOT DISTINCT FROM OLD.repo_id
       AND NEW.repo_branch_name IS NOT DISTINCT FROM OLD.repo_branch_name THEN
        RETURN NEW;
    END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended(
        'projection:' || NEW.repo_id::text || ':refs/heads/' || NEW.repo_branch_name, 0));
    DELETE FROM public.projection_state
    WHERE repo_id = NEW.repo_id AND ref = 'refs/heads/' || NEW.repo_branch_name;
    RETURN NEW;
END;
$$;
CREATE TRIGGER invalidate_bound_ref_projections
    AFTER INSERT OR UPDATE OF repo_id, repo_branch_name ON public.workspace_branch_repos
    FOR EACH ROW EXECUTE FUNCTION public.invalidate_bound_ref_projections();
-- Retain the system role's existing ability to insert/update branch bindings.
CREATE POLICY system_projection_invalidation ON public.projection_state TO g5_app
    USING (true);
GRANT DELETE ON public.projection_state TO g5_app;
-- P1.3 end

-- P3.4 begin: provider session files (design v6 §8.4). They replace the
-- Claude session-store transcript mirror and the Codex rollout mirror tables.
-- A relative path is relative to the provider home (CLAUDE_CONFIG_DIR or
-- CODEX_HOME).
CREATE TABLE public.chat_session_files (
    session_id uuid NOT NULL REFERENCES public.chat_sessions(id) ON DELETE CASCADE,
    org_id uuid NOT NULL REFERENCES public.orgs(id),
    provider text NOT NULL,
    provider_session_id text NOT NULL,
    relative_path text NOT NULL,
    bytes bytea NOT NULL,
    sha256 text NOT NULL,
    size bigint NOT NULL,
    harvested_from_vm_id text,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT chat_session_files_pkey PRIMARY KEY (session_id, provider_session_id, relative_path),
    CONSTRAINT chat_session_files_provider_check CHECK (provider = ANY (ARRAY['claude_code'::text, 'codex'::text])),
    CONSTRAINT chat_session_files_provider_session_id_check CHECK (
        provider_session_id <> ''::text
        AND provider_session_id = btrim(provider_session_id)
        AND octet_length(provider_session_id) <= 256
        AND POSITION(('/'::text) IN (provider_session_id)) = 0
        AND POSITION(('\'::text) IN (provider_session_id)) = 0),
    CONSTRAINT chat_session_files_relative_path_check CHECK (
        ((provider = 'claude_code'::text AND relative_path ~~ 'projects/_%'::text)
            OR (provider = 'codex'::text AND relative_path ~~ 'sessions/_%'::text))
        AND octet_length(relative_path) <= 1024
        AND relative_path !~~ '%/'::text
        AND POSITION(('//'::text) IN (relative_path)) = 0
        AND POSITION(('\'::text) IN (relative_path)) = 0
        AND relative_path !~ '(^|/)\.\.?(/|$)'::text),
    CONSTRAINT chat_session_files_sha256_check CHECK (sha256 ~ '^[0-9a-f]{64}$'::text),
    CONSTRAINT chat_session_files_size_check CHECK (size >= 0 AND size <= 134217728 AND octet_length(bytes) = size)
);
CREATE INDEX idx_chat_session_files_org ON public.chat_session_files USING btree (org_id);
CREATE TRIGGER trg_chat_session_files_set_org BEFORE INSERT ON public.chat_session_files
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();
ALTER TABLE public.chat_session_files ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.chat_session_files FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.chat_session_files
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
-- DELETE: an upload replaces the copy of a Claude file under an older project key.
GRANT SELECT, INSERT, UPDATE, DELETE ON public.chat_session_files TO g5_tenant;
GRANT SELECT ON public.chat_session_files TO g5_app;
-- P3.4 end


-- P3.3 begin
ALTER TABLE public.workspace_settings ADD COLUMN question_timeout_min integer NOT NULL DEFAULT 60
    CHECK (question_timeout_min BETWEEN 1 AND 1440);
CREATE TABLE public.task_questions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id uuid NOT NULL REFERENCES public.orgs(id),
    task_id uuid NOT NULL REFERENCES public.tasks(id) ON DELETE CASCADE,
    -- LEAN-TODO(P4.1): bind to the dispatcher's execution identity.
    exec_id text,
    kind text NOT NULL CHECK (kind IN ('question', 'approval', 'escalation')),
    payload jsonb NOT NULL,
    status text NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'answered', 'expired', 'cancelled')),
    answer jsonb,
    expires_at timestamptz NOT NULL,
    answered_by uuid REFERENCES public.users(id),
    answered_at timestamptz,
    consumed_at timestamptz,
    successor_task_id uuid UNIQUE REFERENCES public.tasks(id),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX task_questions_task ON public.task_questions(task_id, created_at);
CREATE INDEX task_questions_expiry ON public.task_questions(expires_at) WHERE status = 'open';
ALTER TABLE public.task_questions ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.task_questions FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.task_questions
    USING (org_id = ANY (authz.current_org_ids())) WITH CHECK (org_id = ANY (authz.current_org_ids()));
GRANT SELECT, INSERT, UPDATE, DELETE ON public.task_questions TO g5_tenant;
GRANT SELECT, INSERT, UPDATE ON public.task_questions TO g5_app;
-- P3.3 end
