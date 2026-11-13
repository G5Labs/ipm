-- Retire the explicit workspace Restart operation. Preserve operation IDs,
-- generations, deadlines and flush protection for work already in flight:
-- a restart stop finishes as a user stop, and an admitted start is a resume.
-- Use the existing NOLOGIN BYPASSRLS owner to see tenant rows under FORCE RLS,
-- then restore the migrator's exact membership and temporary write grants.
SELECT set_config(
    'g5.retire_restart_authz_lookup_membership_snapshot',
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
    'g5.retire_restart_authz_lookup_had_set',
    CASE WHEN pg_has_role(SESSION_USER, 'authz_lookup_definer', 'SET')
         THEN 'true' ELSE 'false' END,
    false
);
DO $$
BEGIN
    IF current_setting('g5.retire_restart_authz_lookup_had_set', true)
        IS DISTINCT FROM 'true'
    THEN
        EXECUTE format(
            'GRANT authz_lookup_definer TO %I WITH SET TRUE',
            SESSION_USER
        );
    END IF;
END $$;

-- Grant only the columns this backfill writes. REVOKE UPDATE at table level
-- also revokes existing column grants, so record only newly needed column
-- grants and revoke those exact columns after the backfill.
SELECT set_config('g5.retire_restart_column_grants', COALESCE((
    SELECT jsonb_agg(jsonb_build_object('table_name', table_name, 'column_name', column_name))
    FROM (VALUES
        ('workspace_vms', 'stop_reason'),
        ('workspace_vms', 'pending_stop_reason'),
        ('workspace_vms', 'start_intent'),
        ('tasks', 'spec')
    ) AS targets(table_name, column_name)
    WHERE NOT has_column_privilege('authz_lookup_definer', 'public.' || table_name, column_name, 'UPDATE')
), '[]'::jsonb)::text, false);
DO $$
DECLARE target jsonb;
BEGIN
    FOR target IN SELECT * FROM jsonb_array_elements(current_setting('g5.retire_restart_column_grants')::jsonb)
    LOOP
        EXECUTE format('GRANT UPDATE (%I) ON public.%I TO authz_lookup_definer',
            target->>'column_name', target->>'table_name');
    END LOOP;
END $$;

-- This backfill changes vocabulary only; an admitted intent may already be
-- past its deadline awaiting reconciliation. Do not re-run admission's clock
-- check or extend its expiry. The table lock prevents concurrent admission,
-- and all identity fences and row checks remain active throughout.
ALTER TABLE public.workspace_vms DISABLE TRIGGER workspace_vms_validate_start_intent;
SET LOCAL ROLE authz_lookup_definer;
UPDATE public.workspace_vms
SET stop_reason = CASE WHEN stop_reason = 'restart' THEN 'user' ELSE stop_reason END,
    pending_stop_reason = CASE WHEN pending_stop_reason = 'restart' THEN 'user' ELSE pending_stop_reason END,
    start_intent = CASE WHEN start_intent ->> 'kind' = 'restart'
        THEN jsonb_set(start_intent, '{kind}', '"resume"'::jsonb) ELSE start_intent END
WHERE stop_reason = 'restart' OR pending_stop_reason = 'restart'
   OR start_intent ->> 'kind' = 'restart';

-- Preserve restart's task requeue behavior through the retained drain policy,
-- including outcomes that settle after an already-admitted VM start. This does
-- not grant any new start consent; the VM stop remains a loss-protected user
-- stop above. Task identities, statuses and immutable events stay unchanged.
UPDATE public.tasks
SET spec = jsonb_set(spec, '{lifecycle,interrupt_reason}', '"drain"'::jsonb)
WHERE spec #>> '{lifecycle,interrupt_reason}' = 'restart';
-- Drain deferred tenant/intent validation before replacing table constraints.
SET CONSTRAINTS ALL IMMEDIATE;
RESET ROLE;
ALTER TABLE public.workspace_vms ENABLE TRIGGER workspace_vms_validate_start_intent;

CREATE OR REPLACE FUNCTION public.g5_start_intent_is_valid(value jsonb) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    SET search_path TO 'pg_catalog'
    AS $$
SELECT value IS NULL OR (
    jsonb_typeof(value) = 'object'
    AND (value ->> 'id') ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
    AND value ->> 'kind' IN ('resume', 'compound_start', 'rollout_continuation')
    AND value ->> 'status' IN (
        'pending', 'queued_for_capacity', 'starting', 'consumed',
        'expired', 'cancelled', 'failed'
    )
    AND (value ->> 'owner_user_id') ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
    AND (value ->> 'lifecycle_generation') ~ '^[0-9]+$'
    AND (value ->> 'attempts') ~ '^[0-9]+$'
    AND value ? 'created_at'
    AND value ? 'expires_at'
)
$$;

ALTER TABLE public.workspace_vms
    DROP CONSTRAINT workspace_vms_start_intent_check,
    ADD CONSTRAINT workspace_vms_start_intent_check CHECK (public.g5_start_intent_is_valid(start_intent)),
    DROP CONSTRAINT workspace_vms_stop_reason_check,
    ADD CONSTRAINT workspace_vms_stop_reason_check CHECK (
        stop_reason IS NULL OR stop_reason IN (
            'user', 'transfer', 'idle', 'drain', 'rollout', 'credential', 'capacity', 'delete'
        )
    ),
    DROP CONSTRAINT workspace_vms_pending_stop_reason_check,
    ADD CONSTRAINT workspace_vms_pending_stop_reason_check CHECK (
        pending_stop_reason IS NULL OR pending_stop_reason IN (
            'user', 'transfer', 'idle', 'drain', 'rollout', 'credential', 'capacity', 'delete'
        )
    );

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
                   NEW.stop_reason IN ('user', 'transfer', 'idle')
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

DO $$
DECLARE target jsonb;
BEGIN
    FOR target IN SELECT * FROM jsonb_array_elements(current_setting('g5.retire_restart_column_grants')::jsonb)
    LOOP
        EXECUTE format('REVOKE UPDATE (%I) ON public.%I FROM authz_lookup_definer',
            target->>'column_name', target->>'table_name');
    END LOOP;
END $$;

DO $$
DECLARE
    membership_snapshot jsonb :=
        COALESCE(
            current_setting('g5.retire_restart_authz_lookup_membership_snapshot', true),
            '[]'
        )::jsonb;
    membership_row record;
    prior_row jsonb;
BEGIN
    IF current_setting('g5.retire_restart_authz_lookup_had_set', true)
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
